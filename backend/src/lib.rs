//! MySQL Executor 库
//!
//! 一个基于 Rust 的应用程序，用于扫描和执行 MySQL 文件（.sql），
//! 并生成包含完整元数据的 JSON 结果文件。
//!
//! # 功能特性
//!
//! - 扫描并执行配置目录及子目录中的所有 .sql 文件
//! - 生成同名的 JSON 结果文件（如：query.sql → query.json）
//! - 完整支持 SQL 注释（-- 和 /* */）
//! - WITH 语句和 CTE（公共表表达式）
//! - 多结果集
//! - 存储过程和函数
//! - 可配置的按文件执行间隔
//! - 带文件锁的原子性 JSON 写入
//! - 连接池
//! - 大结果集的流式处理支持
//!
//! # 示例
//!
//! ```no_run
//! use mysql_executor::config::Config;
//! use mysql_executor::executor::Executor;
//!
//! let config = Config::load("config.toml").expect("加载配置失败");
//! let executor = Executor::new(config).expect("创建执行器失败");
//! let results = executor.run().expect("执行失败");
//!
//! for result in results {
//!     if result.success {
//!         println!("已处理: {}", result.file_path);
//!     } else {
//!         eprintln!("失败: {} - {:?}", result.file_path, result.error_message);
//!     }
//! }
//! ```

pub mod config;
pub mod database;
pub mod executor;
pub mod json_writer;
pub mod scanner;
