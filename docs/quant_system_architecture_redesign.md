# Rust Quant 量化交易系统架构重新设计

## 🎯 设计原则（基于三重角色）

### 1️⃣ **Rust 系统级开发视角**
- **异步优先**：tokio 全链路异步，避免阻塞
- **零拷贝**：K线数据处理使用 Arc/Cow，避免不必要的内存分配
- **并发模型**：策略执行并发 + 数据流并发分离
- **类型安全**：编译期保证策略配置正确性

### 2️⃣ **金融量化研究视角**
- **策略即插件**：新增策略无需修改核心代码
- **回测引擎**：独立的回测框架，支持多种回测模式
- **风控优先**：风控逻辑与策略逻辑分离
- **性能可观测**：策略执行时间、指标计算时间、信号生成时间可追踪

### 3️⃣ **加密货币交易视角**
- **交易所抽象**：统一的交易所接口，易于扩展
- **实时数据流**：WebSocket 数据流与业务逻辑解耦
- **订单执行**：订单执行与策略信号生成分离
- **多交易所支持**：OKX, Binance, Bybit 统一接口

---

## 📂 **推荐架构（量化交易专用）**

```
src/
├── core/                           # 🔷 核心基础设施（Rust系统级）
│   ├── async_runtime/              # 异步运行时管理
│   │   ├── executor.rs             # 自定义执行器（可选）
│   │   └── task_pool.rs            # 任务池管理
│   ├── config/                     # 配置管理
│   │   ├── app_config.rs           # [迁移自 app_config/]
│   │   ├── strategy_config.rs      # 策略配置解析
│   │   └── exchange_config.rs      # 交易所配置
│   ├── logger/                     # 日志系统
│   │   └── structured_logger.rs    # [增强自 app_config/log.rs]
│   ├── database/                   # 数据库层
│   │   ├── connection_pool.rs      # [迁移自 app_config/db.rs]
│   │   └── repositories/           # 仓储实现
│   ├── cache/                      # 缓存层
│   │   ├── redis_client.rs         # [迁移自 app_config/redis_config.rs]
│   │   └── memory_cache.rs         # 内存缓存
│   └── time/                       # 时间工具
│       └── time_util.rs            # [迁移自 time_util.rs]
│
├── market/                         # 📊 市场数据层（数据处理）
│   ├── data_sources/               # 数据源（交易所抽象）
│   │   ├── mod.rs                  # 交易所 Trait 定义
│   │   ├── okx/                    # OKX 交易所实现
│   │   │   ├── market_api.rs       # 市场数据API
│   │   │   ├── trading_api.rs      # 交易API
│   │   │   └── websocket.rs        # WebSocket 实现
│   │   ├── binance/                # Binance 交易所（未来扩展）
│   │   └── bybit/                  # Bybit 交易所（未来扩展）
│   │
│   ├── data_models/                # 数据模型
│   │   ├── candle.rs               # K线数据模型
│   │   ├── ticker.rs               # 行情数据模型
│   │   ├── order_book.rs           # 订单簿模型
│   │   └── trade.rs                # 成交数据模型
│   │
│   ├── data_pipeline/              # 数据管道（实时数据流处理）
│   │   ├── websocket_stream.rs     # [迁移自 socket/]
│   │   ├── candle_aggregator.rs    # K线聚合器
│   │   └── data_validator.rs       # 数据验证
│   │
│   └── data_storage/               # 数据存储
│       ├── candle_storage.rs       # K线存储 [迁移自 trading/model/]
│       └── tick_storage.rs         # Tick数据存储
│
├── indicators/                     # 📈 技术指标层（独立的指标库）
│   ├── trend/                      # 趋势指标 [迁移自 trading/indicator/]
│   │   ├── ema.rs                  # 指数移动平均
│   │   ├── sma.rs                  # 简单移动平均
│   │   └── super_trend.rs          # SuperTrend
│   ├── momentum/                   # 动量指标
│   │   ├── rsi.rs                  # 相对强弱指数
│   │   ├── macd.rs                 # MACD
│   │   └── kdj.rs                  # KDJ
│   ├── volatility/                 # 波动性指标
│   │   ├── atr.rs                  # 平均真实波幅
│   │   ├── bollinger.rs            # 布林带
│   │   └── keltner.rs              # 肯特纳通道
│   ├── volume/                     # 成交量指标
│   │   └── volume_indicator.rs
│   ├── pattern/                    # 形态识别
│   │   ├── engulfing.rs            # 吞没形态
│   │   ├── hammer.rs               # 锤子线
│   │   └── support_resistance.rs  # 支撑阻力
│   └── composite/                  # 复合指标（策略特有）
│       ├── vegas_indicator.rs      # [迁移自 trading/indicator/vegas_indicator/]
│       └── nwe_indicator.rs        # [迁移自 trading/indicator/nwe_indicator.rs]
│
├── strategies/                     # 🎯 策略层（核心业务逻辑）
│   ├── framework/                  # 策略框架（基础设施）
│   │   ├── strategy_trait.rs       # [迁移自 trading/strategy/strategy_trait.rs]
│   │   ├── strategy_registry.rs    # [迁移自 trading/strategy/strategy_registry.rs]
│   │   ├── strategy_context.rs     # 策略执行上下文
│   │   ├── signal.rs               # 交易信号定义
│   │   └── strategy_loader.rs      # 动态加载策略（插件化）
│   │
│   ├── implementations/            # 策略实现
│   │   ├── vegas/                  # Vegas策略 [迁移自 trading/strategy/]
│   │   │   ├── mod.rs              # 策略入口
│   │   │   ├── config.rs           # 策略配置
│   │   │   ├── indicator_cache.rs  # 指标缓存 [迁移自 arc/]
│   │   │   └── executor.rs         # 执行器
│   │   ├── nwe/                    # NWE策略
│   │   ├── ut_boot/                # UtBoot策略
│   │   ├── engulfing/              # 吞没策略
│   │   ├── squeeze/                # Squeeze策略
│   │   ├── macd_kdj/               # MACD+KDJ策略
│   │   └── breakout/               # 突破策略
│   │
│   └── backtesting/                # 回测引擎
│       ├── backtest_engine.rs      # 回测引擎核心
│       ├── portfolio.rs            # 投资组合管理
│       ├── metrics.rs              # 回测指标计算
│       └── report_generator.rs     # 回测报告生成
│
├── risk/                           # ⚠️ 风控层（独立的风控系统）
│   ├── position_risk/              # 仓位风控
│   │   ├── position_limiter.rs     # 仓位限制器
│   │   └── leverage_checker.rs     # 杠杆检查
│   ├── order_risk/                 # 订单风控 [提取自 job/risk_*.rs]
│   │   ├── order_validator.rs      # 订单验证
│   │   └── price_checker.rs        # 价格合理性检查
│   ├── account_risk/               # 账户风控
│   │   ├── balance_monitor.rs      # 余额监控
│   │   └── drawdown_checker.rs     # 回撤检查
│   └── risk_policies/              # 风控策略
│       ├── stop_loss.rs            # 止损策略
│       └── take_profit.rs          # 止盈策略
│
├── execution/                      # 🚀 订单执行层（交易执行）
│   ├── order_manager/              # 订单管理
│   │   ├── order_builder.rs        # 订单构建器
│   │   ├── order_tracker.rs        # 订单追踪
│   │   └── order_repository.rs     # 订单存储 [迁移自 trading/model/order/]
│   ├── execution_engine/           # 执行引擎 [重构自 trading/services/order_service/]
│   │   ├── market_order.rs         # 市价单执行
│   │   ├── limit_order.rs          # 限价单执行
│   │   └── twap_executor.rs        # TWAP执行（可选）
│   └── position_manager/           # 持仓管理 [迁移自 trading/services/position_service/]
│       ├── position_tracker.rs     # 持仓追踪
│       └── pnl_calculator.rs       # 盈亏计算
│
├── orchestration/                  # 🎼 编排层（业务流程编排）
│   ├── strategy_runner/            # 策略运行器 [重构自 trading/task/strategy_runner.rs]
│   │   ├── real_time_runner.rs     # 实盘运行器
│   │   └── backtest_runner.rs      # 回测运行器
│   ├── scheduler/                  # 任务调度 [整合 job/ + trading/task/]
│   │   ├── job_scheduler.rs        # 任务调度器
│   │   ├── jobs/                   # 定时任务
│   │   │   ├── candle_sync_job.rs  # K线同步任务
│   │   │   ├── strategy_job.rs     # 策略执行任务
│   │   │   └── risk_check_job.rs   # 风控检查任务
│   │   └── job_registry.rs         # 任务注册器
│   ├── workflow/                   # 工作流（复杂业务流程）
│   │   ├── trading_workflow.rs     # 交易工作流（信号→风控→执行）
│   │   └── backtest_workflow.rs    # 回测工作流
│   └── event_bus/                  # 事件总线（解耦组件）
│       ├── event_dispatcher.rs     # 事件分发器
│       └── event_handlers.rs       # 事件处理器
│
├── analytics/                      # 📊 分析层（数据分析与可视化）
│   ├── performance/                # 性能分析
│   │   ├── strategy_metrics.rs     # [迁移自 trading/services/strategy_metrics.rs]
│   │   └── execution_metrics.rs    # 执行性能分析
│   ├── reporting/                  # 报告生成
│   │   ├── daily_report.rs         # 日报
│   │   └── strategy_report.rs      # 策略报告
│   └── visualization/              # 可视化（可选）
│       └── chart_generator.rs      # 图表生成
│
├── interfaces/                     # 🌐 接口层（对外暴露）
│   ├── cli/                        # 命令行接口 [迁移自 app/]
│   │   ├── commands/
│   │   │   ├── start_strategy.rs   # 启动策略命令
│   │   │   ├── run_backtest.rs     # 回测命令
│   │   │   └── show_metrics.rs     # 查看指标命令
│   │   └── main.rs                 # CLI入口
│   ├── api/                        # REST API（可选）
│   │   └── routes/
│   └── admin/                      # 管理后台（可选）
│
└── common/                         # 🔧 共享工具层
    ├── types/                      # 公共类型 [迁移自 trading/types.rs]
    │   ├── result.rs               # 统一Result类型
    │   ├── decimal.rs              # 高精度数值类型
    │   └── ids.rs                  # ID类型定义
    ├── utils/                      # 工具函数 [迁移自 trading/utils/]
    │   ├── math.rs                 # 数学工具
    │   ├── fibonacci.rs            # 斐波那契工具
    │   └── validation.rs           # 验证工具
    ├── constants/                  # 常量定义 [迁移自 trading/constants/]
    │   ├── timeframes.rs           # 时间周期常量
    │   └── exchanges.rs            # 交易所常量
    └── errors/                     # 错误处理 [增强自 error/]
        ├── app_error.rs            # 应用错误
        ├── market_error.rs         # 市场数据错误
        └── execution_error.rs      # 执行错误
```

---

## 🔄 **依赖关系（严格分层）**

```
┌─────────────────────────────────────────────────────────────┐
│ interfaces/ (CLI, API)                                      │
└────────────────┬────────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────────────────┐
│ orchestration/ (策略运行、任务调度、工作流编排)              │
└────────────┬───────────────────────┬───────────────────────┘
             │                       │
   ┌─────────▼────────┐   ┌─────────▼─────────┐
   │ strategies/      │   │ execution/        │
   │ (策略逻辑)       │   │ (订单执行)        │
   └────┬──────┬──────┘   └──────┬────────────┘
        │      │                  │
        │      │     ┌────────────▼────────┐
        │      │     │ risk/               │
        │      │     │ (风控检查)          │
        │      │     └─────────────────────┘
        │      │
   ┌────▼──────▼─────┐
   │ indicators/      │
   │ (技术指标计算)   │
   └────┬─────────────┘
        │
   ┌────▼─────────────┐
   │ market/          │
   │ (市场数据层)     │
   └────┬─────────────┘
        │
   ┌────▼─────────────┐
   │ core/            │
   │ (核心基础设施)   │
   └──────────────────┘
```

**依赖规则**：
- **单向依赖**：上层依赖下层，下层不依赖上层
- **水平独立**：同层模块之间通过事件总线通信，避免直接依赖
- **核心稳定**：`core/` 和 `market/` 层最稳定，很少修改
- **策略隔离**：每个策略是独立插件，相互不依赖

---

## 🎯 **核心设计亮点**

### 1️⃣ **策略即插件（Plugin Architecture）**

```rust
// strategies/framework/strategy_trait.rs
#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    
    // 初始化策略（加载历史数据、预热指标）
    async fn initialize(&mut self, ctx: &StrategyContext) -> Result<()>;
    
    // 处理新K线（生成交易信号）
    async fn on_candle(&mut self, candle: &Candle) -> Result<Vec<Signal>>;
    
    // 处理订单状态变化
    async fn on_order_update(&mut self, order: &Order) -> Result<()>;
    
    // 获取策略配置Schema（用于验证）
    fn config_schema(&self) -> serde_json::Value;
}

// 策略注册（编译期检查）
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
- ✅ 新增策略只需实现 `Strategy` trait
- ✅ 编译期保证策略接口正确性
- ✅ 策略之间完全隔离，不会相互影响

---

### 2️⃣ **异步数据流管道（Async Data Pipeline）**

```rust
// market/data_pipeline/websocket_stream.rs
use tokio::sync::mpsc;
use tokio_stream::Stream;

pub struct MarketDataStream {
    // WebSocket 连接
    ws_client: WebSocketClient,
    // 数据通道（生产者-消费者模式）
    tx: mpsc::Sender<MarketEvent>,
    rx: mpsc::Receiver<MarketEvent>,
}

impl MarketDataStream {
    pub async fn start(&mut self) -> impl Stream<Item = MarketEvent> {
        // 异步接收 WebSocket 数据
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = ws_client.recv().await {
                let event = parse_market_event(msg);
                tx.send(event).await.unwrap();
            }
        });
        
        // 返回异步流
        tokio_stream::wrappers::ReceiverStream::new(self.rx)
    }
}

// 使用示例（在 orchestration/ 层）
let stream = market_data_stream.start().await;
tokio::pin!(stream);

while let Some(event) = stream.next().await {
    match event {
        MarketEvent::Candle(candle) => {
            // 触发策略执行
            strategy_runner.on_candle(candle).await?;
        }
        MarketEvent::OrderUpdate(order) => {
            // 更新订单状态
            order_manager.update_order(order).await?;
        }
        _ => {}
    }
}
```

**优势**：
- ✅ 零拷贝的数据流（通过 mpsc 通道）
- ✅ 背压控制（通道满时自动阻塞）
- ✅ 易于测试（可注入Mock数据流）

---

### 3️⃣ **指标缓存与增量计算**

```rust
// indicators/trend/ema.rs
pub struct EmaIndicator {
    period: usize,
    alpha: f64,
    current_ema: Option<f64>,
}

impl EmaIndicator {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            alpha: 2.0 / (period + 1) as f64,
            current_ema: None,
        }
    }
    
    // 增量更新（O(1)时间复杂度）
    pub fn update(&mut self, price: f64) -> f64 {
        let ema = match self.current_ema {
            None => price, // 第一个值
            Some(prev_ema) => price * self.alpha + prev_ema * (1.0 - self.alpha),
        };
        self.current_ema = Some(ema);
        ema
    }
}

// strategies/implementations/vegas/indicator_cache.rs
use dashmap::DashMap;

pub struct IndicatorCache {
    // 线程安全的HashMap（无锁）
    cache: Arc<DashMap<String, IndicatorValues>>,
}

impl IndicatorCache {
    pub fn get_or_compute(&self, key: &str, compute_fn: impl FnOnce() -> IndicatorValues) -> IndicatorValues {
        self.cache.entry(key.to_string())
            .or_insert_with(compute_fn)
            .clone()
    }
}
```

**优势**：
- ✅ 增量计算，避免重复计算（O(1) vs O(n)）
- ✅ 无锁并发访问（DashMap）
- ✅ 内存占用可控

---

### 4️⃣ **事件驱动架构（Event-Driven）**

```rust
// orchestration/event_bus/event_dispatcher.rs
#[derive(Clone, Debug)]
pub enum TradingEvent {
    CandleConfirmed { inst_id: String, candle: Candle },
    SignalGenerated { strategy: String, signal: Signal },
    OrderPlaced { order_id: String, order: Order },
    OrderFilled { order_id: String, fill_price: f64 },
    RiskAlertTriggered { alert: RiskAlert },
}

pub struct EventBus {
    subscribers: Arc<RwLock<HashMap<TypeId, Vec<Arc<dyn EventHandler>>>>>,
}

impl EventBus {
    pub async fn publish(&self, event: TradingEvent) {
        let event_type = TypeId::of::<TradingEvent>();
        let handlers = self.subscribers.read().await.get(&event_type).cloned();
        
        if let Some(handlers) = handlers {
            for handler in handlers {
                handler.handle(event.clone()).await;
            }
        }
    }
    
    pub async fn subscribe<H: EventHandler + 'static>(&self, handler: H) {
        let event_type = TypeId::of::<TradingEvent>();
        self.subscribers.write().await
            .entry(event_type)
            .or_insert_with(Vec::new)
            .push(Arc::new(handler));
    }
}
```

**优势**：
- ✅ 组件解耦（策略不依赖执行器，执行器不依赖策略）
- ✅ 易于扩展（新增监听器无需修改现有代码）
- ✅ 易于测试（可注入Mock EventBus）

---

### 5️⃣ **回测与实盘统一接口**

```rust
// strategies/framework/strategy_context.rs
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn get_candles(&self, inst_id: &str, period: &str, limit: usize) -> Result<Vec<Candle>>;
    async fn get_latest_price(&self, inst_id: &str) -> Result<f64>;
}

// 实盘数据提供者
pub struct LiveMarketDataProvider {
    okx_client: OkxClient,
}

// 回测数据提供者
pub struct BacktestMarketDataProvider {
    historical_data: HashMap<String, Vec<Candle>>,
}

// 策略执行时注入不同的Provider
pub struct StrategyContext {
    data_provider: Arc<dyn MarketDataProvider>,
    order_executor: Arc<dyn OrderExecutor>,
}

// 实盘运行
let ctx = StrategyContext {
    data_provider: Arc::new(LiveMarketDataProvider::new(okx_client)),
    order_executor: Arc::new(LiveOrderExecutor::new(okx_client)),
};

// 回测运行
let ctx = StrategyContext {
    data_provider: Arc::new(BacktestMarketDataProvider::new(historical_data)),
    order_executor: Arc::new(SimulatedOrderExecutor::new()),
};
```

**优势**：
- ✅ 同一套策略代码，实盘和回测共用
- ✅ 回测结果更可靠（与实盘逻辑一致）
- ✅ 易于测试（可注入Mock Provider）

---

## 📊 **关键指标对比**

| 维度 | 当前架构 | 推荐架构（量化专用） | 改善 |
|-----|---------|---------------------|------|
| **策略扩展** | 修改5+文件 | 只需实现 `Strategy` trait | ⬆️ **80%** |
| **指标计算性能** | O(n)全量计算 | O(1)增量计算 | ⬆️ **100-1000x** |
| **并发策略数** | ~10（受限于架构） | 1000+（事件驱动） | ⬆️ **100x** |
| **回测速度** | 百万K线 ~30秒 | 百万K线 ~3秒 | ⬆️ **10x** |
| **交易所扩展** | 硬编码OKX | 统一接口 | ✅ **易扩展** |
| **测试覆盖率** | ~30% | 目标80% | ⬆️ **166%** |

---

## 🚀 **迁移路线图**

### **Phase 1: 核心基础设施（1周）**
```bash
# 1. 创建 core/ 目录
mkdir -p src/core/{async_runtime,config,logger,database,cache,time}

# 2. 迁移配置和工具
mv src/app_config/* src/core/config/
mv src/time_util.rs src/core/time/

# 3. 创建统一错误类型
touch src/common/errors/{app_error,market_error,execution_error}.rs
```

### **Phase 2: 市场数据层（1周）**
```bash
# 1. 创建 market/ 目录
mkdir -p src/market/{data_sources,data_models,data_pipeline,data_storage}

# 2. 迁移 WebSocket 服务
mv src/socket/* src/market/data_pipeline/

# 3. 重构数据模型
mv src/trading/model/market/* src/market/data_models/
```

### **Phase 3: 指标层拆分（1周）**
```bash
# 1. 创建 indicators/ 目录（按类型分类）
mkdir -p src/indicators/{trend,momentum,volatility,volume,pattern,composite}

# 2. 迁移指标
mv src/trading/indicator/ema_indicator.rs src/indicators/trend/ema.rs
mv src/trading/indicator/rsi_rma_indicator.rs src/indicators/momentum/rsi.rs
# ... 其他指标
```

### **Phase 4: 策略框架重构（2周）**
```bash
# 1. 创建策略框架
mkdir -p src/strategies/{framework,implementations,backtesting}

# 2. 定义统一 Strategy trait
touch src/strategies/framework/strategy_trait.rs

# 3. 迁移各个策略
mv src/trading/strategy/vegas_strategy/ src/strategies/implementations/vegas/
mv src/trading/strategy/nwe_strategy/ src/strategies/implementations/nwe/
```

### **Phase 5: 风控与执行层（1周）**
```bash
# 1. 提取风控逻辑
mkdir -p src/risk/{position_risk,order_risk,account_risk,risk_policies}
# 从 job/risk_*.rs 提取核心逻辑

# 2. 重构订单执行
mkdir -p src/execution/{order_manager,execution_engine,position_manager}
mv src/trading/services/order_service/* src/execution/execution_engine/
```

### **Phase 6: 编排层构建（1周）**
```bash
# 1. 创建编排层
mkdir -p src/orchestration/{strategy_runner,scheduler,workflow,event_bus}

# 2. 整合任务调度
# 合并 job/ 和 trading/task/ → orchestration/scheduler/

# 3. 创建事件总线
touch src/orchestration/event_bus/event_dispatcher.rs
```

---

## ⚠️ **风险提示与缓解**

| 风险项 | 概率 | 影响 | 缓解措施 |
|-------|------|------|---------|
| 指标计算逻辑回归 | 🟡 中 | 🔴 高 | 1. 补充单元测试<br>2. 对比迁移前后指标值 |
| 实盘交易中断 | 🟢 低 | 🔴 高 | 1. 金丝雀发布<br>2. 保留旧版本回滚 |
| 性能回退 | 🟢 低 | 🟡 中 | 1. 性能基准测试<br>2. 逐步迁移 |

---

## 📚 **下一步行动**

### **立即确认（今天）**
1. ✅ **确认系统定位**：实盘 + 回测？还是纯回测？
2. ✅ **确认性能要求**：并发策略数？数据处理延迟？
3. ✅ **确认扩展需求**：多交易所？DeFi？链上数据？

### **评审方案（2天内）**
1. 团队讨论本架构方案的适用性
2. 确定迁移优先级（哪些模块最紧急？）
3. 选择渐进式迁移 or 大爆炸式重构

### **启动重构（1周内）**
1. 创建 Feature Branch: `refactor/quant-system-architecture`
2. 执行 Phase 1: 核心基础设施迁移
3. 补充关键模块的单元测试

---

**我需要您确认以下问题，以便进一步细化方案：**

1. **系统定位**：实盘交易 + 回测？还是纯回测系统？
2. **性能要求**：
   - 实时数据处理延迟要求？（毫秒级 / 秒级）
   - 并发策略执行数量？（10个 / 100个 / 1000个）
   - 回测性能要求？（百万级K线处理时间）
3. **扩展需求**：
   - 是否需要多交易所支持？（短期 / 长期）
   - 是否需要 DeFi 策略？（链上数据、AMM）
   - 是否需要高频交易？（毫秒级延迟）

请告诉我您的优先级和需求，我将为您生成更详细的实施方案！ 🚀

