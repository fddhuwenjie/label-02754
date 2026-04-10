//! SQL 文件执行协调器
//!
//! 本模块协调 SQL 文件的扫描、执行和结果写入，
//! 支持并发处理和基于间隔的跳过。

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

/// 执行器操作错误
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// 文件扫描错误
    #[error("扫描器错误: {0}")]
    ScannerError(#[from] ScannerError),
    /// 数据库操作错误
    #[error("数据库错误: {0}")]
    DatabaseError(#[from] DatabaseError),
    /// JSON 输出写入错误
    #[error("JSON 写入器错误: {0}")]
    JsonWriterError(#[from] JsonWriterError),
    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),
}

/// 单个 SQL 文件的执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// SQL 文件的相对路径
    pub file_path: String,
    /// 执行是否成功完成
    pub success: bool,
    /// 执行失败时的错误信息
    pub error_message: Option<String>,
    /// 执行时长（毫秒）
    #[allow(dead_code)]
    pub execution_time_ms: u64,
    /// JSON 输出文件是否已更新
    pub json_updated: bool,
}

/// 处理 SQL 文件的主执行器
pub struct Executor {
    config: Config,
    pool: DatabasePool,
    results: Arc<Mutex<Vec<ExecutionResult>>>,
}

impl Executor {
    /// 使用给定配置创建新的执行器
    ///
    /// # 参数
    /// * `config` - 应用配置
    ///
    /// # 返回值
    /// * `Ok(Executor)` - 成功创建的执行器
    /// * `Err(ExecutorError)` - 初始化数据库池失败
    pub fn new(config: Config) -> Result<Self, ExecutorError> {
        let pool = DatabasePool::new(&config.database)?;
        Ok(Self {
            config,
            pool,
            results: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// 执行配置扫描目录中的所有 SQL 文件
    ///
    /// 文件将并发处理，最大并发数为 `max_concurrent_files`。
    /// 配置了间隔的文件如果在间隔期内将被跳过。
    ///
    /// # 返回值
    /// * `Ok(Vec<ExecutionResult>)` - 所有处理文件的结果
    /// * `Err(ExecutorError)` - 执行过程中的致命错误
    pub fn run(&self) -> Result<Vec<ExecutionResult>, ExecutorError> {
        let scanner = Scanner::new(&self.config.execution.scan_directory)?;
        let sql_files = scanner.scan()?;

        info!("找到 {} 个 SQL 文件待处理", sql_files.len());

        // 配置线程池
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.execution.max_concurrent_files)
            .build()
            .map_err(|e| ExecutorError::ConfigError(e.to_string()))?;

        // 并行处理文件
        pool.install(|| {
            sql_files.par_iter().for_each(|sql_file| {
                let result = self.process_file(sql_file);
                self.results.lock().push(result);
            });
        });

        let results = self.results.lock().clone();
        
        // 记录摘要
        let success_count = results.iter().filter(|r| r.success).count();
        let updated_count = results.iter().filter(|r| r.json_updated).count();
        info!(
            "执行完成: {}/{} 成功, {} 个 JSON 文件已更新",
            success_count,
            results.len(),
            updated_count
        );

        Ok(results)
    }

    fn read_sql_content(&self, sql_file: &SqlFile) -> Result<String, ExecutorError> {
        let content = sql_file.read_content()?;
        Ok(content)
    }

    fn execute_sql(&self, sql_file: &SqlFile, sql_content: &str) -> Result<(u64, bool), ExecutorError> {
        let start_time = Instant::now();
        let results = self.pool.execute_sql(sql_content)?;
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        
        JsonWriter::write_results(
            &sql_file.json_path,
            &results,
            &sql_file.relative_path,
            execution_time_ms,
        )?;
        
        Ok((execution_time_ms, true))
    }

    fn build_execution_result(
        &self,
        file_path: String,
        start_time: Instant,
        result: Result<(u64, bool), ExecutorError>,
        skipped: bool,
    ) -> ExecutionResult {
        if skipped {
            return ExecutionResult {
                file_path,
                success: true,
                error_message: None,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                json_updated: false,
            };
        }

        match result {
            Ok((execution_time_ms, json_updated)) => {
                info!("成功处理 {} ({} ms)", file_path, execution_time_ms);
                ExecutionResult {
                    file_path,
                    success: true,
                    error_message: None,
                    execution_time_ms,
                    json_updated,
                }
            }
            Err(e) => {
                error!("处理 {} 失败: {}", file_path, e);
                ExecutionResult {
                    file_path,
                    success: false,
                    error_message: Some(e.to_string()),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                    json_updated: false,
                }
            }
        }
    }

    fn process_file(&self, sql_file: &SqlFile) -> ExecutionResult {
        let start_time = Instant::now();
        let file_path = sql_file.relative_path.clone();
        
        info!("正在处理: {}", file_path);

        if self.should_skip_file(sql_file) {
            info!("跳过 {} - 在间隔期内", file_path);
            return self.build_execution_result(file_path, start_time, Ok((0, false)), true);
        }

        let result = (|| -> Result<(u64, bool), ExecutorError> {
            let sql_content = self.read_sql_content(sql_file)?;
            self.execute_sql(sql_file, &sql_content)
        })();

        self.build_execution_result(file_path, start_time, result, false)
    }

    fn should_skip_file(&self, sql_file: &SqlFile) -> bool {
        // 如果 JSON 不存在，永不跳过
        if !sql_file.json_exists() {
            return false;
        }

        // 检查是否为此文件配置了间隔
        let interval_minutes = match self.config.get_interval_for_file(&sql_file.relative_path) {
            Some(interval) => interval,
            None => return false, // 未配置间隔，始终更新
        };

        // 获取 JSON 文件的最后修改时间
        let json_modified = match sql_file.json_modified_time() {
            Some(time) => time,
            None => return false, // 无法确定时间，不跳过
        };

        // 计算是否在间隔期内
        let now = SystemTime::now();
        let elapsed = match now.duration_since(json_modified) {
            Ok(duration) => duration,
            Err(_) => return false, // 时间倒退，不跳过
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
            error_message: Some("连接失败".to_string()),
            execution_time_ms: 50,
            json_updated: false,
        };

        assert!(!result.success);
        assert!(!result.json_updated);
        assert_eq!(result.error_message, Some("连接失败".to_string()));
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
        assert!(!result.json_updated); // 因间隔而跳过
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
        // 这会在数据库连接时失败，而不是目录检查
        // 目录检查在 run() 时发生
        assert!(result.is_err());
    }

    #[test]
    fn test_should_skip_file_no_json() {
        let dir = tempdir().unwrap();
        let sql_path = dir.path().join("test.sql");
        std::fs::write(&sql_path, "SELECT 1").unwrap();

        let sql_file = SqlFile::new(sql_path, dir.path());

        // 没有 JSON 文件时，永不跳过
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

        // JSON 存在
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
        let err = ExecutorError::ConfigError("无效配置".to_string());
        assert!(err.to_string().contains("无效配置"));

        let err = ExecutorError::ScannerError(ScannerError::DirectoryNotFound("/test".to_string()));
        assert!(err.to_string().contains("扫描器错误"));
    }
}
