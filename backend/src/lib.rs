//! MySQL Executor Library
//!
//! A Rust-based application that scans and executes MySQL files (.sql),
//! generating JSON result files with comprehensive metadata.
//!
//! # Features
//!
//! - Scan and execute all .sql files in configured directory and subdirectories
//! - Generate JSON result files with same name (e.g., query.sql → query.json)
//! - Full support for SQL comments (-- and /* */)
//! - WITH statements and CTE (Common Table Expressions)
//! - Multiple result sets
//! - Stored procedures and functions
//! - Configurable execution intervals per file
//! - Atomic JSON writes with file locking
//! - Connection pooling
//! - Streaming support for large result sets
//!
//! # Example
//!
//! ```no_run
//! use mysql_executor::config::Config;
//! use mysql_executor::executor::Executor;
//!
//! let config = Config::load("config.toml").expect("Failed to load config");
//! let executor = Executor::new(config).expect("Failed to create executor");
//! let results = executor.run().expect("Execution failed");
//!
//! for result in results {
//!     if result.success {
//!         println!("Processed: {}", result.file_path);
//!     } else {
//!         eprintln!("Failed: {} - {:?}", result.file_path, result.error_message);
//!     }
//! }
//! ```

pub mod config;
pub mod database;
pub mod executor;
pub mod json_writer;
pub mod scanner;
