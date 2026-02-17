//! JSON output writer module.
//!
//! Handles atomic writing of query results to JSON files with file locking
//! to ensure data integrity during concurrent operations.

use crate::database::{QueryResults, ResultSet};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// JSON writing errors.
#[derive(Error, Debug)]
pub enum JsonWriterError {
    /// File I/O error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    /// JSON serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    /// Failed to acquire file lock
    #[error("Failed to acquire file lock: {0}")]
    LockError(String),
    /// Atomic write operation failed
    #[error("Atomic write failed: {0}")]
    AtomicWriteError(String),
}

/// Execution metadata included in JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMetadata {
    /// Timestamp when the query was executed
    pub execution_time: DateTime<Utc>,
    /// MySQL server version
    pub mysql_version: Option<String>,
    /// Number of result sets returned
    pub total_result_sets: usize,
    /// Source SQL file path
    pub source_file: String,
    /// Query execution duration in milliseconds
    pub execution_duration_ms: u64,
}

/// Single result set in JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonResultSet {
    /// Result set index (0-based)
    pub index: usize,
    /// Column definitions
    pub columns: Vec<JsonColumnInfo>,
    /// Row data as key-value maps
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    /// Number of rows in this result set
    pub row_count: usize,
    /// Number of rows affected by the query
    pub affected_rows: u64,
    /// Whether the result set was truncated due to size limits
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Total rows before truncation (only present if truncated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<usize>,
}

/// Column metadata in JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonColumnInfo {
    /// Column name
    pub name: String,
    /// MySQL data type
    pub data_type: String,
}

/// Complete JSON output structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutput {
    /// Execution metadata
    pub metadata: JsonMetadata,
    /// Query result sets
    pub result_sets: Vec<JsonResultSet>,
}

/// Handles writing query results to JSON files.
pub struct JsonWriter;

impl JsonWriter {
    /// Write query results to a JSON file atomically.
    ///
    /// Uses a temporary file and rename to ensure atomic writes.
    /// The JSON file will not be corrupted even if the process is interrupted.
    ///
    /// # Arguments
    /// * `path` - Output JSON file path
    /// * `results` - Query execution results
    /// * `source_file` - Source SQL file path for metadata
    /// * `execution_duration_ms` - Query execution time in milliseconds
    pub fn write_results<P: AsRef<Path>>(
        path: P,
        results: &QueryResults,
        source_file: &str,
        execution_duration_ms: u64,
    ) -> Result<(), JsonWriterError> {
        let path = path.as_ref();
        
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Convert results to JSON structure
        let output = Self::convert_to_json_output(results, source_file, execution_duration_ms);
        
        // Serialize to JSON
        let json_content = serde_json::to_string_pretty(&output)?;
        
        // Atomic write: write to temp file first, then rename
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

        JsonOutput {
            metadata: JsonMetadata {
                execution_time: Utc::now(),
                mysql_version: results.mysql_version.clone(),
                total_result_sets: result_sets.len(),
                source_file: source_file.to_string(),
                execution_duration_ms,
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
        
        // Create a temporary file in the same directory
        let temp_filename = format!(".tmp_{}.json", Uuid::new_v4());
        let temp_path = parent.join(&temp_filename);
        
        // Write to temporary file with exclusive lock
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
        
        // Atomic rename
        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temp file on failure
            let _ = fs::remove_file(&temp_path);
            JsonWriterError::AtomicWriteError(e.to_string())
        })?;
        
        Ok(())
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
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        column_type: "VARCHAR".to_string(),
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
                    ColumnInfo {
                        name: "null_val".to_string(),
                        column_type: "NULL".to_string(),
                    },
                    ColumnInfo {
                        name: "bool_val".to_string(),
                        column_type: "TINYINT".to_string(),
                    },
                    ColumnInfo {
                        name: "int_val".to_string(),
                        column_type: "INT".to_string(),
                    },
                    ColumnInfo {
                        name: "uint_val".to_string(),
                        column_type: "INT UNSIGNED".to_string(),
                    },
                    ColumnInfo {
                        name: "float_val".to_string(),
                        column_type: "DOUBLE".to_string(),
                    },
                    ColumnInfo {
                        name: "string_val".to_string(),
                        column_type: "VARCHAR".to_string(),
                    },
                    ColumnInfo {
                        name: "date_val".to_string(),
                        column_type: "DATE".to_string(),
                    },
                    ColumnInfo {
                        name: "time_val".to_string(),
                        column_type: "TIME".to_string(),
                    },
                    ColumnInfo {
                        name: "datetime_val".to_string(),
                        column_type: "DATETIME".to_string(),
                    },
                    ColumnInfo {
                        name: "bytes_val".to_string(),
                        column_type: "BLOB".to_string(),
                    },
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

        // Write initial content
        fs::write(&json_path, "old content").unwrap();

        // Atomic write should overwrite
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

        // Verify it's valid JSON by parsing it
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
                ColumnInfo {
                    name: "col1".to_string(),
                    column_type: "INT".to_string(),
                },
                ColumnInfo {
                    name: "col2".to_string(),
                    column_type: "VARCHAR".to_string(),
                },
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
}
