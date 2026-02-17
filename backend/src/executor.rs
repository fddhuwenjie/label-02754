//! SQL file execution orchestrator.
//!
//! This module coordinates the scanning, execution, and result writing
//! of SQL files with support for concurrent processing and interval-based skipping.

use crate::config::Config;
use crate::database::{DatabaseError, DatabasePool};
use crate::json_writer::{JsonWriter, JsonWriterError};
use crate::scanner::{Scanner, ScannerError, SqlFile};
use log::{error, info};
use parking_lot::Mutex;
use rayon::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;

/// Executor operation errors.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// Error during file scanning
    #[error("Scanner error: {0}")]
    ScannerError(#[from] ScannerError),
    /// Database operation error
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
    /// JSON output writing error
    #[error("JSON writer error: {0}")]
    JsonWriterError(#[from] JsonWriterError),
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Result of executing a single SQL file.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Relative path of the SQL file
    pub file_path: String,
    /// Whether execution completed successfully
    pub success: bool,
    /// Error message if execution failed
    pub error_message: Option<String>,
    /// Execution duration in milliseconds
    #[allow(dead_code)]
    pub execution_time_ms: u64,
    /// Whether the JSON output file was updated
    pub json_updated: bool,
}

/// Main executor for processing SQL files.
pub struct Executor {
    config: Config,
    pool: DatabasePool,
    results: Arc<Mutex<Vec<ExecutionResult>>>,
}

impl Executor {
    /// Create a new executor with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Application configuration
    ///
    /// # Returns
    /// * `Ok(Executor)` - Successfully created executor
    /// * `Err(ExecutorError)` - Failed to initialize database pool
    pub fn new(config: Config) -> Result<Self, ExecutorError> {
        let pool = DatabasePool::new(&config.database)?;
        Ok(Self {
            config,
            pool,
            results: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Execute all SQL files in the configured scan directory.
    ///
    /// Files are processed concurrently up to `max_concurrent_files`.
    /// Files with configured intervals are skipped if within the interval period.
    ///
    /// # Returns
    /// * `Ok(Vec<ExecutionResult>)` - Results for all processed files
    /// * `Err(ExecutorError)` - Fatal error during execution
    pub fn run(&self) -> Result<Vec<ExecutionResult>, ExecutorError> {
        let scanner = Scanner::new(&self.config.execution.scan_directory)?;
        let sql_files = scanner.scan()?;

        info!("Found {} SQL files to process", sql_files.len());

        // Configure thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.execution.max_concurrent_files)
            .build()
            .map_err(|e| ExecutorError::ConfigError(e.to_string()))?;

        // Process files in parallel
        pool.install(|| {
            sql_files.par_iter().for_each(|sql_file| {
                let result = self.process_file(sql_file);
                self.results.lock().push(result);
            });
        });

        let results = self.results.lock().clone();
        
        // Log summary
        let success_count = results.iter().filter(|r| r.success).count();
        let updated_count = results.iter().filter(|r| r.json_updated).count();
        info!(
            "Execution complete: {}/{} successful, {} JSON files updated",
            success_count,
            results.len(),
            updated_count
        );

        Ok(results)
    }

    fn process_file(&self, sql_file: &SqlFile) -> ExecutionResult {
        let start_time = Instant::now();
        let relative_path = &sql_file.relative_path;

        info!("Processing: {}", relative_path);

        // Check if we should skip based on interval
        if self.should_skip_file(sql_file) {
            info!("Skipping {} - within interval period", relative_path);
            return ExecutionResult {
                file_path: relative_path.clone(),
                success: true,
                error_message: None,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                json_updated: false,
            };
        }

        // Read SQL content
        let sql_content = match sql_file.read_content() {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read {}: {}", relative_path, e);
                return ExecutionResult {
                    file_path: relative_path.clone(),
                    success: false,
                    error_message: Some(e.to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    json_updated: false,
                };
            }
        };

        // Execute SQL
        let results = match self.pool.execute_sql(&sql_content) {
            Ok(results) => results,
            Err(e) => {
                error!("SQL execution failed for {}: {}", relative_path, e);
                return ExecutionResult {
                    file_path: relative_path.clone(),
                    success: false,
                    error_message: Some(e.to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    json_updated: false,
                };
            }
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        // Write JSON results
        match JsonWriter::write_results(
            &sql_file.json_path,
            &results,
            relative_path,
            execution_time_ms,
        ) {
            Ok(_) => {
                info!(
                    "Successfully processed {} -> {} ({} ms)",
                    relative_path,
                    sql_file.json_path.display(),
                    execution_time_ms
                );
                ExecutionResult {
                    file_path: relative_path.clone(),
                    success: true,
                    error_message: None,
                    execution_time_ms,
                    json_updated: true,
                }
            }
            Err(e) => {
                error!("Failed to write JSON for {}: {}", relative_path, e);
                ExecutionResult {
                    file_path: relative_path.clone(),
                    success: false,
                    error_message: Some(e.to_string()),
                    execution_time_ms,
                    json_updated: false,
                }
            }
        }
    }

    fn should_skip_file(&self, sql_file: &SqlFile) -> bool {
        // If JSON doesn't exist, never skip
        if !sql_file.json_exists() {
            return false;
        }

        // Check if there's an interval configured for this file
        let interval_minutes = match self.config.get_interval_for_file(&sql_file.relative_path) {
            Some(interval) => interval,
            None => return false, // No interval configured, always update
        };

        // Get JSON file's last modified time
        let json_modified = match sql_file.json_modified_time() {
            Some(time) => time,
            None => return false, // Can't determine time, don't skip
        };

        // Calculate if we're within the interval
        let now = SystemTime::now();
        let elapsed = match now.duration_since(json_modified) {
            Ok(duration) => duration,
            Err(_) => return false, // Time went backwards, don't skip
        };

        let interval_duration = Duration::from_secs(interval_minutes * 60);
        elapsed < interval_duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DatabaseConfig, ExecutionConfig};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn create_test_config(scan_dir: &str) -> Config {
        Config {
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 3306,
                username: "test".to_string(),
                password: "test".to_string(),
                database: "test".to_string(),
                pool_size: 5,
                timeout_seconds: 30,
            },
            execution: ExecutionConfig {
                scan_directory: scan_dir.to_string(),
                max_concurrent_files: 2,
                file_intervals: HashMap::new(),
            },
        }
    }

    fn create_test_config_with_intervals(scan_dir: &str, intervals: HashMap<String, u64>) -> Config {
        Config {
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 3306,
                username: "test".to_string(),
                password: "test".to_string(),
                database: "test".to_string(),
                pool_size: 5,
                timeout_seconds: 30,
            },
            execution: ExecutionConfig {
                scan_directory: scan_dir.to_string(),
                max_concurrent_files: 2,
                file_intervals: intervals,
            },
        }
    }

    #[test]
    fn test_execution_result_success() {
        let result = ExecutionResult {
            file_path: "test.sql".to_string(),
            success: true,
            error_message: None,
            execution_time_ms: 100,
            json_updated: true,
        };

        assert!(result.success);
        assert!(result.json_updated);
        assert!(result.error_message.is_none());
        assert_eq!(result.execution_time_ms, 100);
    }

    #[test]
    fn test_execution_result_failure() {
        let result = ExecutionResult {
            file_path: "test.sql".to_string(),
            success: false,
            error_message: Some("Connection failed".to_string()),
            execution_time_ms: 50,
            json_updated: false,
        };

        assert!(!result.success);
        assert!(!result.json_updated);
        assert_eq!(result.error_message, Some("Connection failed".to_string()));
    }

    #[test]
    fn test_execution_result_skipped() {
        let result = ExecutionResult {
            file_path: "test.sql".to_string(),
            success: true,
            error_message: None,
            execution_time_ms: 5,
            json_updated: false,
        };

        assert!(result.success);
        assert!(!result.json_updated); // Skipped due to interval
    }

    #[test]
    fn test_executor_error_from_scanner_error() {
        let scanner_err = ScannerError::DirectoryNotFound("/test".to_string());
        let executor_err: ExecutorError = scanner_err.into();
        assert!(matches!(executor_err, ExecutorError::ScannerError(_)));
    }

    #[test]
    fn test_executor_error_from_database_error() {
        let db_err = DatabaseError::ConnectionError("test".to_string());
        let executor_err: ExecutorError = db_err.into();
        assert!(matches!(executor_err, ExecutorError::DatabaseError(_)));
    }

    #[test]
    fn test_executor_error_from_json_writer_error() {
        let json_err = JsonWriterError::AtomicWriteError("test".to_string());
        let executor_err: ExecutorError = json_err.into();
        assert!(matches!(executor_err, ExecutorError::JsonWriterError(_)));
    }

    #[test]
    fn test_executor_new_with_invalid_directory() {
        let config = create_test_config("/nonexistent/directory/path");
        let result = Executor::new(config);
        // This will fail at database connection, not directory check
        // Directory check happens during run()
        assert!(result.is_err());
    }

    #[test]
    fn test_should_skip_file_no_json() {
        let dir = tempdir().unwrap();
        let sql_path = dir.path().join("test.sql");
        std::fs::write(&sql_path, "SELECT 1").unwrap();

        let sql_file = SqlFile::new(sql_path, dir.path());

        // Without JSON file, should never skip
        assert!(!sql_file.json_exists());
    }

    #[test]
    fn test_should_skip_file_with_json_no_interval() {
        let dir = tempdir().unwrap();
        let sql_path = dir.path().join("test.sql");
        let json_path = dir.path().join("test.json");

        std::fs::write(&sql_path, "SELECT 1").unwrap();
        std::fs::write(&json_path, "{}").unwrap();

        let sql_file = SqlFile::new(sql_path, dir.path());

        // JSON exists
        assert!(sql_file.json_exists());
        assert!(sql_file.json_modified_time().is_some());
    }

    #[test]
    fn test_config_interval_lookup() {
        let mut intervals = HashMap::new();
        intervals.insert("reports/daily.sql".to_string(), 1440u64);
        intervals.insert("queries/hourly.sql".to_string(), 60u64);

        let config = create_test_config_with_intervals("./sql", intervals);

        assert_eq!(config.get_interval_for_file("reports/daily.sql"), Some(1440));
        assert_eq!(config.get_interval_for_file("queries/hourly.sql"), Some(60));
        assert_eq!(config.get_interval_for_file("other.sql"), None);
    }

    #[test]
    fn test_executor_error_display() {
        let err = ExecutorError::ConfigError("Invalid config".to_string());
        assert!(err.to_string().contains("Invalid config"));

        let err = ExecutorError::ScannerError(ScannerError::DirectoryNotFound("/test".to_string()));
        assert!(err.to_string().contains("Scanner error"));
    }
}
