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
    // Initialize logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("MySQL Executor v{}", env!("CARGO_PKG_VERSION"));

    // Determine config file path
    let config_path = get_config_path();
    info!("Using config file: {}", config_path.display());

    // Check if config exists, create default if not
    if !config_path.exists() {
        info!("Config file not found, creating default config...");
        if let Err(e) = Config::create_default_config(&config_path) {
            error!("Failed to create default config: {}", e);
            process::exit(1);
        }
        info!(
            "Default config created at {}. Please edit it with your database settings.",
            config_path.display()
        );
        process::exit(0);
    }

    // Load configuration
    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load config: {}", e);
            process::exit(1);
        }
    };

    info!("Scan directory: {}", config.execution.scan_directory);
    info!(
        "Max concurrent files: {}",
        config.execution.max_concurrent_files
    );

    // Create and run executor
    let executor = match Executor::new(config) {
        Ok(executor) => executor,
        Err(e) => {
            error!("Failed to initialize executor: {}", e);
            process::exit(1);
        }
    };

    match executor.run() {
        Ok(results) => {
            let total = results.len();
            let success = results.iter().filter(|r| r.success).count();
            let failed = total - success;
            let updated = results.iter().filter(|r| r.json_updated).count();

            info!("=== Execution Summary ===");
            info!("Total files processed: {}", total);
            info!("Successful: {}", success);
            info!("Failed: {}", failed);
            info!("JSON files updated: {}", updated);

            if failed > 0 {
                error!("=== Failed Files ===");
                for result in results.iter().filter(|r| !r.success) {
                    error!(
                        "  {} - {}",
                        result.file_path,
                        result.error_message.as_deref().unwrap_or("Unknown error")
                    );
                }
                process::exit(1);
            }
        }
        Err(e) => {
            error!("Execution failed: {}", e);
            process::exit(1);
        }
    }

    info!("MySQL Executor completed successfully");
}

fn get_config_path() -> PathBuf {
    // Check for command line argument
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        return PathBuf::from(&args[1]);
    }

    // Check for environment variable
    if let Ok(path) = env::var("MYSQL_EXECUTOR_CONFIG") {
        return PathBuf::from(path);
    }

    // Default to config.toml in current directory
    PathBuf::from(DEFAULT_CONFIG_FILE)
}
