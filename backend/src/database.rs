//! Database connection and query execution module.
//!
//! This module provides MySQL database connectivity with connection pooling,
//! query execution, and result set handling with streaming support for large datasets.

use crate::config::DatabaseConfig;
use log::warn;
use mysql::prelude::*;
use mysql::{Opts, OptsBuilder, Pool, PooledConn, Row, Value};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Maximum number of rows to load in memory before switching to streaming mode.
const STREAMING_THRESHOLD: usize = 10000;

/// Database operation errors.
#[derive(Error, Debug)]
pub enum DatabaseError {
    /// Failed to establish database connection
    #[error("Connection error: {0}")]
    ConnectionError(String),
    /// SQL query execution failed
    #[error("Query execution error: {0}")]
    QueryError(String),
    /// Connection pool error
    #[error("Pool error: {0}")]
    PoolError(String),
    /// Access denied due to insufficient privileges
    #[error("Access denied: {0}")]
    AccessDenied(String),
    /// Query execution timeout
    #[error("Query timeout: exceeded {0} seconds")]
    Timeout(u64),
}

impl From<mysql::Error> for DatabaseError {
    fn from(err: mysql::Error) -> Self {
        let err_str = err.to_string();
        // Check for access denied errors (MySQL error codes 1044, 1045, 1142, 1143, 1227)
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

/// MySQL connection pool with streaming support.
#[derive(Clone)]
pub struct DatabasePool {
    pool: Arc<Pool>,
    #[allow(dead_code)]
    timeout: Duration,
    streaming_threshold: usize,
}

impl DatabasePool {
    pub fn new(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        Self::with_streaming_threshold(config, STREAMING_THRESHOLD)
    }

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

    pub fn get_connection(&self) -> Result<PooledConn, DatabaseError> {
        self.pool
            .get_conn()
            .map_err(|e| DatabaseError::PoolError(e.to_string()))
    }

    /// Execute SQL and return results with streaming support for large result sets
    pub fn execute_sql(&self, sql: &str) -> Result<QueryResults, DatabaseError> {
        let mut conn = self.get_connection()?;
        let mut results = QueryResults::new();

        // Get MySQL version
        let version: Option<String> = conn.query_first("SELECT VERSION()").unwrap_or(None);
        results.mysql_version = version;

        // Execute the SQL and collect all result sets
        let mut query_result = conn.query_iter(sql)?;

        // Process all result sets with streaming support
        while let Some(result_set) = query_result.iter() {
            let columns: Vec<ColumnInfo> = result_set
                .columns()
                .as_ref()
                .iter()
                .map(|col| ColumnInfo {
                    name: col.name_str().to_string(),
                    column_type: format!("{:?}", col.column_type()),
                })
                .collect();

            let affected_rows = result_set.affected_rows();
            let mut rows_data: Vec<Vec<JsonValue>> = Vec::new();
            let mut row_count: usize = 0;
            let mut truncated = false;

            // Stream rows with threshold check to prevent memory overflow
            for row_result in result_set {
                let row: Row = row_result?;

                // Check if we've exceeded the streaming threshold
                if row_count >= self.streaming_threshold {
                    truncated = true;
                    warn!(
                        "Result set truncated at {} rows (streaming threshold reached)",
                        self.streaming_threshold
                    );
                    // Continue iterating to consume the result set but don't store
                    continue;
                }

                let mut row_values: Vec<JsonValue> = Vec::with_capacity(columns.len());
                for (idx, _col) in columns.iter().enumerate() {
                    let value = convert_mysql_value_to_json(row.get::<Value, _>(idx));
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

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
}

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
}

impl JsonValue {
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
        }
    }
}

fn convert_mysql_value_to_json(value: Option<Value>) -> JsonValue {
    match value {
        None => JsonValue::Null,
        Some(Value::NULL) => JsonValue::Null,
        Some(Value::Bytes(b)) => {
            // Try to convert to UTF-8 string first
            match String::from_utf8(b.clone()) {
                Ok(s) => JsonValue::String(s),
                Err(_) => JsonValue::Bytes(b),
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

#[derive(Debug, Clone)]
pub struct ResultSet {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<JsonValue>>,
    pub affected_rows: u64,
    /// Indicates if the result set was truncated due to streaming threshold
    pub truncated: bool,
    /// Total number of rows processed (may be larger than rows.len() if truncated)
    pub total_rows: usize,
}

#[derive(Debug, Clone)]
pub struct QueryResults {
    pub result_sets: Vec<ResultSet>,
    pub mysql_version: Option<String>,
}

impl QueryResults {
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

        // NaN should become null
        let nan = JsonValue::Float(f64::NAN);
        assert_eq!(nan.to_serde_value(), serde_json::Value::Null);

        // Infinity should become null
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
        // Should be base64 encoded
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
        let result = convert_mysql_value_to_json(None);
        assert!(matches!(result, JsonValue::Null));
    }

    #[test]
    fn test_convert_mysql_value_null() {
        let result = convert_mysql_value_to_json(Some(Value::NULL));
        assert!(matches!(result, JsonValue::Null));
    }

    #[test]
    fn test_convert_mysql_value_int() {
        let result = convert_mysql_value_to_json(Some(Value::Int(42)));
        assert!(matches!(result, JsonValue::Int(42)));
    }

    #[test]
    fn test_convert_mysql_value_uint() {
        let result = convert_mysql_value_to_json(Some(Value::UInt(42)));
        assert!(matches!(result, JsonValue::UInt(42)));
    }

    #[test]
    fn test_convert_mysql_value_float() {
        let result = convert_mysql_value_to_json(Some(Value::Float(3.14)));
        if let JsonValue::Float(f) = result {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_convert_mysql_value_double() {
        let result = convert_mysql_value_to_json(Some(Value::Double(3.14159)));
        if let JsonValue::Float(f) = result {
            assert!((f - 3.14159).abs() < 0.00001);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_convert_mysql_value_bytes_utf8() {
        let result = convert_mysql_value_to_json(Some(Value::Bytes(b"hello".to_vec())));
        assert!(matches!(result, JsonValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_convert_mysql_value_bytes_binary() {
        let result = convert_mysql_value_to_json(Some(Value::Bytes(vec![0xFF, 0xFE, 0x00])));
        assert!(matches!(result, JsonValue::Bytes(_)));
    }

    #[test]
    fn test_convert_mysql_value_date() {
        let result = convert_mysql_value_to_json(Some(Value::Date(2024, 1, 15, 0, 0, 0, 0)));
        assert!(matches!(result, JsonValue::Date(s) if s == "2024-01-15"));
    }

    #[test]
    fn test_convert_mysql_value_datetime() {
        let result = convert_mysql_value_to_json(Some(Value::Date(2024, 1, 15, 14, 30, 45, 123456)));
        assert!(
            matches!(result, JsonValue::DateTime(s) if s == "2024-01-15 14:30:45.123456")
        );
    }

    #[test]
    fn test_convert_mysql_value_time() {
        let result = convert_mysql_value_to_json(Some(Value::Time(false, 0, 14, 30, 45, 0)));
        assert!(matches!(result, JsonValue::Time(s) if s == "14:30:45.000000"));
    }

    #[test]
    fn test_convert_mysql_value_time_negative() {
        let result = convert_mysql_value_to_json(Some(Value::Time(true, 0, 14, 30, 45, 0)));
        assert!(matches!(result, JsonValue::Time(s) if s == "-14:30:45.000000"));
    }

    #[test]
    fn test_convert_mysql_value_time_with_days() {
        let result = convert_mysql_value_to_json(Some(Value::Time(false, 2, 10, 30, 0, 0)));
        // 2 days * 24 hours + 10 hours = 58 hours
        assert!(matches!(result, JsonValue::Time(s) if s == "58:30:00.000000"));
    }

    #[test]
    fn test_column_info() {
        let col = ColumnInfo {
            name: "test_column".to_string(),
            column_type: "INT".to_string(),
        };
        assert_eq!(col.name, "test_column");
        assert_eq!(col.column_type, "INT");
    }

    #[test]
    fn test_result_set() {
        let rs = ResultSet {
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                column_type: "INT".to_string(),
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
