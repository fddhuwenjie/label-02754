//! JSON 输出写入模块
//!
//! 处理查询结果到 JSON 文件的原子性写入，使用文件锁确保并发操作时的数据完整性。
//! 支持跨进程互斥锁，确保多进程同时写入同一文件时的安全性。

use crate::database::{QueryResults, ResultSet};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use fslock::LockFile;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// JSON 写入错误
#[derive(Error, Debug)]
pub enum JsonWriterError {
    /// 文件 I/O 错误
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    /// JSON 序列化错误
    #[error("序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),
    /// 获取文件锁失败
    #[error("获取文件锁失败: {0}")]
    LockError(String),
    /// 原子写入操作失败
    #[error("原子写入失败: {0}")]
    AtomicWriteError(String),
}

/// JSON 输出中包含的执行元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMetadata {
    /// 查询执行的时间戳
    pub execution_time: DateTime<Utc>,
    /// MySQL 服务器版本
    pub mysql_version: Option<String>,
    /// 返回的结果集数量
    pub total_result_sets: usize,
    /// 源 SQL 文件路径
    pub source_file: String,
    /// 查询执行时长（毫秒）
    pub execution_duration_ms: u64,
    /// 所有结果集的总受影响行数
    pub total_affected_rows: u64,
    /// 所有结果集的总行数
    pub total_rows: usize,
}

/// JSON 格式的单个结果集
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonResultSet {
    /// 结果集索引（从 0 开始）
    pub index: usize,
    /// 列定义
    pub columns: Vec<JsonColumnInfo>,
    /// 行数据（键值对映射）
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    /// 此结果集的行数
    pub row_count: usize,
    /// 查询影响的行数
    pub affected_rows: u64,
    /// 结果集是否因大小限制而被截断
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// 截断前的总行数（仅在截断时存在）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<usize>,
}

/// JSON 格式的列元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonColumnInfo {
    /// 列名
    pub name: String,
    /// MySQL 数据类型
    pub data_type: String,
}

/// 完整的 JSON 输出结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutput {
    /// 执行元数据
    pub metadata: JsonMetadata,
    /// 查询结果集
    pub result_sets: Vec<JsonResultSet>,
}

/// 处理查询结果写入 JSON 文件
pub struct JsonWriter;

impl JsonWriter {
    /// 原子性地将查询结果写入 JSON 文件
    ///
    /// 使用临时文件和重命名确保原子写入。
    /// 即使进程被中断，JSON 文件也不会损坏。
    ///
    /// # 参数
    /// * `path` - 输出 JSON 文件路径
    /// * `results` - 查询执行结果
    /// * `source_file` - 源 SQL 文件路径（用于元数据）
    /// * `execution_duration_ms` - 查询执行时间（毫秒）
    pub fn write_results<P: AsRef<Path>>(
        path: P,
        results: &QueryResults,
        source_file: &str,
        execution_duration_ms: u64,
    ) -> Result<(), JsonWriterError> {
        let path = path.as_ref();
        
        // 如果父目录不存在则创建
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 将结果转换为 JSON 结构
        let output = Self::convert_to_json_output(results, source_file, execution_duration_ms);
        
        // 序列化为 JSON
        let json_content = serde_json::to_string_pretty(&output)?;
        
        // 原子写入：先写入临时文件，然后重命名
        Self::atomic_write(path, &json_content)?;
        
        Ok(())
    }

    fn convert_to_json_output(
        results: &QueryResults,
        source_file: &str,
        execution_duration_ms: u64,
    ) -> JsonOutput {
        let result_sets: Vec<JsonResultSet> = results
            .result_sets
            .iter()
            .enumerate()
            .map(|(idx, rs)| Self::convert_result_set(idx, rs))
            .collect();

        // 聚合总受影响行数和总行数
        let total_affected_rows: u64 = result_sets.iter().map(|rs| rs.affected_rows).sum();
        let total_rows: usize = result_sets.iter().map(|rs| rs.row_count).sum();

        JsonOutput {
            metadata: JsonMetadata {
                execution_time: Utc::now(),
                mysql_version: results.mysql_version.clone(),
                total_result_sets: result_sets.len(),
                source_file: source_file.to_string(),
                execution_duration_ms,
                total_affected_rows,
                total_rows,
            },
            result_sets,
        }
    }

    fn convert_result_set(index: usize, rs: &ResultSet) -> JsonResultSet {
        let columns: Vec<JsonColumnInfo> = rs
            .columns
            .iter()
            .map(|c| JsonColumnInfo {
                name: c.name.clone(),
                data_type: c.column_type.clone(),
            })
            .collect();

        let rows: Vec<serde_json::Map<String, serde_json::Value>> = rs
            .rows
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (idx, value) in row.iter().enumerate() {
                    if idx < columns.len() {
                        map.insert(columns[idx].name.clone(), value.to_serde_value());
                    }
                }
                map
            })
            .collect();

        JsonResultSet {
            index,
            columns,
            row_count: rows.len(),
            rows,
            affected_rows: rs.affected_rows,
            truncated: rs.truncated,
            total_rows: if rs.truncated { Some(rs.total_rows) } else { None },
        }
    }

    fn atomic_write<P: AsRef<Path>>(path: P, content: &str) -> Result<(), JsonWriterError> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or(Path::new("."));
        
        // 创建锁文件路径（用于跨进程互斥）
        let lock_filename = format!("{}.lock", path.file_name().unwrap_or_default().to_string_lossy());
        let lock_path = parent.join(&lock_filename);
        
        // 确保锁文件目录存在
        if let Some(lock_parent) = lock_path.parent() {
            fs::create_dir_all(lock_parent)?;
        }
        
        // 获取跨进程互斥锁
        let mut lock_file = LockFile::open(&lock_path)
            .map_err(|e| JsonWriterError::LockError(format!("无法创建锁文件: {}", e)))?;
        
        lock_file
            .lock()
            .map_err(|e| JsonWriterError::LockError(format!("获取跨进程锁失败: {}", e)))?;
        
        // 在同一目录创建临时文件
        let temp_filename = format!(".tmp_{}.json", Uuid::new_v4());
        let temp_path = parent.join(&temp_filename);
        
        let result = (|| {
            // 使用排他锁写入临时文件
            {
                let mut temp_file = File::create(&temp_path)?;
                temp_file
                    .try_lock_exclusive()
                    .map_err(|e| JsonWriterError::LockError(e.to_string()))?;
                
                temp_file.write_all(content.as_bytes())?;
                temp_file.sync_all()?;
                
                temp_file
                    .unlock()
                    .map_err(|e| JsonWriterError::LockError(e.to_string()))?;
            }
            
            // 原子重命名
            fs::rename(&temp_path, path).map_err(|e| {
                // 失败时清理临时文件
                let _ = fs::remove_file(&temp_path);
                JsonWriterError::AtomicWriteError(e.to_string())
            })?;
            
            Ok(())
        })();
        
        // 释放跨进程锁
        let _ = lock_file.unlock();
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{ColumnInfo, JsonValue, ResultSet};
    use tempfile::tempdir;

    fn create_test_results() -> QueryResults {
        QueryResults {
            result_sets: vec![ResultSet {
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        column_type: "INT".to_string(),
                        is_enum: false,
                        is_set: false,
                        is_binary: false,
                        is_json: false,
                        is_geometry: false,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        column_type: "VARCHAR".to_string(),
                        is_enum: false,
                        is_set: false,
                        is_binary: false,
                        is_json: false,
                        is_geometry: false,
                    },
                ],
                rows: vec![vec![
                    JsonValue::Int(1),
                    JsonValue::String("test".to_string()),
                ]],
                affected_rows: 0,
                truncated: false,
                total_rows: 1,
            }],
            mysql_version: Some("8.0.35".to_string()),
        }
    }

    #[test]
    fn test_write_results() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("test.json");

        let results = create_test_results();

        JsonWriter::write_results(&json_path, &results, "test.sql", 100).unwrap();

        assert!(json_path.exists());

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.metadata.source_file, "test.sql");
        assert_eq!(output.result_sets.len(), 1);
        assert_eq!(output.result_sets[0].row_count, 1);
    }

    #[test]
    fn test_atomic_write_creates_valid_json() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("atomic_test.json");

        let content = r#"{"test": "value"}"#;
        JsonWriter::atomic_write(&json_path, content).unwrap();

        let read_content = fs::read_to_string(&json_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_write_results_creates_parent_directories() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("nested").join("dir").join("test.json");

        let results = create_test_results();

        JsonWriter::write_results(&json_path, &results, "test.sql", 50).unwrap();

        assert!(json_path.exists());
    }

    #[test]
    fn test_write_results_with_empty_result_set() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("empty.json");

        let results = QueryResults {
            result_sets: vec![ResultSet {
                columns: vec![ColumnInfo {
                    name: "id".to_string(),
                    column_type: "INT".to_string(),
                    is_enum: false,
                    is_set: false,
                    is_binary: false,
                    is_json: false,
                    is_geometry: false,
                }],
                rows: vec![],
                affected_rows: 0,
                truncated: false,
                total_rows: 0,
            }],
            mysql_version: Some("8.0.35".to_string()),
        };

        JsonWriter::write_results(&json_path, &results, "empty.sql", 10).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.result_sets[0].row_count, 0);
        assert!(output.result_sets[0].rows.is_empty());
    }

    #[test]
    fn test_write_results_with_multiple_result_sets() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("multi.json");

        let results = QueryResults {
            result_sets: vec![
                ResultSet {
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
                },
                ResultSet {
                    columns: vec![ColumnInfo {
                        name: "name".to_string(),
                        column_type: "VARCHAR".to_string(),
                        is_enum: false,
                        is_set: false,
                        is_binary: false,
                        is_json: false,
                        is_geometry: false,
                    }],
                    rows: vec![vec![JsonValue::String("test".to_string())]],
                    affected_rows: 0,
                    truncated: false,
                    total_rows: 1,
                },
            ],
            mysql_version: Some("8.0.35".to_string()),
        };

        JsonWriter::write_results(&json_path, &results, "multi.sql", 20).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.metadata.total_result_sets, 2);
        assert_eq!(output.result_sets.len(), 2);
        assert_eq!(output.result_sets[0].index, 0);
        assert_eq!(output.result_sets[1].index, 1);
    }

    #[test]
    fn test_write_results_with_all_data_types() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("types.json");

        let results = QueryResults {
            result_sets: vec![ResultSet {
                columns: vec![
                    ColumnInfo { name: "null_val".to_string(), column_type: "NULL".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "bool_val".to_string(), column_type: "TINYINT".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "int_val".to_string(), column_type: "INT".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "uint_val".to_string(), column_type: "INT UNSIGNED".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "float_val".to_string(), column_type: "DOUBLE".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "string_val".to_string(), column_type: "VARCHAR".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "date_val".to_string(), column_type: "DATE".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "time_val".to_string(), column_type: "TIME".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "datetime_val".to_string(), column_type: "DATETIME".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                    ColumnInfo { name: "bytes_val".to_string(), column_type: "BLOB".to_string(), is_enum: false, is_set: false, is_binary: true, is_json: false, is_geometry: false },
                ],
                rows: vec![vec![
                    JsonValue::Null,
                    JsonValue::Bool(true),
                    JsonValue::Int(-42),
                    JsonValue::UInt(42),
                    JsonValue::Float(3.14159),
                    JsonValue::String("hello".to_string()),
                    JsonValue::Date("2024-01-15".to_string()),
                    JsonValue::Time("14:30:00.000000".to_string()),
                    JsonValue::DateTime("2024-01-15 14:30:00.000000".to_string()),
                    JsonValue::Bytes(vec![0x48, 0x65, 0x6c, 0x6c, 0x6f]),
                ]],
                affected_rows: 0,
                truncated: false,
                total_rows: 1,
            }],
            mysql_version: Some("8.0.35".to_string()),
        };

        JsonWriter::write_results(&json_path, &results, "types.sql", 30).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        let row = &output.result_sets[0].rows[0];
        assert!(row.get("null_val").unwrap().is_null());
        assert_eq!(row.get("bool_val").unwrap().as_bool(), Some(true));
        assert_eq!(row.get("int_val").unwrap().as_i64(), Some(-42));
        assert_eq!(row.get("uint_val").unwrap().as_u64(), Some(42));
        assert!(row.get("float_val").unwrap().as_f64().is_some());
        assert_eq!(row.get("string_val").unwrap().as_str(), Some("hello"));
        assert_eq!(row.get("date_val").unwrap().as_str(), Some("2024-01-15"));
    }

    #[test]
    fn test_write_results_with_truncated_result_set() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("truncated.json");

        let results = QueryResults {
            result_sets: vec![ResultSet {
                columns: vec![ColumnInfo {
                    name: "id".to_string(),
                    column_type: "INT".to_string(),
                    is_enum: false,
                    is_set: false,
                    is_binary: false,
                    is_json: false,
                    is_geometry: false,
                }],
                rows: vec![vec![JsonValue::Int(1)], vec![JsonValue::Int(2)]],
                affected_rows: 0,
                truncated: true,
                total_rows: 10000,
            }],
            mysql_version: Some("8.0.35".to_string()),
        };

        JsonWriter::write_results(&json_path, &results, "truncated.sql", 100).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        assert!(output.result_sets[0].truncated);
        assert_eq!(output.result_sets[0].total_rows, Some(10000));
        assert_eq!(output.result_sets[0].row_count, 2);
    }

    #[test]
    fn test_metadata_fields() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("metadata.json");

        let results = create_test_results();

        JsonWriter::write_results(&json_path, &results, "test/query.sql", 150).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.metadata.source_file, "test/query.sql");
        assert_eq!(output.metadata.execution_duration_ms, 150);
        assert_eq!(output.metadata.mysql_version, Some("8.0.35".to_string()));
        assert_eq!(output.metadata.total_result_sets, 1);
    }

    #[test]
    fn test_atomic_write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("overwrite.json");

        // 写入初始内容
        fs::write(&json_path, "old content").unwrap();

        // 原子写入应该覆盖
        JsonWriter::atomic_write(&json_path, "new content").unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_json_output_is_valid_json() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("valid.json");

        let results = create_test_results();

        JsonWriter::write_results(&json_path, &results, "test.sql", 100).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();

        // 通过解析验证是有效的 JSON
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.is_object());
        assert!(parsed.get("metadata").is_some());
        assert!(parsed.get("result_sets").is_some());
    }

    #[test]
    fn test_convert_to_json_output() {
        let results = create_test_results();
        let output = JsonWriter::convert_to_json_output(&results, "test.sql", 100);

        assert_eq!(output.metadata.source_file, "test.sql");
        assert_eq!(output.metadata.execution_duration_ms, 100);
        assert_eq!(output.result_sets.len(), 1);
    }

    #[test]
    fn test_convert_result_set() {
        let rs = ResultSet {
            columns: vec![
                ColumnInfo { name: "col1".to_string(), column_type: "INT".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
                ColumnInfo { name: "col2".to_string(), column_type: "VARCHAR".to_string(), is_enum: false, is_set: false, is_binary: false, is_json: false, is_geometry: false },
            ],
            rows: vec![
                vec![JsonValue::Int(1), JsonValue::String("a".to_string())],
                vec![JsonValue::Int(2), JsonValue::String("b".to_string())],
            ],
            affected_rows: 5,
            truncated: false,
            total_rows: 2,
        };

        let json_rs = JsonWriter::convert_result_set(0, &rs);

        assert_eq!(json_rs.index, 0);
        assert_eq!(json_rs.columns.len(), 2);
        assert_eq!(json_rs.row_count, 2);
        assert_eq!(json_rs.affected_rows, 5);
        assert!(!json_rs.truncated);
    }

    #[test]
    fn test_metadata_total_affected_rows() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("affected.json");

        let results = QueryResults {
            result_sets: vec![
                ResultSet {
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
                    affected_rows: 10,
                    truncated: false,
                    total_rows: 1,
                },
                ResultSet {
                    columns: vec![ColumnInfo {
                        name: "id".to_string(),
                        column_type: "INT".to_string(),
                        is_enum: false,
                        is_set: false,
                        is_binary: false,
                        is_json: false,
                        is_geometry: false,
                    }],
                    rows: vec![vec![JsonValue::Int(2)], vec![JsonValue::Int(3)]],
                    affected_rows: 20,
                    truncated: false,
                    total_rows: 2,
                },
            ],
            mysql_version: Some("8.0.35".to_string()),
        };

        JsonWriter::write_results(&json_path, &results, "affected.sql", 50).unwrap();

        let content = fs::read_to_string(&json_path).unwrap();
        let output: JsonOutput = serde_json::from_str(&content).unwrap();

        // 验证总受影响行数聚合
        assert_eq!(output.metadata.total_affected_rows, 30); // 10 + 20
        assert_eq!(output.metadata.total_rows, 3); // 1 + 2
    }
}
