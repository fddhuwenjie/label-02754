mod config;
mod database;
mod executor;
mod json_writer;
mod scanner;

use crate::config::Config;
use crate::executor::Executor;
use env_logger::Env;
use log::{error, info};
use std::env;
use std::path::PathBuf;
use std::process;

const DEFAULT_CONFIG_FILE: &str = "config/config.toml";

fn main() {
    // 初始化日志记录器
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("MySQL Executor v{}", env!("CARGO_PKG_VERSION"));

    // 确定配置文件路径
    let config_path = get_config_path();
    info!("使用配置文件: {}", config_path.display());

    // 检查配置是否存在，不存在则创建默认配置
    if !config_path.exists() {
        info!("配置文件未找到，正在创建默认配置...");
        if let Err(e) = Config::create_default_config(&config_path) {
            error!("创建默认配置失败: {}", e);
            process::exit(1);
        }
        info!(
            "默认配置已创建于 {}。请编辑并填入您的数据库设置。",
            config_path.display()
        );
        process::exit(0);
    }

    // 加载配置
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(e) => {
            error!("加载配置失败: {}", e);
            process::exit(1);
        }
    };

    info!("扫描目录: {}", config.execution.scan_directory);
    info!(
        "最大并发文件数: {}",
        config.execution.max_concurrent_files
    );

    // 创建并运行执行器
    let executor = match Executor::new(config) {
        Ok(executor) => executor,
        Err(e) => {
            error!("初始化执行器失败: {}", e);
            process::exit(1);
        }
    };

    match executor.run() {
        Ok(results) => {
            let total = results.len();
            let success = results.iter().filter(|r| r.success).count();
            let failed = total - success;
            let updated = results.iter().filter(|r| r.json_updated).count();

            info!("=== 执行摘要 ===");
            info!("处理文件总数: {}", total);
            info!("成功: {}", success);
            info!("失败: {}", failed);
            info!("已更新 JSON 文件: {}", updated);

            if failed > 0 {
                error!("=== 失败文件 ===");
                for result in results.iter().filter(|r| !r.success) {
                    error!(
                        "  {} - {}",
                        result.file_path,
                        result.error_message.as_deref().unwrap_or("未知错误")
                    );
                }
                process::exit(1);
            }
        }
        Err(e) => {
            error!("执行失败: {}", e);
            process::exit(1);
        }
    }

    info!("MySQL Executor 执行完成");
}

/// 获取配置文件路径
///
/// 优先级：
/// 1. 命令行参数
/// 2. 环境变量 MYSQL_EXECUTOR_CONFIG
/// 3. 默认路径 config/config.toml
fn get_config_path() -> PathBuf {
    // 检查命令行参数
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        return PathBuf::from(&args[1]);
    }

    // 检查环境变量
    if let Ok(path) = env::var("MYSQL_EXECUTOR_CONFIG") {
        return PathBuf::from(path);
    }

    // 默认使用当前目录下的 config.toml
    PathBuf::from(DEFAULT_CONFIG_FILE)
}
