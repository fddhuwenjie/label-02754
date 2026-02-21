//! Integration tests for MySQL Executor
//!
//! These tests require a running MySQL instance.
//! Set the following environment variables to run:
//! - MYSQL_TEST_HOST (default: localhost)
//! - MYSQL_TEST_PORT (default: 3306)
//! - MYSQL_TEST_USER (default: root)
//! - MYSQL_TEST_PASSWORD (default: password)
//! - MYSQL_TEST_DATABASE (default: test)
//!
//! Run with: cargo test --test integration_tests -- --ignored

use mysql_executor::config::{Config, DatabaseConfig, ExecutionConfig};
use mysql_executor::database::DatabasePool;
use mysql_executor::executor::Executor;
use mysql_executor::json_writer::JsonWriter;
use mysql_executor::scanner::Scanner;
use std::collections::HashMap;
use std::env;
use std::fs;
use tempfile::tempdir;

fn get_test_db_config() -> DatabaseConfig {
    DatabaseConfig {
        host: env::var("MYSQL_TEST_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: env::var("MYSQL_TEST_PORT")
            .unwrap_or_else(|_| "3306".to_string())
            .parse()
            .unwrap_or(3306),
        username: env::var("MYSQL_TEST_USER").unwrap_or_else(|_| "root".to_string()),
        password: env::var("MYSQL_TEST_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        database: env::var("MYSQL_TEST_DATABASE").unwrap_or_else(|_| "test".to_string()),
        pool_size: 5,
        timeout_seconds: 30,
    }
}

fn create_test_config(scan_dir: &str) -> Config {
    Config {
        database: get_test_db_config(),
        execution: ExecutionConfig {
            scan_directory: scan_dir.to_string(),
            max_concurrent_files: 2,
            file_intervals: HashMap::new(),
        },
    }
}

/// Test database connection
#[test]
#[ignore]
fn test_database_connection() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config);
    assert!(pool.is_ok(), "Failed to create database pool");

    let pool = pool.unwrap();
    let conn = pool.get_connection();
    assert!(conn.is_ok(), "Failed to get connection from pool");
}

/// Test simple SELECT query execution
#[test]
#[ignore]
fn test_simple_select() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let results = pool.execute_sql("SELECT 1 as value, 'hello' as message");
    assert!(results.is_ok(), "Query execution failed");

    let results = results.unwrap();
    assert_eq!(results.result_sets.len(), 1);
    assert_eq!(results.result_sets[0].columns.len(), 2);
    assert_eq!(results.result_sets[0].rows.len(), 1);
    assert!(results.mysql_version.is_some());
}

/// Test multiple result sets
#[test]
#[ignore]
fn test_multiple_result_sets() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let sql = "SELECT 1 as a; SELECT 2 as b; SELECT 3 as c;";
    let results = pool.execute_sql(sql);
    assert!(results.is_ok(), "Query execution failed");

    let results = results.unwrap();
    assert_eq!(results.result_sets.len(), 3);
}

/// Test CTE (Common Table Expression) support
#[test]
#[ignore]
fn test_cte_support() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let sql = r#"
        WITH numbers AS (
            SELECT 1 as n
            UNION ALL
            SELECT 2
            UNION ALL
            SELECT 3
        )
        SELECT * FROM numbers ORDER BY n;
    "#;

    let results = pool.execute_sql(sql);
    assert!(results.is_ok(), "CTE query failed");

    let results = results.unwrap();
    assert_eq!(results.result_sets.len(), 1);
    assert_eq!(results.result_sets[0].rows.len(), 3);
}

/// Test SQL comments handling
#[test]
#[ignore]
fn test_sql_comments() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let sql = r#"
        -- This is a single line comment
        SELECT 1 as value
        /* This is a
           multi-line comment */
        ;
    "#;

    let results = pool.execute_sql(sql);
    assert!(results.is_ok(), "Query with comments failed");
}

/// Test various data types
#[test]
#[ignore]
fn test_data_types() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let sql = r#"
        SELECT 
            CAST(123 AS SIGNED) as int_val,
            CAST(3.14159 AS DECIMAL(10,5)) as decimal_val,
            'hello' as string_val,
            CURDATE() as date_val,
            NOW() as datetime_val,
            NULL as null_val,
            TRUE as bool_val
    "#;

    let results = pool.execute_sql(sql);
    assert!(results.is_ok(), "Data types query failed");

    let results = results.unwrap();
    assert_eq!(results.result_sets[0].columns.len(), 7);
}

/// Test error handling for invalid SQL
#[test]
#[ignore]
fn test_invalid_sql() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    let results = pool.execute_sql("INVALID SQL SYNTAX HERE");
    assert!(results.is_err(), "Invalid SQL should return error");
}

/// Test end-to-end execution with file scanning
#[test]
#[ignore]
fn test_end_to_end_execution() {
    let dir = tempdir().expect("Failed to create temp dir");
    let sql_dir = dir.path().join("sql");
    fs::create_dir_all(&sql_dir).expect("Failed to create sql dir");

    // Create test SQL file
    let sql_content = "SELECT 1 as test_value, 'integration_test' as test_name;";
    fs::write(sql_dir.join("test.sql"), sql_content).expect("Failed to write SQL file");

    let config = create_test_config(sql_dir.to_str().unwrap());
    let executor = Executor::new(config);

    if let Ok(executor) = executor {
        let results = executor.run();
        assert!(results.is_ok(), "Executor run failed");

        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].json_updated);

        // Verify JSON file was created
        let json_path = sql_dir.join("test.json");
        assert!(json_path.exists(), "JSON file was not created");

        // Verify JSON content
        let json_content = fs::read_to_string(&json_path).expect("Failed to read JSON");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_content).expect("Invalid JSON");
        assert!(parsed.get("metadata").is_some());
        assert!(parsed.get("result_sets").is_some());
    }
}

/// Test file interval skipping
#[test]
#[ignore]
fn test_file_interval_skipping() {
    let dir = tempdir().expect("Failed to create temp dir");
    let sql_dir = dir.path().join("sql");
    fs::create_dir_all(&sql_dir).expect("Failed to create sql dir");

    // Create test SQL file
    fs::write(sql_dir.join("test.sql"), "SELECT 1;").expect("Failed to write SQL file");

    // Create existing JSON file
    let json_content = r#"{"metadata":{},"result_sets":[]}"#;
    fs::write(sql_dir.join("test.json"), json_content).expect("Failed to write JSON file");

    // Configure with long interval
    let mut intervals = HashMap::new();
    intervals.insert("test.sql".to_string(), 9999u64); // Very long interval

    let config = Config {
        database: get_test_db_config(),
        execution: ExecutionConfig {
            scan_directory: sql_dir.to_str().unwrap().to_string(),
            max_concurrent_files: 2,
            file_intervals: intervals,
        },
    };

    if let Ok(executor) = Executor::new(config) {
        let results = executor.run();
        if let Ok(results) = results {
            // File should be skipped due to interval
            assert_eq!(results.len(), 1);
            assert!(results[0].success);
            assert!(!results[0].json_updated); // Should NOT be updated
        }
    }
}

/// Test concurrent file processing
#[test]
#[ignore]
fn test_concurrent_processing() {
    let dir = tempdir().expect("Failed to create temp dir");
    let sql_dir = dir.path().join("sql");
    fs::create_dir_all(&sql_dir).expect("Failed to create sql dir");

    // Create multiple SQL files
    for i in 1..=5 {
        let sql_content = format!("SELECT {} as value;", i);
        fs::write(sql_dir.join(format!("test{}.sql", i)), sql_content)
            .expect("Failed to write SQL file");
    }

    let config = create_test_config(sql_dir.to_str().unwrap());

    if let Ok(executor) = Executor::new(config) {
        let results = executor.run();
        assert!(results.is_ok(), "Concurrent execution failed");

        let results = results.unwrap();
        assert_eq!(results.len(), 5);

        let success_count = results.iter().filter(|r| r.success).count();
        assert_eq!(success_count, 5, "Not all files processed successfully");
    }
}

/// Test scanner finds SQL files in subdirectories
#[test]
fn test_scanner_subdirectories() {
    let dir = tempdir().expect("Failed to create temp dir");
    let sql_dir = dir.path().join("sql");
    let sub_dir = sql_dir.join("subdir");
    let nested_dir = sub_dir.join("nested");

    fs::create_dir_all(&nested_dir).expect("Failed to create dirs");

    fs::write(sql_dir.join("root.sql"), "SELECT 1").unwrap();
    fs::write(sub_dir.join("sub.sql"), "SELECT 2").unwrap();
    fs::write(nested_dir.join("nested.sql"), "SELECT 3").unwrap();

    let scanner = Scanner::new(&sql_dir).expect("Failed to create scanner");
    let files = scanner.scan().expect("Failed to scan");

    assert_eq!(files.len(), 3);
}

/// Test JSON writer atomic write safety
#[test]
fn test_json_writer_atomic_safety() {
    use mysql_executor::database::{ColumnInfo, JsonValue, QueryResults, ResultSet};

    let dir = tempdir().expect("Failed to create temp dir");
    let json_path = dir.path().join("atomic_test.json");

    // Write initial content
    fs::write(&json_path, "initial content").unwrap();

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
            rows: vec![vec![JsonValue::Int(42)]],
            affected_rows: 0,
            truncated: false,
            total_rows: 1,
        }],
        mysql_version: Some("8.0.35".to_string()),
    };

    JsonWriter::write_results(&json_path, &results, "test.sql", 100).expect("Write failed");

    // Verify content was replaced
    let content = fs::read_to_string(&json_path).unwrap();
    assert!(content.contains("result_sets"));
    assert!(!content.contains("initial content"));
}

/// Test error preservation - JSON should not be modified on SQL error
#[test]
#[ignore]
fn test_error_preserves_json() {
    let dir = tempdir().expect("Failed to create temp dir");
    let sql_dir = dir.path().join("sql");
    fs::create_dir_all(&sql_dir).expect("Failed to create sql dir");

    // Create SQL file with invalid syntax
    fs::write(sql_dir.join("invalid.sql"), "INVALID SQL SYNTAX").unwrap();

    // Create existing JSON file
    let original_json = r#"{"original": "content"}"#;
    fs::write(sql_dir.join("invalid.json"), original_json).unwrap();

    let config = create_test_config(sql_dir.to_str().unwrap());

    if let Ok(executor) = Executor::new(config) {
        let _ = executor.run();

        // JSON should be unchanged
        let json_content = fs::read_to_string(sql_dir.join("invalid.json")).unwrap();
        assert_eq!(json_content, original_json);
    }
}

/// Test stored procedure execution
#[test]
#[ignore]
fn test_stored_procedure() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    // Create a simple stored procedure
    let create_proc = r#"
        DROP PROCEDURE IF EXISTS test_proc;
    "#;
    let _ = pool.execute_sql(create_proc);

    let create_proc = r#"
        CREATE PROCEDURE test_proc(IN x INT)
        BEGIN
            SELECT x * 2 as doubled;
            SELECT x * 3 as tripled;
        END
    "#;
    let result = pool.execute_sql(create_proc);
    assert!(result.is_ok(), "Failed to create stored procedure");

    // Call the stored procedure
    let call_result = pool.execute_sql("CALL test_proc(5)");
    assert!(call_result.is_ok(), "Failed to call stored procedure");

    let results = call_result.unwrap();
    // Stored procedure returns multiple result sets
    assert!(results.result_sets.len() >= 2, "Expected at least 2 result sets from stored procedure");

    // Cleanup
    let _ = pool.execute_sql("DROP PROCEDURE IF EXISTS test_proc");
}

/// Test stored function execution
#[test]
#[ignore]
fn test_stored_function() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    // Create a simple function
    let create_func = r#"
        DROP FUNCTION IF EXISTS test_func;
    "#;
    let _ = pool.execute_sql(create_func);

    let create_func = r#"
        CREATE FUNCTION test_func(x INT) RETURNS INT
        DETERMINISTIC
        BEGIN
            RETURN x * 2;
        END
    "#;
    let result = pool.execute_sql(create_func);
    assert!(result.is_ok(), "Failed to create function");

    // Use the function
    let call_result = pool.execute_sql("SELECT test_func(5) as result");
    assert!(call_result.is_ok(), "Failed to call function");

    let results = call_result.unwrap();
    assert_eq!(results.result_sets.len(), 1);
    assert_eq!(results.result_sets[0].rows.len(), 1);

    // Cleanup
    let _ = pool.execute_sql("DROP FUNCTION IF EXISTS test_func");
}

/// Test access denied error handling
#[test]
#[ignore]
fn test_access_denied_error() {
    // Try to connect with invalid credentials
    let config = DatabaseConfig {
        host: env::var("MYSQL_TEST_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: env::var("MYSQL_TEST_PORT")
            .unwrap_or_else(|_| "3306".to_string())
            .parse()
            .unwrap_or(3306),
        username: "nonexistent_user".to_string(),
        password: "wrong_password".to_string(),
        database: "test".to_string(),
        pool_size: 1,
        timeout_seconds: 5,
    };

    let result = DatabasePool::new(&config);
    assert!(result.is_err(), "Should fail with invalid credentials");

    if let Err(e) = result {
        let err_str = e.to_string().to_lowercase();
        assert!(
            err_str.contains("access denied") || err_str.contains("connection"),
            "Expected access denied or connection error, got: {}",
            e
        );
    }
}

/// Test permission error on restricted operation
#[test]
#[ignore]
fn test_permission_error_on_restricted_table() {
    let config = get_test_db_config();
    let pool = DatabasePool::new(&config).expect("Failed to create pool");

    // Try to access mysql system table (usually restricted)
    let result = pool.execute_sql("SELECT * FROM mysql.user LIMIT 1");
    
    // This may succeed or fail depending on user privileges
    // We just verify it doesn't panic and returns a proper result or error
    match result {
        Ok(_) => {
            // User has privileges, that's fine
        }
        Err(e) => {
            // Should be a proper error, not a panic
            let err_str = e.to_string();
            assert!(!err_str.is_empty(), "Error message should not be empty");
        }
    }
}
