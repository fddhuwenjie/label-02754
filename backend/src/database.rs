//! 数据库连接和查询执行模块
//!
//! 本模块提供 MySQL 数据库连接功能，包括连接池、查询执行，
//! 以及支持大数据集流式处理的结果集处理。
//! 支持 ENUM/SET/GEOMETRY/JSON 等特殊类型的语义化处理。

use crate::config::DatabaseConfig;
use log::warn;
use mysql::prelude::*;
use mysql::{Opts, OptsBuilder, Pool, PooledConn, Row, Value};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use std::thread;
use std::sync::mpsc;

/// 切换到流式模式前的最大内存行数
const STREAMING_THRESHOLD: usize = 10000;

/// 数据库操作错误
#[derive(Error, Debug)]
pub enum DatabaseError {
    /// 建立数据库连接失败
    #[error("连接错误: {0}")]
    ConnectionError(String),
    /// SQL 查询执行失败
    #[error("查询执行错误: {0}")]
    QueryError(String),
    /// 连接池错误
    #[error("连接池错误: {0}")]
    PoolError(String),
    /// 权限不足导致访问被拒绝
    #[error("访问被拒绝: {0}")]
    AccessDenied(String),
    /// 查询执行超时
    #[error("查询超时: 超过 {0} 秒")]
    Timeout(u64),
}

impl From<mysql::Error> for DatabaseError {
    fn from(err: mysql::Error) -> Self {
        let err_str = err.to_string();
        // 检查访问被拒绝错误（MySQL 错误码 1044, 1045, 1142, 1143, 1227）
        if err_str.contains("Access denied")
            || err_str.contains("access denied")
            || err_str.contains("1044")
            || err_str.contains("1045")
            || err_str.contains("1142")
            || err_str.contains("1143")
            || err_str.contains("1227")
        {
            DatabaseError::AccessDenied(err_str)
        } else if err_str.contains("timed out") || err_str.contains("timeout") {
            DatabaseError::Timeout(0)
        } else {
            DatabaseError::QueryError(err_str)
        }
    }
}

/// 支持流式处理的 MySQL 连接池
#[derive(Clone)]
pub struct DatabasePool {
    pool: Arc<Pool>,
    timeout: Duration,
    streaming_threshold: usize,
}

impl DatabasePool {
    /// 创建新的数据库连接池
    pub fn new(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        Self::with_streaming_threshold(config, STREAMING_THRESHOLD)
    }

    /// 创建带自定义流式阈值的数据库连接池
    pub fn with_streaming_threshold(
        config: &DatabaseConfig,
        streaming_threshold: usize,
    ) -> Result<Self, DatabaseError> {
        let opts: Opts = OptsBuilder::new()
            .ip_or_hostname(Some(&config.host))
            .tcp_port(config.port)
            .user(Some(&config.username))
            .pass(Some(&config.password))
            .db_name(Some(&config.database))
            .tcp_connect_timeout(Some(Duration::from_secs(30)))
            .read_timeout(Some(Duration::from_secs(config.timeout_seconds)))
            .write_timeout(Some(Duration::from_secs(config.timeout_seconds)))
            .prefer_socket(false)
            .into();

        let pool = Pool::new(opts).map_err(|e| DatabaseError::ConnectionError(e.to_string()))?;

        Ok(Self {
            pool: Arc::new(pool),
            timeout: Duration::from_secs(config.timeout_seconds),
            streaming_threshold,
        })
    }

    /// 从连接池获取连接
    #[allow(dead_code)]
    pub fn get_connection(&self) -> Result<PooledConn, DatabaseError> {
        self.pool
            .get_conn()
            .map_err(|e| DatabaseError::PoolError(e.to_string()))
    }

    /// 执行 SQL 并返回结果，支持大结果集的流式处理和应用层超时控制
    pub fn execute_sql(&self, sql: &str) -> Result<QueryResults, DatabaseError> {
        let timeout = self.timeout;
        let pool = self.pool.clone();
        let streaming_threshold = self.streaming_threshold;
        let sql = sql.to_string();
        
        // 使用 channel 实现应用层超时控制
        let (tx, rx) = mpsc::channel();
        
        let handle = thread::spawn(move || {
            let result = Self::execute_sql_internal(&pool, &sql, streaming_threshold);
            let _ = tx.send(result);
        });
        
        // 等待结果或超时
        match rx.recv_timeout(timeout) {
            Ok(result) => {
                let _ = handle.join();
                result
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // 超时，线程会在后台继续运行直到完成，但我们返回超时错误
                Err(DatabaseError::Timeout(timeout.as_secs()))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(DatabaseError::QueryError("查询执行线程异常终止".to_string()))
            }
        }
    }
    
    /// 内部执行 SQL 的方法
    fn execute_sql_internal(
        pool: &Pool,
        sql: &str,
        streaming_threshold: usize,
    ) -> Result<QueryResults, DatabaseError> {
        let mut conn = pool
            .get_conn()
            .map_err(|e| DatabaseError::PoolError(e.to_string()))?;
        let mut results = QueryResults::new();

        // 获取 MySQL 版本
        let version: Option<String> = conn.query_first("SELECT VERSION()").unwrap_or(None);
        results.mysql_version = version;

        // 执行 SQL 并收集所有结果集
        let mut query_result = conn.query_iter(sql)?;

        // 使用流式处理处理所有结果集
        while let Some(result_set) = query_result.iter() {
            let columns: Vec<ColumnInfo> = result_set
                .columns()
                .as_ref()
                .iter()
                .map(|col| {
                    let col_type = col.column_type();
                    let flags = col.flags();
                    ColumnInfo {
                        name: col.name_str().to_string(),
                        column_type: format!("{:?}", col_type),
                        is_enum: flags.contains(mysql::consts::ColumnFlags::ENUM_FLAG),
                        is_set: flags.contains(mysql::consts::ColumnFlags::SET_FLAG),
                        is_binary: flags.contains(mysql::consts::ColumnFlags::BINARY_FLAG),
                        is_json: col_type == mysql::consts::ColumnType::MYSQL_TYPE_JSON,
                        is_geometry: col_type == mysql::consts::ColumnType::MYSQL_TYPE_GEOMETRY,
                    }
                })
                .collect();

            let affected_rows = result_set.affected_rows();
            let mut rows_data: Vec<Vec<JsonValue>> = Vec::new();
            let mut row_count: usize = 0;
            let mut truncated = false;

            // 流式读取行，检查阈值以防止内存溢出
            for row_result in result_set {
                let row: Row = row_result?;

                // 检查是否超过流式阈值
                if row_count >= streaming_threshold {
                    truncated = true;
                    warn!(
                        "结果集在 {} 行处被截断（达到流式阈值）",
                        streaming_threshold
                    );
                    // 继续迭代以消费结果集，但不存储
                    continue;
                }

                let mut row_values: Vec<JsonValue> = Vec::with_capacity(columns.len());
                for (idx, col) in columns.iter().enumerate() {
                    let value = convert_mysql_value_to_json(row.get::<Value, _>(idx), col);
                    row_values.push(value);
                }
                rows_data.push(row_values);
                row_count += 1;
            }

            results.result_sets.push(ResultSet {
                columns,
                rows: rows_data,
                affected_rows,
                truncated,
                total_rows: row_count,
            });
        }

        Ok(results)
    }
}

/// 列信息
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// 列名
    pub name: String,
    /// 列类型
    pub column_type: String,
    /// 是否为 ENUM 类型
    pub is_enum: bool,
    /// 是否为 SET 类型
    pub is_set: bool,
    /// 是否为二进制类型
    pub is_binary: bool,
    /// 是否为 JSON 类型
    pub is_json: bool,
    /// 是否为 GEOMETRY 类型
    pub is_geometry: bool,
}

/// JSON 值枚举，用于 MySQL 到 JSON 的类型转换
#[derive(Debug, Clone)]
pub enum JsonValue {
    Null,
    #[allow(dead_code)]
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Date(String),
    Time(String),
    DateTime(String),
    /// ENUM 类型，包含枚举值
    Enum(String),
    /// SET 类型，包含集合值列表
    Set(Vec<String>),
    /// JSON 类型，包含解析后的 JSON 值
    Json(serde_json::Value),
    /// GEOMETRY 类型，包含 WKB 的 base64 编码和可选的 WKT 表示
    Geometry { wkb_base64: String, wkt: Option<String> },
}

impl JsonValue {
    /// 转换为 serde_json::Value
    pub fn to_serde_value(&self) -> serde_json::Value {
        match self {
            JsonValue::Null => serde_json::Value::Null,
            JsonValue::Bool(b) => serde_json::Value::Bool(*b),
            JsonValue::Int(i) => serde_json::json!(*i),
            JsonValue::UInt(u) => serde_json::json!(*u),
            JsonValue::Float(f) => {
                if f.is_nan() || f.is_infinite() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(*f)
                }
            }
            JsonValue::String(s) => serde_json::Value::String(s.clone()),
            JsonValue::Bytes(b) => {
                serde_json::Value::String(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    b,
                ))
            }
            JsonValue::Date(d) => serde_json::Value::String(d.clone()),
            JsonValue::Time(t) => serde_json::Value::String(t.clone()),
            JsonValue::DateTime(dt) => serde_json::Value::String(dt.clone()),
            JsonValue::Enum(e) => serde_json::json!({
                "_type": "enum",
                "value": e
            }),
            JsonValue::Set(s) => serde_json::json!({
                "_type": "set",
                "values": s
            }),
            JsonValue::Json(j) => serde_json::json!({
                "_type": "json",
                "value": j
            }),
            JsonValue::Geometry { wkb_base64, wkt } => serde_json::json!({
                "_type": "geometry",
                "wkb_base64": wkb_base64,
                "wkt": wkt
            }),
        }
    }
}

/// 尝试将 WKB 转换为简单的 WKT 表示（仅支持 POINT 类型）
fn try_wkb_to_wkt(wkb: &[u8]) -> Option<String> {
    // WKB 格式: byte_order(1) + type(4) + coordinates
    // 仅处理简单的 POINT 类型
    if wkb.len() < 21 {
        return None;
    }
    
    let byte_order = wkb[0];
    let geom_type = if byte_order == 1 {
        // Little endian
        u32::from_le_bytes([wkb[1], wkb[2], wkb[3], wkb[4]])
    } else {
        // Big endian
        u32::from_be_bytes([wkb[1], wkb[2], wkb[3], wkb[4]])
    };
    
    // Type 1 = Point
    if geom_type == 1 && wkb.len() >= 21 {
        let (x, y) = if byte_order == 1 {
            let x = f64::from_le_bytes([wkb[5], wkb[6], wkb[7], wkb[8], wkb[9], wkb[10], wkb[11], wkb[12]]);
            let y = f64::from_le_bytes([wkb[13], wkb[14], wkb[15], wkb[16], wkb[17], wkb[18], wkb[19], wkb[20]]);
            (x, y)
        } else {
            let x = f64::from_be_bytes([wkb[5], wkb[6], wkb[7], wkb[8], wkb[9], wkb[10], wkb[11], wkb[12]]);
            let y = f64::from_be_bytes([wkb[13], wkb[14], wkb[15], wkb[16], wkb[17], wkb[18], wkb[19], wkb[20]]);
            (x, y)
        };
        return Some(format!("POINT({} {})", x, y));
    }
    
    None
}

/// 将 MySQL 值转换为 JSON 值，支持特殊类型的语义化处理
fn convert_mysql_value_to_json(value: Option<Value>, col_info: &ColumnInfo) -> JsonValue {
    match value {
        None => JsonValue::Null,
        Some(Value::NULL) => JsonValue::Null,
        Some(Value::Bytes(b)) => {
            // 根据列类型进行语义化处理
            if col_info.is_json {
                // JSON 类型：尝试解析为 JSON 对象
                match String::from_utf8(b.clone()) {
                    Ok(s) => match serde_json::from_str(&s) {
                        Ok(json_val) => JsonValue::Json(json_val),
                        Err(_) => JsonValue::String(s),
                    },
                    Err(_) => JsonValue::Bytes(b),
                }
            } else if col_info.is_enum {
                // ENUM 类型
                match String::from_utf8(b) {
                    Ok(s) => JsonValue::Enum(s),
                    Err(e) => JsonValue::Bytes(e.into_bytes()),
                }
            } else if col_info.is_set {
                // SET 类型：按逗号分割
                match String::from_utf8(b) {
                    Ok(s) => {
                        let values: Vec<String> = if s.is_empty() {
                            vec![]
                        } else {
                            s.split(',').map(|v| v.to_string()).collect()
                        };
                        JsonValue::Set(values)
                    }
                    Err(e) => JsonValue::Bytes(e.into_bytes()),
                }
            } else if col_info.is_geometry {
                // GEOMETRY 类型：返回 WKB 的 base64 编码和可选的 WKT
                let wkb_base64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &b,
                );
                let wkt = try_wkb_to_wkt(&b);
                JsonValue::Geometry { wkb_base64, wkt }
            } else if col_info.is_binary {
                // 二进制类型
                JsonValue::Bytes(b)
            } else {
                // 普通字符串
                match String::from_utf8(b.clone()) {
                    Ok(s) => JsonValue::String(s),
                    Err(_) => JsonValue::Bytes(b),
                }
            }
        }
        Some(Value::Int(i)) => JsonValue::Int(i),
        Some(Value::UInt(u)) => JsonValue::UInt(u),
        Some(Value::Float(f)) => JsonValue::Float(f as f64),
        Some(Value::Double(d)) => JsonValue::Float(d),
        Some(Value::Date(year, month, day, hour, min, sec, micro)) => {
            if hour == 0 && min == 0 && sec == 0 && micro == 0 {
                JsonValue::Date(format!("{:04}-{:02}-{:02}", year, month, day))
            } else {
                JsonValue::DateTime(format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
                    year, month, day, hour, min, sec, micro
                ))
            }
        }
        Some(Value::Time(negative, days, hours, minutes, seconds, micro)) => {
            let sign = if negative { "-" } else { "" };
            let total_hours = days * 24 + hours as u32;
            JsonValue::Time(format!(
                "{}{:02}:{:02}:{:02}.{:06}",
                sign, total_hours, minutes, seconds, micro
            ))
        }
    }
}

/// 单个结果集
#[derive(Debug, Clone)]
pub struct ResultSet {
    /// 列信息列表
    pub columns: Vec<ColumnInfo>,
    /// 行数据
    pub rows: Vec<Vec<JsonValue>>,
    /// 影响的行数
    pub affected_rows: u64,
    /// 是否因流式阈值而被截断
    pub truncated: bool,
    /// 处理的总行数（如果被截断，可能大于 rows.len()）
    pub total_rows: usize,
}

/// 查询结果
#[derive(Debug, Clone)]
pub struct QueryResults {
    /// 结果集列表
    pub result_sets: Vec<ResultSet>,
    /// MySQL 版本
    pub mysql_version: Option<String>,
}

impl QueryResults {
    /// 创建新的空查询结果
    pub fn new() -> Self {
        Self {
            result_sets: Vec::new(),
            mysql_version: None,
        }
    }
}

impl Default for QueryResults {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_value_null() {
        let value = JsonValue::Null;
        assert_eq!(value.to_serde_value(), serde_json::Value::Null);
    }

    #[test]
    fn test_json_value_bool() {
        let value_true = JsonValue::Bool(true);
        let value_false = JsonValue::Bool(false);
        assert_eq!(value_true.to_serde_value(), serde_json::Value::Bool(true));
        assert_eq!(value_false.to_serde_value(), serde_json::Value::Bool(false));
    }

    #[test]
    fn test_json_value_int() {
        let value = JsonValue::Int(42);
        assert_eq!(value.to_serde_value(), serde_json::json!(42));

        let negative = JsonValue::Int(-100);
        assert_eq!(negative.to_serde_value(), serde_json::json!(-100));

        let large = JsonValue::Int(i64::MAX);
        assert_eq!(large.to_serde_value(), serde_json::json!(i64::MAX));
    }

    #[test]
    fn test_json_value_uint() {
        let value = JsonValue::UInt(42);
        assert_eq!(value.to_serde_value(), serde_json::json!(42u64));

        let large = JsonValue::UInt(u64::MAX);
        assert_eq!(large.to_serde_value(), serde_json::json!(u64::MAX));
    }

    #[test]
    fn test_json_value_float() {
        let value = JsonValue::Float(3.14159);
        let result = value.to_serde_value();
        assert!(result.is_number());

        // NaN 应该变成 null
        let nan = JsonValue::Float(f64::NAN);
        assert_eq!(nan.to_serde_value(), serde_json::Value::Null);

        // 无穷大应该变成 null
        let inf = JsonValue::Float(f64::INFINITY);
        assert_eq!(inf.to_serde_value(), serde_json::Value::Null);
    }

    #[test]
    fn test_json_value_string() {
        let value = JsonValue::String("hello world".to_string());
        assert_eq!(
            value.to_serde_value(),
            serde_json::Value::String("hello world".to_string())
        );

        let empty = JsonValue::String("".to_string());
        assert_eq!(
            empty.to_serde_value(),
            serde_json::Value::String("".to_string())
        );

        let unicode = JsonValue::String("你好世界".to_string());
        assert_eq!(
            unicode.to_serde_value(),
            serde_json::Value::String("你好世界".to_string())
        );
    }

    #[test]
    fn test_json_value_bytes() {
        let value = JsonValue::Bytes(vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]); // "Hello"
        let result = value.to_serde_value();
        assert!(result.is_string());
        // 应该是 base64 编码
        assert_eq!(result.as_str().unwrap(), "SGVsbG8=");
    }

    #[test]
    fn test_json_value_date() {
        let value = JsonValue::Date("2024-01-15".to_string());
        assert_eq!(
            value.to_serde_value(),
            serde_json::Value::String("2024-01-15".to_string())
        );
    }

    #[test]
    fn test_json_value_time() {
        let value = JsonValue::Time("14:30:00.000000".to_string());
        assert_eq!(
            value.to_serde_value(),
            serde_json::Value::String("14:30:00.000000".to_string())
        );
    }

    #[test]
    fn test_json_value_datetime() {
        let value = JsonValue::DateTime("2024-01-15 14:30:00.000000".to_string());
        assert_eq!(
            value.to_serde_value(),
            serde_json::Value::String("2024-01-15 14:30:00.000000".to_string())
        );
    }

    #[test]
    fn test_convert_mysql_value_none() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "VARCHAR".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(None, &col);
        assert!(matches!(result, JsonValue::Null));
    }

    #[test]
    fn test_convert_mysql_value_null() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "VARCHAR".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::NULL), &col);
        assert!(matches!(result, JsonValue::Null));
    }

    #[test]
    fn test_convert_mysql_value_int() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "INT".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Int(42)), &col);
        assert!(matches!(result, JsonValue::Int(42)));
    }

    #[test]
    fn test_convert_mysql_value_uint() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "INT UNSIGNED".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::UInt(42)), &col);
        assert!(matches!(result, JsonValue::UInt(42)));
    }

    #[test]
    fn test_convert_mysql_value_float() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "FLOAT".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Float(3.14)), &col);
        if let JsonValue::Float(f) = result {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("期望 Float");
        }
    }

    #[test]
    fn test_convert_mysql_value_double() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "DOUBLE".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Double(3.14159)), &col);
        if let JsonValue::Float(f) = result {
            assert!((f - 3.14159).abs() < 0.00001);
        } else {
            panic!("期望 Float");
        }
    }

    #[test]
    fn test_convert_mysql_value_bytes_utf8() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "VARCHAR".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Bytes(b"hello".to_vec())), &col);
        assert!(matches!(result, JsonValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_convert_mysql_value_bytes_binary() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "BLOB".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: true,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Bytes(vec![0xFF, 0xFE, 0x00])), &col);
        assert!(matches!(result, JsonValue::Bytes(_)));
    }

    #[test]
    fn test_convert_mysql_value_date() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "DATE".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Date(2024, 1, 15, 0, 0, 0, 0)), &col);
        assert!(matches!(result, JsonValue::Date(s) if s == "2024-01-15"));
    }

    #[test]
    fn test_convert_mysql_value_datetime() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "DATETIME".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Date(2024, 1, 15, 14, 30, 45, 123456)), &col);
        assert!(
            matches!(result, JsonValue::DateTime(s) if s == "2024-01-15 14:30:45.123456")
        );
    }

    #[test]
    fn test_convert_mysql_value_time() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "TIME".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Time(false, 0, 14, 30, 45, 0)), &col);
        assert!(matches!(result, JsonValue::Time(s) if s == "14:30:45.000000"));
    }

    #[test]
    fn test_convert_mysql_value_time_negative() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "TIME".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Time(true, 0, 14, 30, 45, 0)), &col);
        assert!(matches!(result, JsonValue::Time(s) if s == "-14:30:45.000000"));
    }

    #[test]
    fn test_convert_mysql_value_time_with_days() {
        let col = ColumnInfo {
            name: "test".to_string(),
            column_type: "TIME".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Time(false, 2, 10, 30, 0, 0)), &col);
        // 2 天 * 24 小时 + 10 小时 = 58 小时
        assert!(matches!(result, JsonValue::Time(s) if s == "58:30:00.000000"));
    }

    #[test]
    fn test_convert_mysql_value_enum() {
        let col = ColumnInfo {
            name: "status".to_string(),
            column_type: "ENUM".to_string(),
            is_enum: true,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Bytes(b"active".to_vec())), &col);
        assert!(matches!(&result, JsonValue::Enum(s) if s == "active"));
        
        // 验证序列化格式
        let json = result.to_serde_value();
        assert_eq!(json["_type"], "enum");
        assert_eq!(json["value"], "active");
    }

    #[test]
    fn test_convert_mysql_value_set() {
        let col = ColumnInfo {
            name: "tags".to_string(),
            column_type: "SET".to_string(),
            is_enum: false,
            is_set: true,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Bytes(b"tag1,tag2,tag3".to_vec())), &col);
        if let JsonValue::Set(values) = &result {
            assert_eq!(values, &vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()]);
        } else {
            panic!("期望 Set");
        }
        
        // 验证序列化格式
        let json = result.to_serde_value();
        assert_eq!(json["_type"], "set");
        assert_eq!(json["values"], serde_json::json!(["tag1", "tag2", "tag3"]));
    }

    #[test]
    fn test_convert_mysql_value_set_empty() {
        let col = ColumnInfo {
            name: "tags".to_string(),
            column_type: "SET".to_string(),
            is_enum: false,
            is_set: true,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        let result = convert_mysql_value_to_json(Some(Value::Bytes(b"".to_vec())), &col);
        if let JsonValue::Set(values) = &result {
            assert!(values.is_empty());
        } else {
            panic!("期望 Set");
        }
    }

    #[test]
    fn test_convert_mysql_value_json() {
        let col = ColumnInfo {
            name: "data".to_string(),
            column_type: "JSON".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: true,
            is_geometry: false,
        };
        let json_str = r#"{"name":"test","value":123}"#;
        let result = convert_mysql_value_to_json(Some(Value::Bytes(json_str.as_bytes().to_vec())), &col);
        if let JsonValue::Json(json_val) = &result {
            assert_eq!(json_val["name"], "test");
            assert_eq!(json_val["value"], 123);
        } else {
            panic!("期望 Json");
        }
        
        // 验证序列化格式
        let json = result.to_serde_value();
        assert_eq!(json["_type"], "json");
    }

    #[test]
    fn test_convert_mysql_value_geometry_point() {
        let col = ColumnInfo {
            name: "location".to_string(),
            column_type: "GEOMETRY".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: true,
        };
        // WKB for POINT(1.0 2.0) - little endian
        let mut wkb = vec![0x01]; // byte order: little endian
        wkb.extend_from_slice(&1u32.to_le_bytes()); // type: Point
        wkb.extend_from_slice(&1.0f64.to_le_bytes()); // x
        wkb.extend_from_slice(&2.0f64.to_le_bytes()); // y
        
        let result = convert_mysql_value_to_json(Some(Value::Bytes(wkb)), &col);
        if let JsonValue::Geometry { wkb_base64, wkt } = &result {
            assert!(!wkb_base64.is_empty());
            assert_eq!(wkt.as_deref(), Some("POINT(1 2)"));
        } else {
            panic!("期望 Geometry");
        }
        
        // 验证序列化格式
        let json = result.to_serde_value();
        assert_eq!(json["_type"], "geometry");
        assert!(json["wkb_base64"].is_string());
        assert!(json["wkt"].is_string());
    }

    #[test]
    fn test_column_info() {
        let col = ColumnInfo {
            name: "test_column".to_string(),
            column_type: "INT".to_string(),
            is_enum: false,
            is_set: false,
            is_binary: false,
            is_json: false,
            is_geometry: false,
        };
        assert_eq!(col.name, "test_column");
        assert_eq!(col.column_type, "INT");
        assert!(!col.is_enum);
    }

    #[test]
    fn test_result_set() {
        let rs = ResultSet {
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                column_type: "INT".to_string(),
                is_enum: false,
                is_set: false,
                is_binary: false,
                is_json: false,
                is_geometry: false,
            }],
            rows: vec![vec![JsonValue::Int(1)]],
            affected_rows: 0,
            truncated: false,
            total_rows: 1,
        };
        assert_eq!(rs.columns.len(), 1);
        assert_eq!(rs.rows.len(), 1);
        assert!(!rs.truncated);
    }

    #[test]
    fn test_query_results_new() {
        let results = QueryResults::new();
        assert!(results.result_sets.is_empty());
        assert!(results.mysql_version.is_none());
    }

    #[test]
    fn test_query_results_default() {
        let results = QueryResults::default();
        assert!(results.result_sets.is_empty());
        assert!(results.mysql_version.is_none());
    }
}
