# Rust Quant 拆包与服务化策略

## 🎯 决策矩阵：拆包 vs 拆服务

### **评估维度**

| 组件 | 延迟要求 | 计算密集度 | 独立性 | 拆包 | 拆服务 | 推荐方案 |
|-----|---------|-----------|-------|------|--------|---------|
| **市场数据** | 🔴 极低（<10ms） | 🟢 低 | 🟡 中 | ✅ | ⚠️ | 📦 **拆包** |
| **技术指标** | 🔴 极低（<5ms） | 🔴 高 | 🟢 高 | ✅ | ❌ | 📦 **拆包** |
| **策略引擎** | 🔴 极低（<10ms） | 🔴 高 | 🟢 高 | ✅ | ❌ | 📦 **拆包** |
| **风控检查** | 🔴 极低（<5ms） | 🟢 低 | 🟡 中 | ✅ | ❌ | 📦 **拆包** |
| **订单执行** | 🔴 极低（<10ms） | 🟢 低 | 🟢 高 | ✅ | ⚠️ | 📦 **拆包** |
| **回测引擎** | 🟢 宽松（秒级） | 🔴 高 | ✅ 高 | ✅ | ✅ | 🚀 **可拆服务** |
| **数据采集** | 🟡 中等（秒级） | 🟢 低 | ✅ 高 | ✅ | ✅ | 🚀 **可拆服务** |
| **分析报告** | 🟢 宽松（分钟级） | 🟡 中 | ✅ 高 | ✅ | ✅ | 🚀 **可拆服务** |

**判断规则**：
- 🔴 **延迟敏感** + 🔴 **计算密集** → 必须在同一进程（拆包）
- 🟢 **延迟宽松** + ✅ **高独立性** → 可以拆服务
- ⚠️ **边界情况** → 先拆包，未来按需拆服务

---

## 📦 **方案一：Cargo Workspace 拆包（推荐优先实施）**

### **目录结构**

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "crates/core",
    "crates/market",
    "crates/indicators",
    "crates/strategies",
    "crates/risk",
    "crates/execution",
    "crates/orchestration",
    "crates/analytics",
    "crates/common",
    # 主程序
    "rust-quant-cli",
]

[workspace.package]
version = "0.2.0"
edition = "2021"
rust-version = "1.75.0"

[workspace.dependencies]
# 共享依赖版本管理
tokio = { version = "1.37", features = ["rt-multi-thread", "macros", "full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
# ... 其他依赖
```

### **包划分详细设计**

#### 🔷 **crates/core** - 核心基础设施包
```toml
[package]
name = "rust-quant-core"
version.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
tracing = "0.1"
rbatis = "4.5"
redis = { version = "0.25", features = ["tokio-comp"] }
```

**职责**：
- 配置管理（Config）
- 日志系统（Logger）
- 数据库连接池（Database）
- Redis客户端（Cache）
- 时间工具（Time Utils）
- 错误类型定义（Errors）

**导出接口**：
```rust
// crates/core/src/lib.rs
pub mod config;
pub mod database;
pub mod cache;
pub mod logger;
pub mod time;
pub mod errors;

// 重新导出常用类型
pub use config::AppConfig;
pub use database::DbPool;
pub use cache::RedisClient;
pub use errors::{Result, AppError};
```

---

#### 📊 **crates/market** - 市场数据包
```toml
[package]
name = "rust-quant-market"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-common = { path = "../common" }
okx = "0.1.9"
tokio.workspace = true
tokio-tungstenite = "0.23"
```

**职责**：
- 交易所抽象（Exchange Trait）
- OKX 实现（OkxExchange）
- WebSocket 数据流（MarketDataStream）
- K线数据模型（Candle）
- 数据持久化（CandleRepository）

**导出接口**：
```rust
// crates/market/src/lib.rs
pub mod exchanges;      // 交易所抽象
pub mod models;         // 数据模型
pub mod streams;        // 数据流
pub mod repositories;   // 持久化

// 关键类型导出
pub use exchanges::{Exchange, OkxExchange};
pub use models::{Candle, Ticker, OrderBook};
pub use streams::MarketDataStream;
```

**优势**：
- ✅ 独立测试（Mock 交易所）
- ✅ 未来支持多交易所（只需实现 `Exchange` trait）
- ✅ 编译隔离（修改市场数据不影响策略编译）

---

#### 📈 **crates/indicators** - 技术指标包
```toml
[package]
name = "rust-quant-indicators"
version.workspace = true

[dependencies]
rust-quant-common = { path = "../common" }
ta = "0.5"  # 可选：复用现有TA库
serde.workspace = true
```

**职责**：
- 趋势指标（EMA, SMA, SuperTrend）
- 动量指标（RSI, MACD, KDJ）
- 波动性指标（ATR, Bollinger）
- 成交量指标（Volume Ratio）
- 形态识别（Engulfing, Hammer）

**导出接口**：
```rust
// crates/indicators/src/lib.rs
pub mod trend;      // EMA, SMA, SuperTrend
pub mod momentum;   // RSI, MACD, KDJ
pub mod volatility; // ATR, Bollinger
pub mod volume;     // Volume indicators
pub mod pattern;    // Pattern recognition

// 统一指标接口
pub trait Indicator {
    type Input;
    type Output;
    
    fn update(&mut self, input: Self::Input) -> Self::Output;
    fn reset(&mut self);
}

// 示例：EMA 指标
pub struct Ema {
    period: usize,
    alpha: f64,
    current: Option<f64>,
}

impl Indicator for Ema {
    type Input = f64;
    type Output = f64;
    
    fn update(&mut self, price: f64) -> f64 {
        let ema = match self.current {
            None => price,
            Some(prev) => price * self.alpha + prev * (1.0 - self.alpha),
        };
        self.current = Some(ema);
        ema
    }
    
    fn reset(&mut self) {
        self.current = None;
    }
}
```

**优势**：
- ✅ 纯计算逻辑，无外部依赖
- ✅ 可独立进行单元测试和性能测试
- ✅ 可发布到 crates.io（开源贡献）
- ✅ 未来可集成机器学习模型

---

#### 🎯 **crates/strategies** - 策略引擎包
```toml
[package]
name = "rust-quant-strategies"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-market = { path = "../market" }
rust-quant-indicators = { path = "../indicators" }
rust-quant-common = { path = "../common" }
async-trait = "0.1"
```

**职责**：
- 策略框架（Strategy Trait）
- 策略注册器（StrategyRegistry）
- 策略上下文（StrategyContext）
- 具体策略实现（Vegas, NWE, UtBoot等）
- 回测引擎（BacktestEngine）

**导出接口**：
```rust
// crates/strategies/src/lib.rs
pub mod framework;       // 策略框架
pub mod implementations; // 具体策略
pub mod backtesting;     // 回测引擎

// 核心 Trait
#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    async fn initialize(&mut self, ctx: &StrategyContext) -> Result<()>;
    async fn on_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>>;
}

// 策略注册器
pub struct StrategyRegistry {
    strategies: HashMap<String, Box<dyn Strategy>>,
}

impl StrategyRegistry {
    pub fn register<S: Strategy + 'static>(&mut self, strategy: S) {
        self.strategies.insert(strategy.name().to_string(), Box::new(strategy));
    }
}
```

**优势**：
- ✅ 策略即插件（新增策略无需修改核心代码）
- ✅ 策略之间完全隔离
- ✅ 回测与实盘代码共用

---

#### ⚠️ **crates/risk** - 风控引擎包
```toml
[package]
name = "rust-quant-risk"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-market = { path = "../market" }
rust-quant-common = { path = "../common" }
```

**职责**：
- 仓位风控（PositionLimiter）
- 订单风控（OrderValidator）
- 账户风控（BalanceMonitor）
- 止损止盈（StopLoss/TakeProfit）

**导出接口**：
```rust
// crates/risk/src/lib.rs
pub mod position;
pub mod order;
pub mod account;
pub mod policies;

// 风控检查接口
#[async_trait]
pub trait RiskChecker: Send + Sync {
    async fn check(&self, order: &Order) -> Result<RiskCheckResult>;
}

pub struct RiskCheckResult {
    pub passed: bool,
    pub reason: Option<String>,
}
```

---

#### 🚀 **crates/execution** - 订单执行包
```toml
[package]
name = "rust-quant-execution"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-market = { path = "../market" }
rust-quant-risk = { path = "../risk" }
rust-quant-common = { path = "../common" }
```

**职责**：
- 订单管理（OrderManager）
- 订单执行（OrderExecutor）
- 持仓管理（PositionManager）
- 盈亏计算（PnLCalculator）

---

#### 🎼 **crates/orchestration** - 编排引擎包
```toml
[package]
name = "rust-quant-orchestration"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-market = { path = "../market" }
rust-quant-strategies = { path = "../strategies" }
rust-quant-risk = { path = "../risk" }
rust-quant-execution = { path = "../execution" }
tokio-cron-scheduler = "0.10"
```

**职责**：
- 策略运行器（StrategyRunner）
- 任务调度器（JobScheduler）
- 工作流编排（TradingWorkflow）
- 事件总线（EventBus）

---

#### 📊 **crates/analytics** - 分析引擎包
```toml
[package]
name = "rust-quant-analytics"
version.workspace = true

[dependencies]
rust-quant-core = { path = "../core" }
rust-quant-strategies = { path = "../strategies" }
polars = "0.33"  # 数据分析库
```

**职责**：
- 性能分析（PerformanceMetrics）
- 报告生成（ReportGenerator）
- 可视化（ChartGenerator）

---

#### 🔧 **crates/common** - 共享工具包
```toml
[package]
name = "rust-quant-common"
version.workspace = true

[dependencies]
serde.workspace = true
chrono = "0.4"
```

**职责**：
- 公共类型（Types）
- 工具函数（Utils）
- 常量定义（Constants）

---

### **包依赖关系图**

```
                   ┌─────────────┐
                   │   common    │
                   └──────┬──────┘
                          │
                   ┌──────▼──────┐
                   │    core     │
                   └──────┬──────┘
                          │
         ┌────────────────┼────────────────┐
         │                │                │
    ┌────▼────┐      ┌───▼────┐     ┌────▼─────┐
    │ market  │      │indicators│     │   risk   │
    └────┬────┘      └───┬────┘     └────┬─────┘
         │               │                │
         └───────┬───────┴────────────────┘
                 │
          ┌──────▼──────┐
          │ strategies  │
          └──────┬──────┘
                 │
          ┌──────▼──────┐
          │ execution   │
          └──────┬──────┘
                 │
          ┌──────▼──────────┐
          │ orchestration   │
          └──────┬──────────┘
                 │
          ┌──────▼──────┐
          │ analytics   │
          └─────────────┘
```

**依赖规则**：
- ✅ 单向依赖（上层依赖下层）
- ✅ 同层独立（strategies 不依赖 risk）
- ✅ 通过 orchestration 协调

---

## 🚀 **方案二：选择性服务化（长期规划）**

### **哪些可以拆成独立服务？**

#### 1️⃣ **数据采集服务（Data Collector Service）**

**理由**：
- ✅ 独立性高（只负责数据采集）
- ✅ 延迟要求宽松（秒级即可）
- ✅ 可水平扩展（多实例采集不同交易所）
- ✅ 故障隔离（采集失败不影响交易）

**技术栈**：
- Rust + Tokio（异步采集）
- WebSocket 长连接
- 数据写入 MySQL/TimescaleDB
- Redis 缓存最新数据

**通信方式**：
```rust
// 通过 Redis Pub/Sub 推送实时数据
pub async fn publish_candle(&self, candle: &Candle) -> Result<()> {
    let channel = format!("market:candle:{}", candle.inst_id);
    self.redis_client.publish(&channel, serde_json::to_string(candle)?).await?;
    Ok(())
}
```

---

#### 2️⃣ **回测服务（Backtest Service）**

**理由**：
- ✅ 独立性极高（不影响实盘交易）
- ✅ 计算密集（可独立扩展CPU资源）
- ✅ 延迟要求宽松（分钟级结果）
- ✅ 可并行执行多个回测任务

**技术栈**：
- Rust + Rayon（并行计算）
- gRPC API（接收回测任务）
- PostgreSQL（存储回测结果）

**通信方式**：
```protobuf
// backtest.proto
service BacktestService {
    rpc RunBacktest(BacktestRequest) returns (BacktestResult);
    rpc GetBacktestStatus(BacktestId) returns (BacktestStatus);
}

message BacktestRequest {
    string strategy_name = 1;
    string inst_id = 2;
    int64 start_time = 3;
    int64 end_time = 4;
    string config_json = 5;
}
```

---

#### 3️⃣ **分析报告服务（Analytics Service）**

**理由**：
- ✅ 独立性高（只做数据分析）
- ✅ 延迟要求宽松（分钟级）
- ✅ 可使用 Python 生态（Pandas, Matplotlib）

**技术栈**：
- Python + FastAPI
- Pandas + Plotly（数据分析与可视化）
- 读取 PostgreSQL 数据

**通信方式**：
```python
# 通过 REST API 提供分析结果
@app.get("/api/v1/strategy/{strategy_id}/report")
async def get_strategy_report(strategy_id: str):
    report = await generate_report(strategy_id)
    return report
```

---

### **核心交易逻辑保持单体（不拆服务）**

**必须在同一进程的组件**：
- 🔴 **市场数据接收** - WebSocket 连接需要稳定
- 🔴 **技术指标计算** - 需要毫秒级响应
- 🔴 **策略信号生成** - 需要毫秒级响应
- 🔴 **风控检查** - 需要同步检查，避免网络延迟
- 🔴 **订单执行** - 需要极低延迟

**理由**：
- ⚠️ 网络延迟（gRPC ~1-5ms，不可接受）
- ⚠️ 序列化开销（Protobuf 编解码耗时）
- ⚠️ 故障传播（一个服务挂掉影响整体）
- ⚠️ 部署复杂度（多服务协调困难）

---

## 📐 **推荐实施路径（分阶段）**

### **阶段一：拆包（1-2个月）** ⭐ **优先执行**

```bash
# 1. 创建 Workspace 结构
mkdir -p crates/{core,market,indicators,strategies,risk,execution,orchestration,analytics,common}

# 2. 迁移代码到各个包
# 先迁移无依赖的包（common, core）
# 再迁移有依赖的包（market, indicators）
# 最后迁移编排层（orchestration）

# 3. 更新依赖关系
# 每个包的 Cargo.toml 指定依赖

# 4. 编译验证
cargo build --workspace

# 5. 运行测试
cargo test --workspace
```

**收益**：
- ✅ 编译时间减少（增量编译）
- ✅ 代码隔离清晰（职责明确）
- ✅ 测试独立运行（快速反馈）
- ✅ 未来易于拆服务

---

### **阶段二：优化性能（0.5-1个月）** 

```bash
# 1. 性能基准测试
cargo bench --workspace

# 2. 优化热点代码
# - 指标计算增量化
# - 数据流零拷贝
# - 异步任务并发

# 3. 性能监控
# - 添加 tracing 埋点
# - 集成 Prometheus
```

**目标**：
- ✅ 指标计算延迟 < 5ms
- ✅ 策略信号生成 < 10ms
- ✅ 订单执行延迟 < 20ms

---

### **阶段三：选择性服务化（3-6个月，可选）**

```bash
# 1. 拆分数据采集服务
# - 独立 Rust 项目
# - WebSocket 数据采集
# - Redis Pub/Sub 推送

# 2. 拆分回测服务
# - gRPC API
# - 并行回测引擎
# - 结果持久化

# 3. 拆分分析服务
# - Python FastAPI
# - Pandas 数据分析
# - Plotly 可视化
```

**条件**：
- ⚠️ 只有在单体性能达标后才考虑拆服务
- ⚠️ 核心交易逻辑永远保持单体

---

## 🎯 **最终建议**

### **短期（1-2个月）：Cargo Workspace 拆包**

```toml
# 项目结构
rust-quant/
├── Cargo.toml (workspace)
├── crates/
│   ├── core/
│   ├── market/
│   ├── indicators/
│   ├── strategies/
│   ├── risk/
│   ├── execution/
│   ├── orchestration/
│   ├── analytics/
│   └── common/
├── rust-quant-cli/  (主程序)
└── services/        (未来的服务)
    ├── data-collector/
    ├── backtest/
    └── analytics/
```

**优势**：
- ✅ 保持低延迟（同一进程）
- ✅ 编译隔离（模块独立编译）
- ✅ 测试友好（包级别测试）
- ✅ 未来易拆服务（清晰的边界）

### **长期（6个月+）：选择性服务化**

- ✅ 数据采集服务（独立部署）
- ✅ 回测服务（独立部署）
- ✅ 分析服务（Python 生态）
- 🔴 核心交易保持单体（性能优先）

---

## 📊 **性能对比**

| 架构 | 延迟 | 吞吐量 | 可维护性 | 可扩展性 |
|-----|------|-------|---------|---------|
| **单体（当前）** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ |
| **拆包（推荐）** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ |
| **拆服务（长期）** | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |

---

## 🚀 **下一步行动**

### **立即执行（本周）**

```bash
# 1. 创建 Workspace 结构
cd rust-quant
mkdir -p crates/{core,market,indicators,strategies,risk,execution,orchestration,analytics,common}

# 2. 编写根 Cargo.toml
cat > Cargo.toml << 'EOF'
[workspace]
members = [
    "crates/core",
    "crates/market",
    "crates/indicators",
    "crates/strategies",
    "crates/risk",
    "crates/execution",
    "crates/orchestration",
    "crates/analytics",
    "crates/common",
    "rust-quant-cli",
]

[workspace.package]
version = "0.2.0"
edition = "2021"

[workspace.dependencies]
tokio = { version = "1.37", features = ["rt-multi-thread", "macros", "full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
EOF

# 3. 为每个包创建 Cargo.toml
for crate in core market indicators strategies risk execution orchestration analytics common; do
    mkdir -p crates/$crate/src
    cat > crates/$crate/Cargo.toml << EOF
[package]
name = "rust-quant-$crate"
version.workspace = true
edition.workspace = true

[dependencies]
EOF
    echo "pub fn hello() {}" > crates/$crate/src/lib.rs
done

# 4. 验证编译
cargo build --workspace
```

---

**您觉得这个方案如何？我可以为您生成：**
1. ✅ 详细的包迁移脚本
2. ✅ 每个包的 Cargo.toml 配置
3. ✅ 包之间的接口定义示例
4. ✅ Workspace 最佳实践指南

需要我继续深化哪个部分？ 🚀

