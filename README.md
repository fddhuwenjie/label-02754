# MySQL Executor

一个基于 Rust 的应用程序，用于扫描和执行 MySQL 文件（.sql），并生成包含完整元数据的 JSON 结果文件。

## 运行方式

### 快速开始（推荐）

使用一键脚本自动安装依赖、构建和测试：

```bash
# macOS / Linux
./run.sh

# Windows
run.bat
```

脚本支持的命令：

| 命令 | 说明 |
|------|------|
| `./run.sh` | 默认：自动构建 + 运行测试 |
| `./run.sh install` | 安装所有依赖（Rust、Docker） |
| `./run.sh build` | 仅构建项目 |
| `./run.sh test` | 仅运行测试 |
| `./run.sh docker` | 使用 Docker Compose 启动服务 |
| `./run.sh local` | 本地运行（需配置数据库） |
| `./run.sh stop` | 停止 Docker 服务 |
| `./run.sh logs` | 查看 Docker 日志 |
| `./run.sh clean` | 清理构建产物 |
| `./run.sh help` | 显示帮助信息 |

脚本会自动检测操作系统和包管理器，缺失的依赖会自动安装。

### Docker

1. 克隆仓库并进入项目目录

2. 启动所有服务：
```bash
docker-compose up --build -d
```

3. 查看服务状态：
```bash
docker-compose ps
```

4. 查看日志：
```bash
docker-compose logs -f mysql-executor
```

5. 停止服务：
```bash
docker-compose down
```

6. 停止并删除数据卷：
```bash
docker-compose down -v
```

### 本地开发

1. 安装 Rust（1.75+）：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. 安装 MySQL 8.0+ 并创建数据库

3. 配置应用：
```bash
cp config/config.toml.example config/config.toml
# 编辑 config/config.toml，填入你的数据库配置
```

4. 构建并运行：
```bash
cd backend
cargo build --release
./target/release/mysql-executor
```

或直接运行：
```bash
cd backend
cargo run --release
```

环境变量：
- `MYSQL_EXECUTOR_CONFIG`：配置文件路径（默认：`config/config.toml`）
- `RUST_LOG`：日志级别（默认：`info`）

### 配置路径说明

| 环境 | 默认配置路径 | 说明 |
|------|-------------|------|
| 本地开发 | `config/config.toml` | 相对于工作目录 |
| Docker 容器 | `/app/config/config.toml` | 通过 `MYSQL_EXECUTOR_CONFIG` 环境变量设置 |

本地运行时，请确保在项目根目录下，或设置 `MYSQL_EXECUTOR_CONFIG` 为配置文件的绝对路径。

## 服务列表

| 服务 | 描述 | 端口 |
|------|------|------|
| mysql | MySQL 8.0 数据库 | 3754（外部）/ 3306（内部） |
| mysql-executor | Rust SQL 执行器应用 | 无（批处理进程） |

### 服务详情

**MySQL 数据库**
- 镜像：mysql:8.0
- 字符集：utf8mb4
- 排序规则：utf8mb4_unicode_ci
- 已启用健康检查
- 数据持久化存储

**MySQL Executor**
- 跨平台支持（ARM64 & AMD64）
- 并发 SQL 文件处理
- 原子性 JSON 文件写入
- 连接池管理

## 测试账号

### MySQL 数据库

| 用户名 | 密码 | 数据库 | 描述 |
|--------|------|--------|------|
| root | rootpassword | - | 超级管理员 |
| executor | executorpass | testdb | 应用用户 |

### 连接示例

**MySQL CLI：**
```bash
mysql -h 127.0.0.1 -P 3754 -u executor -pexecutorpass testdb
```

**Docker exec：**
```bash
docker exec -it mysql-executor-db mysql -u executor -pexecutorpass testdb
```

## 题目内容

开发一个基于Rust语言的程序，实现以下详细功能和要求： 

1. 文件处理与执行功能： 
- 程序需扫描并执行其所在目录及所有子目录中的MySQL文件（.sql扩展名） 
- 对每个执行成功的MySQL文件，生成一个同名的JSON结果文件（例如：query.sql生成query.json） 
- 确保JSON文件与源MySQL文件保持相同的目录结构和相对路径 

2. SQL兼容性要求： 
- 完全支持标准SQL注释（单行注释--和多行注释/* */） 
- 兼容WITH语句和公共表表达式(CTE)语法 
- 正确处理多表结果集返回场景 
- 支持存储过程、函数调用及复杂查询语句的执行 

3. 配置文件管理： 
- 在程序根目录下创建并读取配置文件（推荐使用.toml或.json格式） 
- 配置文件必须包含：MySQL数据库连接信息（主机地址、端口号、用户名、密码、默认数据库名） 
- 配置文件需指定程序扫描和执行MySQL文件的起始目录路径 
- 配置文件应支持为每个MySQL文件单独定义JSON生成间隔时间（以分钟为单位） 

4. 执行策略与文件更新机制： 
- 当配置文件中未定义特定MySQL文件的生成间隔时，每次程序运行都覆盖更新对应的JSON文件 
- 当配置文件中定义了生成间隔时，仅当距离上次生成时间超过间隔时间才更新JSON文件 
- 若目标JSON文件不存在，无论是否定义生成间隔，均立即执行MySQL文件并生成JSON文件 
- 所有时间判断需基于JSON文件的最后修改时间戳 

5. 错误处理与故障恢复： 
- 实现全面的错误捕获机制，包括MySQL连接错误、SQL语法错误、执行超时、权限不足等 
- 当MySQL执行过程中出现任何错误或异常时，保持原有JSON文件不变，不进行任何更新操作 
- 程序需记录详细错误日志，包括错误类型、发生时间、涉及的MySQL文件路径及具体错误信息 

6. 并发控制与原子操作： 
- 实现并发安全的文件处理机制，支持多线程同时处理不同目录的MySQL文件 
- 采用原子更新策略处理JSON文件：先写入临时文件，验证无误后再原子性替换目标文件 
- 实现文件锁定机制，防止多个进程或线程同时操作同一文件 
- 确保在程序崩溃或中断时，已有JSON文件不会出现数据不全或文件破损情况 

7. JSON序列化与数据类型处理： 
- 实现MySQL数据类型到JSON类型的完整映射，包括： 
* 数值类型（INT, BIGINT, FLOAT, DOUBLE等） 
* 字符串类型（VARCHAR, TEXT, JSON等） 
* 日期时间类型（DATE, TIME, DATETIME, TIMESTAMP） 
* 二进制类型（BLOB, BINARY等） 
* 特殊类型（ENUM, SET, GEOMETRY等） 
- 正确处理多表结果集，在JSON中使用清晰的结构区分不同表的结果 
- 确保生成的JSON文件符合严格的JSON语法规范，可通过JSON格式验证工具检测 
- 为JSON文件添加元数据信息，包括：执行时间、MySQL版本、受影响行数、结果集数量等 

8. 性能与资源管理： 
- 实现数据库连接池管理，优化连接复用 
- 对大结果集实现流式处理，避免内存溢出 
- 限制并发执行的MySQL文件数量，防止系统资源耗尽 
- 实现查询超时机制，避免长时间运行的查询阻塞程序执行 

程序开发完成后，需提供完整的单元测试和集成测试，确保所有功能点符合要求，并生成详细的用户使用文档和API说明。

### 功能需求

1. **文件处理与执行**
   - 扫描并执行配置目录及子目录中的所有 .sql 文件
   - 生成同名的 JSON 结果文件（如：query.sql → query.json）
   - 保持输出文件的目录结构

2. **SQL 兼容性**
   - 完整支持 SQL 注释（-- 和 /* */）
   - WITH 语句和 CTE（公共表表达式）
   - 多结果集
   - 存储过程和函数

3. **配置管理**
   - TOML 配置文件
   - 数据库连接设置（主机、端口、用户名、密码、数据库）
   - 扫描目录路径
   - 按文件配置生成间隔（分钟）

4. **执行策略**
   - 无间隔配置：每次运行都更新 JSON
   - 有间隔配置：仅当超过间隔时间时更新
   - JSON 不存在：立即生成
   - 时间基于 JSON 文件的最后修改时间戳

5. **错误处理**
   - 全面的错误捕获（连接、语法、超时、权限）
   - 出错时保留现有 JSON
   - 详细的错误日志

6. **并发与原子性**
   - 线程安全的文件处理
   - 原子性 JSON 写入（临时文件 + 重命名）
   - 文件锁机制
   - 崩溃安全操作

7. **JSON 序列化**
   - 完整的 MySQL 到 JSON 类型映射
   - 多结果集支持
   - 元数据（执行时间、MySQL 版本、影响行数等）

8. **性能**
   - 连接池
   - 可配置的并发限制
   - 查询超时机制

## 项目结构

```
.
├── backend/                    # Rust 应用
│   ├── src/
│   │   ├── main.rs            # 入口点
│   │   ├── config.rs          # 配置处理
│   │   ├── database.rs        # MySQL 连接与执行
│   │   ├── executor.rs        # 主执行逻辑
│   │   ├── json_writer.rs     # JSON 输出处理
│   │   └── scanner.rs         # SQL 文件发现
│   ├── Cargo.toml             # Rust 依赖
│   └── Dockerfile             # 多架构 Docker 构建
├── config/                     # 配置文件
│   ├── config.toml            # 当前配置
│   └── config.toml.example    # 配置模板
├── sql/                        # 待执行的 SQL 文件
│   ├── example_queries/       # 示例查询
│   └── reports/               # 报表查询
├── init-db/                    # MySQL 初始化脚本
├── docker-compose.yml          # Docker Compose 配置
├── .gitignore                  # Git 忽略规则
└── README.md                   # 本文件
```

## 配置说明

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
# "reports/daily_summary.sql" = 1440  # 每天一次
```

## JSON 输出格式

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

## 数据类型映射

| MySQL 类型 | JSON 类型 |
|------------|-----------|
| INT, BIGINT | number（整数） |
| FLOAT, DOUBLE, DECIMAL | number（浮点数） |
| VARCHAR, TEXT, CHAR | string |
| DATE | string（YYYY-MM-DD） |
| TIME | string（HH:MM:SS.ffffff） |
| DATETIME, TIMESTAMP | string（YYYY-MM-DD HH:MM:SS.ffffff） |
| BLOB, BINARY | string（base64 编码） |
| JSON | string（JSON 字符串） |
| ENUM, SET | string |
| NULL | null |

## API 参考

### 命令行

```bash
# 使用默认配置运行（config.toml）
mysql-executor

# 使用自定义配置运行
mysql-executor /path/to/config.toml

# 使用环境变量运行
MYSQL_EXECUTOR_CONFIG=/path/to/config.toml mysql-executor
```

### 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| MYSQL_EXECUTOR_CONFIG | 配置文件路径 | config/config.toml |
| RUST_LOG | 日志级别（error, warn, info, debug, trace） | info |

### 库 API

本应用也可作为库使用：

```rust
use mysql_executor::config::Config;
use mysql_executor::executor::Executor;

// 加载配置
let config = Config::load("config.toml")?;

// 创建并运行执行器
let executor = Executor::new(config)?;
let results = executor.run()?;

// 处理结果
for result in results {
    println!("{}: success={}, updated={}", 
        result.file_path, 
        result.success, 
        result.json_updated
    );
}
```

### 模块参考

#### `config` 模块

| 类型 | 描述 |
|------|------|
| `Config` | 主配置结构体 |
| `DatabaseConfig` | 数据库连接设置 |
| `ExecutionConfig` | 执行行为设置 |
| `ConfigError` | 配置错误类型 |

主要方法：
- `Config::load(path)` - 从 TOML 文件加载配置
- `Config::create_default_config(path)` - 创建默认配置文件
- `Config::get_interval_for_file(path)` - 获取文件的配置间隔

#### `database` 模块

| 类型 | 描述 |
|------|------|
| `DatabasePool` | 连接池管理器 |
| `QueryResults` | SQL 执行结果 |
| `ResultSet` | 单个结果集数据 |
| `ColumnInfo` | 列元数据 |
| `JsonValue` | MySQL 到 JSON 值包装器 |
| `DatabaseError` | 数据库错误类型 |

主要方法：
- `DatabasePool::new(config)` - 创建新连接池
- `DatabasePool::execute_sql(sql)` - 执行 SQL 并返回结果
- `DatabasePool::get_connection()` - 获取池化连接

#### `executor` 模块

| 类型 | 描述 |
|------|------|
| `Executor` | 主执行协调器 |
| `ExecutionResult` | 单文件执行结果 |
| `ExecutorError` | 执行器错误类型 |

主要方法：
- `Executor::new(config)` - 创建新执行器
- `Executor::run()` - 执行所有 SQL 文件

#### `scanner` 模块

| 类型 | 描述 |
|------|------|
| `Scanner` | SQL 文件发现器 |
| `SqlFile` | SQL 文件元数据 |
| `ScannerError` | 扫描器错误类型 |

主要方法：
- `Scanner::new(directory)` - 为目录创建扫描器
- `Scanner::scan()` - 查找所有 SQL 文件
- `SqlFile::read_content()` - 读取 SQL 文件内容
- `SqlFile::json_exists()` - 检查 JSON 输出是否存在

#### `json_writer` 模块

| 类型 | 描述 |
|------|------|
| `JsonWriter` | JSON 输出处理器 |
| `JsonOutput` | 完整 JSON 结构 |
| `JsonMetadata` | 执行元数据 |
| `JsonResultSet` | JSON 格式的结果集 |
| `JsonWriterError` | 写入器错误类型 |

主要方法：
- `JsonWriter::write_results(path, results, source, duration)` - 原子性写入 JSON

### 错误代码

| 错误类型 | 描述 |
|----------|------|
| `ConfigError::NotFound` | 配置文件未找到 |
| `ConfigError::ParseError` | 无效的 TOML 语法 |
| `DatabaseError::ConnectionError` | 无法连接到 MySQL |
| `DatabaseError::QueryError` | SQL 执行失败 |
| `DatabaseError::PoolError` | 连接池错误 |
| `DatabaseError::AccessDenied` | 数据库权限不足 |
| `DatabaseError::Timeout` | 查询超时 |
| `ScannerError::DirectoryNotFound` | 扫描目录未找到 |
| `ScannerError::ReadError` | 读取 SQL 文件失败 |
| `JsonWriterError::IoError` | 文件 I/O 错误 |
| `JsonWriterError::LockError` | 文件锁获取失败 |
| `JsonWriterError::AtomicWriteError` | 原子重命名失败 |

## 测试

### 运行单元测试

```bash
cd backend
cargo test
```

### 详细输出运行

```bash
cd backend
cargo test -- --nocapture
```

### 运行集成测试

集成测试需要运行中的 MySQL 实例：

```bash
# 设置环境变量
export MYSQL_TEST_HOST=localhost
export MYSQL_TEST_PORT=3306
export MYSQL_TEST_USER=root
export MYSQL_TEST_PASSWORD=password
export MYSQL_TEST_DATABASE=test

# 运行集成测试
cd backend
cargo test --test integration_tests -- --ignored
```

或使用 Docker：

```bash
# 启动 MySQL
docker-compose up -d mysql

# 等待 MySQL 就绪
sleep 30

# 运行集成测试
docker exec mysql-executor-db mysql -u root -prootpassword -e "CREATE DATABASE IF NOT EXISTS test"
cd backend
MYSQL_TEST_HOST=127.0.0.1 MYSQL_TEST_PORT=3754 MYSQL_TEST_USER=root MYSQL_TEST_PASSWORD=rootpassword MYSQL_TEST_DATABASE=test cargo test --test integration_tests -- --ignored
```

### 测试覆盖

测试套件覆盖：

- 配置加载和验证
- 数据库连接和池化
- SQL 执行（简单查询、CTE、多结果集）
- 数据类型转换（所有 MySQL 类型）
- JSON 序列化和原子写入
- 文件扫描和目录遍历
- 并发执行
- 错误处理和恢复
- 基于间隔的跳过

## 故障排除

### 常见问题

1. **连接被拒绝**
   - 确保 MySQL 正在运行且可访问
   - 检查主机/端口配置
   - 验证防火墙设置

2. **认证失败**
   - 验证用户名/密码
   - 检查用户权限

3. **JSON 写入权限被拒绝**
   - 检查目录权限
   - 确保对输出目录有写入权限

4. **查询超时**
   - 增加配置中的 `timeout_seconds`
   - 优化慢查询

### 日志

```bash
# Docker 日志
docker-compose logs mysql-executor

# 增加日志详细程度
RUST_LOG=debug docker-compose up mysql-executor
```

## 许可证

MIT License

## 生产部署

### 推荐配置

```toml
[database]
host = "mysql-primary.internal"
port = 3306
username = "executor_prod"
password = "${MYSQL_PASSWORD}"  # 使用环境变量
database = "production"
pool_size = 20                  # 根据工作负载调整
timeout_seconds = 600           # 复杂查询 10 分钟

[execution]
scan_directory = "/data/sql"
max_concurrent_files = 8        # 根据 CPU 核心数调整

[execution.file_intervals]
"reports/daily_summary.sql" = 1440   # 每天一次
"reports/hourly_stats.sql" = 60      # 每小时一次
```

### 性能调优

| 参数 | 开发环境 | 生产环境 | 说明 |
|------|----------|----------|------|
| `pool_size` | 5-10 | 20-50 | 基于并发查询数 |
| `max_concurrent_files` | 2-4 | 4-16 | 基于 CPU 核心数 |
| `timeout_seconds` | 30-60 | 300-600 | 基于查询复杂度 |

### 安全最佳实践

1. **数据库用户权限**：创建具有最小必要权限的专用用户
   ```sql
   CREATE USER 'executor_prod'@'%' IDENTIFIED BY 'secure_password';
   GRANT SELECT ON production.* TO 'executor_prod'@'%';
   -- 仅在存储过程需要时添加 INSERT/UPDATE
   ```

2. **网络安全**：使用内网或 VPN 进行数据库连接

3. **密钥管理**：使用环境变量或密钥管理器存储密码
   ```bash
   export MYSQL_PASSWORD="$(aws secretsmanager get-secret-value --secret-id mysql-prod --query SecretString --output text)"
   ```

### 监控

1. **日志聚合**：配置 `RUST_LOG` 并将日志转发到日志系统
   ```bash
   RUST_LOG=info,mysql_executor=debug
   ```

2. **监控指标**：
   - 每个文件的执行时长
   - 成功/失败率
   - JSON 文件更新数量
   - 数据库连接池使用情况

3. **健康检查**：监控进程退出码（0 = 成功，1 = 有失败）

### 定时调度

使用 cron 或任务调度器定期运行：

```bash
# 每 5 分钟运行一次
*/5 * * * * /usr/local/bin/mysql-executor >> /var/log/mysql-executor.log 2>&1
```

或使用 systemd timer 获得更好的控制：

```ini
# /etc/systemd/system/mysql-executor.timer
[Unit]
Description=MySQL Executor 定时器

[Timer]
OnCalendar=*:0/5
Persistent=true

[Install]
WantedBy=timers.target
```
