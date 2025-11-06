#!/bin/bash
# Rust Quant Workspace 拆包迁移脚本
# 版本: v1.0
# 日期: 2025-11-06
# 目标: 将单体项目拆分为 Cargo Workspace 多包架构

set -e  # 遇到错误立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Rust Quant Workspace 拆包迁移${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# 检查是否在 Git 仓库中
if [ ! -d ".git" ]; then
    echo -e "${RED}错误: 当前目录不是 Git 仓库${NC}"
    exit 1
fi

# 检查是否有未提交的更改
if ! git diff-index --quiet HEAD --; then
    echo -e "${YELLOW}警告: 检测到未提交的更改${NC}"
    read -p "是否继续? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# 创建迁移分支
BRANCH_NAME="refactor/workspace-migration"
echo -e "${YELLOW}创建分支: $BRANCH_NAME${NC}"

if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
    echo -e "${YELLOW}分支已存在，切换到该分支${NC}"
    git checkout "$BRANCH_NAME"
else
    git checkout -b "$BRANCH_NAME"
    echo -e "${GREEN}✓ 分支创建成功${NC}"
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 1: 创建 Workspace 目录结构${NC}"
echo -e "${YELLOW}========================================${NC}"

# 创建 crates 目录
mkdir -p crates/{core,market,indicators,strategies,risk,execution,orchestration,analytics,common}

# 为每个包创建 src 目录
for crate in core market indicators strategies risk execution orchestration analytics common; do
    mkdir -p "crates/$crate/src"
    echo -e "${GREEN}✓ 创建 crates/$crate/src${NC}"
done

# 创建主程序目录
mkdir -p rust-quant-cli/src

echo -e "${GREEN}✓ Workspace 目录结构创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 2: 生成 Workspace 根 Cargo.toml${NC}"
echo -e "${YELLOW}========================================${NC}"

# 备份原 Cargo.toml
if [ -f "Cargo.toml" ]; then
    mv Cargo.toml Cargo.toml.backup
    echo -e "${YELLOW}已备份原 Cargo.toml -> Cargo.toml.backup${NC}"
fi

# 创建新的 Workspace Cargo.toml
cat > Cargo.toml << 'EOF'
[workspace]
members = [
    "crates/common",
    "crates/core",
    "crates/market",
    "crates/indicators",
    "crates/strategies",
    "crates/risk",
    "crates/execution",
    "crates/orchestration",
    "crates/analytics",
    "rust-quant-cli",
]

resolver = "2"

[workspace.package]
version = "0.2.0"
edition = "2021"
rust-version = "1.75.0"
authors = ["Rust Quant Team"]
license = "MIT"

[workspace.dependencies]
# === 核心依赖 ===
tokio = { version = "1.37.0", features = ["rt", "rt-multi-thread", "macros", "full"] }
anyhow = "1.0.86"
thiserror = "1.0.61"
serde = { version = "1.0.202", features = ["derive"] }
serde_json = "1.0.117"
async-trait = "0.1.81"

# === 日志和追踪 ===
tracing = "0.1"
tracing-subscriber = { version = "0.3.0", features = ["env-filter", "json"] }
tracing-appender = "0.2.3"
log = "0.4"
fast_log = "1.6"
flexi_logger = "0.28.3"
env_logger = "0.11.3"

# === 数据库 ===
rbatis = "4.5"
rbdc-mysql = { version = "4.5", default-features = false, features = ["tls-native-tls"] }
rbs = "4.5"

# === 缓存 ===
redis = { version = "0.25.3", features = ["aio", "tokio-comp"] }
dashmap = "6.1.0"

# === 时间处理 ===
chrono = "0.4.38"

# === 网络通信 ===
reqwest = "0.11.27"
tokio-tungstenite = { version = "0.23", features = ["native-tls"] }
futures = "0.3.30"
futures-channel = "0.3.30"
futures-util = "0.3.30"

# === 加密和编码 ===
hmac = "0.12.1"
sha2 = "0.10.8"
hex = "0.4.3"
base64 = "0.21.7"
hmac-sha256 = "0.1"

# === 配置管理 ===
dotenv = "0.15.0"
once_cell = "1.19.0"

# === 任务调度 ===
tokio-cron-scheduler = { version = "0.10.0", features = ["signal"] }
tokio-retry = "0.3.0"

# === 技术分析库 ===
ta = "0.5.0"
technical_indicators = "0.5.0"
tech_analysis = "0.1.1"
simple_moving_average = "1.0.2"

# === 数值计算 ===
ndarray = "0.15"
linregress = "0.5.4"
approx = "0.5.1"
float-cmp = "0.10.0"

# === 工具库 ===
uuid = { version = "1.4.1", features = ["v4"] }
lazy_static = "1.4.0"
clap = { version = "4.5.4", features = ["derive"] }

# === 交易所 SDK ===
okx = { version = "0.1.9" }

# === 邮件服务（可选）===
lettre = "0.11"

# === Workspace 内部依赖 ===
rust-quant-common = { path = "crates/common" }
rust-quant-core = { path = "crates/core" }
rust-quant-market = { path = "crates/market" }
rust-quant-indicators = { path = "crates/indicators" }
rust-quant-strategies = { path = "crates/strategies" }
rust-quant-risk = { path = "crates/risk" }
rust-quant-execution = { path = "crates/execution" }
rust-quant-orchestration = { path = "crates/orchestration" }
rust-quant-analytics = { path = "crates/analytics" }
EOF

echo -e "${GREEN}✓ Workspace Cargo.toml 创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 3: 生成各包的 Cargo.toml${NC}"
echo -e "${YELLOW}========================================${NC}"

# === crates/common/Cargo.toml ===
cat > crates/common/Cargo.toml << 'EOF'
[package]
name = "rust-quant-common"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
thiserror.workspace = true
anyhow.workspace = true
EOF

# === crates/core/Cargo.toml ===
cat > crates/core/Cargo.toml << 'EOF'
[package]
name = "rust-quant-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
tracing-appender.workspace = true
chrono.workspace = true
dotenv.workspace = true
once_cell.workspace = true

# 数据库
rbatis.workspace = true
rbdc-mysql.workspace = true
rbs.workspace = true

# 缓存
redis.workspace = true
dashmap.workspace = true
EOF

# === crates/market/Cargo.toml ===
cat > crates/market/Cargo.toml << 'EOF'
[package]
name = "rust-quant-market"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono.workspace = true

# 网络通信
reqwest.workspace = true
tokio-tungstenite.workspace = true
futures.workspace = true
futures-channel.workspace = true
futures-util.workspace = true

# 交易所 SDK
okx.workspace = true

# 数据库
rbatis.workspace = true
EOF

# === crates/indicators/Cargo.toml ===
cat > crates/indicators/Cargo.toml << 'EOF'
[package]
name = "rust-quant-indicators"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true

# 外部依赖
serde.workspace = true
thiserror.workspace = true
anyhow.workspace = true

# 技术分析库
ta.workspace = true
technical_indicators.workspace = true
tech_analysis.workspace = true
simple_moving_average.workspace = true

# 数值计算
ndarray.workspace = true
linregress.workspace = true
approx.workspace = true
float-cmp.workspace = true
EOF

# === crates/strategies/Cargo.toml ===
cat > crates/strategies/Cargo.toml << 'EOF'
[package]
name = "rust-quant-strategies"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-market.workspace = true
rust-quant-indicators.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono.workspace = true
dashmap.workspace = true
EOF

# === crates/risk/Cargo.toml ===
cat > crates/risk/Cargo.toml << 'EOF'
[package]
name = "rust-quant-risk"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-market.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono.workspace = true
EOF

# === crates/execution/Cargo.toml ===
cat > crates/execution/Cargo.toml << 'EOF'
[package]
name = "rust-quant-execution"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-market.workspace = true
rust-quant-risk.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono.workspace = true

# 交易所 SDK
okx.workspace = true

# 数据库
rbatis.workspace = true
EOF

# === crates/orchestration/Cargo.toml ===
cat > crates/orchestration/Cargo.toml << 'EOF'
[package]
name = "rust-quant-orchestration"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-market.workspace = true
rust-quant-strategies.workspace = true
rust-quant-risk.workspace = true
rust-quant-execution.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
thiserror.workspace = true
serde.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono.workspace = true

# 任务调度
tokio-cron-scheduler.workspace = true
tokio-retry.workspace = true

# 缓存
redis.workspace = true
dashmap.workspace = true
EOF

# === crates/analytics/Cargo.toml ===
cat > crates/analytics/Cargo.toml << 'EOF'
[package]
name = "rust-quant-analytics"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-strategies.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
serde.workspace = true
tracing.workspace = true
chrono.workspace = true

# 数据库
rbatis.workspace = true
EOF

# === rust-quant-cli/Cargo.toml ===
cat > rust-quant-cli/Cargo.toml << 'EOF'
[package]
name = "rust-quant-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "rust-quant"
path = "src/main.rs"

[dependencies]
# Workspace 内部依赖
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-market.workspace = true
rust-quant-indicators.workspace = true
rust-quant-strategies.workspace = true
rust-quant-risk.workspace = true
rust-quant-execution.workspace = true
rust-quant-orchestration.workspace = true
rust-quant-analytics.workspace = true

# 外部依赖
tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true
dotenv.workspace = true
clap.workspace = true
EOF

echo -e "${GREEN}✓ 所有包的 Cargo.toml 创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 4: 创建基础 lib.rs 文件${NC}"
echo -e "${YELLOW}========================================${NC}"

# === crates/common/src/lib.rs ===
cat > crates/common/src/lib.rs << 'EOF'
//! # Rust Quant Common
//! 
//! 公共类型、工具函数和常量定义

pub mod types;
pub mod utils;
pub mod constants;
pub mod errors;

// 重新导出常用类型
pub use types::*;
pub use errors::{Result, AppError};
EOF

mkdir -p crates/common/src/{types,utils,constants,errors}
touch crates/common/src/types/mod.rs
touch crates/common/src/utils/mod.rs
touch crates/common/src/constants/mod.rs
cat > crates/common/src/errors/mod.rs << 'EOF'
//! 统一错误类型定义

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("配置错误: {0}")]
    Config(String),
    
    #[error("数据库错误: {0}")]
    Database(String),
    
    #[error("网络错误: {0}")]
    Network(String),
    
    #[error("解析错误: {0}")]
    Parse(String),
    
    #[error("未知错误: {0}")]
    Unknown(String),
}
EOF

# === crates/core/src/lib.rs ===
cat > crates/core/src/lib.rs << 'EOF'
//! # Rust Quant Core
//! 
//! 核心基础设施：配置、数据库、缓存、日志

pub mod config;
pub mod database;
pub mod cache;
pub mod logger;
pub mod time;

// 重新导出常用类型
pub use config::AppConfig;
pub use database::DbPool;
pub use cache::RedisClient;
EOF

mkdir -p crates/core/src/{config,database,cache,logger,time}
touch crates/core/src/config/mod.rs
touch crates/core/src/database/mod.rs
touch crates/core/src/cache/mod.rs
touch crates/core/src/logger/mod.rs
touch crates/core/src/time/mod.rs

# === crates/market/src/lib.rs ===
cat > crates/market/src/lib.rs << 'EOF'
//! # Rust Quant Market
//! 
//! 市场数据：交易所抽象、数据流、持久化

pub mod exchanges;
pub mod models;
pub mod streams;
pub mod repositories;

// 重新导出常用类型
pub use exchanges::{Exchange, ExchangeClient};
pub use models::{Candle, Ticker};
EOF

mkdir -p crates/market/src/{exchanges,models,streams,repositories}
touch crates/market/src/exchanges/mod.rs
touch crates/market/src/models/mod.rs
touch crates/market/src/streams/mod.rs
touch crates/market/src/repositories/mod.rs

# === crates/indicators/src/lib.rs ===
cat > crates/indicators/src/lib.rs << 'EOF'
//! # Rust Quant Indicators
//! 
//! 技术指标库：趋势、动量、波动性、成交量

pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
pub mod pattern;

// 统一指标接口
pub trait Indicator {
    type Input;
    type Output;
    
    fn update(&mut self, input: Self::Input) -> Self::Output;
    fn reset(&mut self);
}
EOF

mkdir -p crates/indicators/src/{trend,momentum,volatility,volume,pattern}
touch crates/indicators/src/trend/mod.rs
touch crates/indicators/src/momentum/mod.rs
touch crates/indicators/src/volatility/mod.rs
touch crates/indicators/src/volume/mod.rs
touch crates/indicators/src/pattern/mod.rs

# === crates/strategies/src/lib.rs ===
cat > crates/strategies/src/lib.rs << 'EOF'
//! # Rust Quant Strategies
//! 
//! 策略引擎：策略框架、具体实现、回测引擎

pub mod framework;
pub mod implementations;
pub mod backtesting;

// 重新导出核心 Trait
pub use framework::strategy_trait::Strategy;
pub use framework::strategy_registry::StrategyRegistry;
EOF

mkdir -p crates/strategies/src/{framework,implementations,backtesting}
touch crates/strategies/src/framework/mod.rs
touch crates/strategies/src/implementations/mod.rs
touch crates/strategies/src/backtesting/mod.rs

# === crates/risk/src/lib.rs ===
cat > crates/risk/src/lib.rs << 'EOF'
//! # Rust Quant Risk
//! 
//! 风控引擎：仓位风控、订单风控、账户风控

pub mod position;
pub mod order;
pub mod account;
pub mod policies;
EOF

mkdir -p crates/risk/src/{position,order,account,policies}
touch crates/risk/src/position/mod.rs
touch crates/risk/src/order/mod.rs
touch crates/risk/src/account/mod.rs
touch crates/risk/src/policies/mod.rs

# === crates/execution/src/lib.rs ===
cat > crates/execution/src/lib.rs << 'EOF'
//! # Rust Quant Execution
//! 
//! 订单执行：订单管理、执行引擎、持仓管理

pub mod order_manager;
pub mod execution_engine;
pub mod position_manager;
EOF

mkdir -p crates/execution/src/{order_manager,execution_engine,position_manager}
touch crates/execution/src/order_manager/mod.rs
touch crates/execution/src/execution_engine/mod.rs
touch crates/execution/src/position_manager/mod.rs

# === crates/orchestration/src/lib.rs ===
cat > crates/orchestration/src/lib.rs << 'EOF'
//! # Rust Quant Orchestration
//! 
//! 编排引擎：策略运行、任务调度、事件总线

pub mod strategy_runner;
pub mod scheduler;
pub mod workflow;
pub mod event_bus;
EOF

mkdir -p crates/orchestration/src/{strategy_runner,scheduler,workflow,event_bus}
touch crates/orchestration/src/strategy_runner/mod.rs
touch crates/orchestration/src/scheduler/mod.rs
touch crates/orchestration/src/workflow/mod.rs
touch crates/orchestration/src/event_bus/mod.rs

# === crates/analytics/src/lib.rs ===
cat > crates/analytics/src/lib.rs << 'EOF'
//! # Rust Quant Analytics
//! 
//! 分析引擎：性能分析、报告生成

pub mod performance;
pub mod reporting;
EOF

mkdir -p crates/analytics/src/{performance,reporting}
touch crates/analytics/src/performance/mod.rs
touch crates/analytics/src/reporting/mod.rs

# === rust-quant-cli/src/main.rs ===
cat > rust-quant-cli/src/main.rs << 'EOF'
//! Rust Quant CLI 主程序

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Rust Quant CLI v0.2.0");
    println!("Workspace 迁移成功！");
    
    // TODO: 添加实际的启动逻辑
    
    Ok(())
}
EOF

echo -e "${GREEN}✓ 所有 lib.rs 文件创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 5: 编译验证${NC}"
echo -e "${YELLOW}========================================${NC}"

echo -e "${BLUE}正在编译 workspace...${NC}"
if cargo check --workspace; then
    echo -e "${GREEN}✓ Workspace 编译成功！${NC}"
else
    echo -e "${RED}✗ Workspace 编译失败${NC}"
    echo -e "${YELLOW}请检查错误信息并修复${NC}"
    exit 1
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 6: 创建迁移指南文档${NC}"
echo -e "${YELLOW}========================================${NC}"

cat > WORKSPACE_MIGRATION_GUIDE.md << 'EOF'
# Workspace 迁移指南

## 📂 新目录结构

```
rust-quant/
├── Cargo.toml (workspace root)
├── crates/
│   ├── common/          # 公共类型和工具
│   ├── core/            # 核心基础设施
│   ├── market/          # 市场数据
│   ├── indicators/      # 技术指标
│   ├── strategies/      # 策略引擎
│   ├── risk/           # 风控引擎
│   ├── execution/      # 订单执行
│   ├── orchestration/  # 编排引擎
│   └── analytics/      # 分析引擎
└── rust-quant-cli/     # 主程序
```

## 🔄 代码迁移映射

### 1. common 包
```bash
# 迁移公共类型
src/trading/types.rs → crates/common/src/types/

# 迁移工具函数
src/trading/utils/ → crates/common/src/utils/
src/time_util.rs → crates/common/src/utils/time.rs

# 迁移常量
src/trading/constants/ → crates/common/src/constants/

# 迁移错误定义
src/error/ → crates/common/src/errors/
```

### 2. core 包
```bash
# 迁移配置
src/app_config/ → crates/core/src/config/

# 数据库（已在 core/database/）
# 缓存（已在 core/cache/）
# 日志（已在 core/logger/）
```

### 3. market 包
```bash
# 迁移市场数据模型
src/trading/model/market/ → crates/market/src/models/

# 迁移 WebSocket
src/socket/ → crates/market/src/streams/

# 迁移数据持久化
src/trading/services/candle_service/ → crates/market/src/repositories/
```

### 4. indicators 包
```bash
# 迁移趋势指标
src/trading/indicator/ema_indicator.rs → crates/indicators/src/trend/ema.rs
src/trading/indicator/sma.rs → crates/indicators/src/trend/sma.rs

# 迁移动量指标
src/trading/indicator/rsi_rma_indicator.rs → crates/indicators/src/momentum/rsi.rs
src/trading/indicator/macd_simple_indicator.rs → crates/indicators/src/momentum/macd.rs

# 迁移波动性指标
src/trading/indicator/atr.rs → crates/indicators/src/volatility/atr.rs
src/trading/indicator/bollings.rs → crates/indicators/src/volatility/bollinger.rs

# 迁移成交量指标
src/trading/indicator/volume_indicator.rs → crates/indicators/src/volume/
```

### 5. strategies 包
```bash
# 迁移策略框架
src/trading/strategy/strategy_trait.rs → crates/strategies/src/framework/
src/trading/strategy/strategy_registry.rs → crates/strategies/src/framework/

# 迁移具体策略
src/trading/strategy/vegas_executor.rs → crates/strategies/src/implementations/vegas/
src/trading/strategy/nwe_executor.rs → crates/strategies/src/implementations/nwe/
src/trading/strategy/ut_boot_strategy.rs → crates/strategies/src/implementations/ut_boot/
```

### 6. risk 包
```bash
# 提取风控逻辑
src/job/risk_*.rs → crates/risk/src/
```

### 7. execution 包
```bash
# 迁移订单执行
src/trading/services/order_service/ → crates/execution/src/execution_engine/
src/trading/services/position_service/ → crates/execution/src/position_manager/
```

### 8. orchestration 包
```bash
# 迁移策略运行器
src/trading/task/strategy_runner.rs → crates/orchestration/src/strategy_runner/

# 迁移任务调度
src/job/ → crates/orchestration/src/scheduler/jobs/
```

## 🚀 下一步行动

### 阶段 1: 迁移公共模块（1周）
```bash
# 1. 迁移 common 包
# 2. 迁移 core 包
# 3. 编译验证
cargo check --package rust-quant-common
cargo check --package rust-quant-core
```

### 阶段 2: 迁移市场数据层（1周）
```bash
# 1. 迁移 market 包
# 2. 编译验证
cargo check --package rust-quant-market
```

### 阶段 3: 迁移指标和策略层（2周）
```bash
# 1. 迁移 indicators 包
# 2. 迁移 strategies 包
# 3. 编译验证
cargo check --package rust-quant-indicators
cargo check --package rust-quant-strategies
```

### 阶段 4: 迁移执行和编排层（1周）
```bash
# 1. 迁移 risk 包
# 2. 迁移 execution 包
# 3. 迁移 orchestration 包
# 4. 编译验证
cargo check --workspace
```

### 阶段 5: 迁移主程序（1周）
```bash
# 1. 迁移 main.rs 和 app/bootstrap.rs
# 2. 更新导入路径
# 3. 完整编译和测试
cargo build --workspace
cargo test --workspace
```

## 📋 迁移检查清单

- [ ] 公共类型和工具迁移
- [ ] 核心基础设施迁移
- [ ] 市场数据层迁移
- [ ] 技术指标迁移
- [ ] 策略引擎迁移
- [ ] 风控引擎迁移
- [ ] 订单执行迁移
- [ ] 编排引擎迁移
- [ ] 主程序迁移
- [ ] 所有测试通过
- [ ] 文档更新

## 🔧 常用命令

```bash
# 编译整个 workspace
cargo build --workspace

# 编译特定包
cargo build --package rust-quant-core

# 运行测试
cargo test --workspace

# 运行特定包测试
cargo test --package rust-quant-indicators

# 检查编译（不生成二进制）
cargo check --workspace

# 格式化代码
cargo fmt --all

# Clippy 检查
cargo clippy --workspace -- -D warnings

# 查看依赖树
cargo tree

# 查看特定包的依赖
cargo tree --package rust-quant-strategies
```

## ⚠️ 注意事项

1. **保留旧代码**：迁移期间保留 `src/` 目录作为参考
2. **小步提交**：每迁移一个包就提交一次
3. **测试优先**：每个包迁移后立即编写或迁移测试
4. **导入路径**：注意更新导入路径（从 `crate::` 到 `rust_quant_xxx::`）
EOF

echo -e "${GREEN}✓ 迁移指南创建完成：WORKSPACE_MIGRATION_GUIDE.md${NC}"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Workspace 骨架搭建完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# 显示目录结构
echo -e "${YELLOW}新创建的 Workspace 结构：${NC}"
tree -L 3 -d crates/ rust-quant-cli/ 2>/dev/null || find crates/ rust-quant-cli/ -type d | sed 's|[^/]*/| |g'

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}下一步操作建议：${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""
echo "1. 查看迁移指南："
echo "   ${GREEN}cat WORKSPACE_MIGRATION_GUIDE.md${NC}"
echo ""
echo "2. 验证编译："
echo "   ${GREEN}cargo check --workspace${NC}"
echo ""
echo "3. 开始代码迁移（按阶段执行）："
echo "   ${GREEN}# 阶段1: 迁移 common 和 core${NC}"
echo "   ${GREEN}# 阶段2: 迁移 market${NC}"
echo "   ${GREEN}# 阶段3: 迁移 indicators 和 strategies${NC}"
echo ""
echo "4. 提交初始结构："
echo "   ${GREEN}git add .${NC}"
echo "   ${GREEN}git commit -m \"feat: 创建 Workspace 骨架结构\"${NC}"
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}祝迁移顺利！${NC}"
echo -e "${GREEN}========================================${NC}"

