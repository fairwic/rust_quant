# 🚀 服务启动指南

**生成时间**: 2025-11-10  
**最后更新**: 2025-11-10  
**架构版本**: DDD Workspace 架构 (14个crate包)  
**项目版本**: v0.2.0

---

## ✅ 编译状态

**Release 版本编译**: ✅ 成功

```bash
Finished `release` profile [optimized] target(s) in 1m 17s
```

**可执行文件位置**: `./target/release/rust-quant`  
**入口文件**: `crates/rust-quant-cli/src/main.rs`

---

## 📋 启动前准备

### 1. 数据库服务

**Postgres**:
```bash
# 启动 Postgres (macOS with Homebrew)
brew services start postgresql

# 或使用 Podman
podman run -d \
  --name rust-quant-postgres \
  -p 5432:5432 \
  -e POSTGRES_PASSWORD=postgres123 \
  -e POSTGRES_DB=quant_core \
  postgres:16
```

**检查连接**:
```bash
psql "postgres://postgres:postgres123@127.0.0.1:5432/quant_core" -c "SELECT 1"
```

### 2. Redis 服务

**启动 Redis**:
```bash
# macOS with Homebrew
brew services start redis

# 或使用 Podman
podman run -d \
  --name rust-quant-redis \
  -p 6379:6379 \
  redis:alpine
```

**检查连接**:
```bash
redis-cli ping
# 应该返回 PONG
```

### 3. 环境变量配置

**检查 `.env` 文件**:
```bash
cat .env
```

**必需的环境变量**:
```bash
# 应用环境
APP_ENV=local

# 数据库配置
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@127.0.0.1:5432/quant_core
DATABASE_URL=postgres://postgres:postgres123@127.0.0.1:5432/quant_core
DATABASE_MAX_CONNECTIONS=10

# Redis 配置
REDIS_URL=redis://127.0.0.1:6379
REDIS_POOL_SIZE=20

# OKX API 配置 (如果需要实盘交易)
OKX_API_KEY=your_api_key
OKX_SECRET_KEY=your_secret_key
OKX_PASSPHRASE=your_passphrase

# 功能开关
IS_RUN_SYNC_DATA_JOB=false      # 是否同步数据
IS_BACK_TEST=false               # 是否执行回测
IS_OPEN_SOCKET=false             # 是否开启 WebSocket
IS_RUN_REAL_STRATEGY=false       # 是否运行实盘策略

# 策略配置
RUN_STRATEGY_PERIOD=5m           # 策略运行周期
```

---

## 🚀 启动服务

### 方式 1: 直接运行

```bash
cd /Users/mac2/onions/rust_quant
./target/release/rust-quant
```

### 方式 2: 使用 cargo run (开发模式)

```bash
cargo run --release
```

### 方式 3: 指定环境变量运行

```bash
# 只运行数据同步
IS_RUN_SYNC_DATA_JOB=true \
IS_BACK_TEST=false \
IS_RUN_REAL_STRATEGY=false \
./target/release/rust-quant

# 只运行回测
IS_RUN_SYNC_DATA_JOB=false \
IS_BACK_TEST=true \
IS_RUN_REAL_STRATEGY=false \
./target/release/rust-quant

# 运行实盘策略
IS_RUN_SYNC_DATA_JOB=false \
IS_BACK_TEST=false \
IS_RUN_REAL_STRATEGY=true \
./target/release/rust-quant
```

---

## 🔍 启动日志解读

### 正常启动日志

```
2025-11-10T06:21:14.578Z INFO  Log configuration setup successfully!
2025-11-10T06:21:14.579Z INFO  Environment: local, Log Level: info
2025-11-10T06:21:14.584Z INFO  Database connection successful
2025-11-10T06:21:14.585Z INFO  Redis connection successful
2025-11-10T06:21:14.586Z INFO  应用初始化完成
2025-11-10T06:21:14.587Z INFO  📊 监控交易对: ["SOL-USDT-SWAP", "BTC-USDT-SWAP"]
2025-11-10T06:21:14.588Z INFO  ✅ 任务调度器初始化成功
2025-11-10T06:21:14.589Z INFO  💓 程序正在运行中...
```

### 常见错误日志

#### 错误 1: 数据库连接失败

```
ERROR Failed to connect to database: Connection refused (os error 61)
```

**解决方案**:
1. 检查 Postgres 是否启动: `brew services list | grep postgresql`
2. 检查端口占用: `lsof -i :5432`
3. 检查 `.env` 中的 `QUANT_CORE_DATABASE_URL` / `DATABASE_URL`

#### 错误 2: Redis 连接失败

```
ERROR Failed to connect to Redis: Connection refused
```

**解决方案**:
1. 检查 Redis 是否启动: `brew services list | grep redis`
2. 检查端口占用: `lsof -i :6379`
3. 检查 `.env` 中的 `REDIS_URL`

#### 错误 3: OKX API 认证失败

```
ERROR OKX API authentication failed
```

**解决方案**:
1. 检查 `.env` 中的 OKX 配置
2. 确认 API Key 权限
3. 如果只是回测，可以关闭实盘功能

---

## 🎯 不同运行模式

### 模式 1: 纯回测模式 (推荐用于测试)

**配置** (`.env`):
```bash
APP_ENV=local
IS_RUN_SYNC_DATA_JOB=false
IS_BACK_TEST=true
IS_OPEN_SOCKET=false
IS_RUN_REAL_STRATEGY=false
```

**特点**:
- ✅ 不需要 OKX API
- ✅ 只需要数据库和历史数据
- ✅ 安全，不会执行真实交易

**使用场景**:
- 策略回测
- 参数优化
- 历史数据分析

### 模式 2: 数据同步模式

**配置** (`.env`):
```bash
APP_ENV=local
IS_RUN_SYNC_DATA_JOB=true
IS_BACK_TEST=false
IS_OPEN_SOCKET=false
IS_RUN_REAL_STRATEGY=false
```

**特点**:
- ⚠️ 需要 OKX API (只读权限即可)
- ✅ 同步最新市场数据
- ✅ 不执行交易

**使用场景**:
- 更新历史数据
- 准备回测数据

### 模式 3: WebSocket 实时数据

**配置** (`.env`):
```bash
APP_ENV=local
IS_RUN_SYNC_DATA_JOB=false
IS_BACK_TEST=false
IS_OPEN_SOCKET=true
IS_RUN_REAL_STRATEGY=false
```

**特点**:
- ⚠️ 需要 OKX API
- ✅ 实时接收市场数据
- ✅ 不执行交易

**使用场景**:
- 实时监控市场
- 准备实盘运行

### 模式 4: 实盘策略 (⚠️ 谨慎)

**配置** (`.env`):
```bash
APP_ENV=prod
IS_RUN_SYNC_DATA_JOB=false
IS_BACK_TEST=false
IS_OPEN_SOCKET=true
IS_RUN_REAL_STRATEGY=true
```

**特点**:
- ⚠️ 需要 OKX API (交易权限)
- ⚠️ 会执行真实交易
- ⚠️ 需要充足的风险控制

**使用场景**:
- 实盘交易
- **仅在充分测试后使用**

---

## 🛠️ 数据库初始化

### 1. 创建数据库

```sql
CREATE DATABASE IF NOT EXISTS rust_quant 
  CHARACTER SET utf8mb4 
  COLLATE utf8mb4_unicode_ci;

USE rust_quant;
```

### 2. 运行 SQL 脚本

```bash
# 如果有初始化脚本
psql "$QUANT_CORE_DATABASE_URL" -f create_table.sql
```

### 3. 检查表结构

```sql
-- 查看所有表
SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename;

-- 检查关键表
\d+ back_test_log
\d+ back_test_detail
\d+ strategy_configs
```

---

## 📊 健康检查

### 启动后验证

**1. 检查进程**:
```bash
ps aux | grep rust_quant
```

**2. 检查日志**:
```bash
# 如果配置了文件日志
tail -f log_files/info.log
tail -f log_files/error.log
```

**3. 检查数据库连接**:
```bash
psql "$QUANT_CORE_DATABASE_URL" -c "SELECT COUNT(*) FROM back_test_log;"
```

**4. 检查 Redis**:
```bash
redis-cli
> KEYS rust-quant:*
> INFO stats
```

---

## 🐛 故障排查

### 问题 1: 程序立即退出

**可能原因**:
1. 所有功能开关都是 false
2. 配置错误导致 panic

**解决方案**:
```bash
# 查看完整错误
RUST_BACKTRACE=1 ./target/release/rust_quant

# 启用至少一个功能
IS_BACK_TEST=true ./target/release/rust_quant
```

### 问题 2: 内存占用过高

**解决方案**:
1. 调整数据库连接池大小: `DATABASE_MAX_CONNECTIONS=5`
2. 调整 Redis 连接池: `REDIS_POOL_SIZE=10`
3. 限制回测数据量

### 问题 3: CPU 占用过高

**可能原因**:
- WebSocket 数据量大
- 策略计算密集

**解决方案**:
1. 减少监控的交易对数量
2. 增加策略执行间隔
3. 优化策略算法

---

## 📝 推荐的启动顺序

### 首次启动 (测试环境)

```bash
# 1. 启动基础服务
brew services start postgresql
brew services start redis

# 2. 确认服务正常
psql "$QUANT_CORE_DATABASE_URL" -c "SELECT 1"
redis-cli ping

# 3. 检查配置
cat .env | grep -E "DATABASE_URL|REDIS_URL"

# 4. 初始化数据库
psql "$QUANT_CORE_DATABASE_URL" -f create_table.sql

# 5. 测试编译
cargo build --release

# 6. 运行回测模式测试
IS_BACK_TEST=true \
IS_RUN_SYNC_DATA_JOB=false \
IS_RUN_REAL_STRATEGY=false \
./target/release/rust_quant

# 7. 如果回测成功，可以尝试其他模式
```

---

## 🎯 当前架构启动流程

```
1. main() 入口 (crates/rust-quant-cli/src/main.rs)
   ↓
2. rust_quant_cli::app_init()
   ├─ 初始化日志系统 (env_logger + tracing)
   ├─ 加载环境变量 (dotenv)
   ├─ 连接数据库 (Postgres via sqlx)
   ├─ 连接 Redis (连接池)
   └─ 初始化完成
   ↓
3. rust_quant_cli::run() (crates/rust-quant-cli/src/app/bootstrap.rs)
   ├─ 初始化任务调度器 (tokio-cron-scheduler)
   ├─ 校验系统时间 (非 local 环境，与 OKX 时间同步)
   └─ 运行 run_modes()
       ├─ 数据同步模式 (if IS_RUN_SYNC_DATA_JOB)
       │   └─ tickets_job::sync_tickers()
       ├─ 回测模式 (if IS_BACK_TEST)
       │   └─ TODO: 回测逻辑待实现
       ├─ WebSocket 模式 (if IS_OPEN_SOCKET)
       │   └─ TODO: WebSocket 逻辑待实现
       └─ 实盘策略 (if IS_RUN_REAL_STRATEGY)
           └─ TODO: 实盘策略逻辑待实现
   ↓
4. 启动心跳任务 (每 10 分钟)
   ↓
5. 等待退出信号 (SIGINT/SIGTERM/SIGQUIT)
   ↓
6. 优雅关闭
   ├─ 停止心跳任务
   ├─ 停止所有策略 (如果有运行)
   ├─ 关闭调度器
   ├─ 关闭数据库连接池
   └─ 关闭 Redis 连接池
```

---

## ✅ 成功启动的标志

**日志输出**:
```
✅ Log configuration setup successfully!
✅ Database connection successful
✅ Redis connection successful
✅ 应用初始化完成
✅ 任务调度器初始化成功
📊 监控交易对: [...]
💓 程序正在运行中...
```

**进程稳定运行**:
```bash
ps aux | grep rust_quant
# 应该看到进程持续运行
```

---

## 🔧 开发调试模式

**启用详细日志**:
```bash
RUST_LOG=debug ./target/release/rust_quant
```

**启用 backtrace**:
```bash
RUST_BACKTRACE=1 ./target/release/rust-quant
```

**完整调试模式**:
```bash
RUST_LOG=debug \
RUST_BACKTRACE=1 \
IS_BACK_TEST=true \
./target/release/rust-quant 2>&1 | tee startup.log
```

---

## 📞 获取帮助

### 查看日志
```bash
# 实时查看日志
tail -f log_files/info.log

# 查看错误日志
tail -f log_files/error.log

# 搜索特定错误
grep -i "error" log_files/*.log
```

### 检查配置
```bash
# 查看所有环境变量
cat .env

# 验证配置加载
cargo run -- --help  # (如果实现了 CLI 参数)
```

---

**当前状态**: 

- ✅ **编译成功**: Release 版本已编译
- ⚠️ **数据库**: 需要启动 Postgres 服务
- ⚠️ **Redis**: 需要启动 Redis 服务
- ⚠️ **配置**: 需要检查 `.env` 文件

**下一步**: 启动 Postgres 和 Redis，然后重新运行程序
