//! Configuration management module.
//!
//! Handles loading and parsing of TOML configuration files for database
//! connection settings and execution parameters.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Configuration loading errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to read configuration file from disk
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    /// Invalid TOML syntax in configuration file
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
    /// Configuration file does not exist at specified path
    #[error("Config file not found at: {0}")]
    NotFound(String),
}

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// MySQL server hostname or IP address
    pub host: String,
    /// MySQL server port (default: 3306)
    pub port: u16,
    /// Database username
    pub username: String,
    /// Database password
    pub password: String,
    /// Database name to connect to
    pub database: String,
    /// Connection pool size (default: 10)
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// Query timeout in seconds (default: 300)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_pool_size() -> u32 {
    10
}

fn default_timeout() -> u64 {
    300
}

/// SQL file execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Directory to scan for SQL files
    pub scan_directory: String,
    /// Maximum number of files to process concurrently (default: 4)
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_files: usize,
    /// Per-file execution intervals in minutes (file path -> interval)
    #[serde(default)]
    pub file_intervals: HashMap<String, u64>,
}

fn default_max_concurrent() -> usize {
    4
}

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database connection settings
    pub database: DatabaseConfig,
    /// Execution behavior settings
    pub execution: ExecutionConfig,
}

impl Config {
    /// Load configuration from a TOML file.
    ///
    /// # Arguments
    /// * `path` - Path to the configuration file
    ///
    /// # Returns
    /// * `Ok(Config)` - Successfully loaded configuration
    /// * `Err(ConfigError)` - Failed to load or parse configuration
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(ConfigError::NotFound(path.display().to_string()));
        }
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Get the configured execution interval for a specific file.
    ///
    /// # Arguments
    /// * `relative_path` - Relative path of the SQL file
    ///
    /// # Returns
    /// * `Some(minutes)` - Configured interval in minutes
    /// * `None` - No interval configured (always execute)
    pub fn get_interval_for_file(&self, relative_path: &str) -> Option<u64> {
        self.execution.file_intervals.get(relative_path).copied()
    }

    /// Create a default configuration file at the specified path.
    ///
    /// # Arguments
    /// * `path` - Path where the configuration file will be created
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
            .expect("Failed to serialize default config");
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
        
        // Minimal config without optional fields
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
        
        // Check default values
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
