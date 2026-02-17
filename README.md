# MySQL Executor

A Rust-based application that scans and executes MySQL files (.sql), generating JSON result files with comprehensive metadata.

## How to Run

### Docker (Recommended)

1. Clone the repository and navigate to the project directory

2. Start all services:
```bash
docker-compose up --build -d
```

3. Check service status:
```bash
docker-compose ps
```

4. View logs:
```bash
docker-compose logs -f mysql-executor
```

5. Stop services:
```bash
docker-compose down
```

6. Stop and remove volumes:
```bash
docker-compose down -v
```

### Local Development

1. Install Rust (1.75+):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Install MySQL 8.0+ and create a database

3. Configure the application:
```bash
cp config/config.toml.example config/config.toml
# Edit config/config.toml with your database settings
```

4. Build and run:
```bash
cd backend
cargo build --release
./target/release/mysql-executor
```

Or run directly:
```bash
cd backend
cargo run --release
```

Environment variables:
- `MYSQL_EXECUTOR_CONFIG`: Path to config file (default: `config/config.toml`)
- `RUST_LOG`: Log level (default: `info`)

### Configuration Path Notes

| Environment | Default Config Path | Notes |
|-------------|---------------------|-------|
| Local Development | `config/config.toml` | Relative to working directory |
| Docker Container | `/app/config/config.toml` | Set via `MYSQL_EXECUTOR_CONFIG` env var |

When running locally, ensure you're in the project root directory, or set `MYSQL_EXECUTOR_CONFIG` to the absolute path of your config file.

## Services

| Service | Description | Port |
|---------|-------------|------|
| mysql | MySQL 8.0 Database | 3754 (external) / 3306 (internal) |
| mysql-executor | Rust SQL Executor Application | N/A (batch process) |

### Service Details

**MySQL Database**
- Image: mysql:8.0
- Character Set: utf8mb4
- Collation: utf8mb4_unicode_ci
- Health check enabled
- Persistent volume for data

**MySQL Executor**
- Cross-platform support (ARM64 & AMD64)
- Concurrent SQL file processing
- Atomic JSON file writes
- Connection pooling

## Test Accounts

### MySQL Database

| Username | Password | Database | Description |
|----------|----------|----------|-------------|
| root | rootpassword | - | Root administrator |
| executor | executorpass | testdb | Application user |

### Connection Examples

**MySQL CLI:**
```bash
mysql -h 127.0.0.1 -P 3754 -u executor -pexecutorpass testdb
```

**Docker exec:**
```bash
docker exec -it mysql-executor-db mysql -u executor -pexecutorpass testdb
```

## Requirements

### Functional Requirements

1. **File Processing & Execution**
   - Scan and execute all .sql files in configured directory and subdirectories
   - Generate JSON result files with same name (e.g., query.sql → query.json)
   - Maintain directory structure for output files

2. **SQL Compatibility**
   - Full support for SQL comments (-- and /* */)
   - WITH statements and CTE (Common Table Expressions)
   - Multiple result sets
   - Stored procedures and functions

3. **Configuration Management**
   - TOML configuration file
   - Database connection settings (host, port, username, password, database)
   - Scan directory path
   - Per-file generation intervals (in minutes)

4. **Execution Strategy**
   - Without interval: Update JSON on every run
   - With interval: Update only if elapsed time exceeds interval
   - Missing JSON: Always generate immediately
   - Time based on JSON file's last modified timestamp

5. **Error Handling**
   - Comprehensive error capture (connection, syntax, timeout, permissions)
   - Preserve existing JSON on errors
   - Detailed error logging

6. **Concurrency & Atomicity**
   - Thread-safe file processing
   - Atomic JSON writes (temp file + rename)
   - File locking mechanism
   - Crash-safe operations

7. **JSON Serialization**
   - Complete MySQL to JSON type mapping
   - Multiple result set support
   - Metadata (execution time, MySQL version, affected rows, etc.)

8. **Performance**
   - Connection pooling
   - Configurable concurrency limits
   - Query timeout mechanism

## Project Structure

```
.
├── backend/                    # Rust application
│   ├── src/
│   │   ├── main.rs            # Entry point
│   │   ├── config.rs          # Configuration handling
│   │   ├── database.rs        # MySQL connection & execution
│   │   ├── executor.rs        # Main execution logic
│   │   ├── json_writer.rs     # JSON output handling
│   │   └── scanner.rs         # SQL file discovery
│   ├── Cargo.toml             # Rust dependencies
│   └── Dockerfile             # Multi-arch Docker build
├── config/                     # Configuration files
│   ├── config.toml            # Active configuration
│   └── config.toml.example    # Configuration template
├── sql/                        # SQL files to execute
│   ├── example_queries/       # Example queries
│   └── reports/               # Report queries
├── init-db/                    # MySQL initialization scripts
├── docker-compose.yml          # Docker Compose configuration
├── .gitignore                  # Git ignore rules
└── README.md                   # This file
```

## Configuration

### config.toml

```toml
[database]
host = "mysql"
port = 3306
username = "executor"
password = "executorpass"
database = "testdb"
pool_size = 10
timeout_seconds = 300

[execution]
scan_directory = "/app/sql"
max_concurrent_files = 4

[execution.file_intervals]
# "reports/daily_summary.sql" = 1440  # Once per day
```

## JSON Output Format

```json
{
  "metadata": {
    "execution_time": "2024-01-15T10:30:00Z",
    "mysql_version": "8.0.35",
    "total_result_sets": 1,
    "source_file": "example_queries/simple_select.sql",
    "execution_duration_ms": 15
  },
  "result_sets": [
    {
      "index": 0,
      "columns": [
        {"name": "id", "data_type": "INT"},
        {"name": "username", "data_type": "VARCHAR"}
      ],
      "rows": [
        {"id": 1, "username": "testuser1"}
      ],
      "row_count": 1,
      "affected_rows": 0
    }
  ]
}
```

## Data Type Mapping

| MySQL Type | JSON Type |
|------------|-----------|
| INT, BIGINT | number (integer) |
| FLOAT, DOUBLE, DECIMAL | number (float) |
| VARCHAR, TEXT, CHAR | string |
| DATE | string (YYYY-MM-DD) |
| TIME | string (HH:MM:SS.ffffff) |
| DATETIME, TIMESTAMP | string (YYYY-MM-DD HH:MM:SS.ffffff) |
| BLOB, BINARY | string (base64 encoded) |
| JSON | string (JSON string) |
| ENUM, SET | string |
| NULL | null |

## API Reference

### Command Line

```bash
# Run with default config (config.toml)
mysql-executor

# Run with custom config
mysql-executor /path/to/config.toml

# Run with environment variable
MYSQL_EXECUTOR_CONFIG=/path/to/config.toml mysql-executor
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| MYSQL_EXECUTOR_CONFIG | Path to configuration file | config/config.toml |
| RUST_LOG | Log level (error, warn, info, debug, trace) | info |

### Library API

The application can also be used as a library:

```rust
use mysql_executor::config::Config;
use mysql_executor::executor::Executor;

// Load configuration
let config = Config::load("config.toml")?;

// Create and run executor
let executor = Executor::new(config)?;
let results = executor.run()?;

// Process results
for result in results {
    println!("{}: success={}, updated={}", 
        result.file_path, 
        result.success, 
        result.json_updated
    );
}
```

### Module Reference

#### `config` Module

| Type | Description |
|------|-------------|
| `Config` | Main configuration struct |
| `DatabaseConfig` | Database connection settings |
| `ExecutionConfig` | Execution behavior settings |
| `ConfigError` | Configuration error types |

Key methods:
- `Config::load(path)` - Load configuration from TOML file
- `Config::create_default_config(path)` - Create default config file
- `Config::get_interval_for_file(path)` - Get configured interval for a file

#### `database` Module

| Type | Description |
|------|-------------|
| `DatabasePool` | Connection pool manager |
| `QueryResults` | SQL execution results |
| `ResultSet` | Single result set data |
| `ColumnInfo` | Column metadata |
| `JsonValue` | MySQL to JSON value wrapper |
| `DatabaseError` | Database error types |

Key methods:
- `DatabasePool::new(config)` - Create new connection pool
- `DatabasePool::execute_sql(sql)` - Execute SQL and return results
- `DatabasePool::get_connection()` - Get pooled connection

#### `executor` Module

| Type | Description |
|------|-------------|
| `Executor` | Main execution orchestrator |
| `ExecutionResult` | Per-file execution result |
| `ExecutorError` | Executor error types |

Key methods:
- `Executor::new(config)` - Create new executor
- `Executor::run()` - Execute all SQL files

#### `scanner` Module

| Type | Description |
|------|-------------|
| `Scanner` | SQL file discovery |
| `SqlFile` | SQL file metadata |
| `ScannerError` | Scanner error types |

Key methods:
- `Scanner::new(directory)` - Create scanner for directory
- `Scanner::scan()` - Find all SQL files
- `SqlFile::read_content()` - Read SQL file content
- `SqlFile::json_exists()` - Check if JSON output exists

#### `json_writer` Module

| Type | Description |
|------|-------------|
| `JsonWriter` | JSON output handler |
| `JsonOutput` | Complete JSON structure |
| `JsonMetadata` | Execution metadata |
| `JsonResultSet` | Result set in JSON format |
| `JsonWriterError` | Writer error types |

Key methods:
- `JsonWriter::write_results(path, results, source, duration)` - Write JSON atomically

### Error Codes

| Error Type | Description |
|------------|-------------|
| `ConfigError::NotFound` | Configuration file not found |
| `ConfigError::ParseError` | Invalid TOML syntax |
| `DatabaseError::ConnectionError` | Failed to connect to MySQL |
| `DatabaseError::QueryError` | SQL execution failed |
| `DatabaseError::PoolError` | Connection pool error |
| `DatabaseError::AccessDenied` | Insufficient database privileges |
| `DatabaseError::Timeout` | Query timeout exceeded |
| `ScannerError::DirectoryNotFound` | Scan directory not found |
| `ScannerError::ReadError` | Failed to read SQL file |
| `JsonWriterError::IoError` | File I/O error |
| `JsonWriterError::LockError` | File lock acquisition failed |
| `JsonWriterError::AtomicWriteError` | Atomic rename failed |

## Testing

### Run Unit Tests

```bash
cd backend
cargo test
```

### Run with Verbose Output

```bash
cd backend
cargo test -- --nocapture
```

### Run Integration Tests

Integration tests require a running MySQL instance:

```bash
# Set environment variables
export MYSQL_TEST_HOST=localhost
export MYSQL_TEST_PORT=3306
export MYSQL_TEST_USER=root
export MYSQL_TEST_PASSWORD=password
export MYSQL_TEST_DATABASE=test

# Run integration tests
cd backend
cargo test --test integration_tests -- --ignored
```

Or use Docker:

```bash
# Start MySQL
docker-compose up -d mysql

# Wait for MySQL to be ready
sleep 30

# Run integration tests
docker exec mysql-executor-db mysql -u root -prootpassword -e "CREATE DATABASE IF NOT EXISTS test"
cd backend
MYSQL_TEST_HOST=127.0.0.1 MYSQL_TEST_PORT=3754 MYSQL_TEST_USER=root MYSQL_TEST_PASSWORD=rootpassword MYSQL_TEST_DATABASE=test cargo test --test integration_tests -- --ignored
```

### Test Coverage

The test suite covers:

- Configuration loading and validation
- Database connection and pooling
- SQL execution (simple queries, CTEs, multiple result sets)
- Data type conversion (all MySQL types)
- JSON serialization and atomic writes
- File scanning and directory traversal
- Concurrent execution
- Error handling and recovery
- Interval-based skipping

## Troubleshooting

### Common Issues

1. **Connection refused**
   - Ensure MySQL is running and accessible
   - Check host/port configuration
   - Verify firewall settings

2. **Authentication failed**
   - Verify username/password
   - Check user privileges

3. **Permission denied on JSON write**
   - Check directory permissions
   - Ensure write access to output directory

4. **Query timeout**
   - Increase `timeout_seconds` in config
   - Optimize slow queries

### Logs

```bash
# Docker logs
docker-compose logs mysql-executor

# Increase log verbosity
RUST_LOG=debug docker-compose up mysql-executor
```

## License

MIT License

## Production Deployment

### Recommended Configuration

```toml
[database]
host = "mysql-primary.internal"
port = 3306
username = "executor_prod"
password = "${MYSQL_PASSWORD}"  # Use environment variable
database = "production"
pool_size = 20                  # Adjust based on workload
timeout_seconds = 600           # 10 minutes for complex queries

[execution]
scan_directory = "/data/sql"
max_concurrent_files = 8        # Adjust based on CPU cores

[execution.file_intervals]
"reports/daily_summary.sql" = 1440   # Once per day
"reports/hourly_stats.sql" = 60      # Once per hour
```

### Performance Tuning

| Parameter | Development | Production | Notes |
|-----------|-------------|------------|-------|
| `pool_size` | 5-10 | 20-50 | Based on concurrent queries |
| `max_concurrent_files` | 2-4 | 4-16 | Based on CPU cores |
| `timeout_seconds` | 30-60 | 300-600 | Based on query complexity |

### Security Best Practices

1. **Database User Privileges**: Create a dedicated user with minimal required privileges
   ```sql
   CREATE USER 'executor_prod'@'%' IDENTIFIED BY 'secure_password';
   GRANT SELECT ON production.* TO 'executor_prod'@'%';
   -- Add INSERT/UPDATE only if needed for stored procedures
   ```

2. **Network Security**: Use internal network or VPN for database connections

3. **Secrets Management**: Use environment variables or secrets manager for passwords
   ```bash
   export MYSQL_PASSWORD="$(aws secretsmanager get-secret-value --secret-id mysql-prod --query SecretString --output text)"
   ```

### Monitoring

1. **Log Aggregation**: Configure `RUST_LOG` and forward logs to your logging system
   ```bash
   RUST_LOG=info,mysql_executor=debug
   ```

2. **Metrics to Monitor**:
   - Execution duration per file
   - Success/failure rates
   - JSON files updated count
   - Database connection pool usage

3. **Health Checks**: Monitor the process exit code (0 = success, 1 = failures occurred)

### Scheduling

Use cron or a job scheduler to run periodically:

```bash
# Run every 5 minutes
*/5 * * * * /usr/local/bin/mysql-executor >> /var/log/mysql-executor.log 2>&1
```

Or use systemd timer for better control:

```ini
# /etc/systemd/system/mysql-executor.timer
[Unit]
Description=MySQL Executor Timer

[Timer]
OnCalendar=*:0/5
Persistent=true

[Install]
WantedBy=timers.target
```
