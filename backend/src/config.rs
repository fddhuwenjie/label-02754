//! 配置管理模块
//!
//! 处理 TOML 配置文件的加载和解析，包括数据库连接设置和执行参数。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// 配置加载错误
#[derive(Error, Debug)]
pub enum ConfigError {
    /// 从磁盘读取配置文件失败
    #[error("读取配置文件失败: {0}")]
    ReadError(#[from] std::io::Error),
    /// 配置文件中存在无效的 TOML 语法
    #[error("解析配置文件失败: {0}")]
    ParseError(#[from] toml::de::Error),
    /// 指定路径的配置文件不存在
    #[error("配置文件未找到: {0}")]
    NotFound(String),
}

/// 数据库连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// MySQL 服务器主机名或 IP 地址
    pub host: String,
    /// MySQL 服务器端口（默认：3306）
    pub port: u16,
    /// 数据库用户名
    pub username: String,
    /// 数据库密码
    pub password: String,
    /// 要连接的数据库名称
    pub database: String,
    /// 连接池大小（默认：10）
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// 查询超时时间（秒）（默认：300）
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_pool_size() -> u32 {
    10
}

fn default_timeout() -> u64 {
    300
}

/// SQL 文件执行配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// 扫描 SQL 文件的目录
    pub scan_directory: String,
    /// 最大并发处理文件数（默认：4）
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_files: usize,
    /// 按文件配置的执行间隔（分钟）（文件路径 -> 间隔）
    #[serde(default)]
    pub file_intervals: HashMap<String, u64>,
}

fn default_max_concurrent() -> usize {
    4
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 数据库连接设置
    pub database: DatabaseConfig,
    /// 执行行为设置
    pub execution: ExecutionConfig,
}

impl Config {
    /// 从 TOML 文件加载配置
    ///
    /// # 参数
    /// * `path` - 配置文件路径
    ///
    /// # 返回值
    /// * `Ok(Config)` - 成功加载的配置
    /// * `Err(ConfigError)` - 加载或解析配置失败
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(ConfigError::NotFound(path.display().to_string()));
        }
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// 获取指定文件的配置执行间隔
    ///
    /// # 参数
    /// * `relative_path` - SQL 文件的相对路径
    ///
    /// # 返回值
    /// * `Some(minutes)` - 配置的间隔（分钟）
    /// * `None` - 未配置间隔（始终执行）
    pub fn get_interval_for_file(&self, relative_path: &str) -> Option<u64> {
        self.execution.file_intervals.get(relative_path).copied()
    }

    /// 在指定路径创建默认配置文件
    ///
    /// # 参数
    /// * `path` - 配置文件将被创建的路径
    pub fn create_default_config<P: AsRef<Path>>(path: P) -> Result<(), std::io::Error> {
        let default_config = Config {
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 3306,
                username: "root".to_string(),
                password: "password".to_string(),
                database: "test".to_string(),
                pool_size: 10,
                timeout_seconds: 300,
            },
            execution: ExecutionConfig {
                scan_directory: "./sql".to_string(),
                max_concurrent_files: 4,
                file_intervals: HashMap::new(),
            },
        };
        let content = toml::to_string_pretty(&default_config)
            .expect("序列化默认配置失败");
        fs::write(path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        
        let config_content = r#"
[database]
host = "localhost"
port = 3306
username = "root"
password = "test"
database = "testdb"

[execution]
scan_directory = "./sql"
max_concurrent_files = 4

[execution.file_intervals]
"queries/daily.sql" = 1440
"queries/hourly.sql" = 60
"#;
        fs::write(&config_path, config_content).unwrap();
        
        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 3306);
        assert_eq!(config.execution.scan_directory, "./sql");
        assert_eq!(config.get_interval_for_file("queries/daily.sql"), Some(1440));
        assert_eq!(config.get_interval_for_file("queries/hourly.sql"), Some(60));
        assert_eq!(config.get_interval_for_file("unknown.sql"), None);
    }

    #[test]
    fn test_create_default_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        
        Config::create_default_config(&config_path).unwrap();
        assert!(config_path.exists());
        
        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.database.host, "localhost");
    }

    #[test]
    fn test_config_not_found() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::NotFound(_)));
    }

    #[test]
    fn test_config_parse_error() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("invalid.toml");
        
        fs::write(&config_path, "invalid toml content [[[").unwrap();
        
        let result = Config::load(&config_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test]
    fn test_default_values() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("minimal.toml");
        
        // 不包含可选字段的最小配置
        let config_content = r#"
[database]
host = "localhost"
port = 3306
username = "root"
password = "test"
database = "testdb"

[execution]
scan_directory = "./sql"
"#;
        fs::write(&config_path, config_content).unwrap();
        
        let config = Config::load(&config_path).unwrap();
        
        // 检查默认值
        assert_eq!(config.database.pool_size, 10);
        assert_eq!(config.database.timeout_seconds, 300);
        assert_eq!(config.execution.max_concurrent_files, 4);
        assert!(config.execution.file_intervals.is_empty());
    }

    #[test]
    fn test_get_interval_for_file_with_path_variations() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        
        let config_content = r#"
[database]
host = "localhost"
port = 3306
username = "root"
password = "test"
database = "testdb"

[execution]
scan_directory = "./sql"

[execution.file_intervals]
"reports/daily.sql" = 1440
"queries/hourly.sql" = 60
"simple.sql" = 30
"#;
        fs::write(&config_path, config_content).unwrap();
        
        let config = Config::load(&config_path).unwrap();
        
        assert_eq!(config.get_interval_for_file("reports/daily.sql"), Some(1440));
        assert_eq!(config.get_interval_for_file("queries/hourly.sql"), Some(60));
        assert_eq!(config.get_interval_for_file("simple.sql"), Some(30));
        assert_eq!(config.get_interval_for_file("nonexistent.sql"), None);
        assert_eq!(config.get_interval_for_file("reports/other.sql"), None);
    }

    #[test]
    fn test_database_config_fields() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        
        let config_content = r#"
[database]
host = "db.example.com"
port = 3307
username = "admin"
password = "secret123"
database = "production"
pool_size = 20
timeout_seconds = 600

[execution]
scan_directory = "/var/sql"
max_concurrent_files = 8
"#;
        fs::write(&config_path, config_content).unwrap();
        
        let config = Config::load(&config_path).unwrap();
        
        assert_eq!(config.database.host, "db.example.com");
        assert_eq!(config.database.port, 3307);
        assert_eq!(config.database.username, "admin");
        assert_eq!(config.database.password, "secret123");
        assert_eq!(config.database.database, "production");
        assert_eq!(config.database.pool_size, 20);
        assert_eq!(config.database.timeout_seconds, 600);
        assert_eq!(config.execution.scan_directory, "/var/sql");
        assert_eq!(config.execution.max_concurrent_files, 8);
    }
}
