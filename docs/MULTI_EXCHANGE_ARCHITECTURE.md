# 多交易所架构设计

## 当前架构问题

当前 services 层直接依赖 `okx` SDK：

```rust
// ❌ 问题：直接依赖具体交易所实现
use okx::api::market::OkxMarket;
use okx::dto::market_dto::TickerOkxResDto;

pub struct TickerService;
impl TickerService {
    pub async fn fetch_ticker(&self, inst_id: &str) -> Result<TickerOkxResDto> {
        OkxMarket::from_env()?.get_ticker(inst_id).await  // 硬编码okx
    }
}
```

**缺点**：
- 难以扩展到其他交易所（Binance、Bybit等）
- 修改交易所需要修改 services 层代码
- 无法支持多交易所同时运行（套利场景）

---

## 多交易所架构方案

### 架构设计（依赖倒置原则）

```
┌─────────────────────────────────────────────────────┐
│  Orchestration 层（任务编排）                        │
└──────────────────┬──────────────────────────────────┘
                   │ 调用
┌─────────────────▼──────────────────────────────────┐
│  Services 层（业务逻辑）                             │
│  依赖 domain::traits::ExchangeMarketData (接口)     │
└──────────────────┬──────────────────────────────────┘
                   │ 依赖接口
┌─────────────────▼──────────────────────────────────┐
│  Domain 层（领域模型 + 接口定义）                    │
│  - traits::ExchangeMarketData                       │
│  - entities::TickerData (统一数据模型)              │
└─────────────────────────────────────────────────────┘
                   ▲ 实现接口
┌─────────────────┴──────────────────────────────────┐
│  Infrastructure 层（具体实现）                       │
│  - exchanges::OkxMarketDataAdapter                  │
│  - exchanges::BinanceMarketDataAdapter              │
│  - exchanges::BybitMarketDataAdapter                │
└─────────────────────────────────────────────────────┘
```

---

## 实施步骤

### 步骤1：Domain层定义交易所接口

**文件**: `crates/domain/src/traits/exchange_trait.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;

use crate::entities::{TickerData, CandleData, AccountBalance};

/// 交易所市场数据接口
#[async_trait]
pub trait ExchangeMarketData: Send + Sync {
    /// 获取单个Ticker
    async fn get_ticker(&self, symbol: &str) -> Result<TickerData>;
    
    /// 获取批量Ticker
    async fn get_tickers(&self, inst_type: &str) -> Result<Vec<TickerData>>;
    
    /// 获取K线数据
    async fn get_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<CandleData>>;
    
    /// 获取交易所名称
    fn name(&self) -> &str;
}

/// 交易所账户接口
#[async_trait]
pub trait ExchangeAccount: Send + Sync {
    /// 获取账户余额
    async fn get_balance(&self, currency: Option<&str>) -> Result<Vec<AccountBalance>>;
    
    /// 获取持仓信息
    async fn get_positions(&self, inst_type: Option<&str>) -> Result<Vec<Position>>;
}

/// 交易所交易接口
#[async_trait]
pub trait ExchangeTrading: Send + Sync {
    /// 下单
    async fn place_order(&self, order: &OrderRequest) -> Result<OrderResponse>;
    
    /// 撤单
    async fn cancel_order(&self, order_id: &str) -> Result<()>;
    
    /// 查询订单
    async fn get_order(&self, order_id: &str) -> Result<OrderInfo>;
}
```

---

### 步骤2：Domain层定义统一数据模型

**文件**: `crates/domain/src/entities/exchange_data.rs`

```rust
use serde::{Deserialize, Serialize};

/// 统一的Ticker数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerData {
    pub exchange: String,      // 交易所名称
    pub symbol: String,         // 交易对
    pub last_price: f64,        // 最新价
    pub bid_price: f64,         // 买一价
    pub ask_price: f64,         // 卖一价
    pub volume_24h: f64,        // 24小时成交量
    pub high_24h: f64,          // 24小时最高价
    pub low_24h: f64,           // 24小时最低价
    pub timestamp: i64,         // 时间戳
}

/// 统一的K线数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleData {
    pub exchange: String,
    pub symbol: String,
    pub timeframe: String,
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub confirmed: bool,
}

/// 统一的账户余额模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub exchange: String,
    pub currency: String,
    pub available: f64,
    pub frozen: f64,
    pub total: f64,
}
```

---

### 步骤3：Infrastructure层实现交易所Adapter

**文件**: `crates/infrastructure/src/exchanges/okx_adapter.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::traits::ExchangeMarketData;
use rust_quant_domain::entities::{TickerData, CandleData};
use okx::api::{api_trait::OkxApiTrait, market::OkxMarket};

/// OKX交易所适配器
pub struct OkxMarketDataAdapter {
    client: OkxMarket,
}

impl OkxMarketDataAdapter {
    pub fn new() -> Self {
        Self {
            client: OkxMarket::from_env().expect("OKX配置错误"),
        }
    }
}

#[async_trait]
impl ExchangeMarketData for OkxMarketDataAdapter {
    async fn get_ticker(&self, symbol: &str) -> Result<TickerData> {
        let okx_tickers = self.client.get_ticker(symbol).await?;
        
        let okx_ticker = okx_tickers
            .first()
            .ok_or_else(|| anyhow::anyhow!("无Ticker数据"))?;
        
        // 转换为统一模型
        Ok(TickerData {
            exchange: "okx".to_string(),
            symbol: okx_ticker.inst_id.clone(),
            last_price: okx_ticker.last.parse().unwrap_or(0.0),
            bid_price: okx_ticker.bid_px.parse().unwrap_or(0.0),
            ask_price: okx_ticker.ask_px.parse().unwrap_or(0.0),
            volume_24h: okx_ticker.vol24h.parse().unwrap_or(0.0),
            high_24h: okx_ticker.high24h.parse().unwrap_or(0.0),
            low_24h: okx_ticker.low24h.parse().unwrap_or(0.0),
            timestamp: okx_ticker.ts.parse().unwrap_or(0),
        })
    }
    
    async fn get_tickers(&self, inst_type: &str) -> Result<Vec<TickerData>> {
        let okx_tickers = self.client.get_tickers(inst_type).await?;
        
        Ok(okx_tickers
            .into_iter()
            .map(|t| TickerData {
                exchange: "okx".to_string(),
                symbol: t.inst_id,
                last_price: t.last.parse().unwrap_or(0.0),
                bid_price: t.bid_px.parse().unwrap_or(0.0),
                ask_price: t.ask_px.parse().unwrap_or(0.0),
                volume_24h: t.vol24h.parse().unwrap_or(0.0),
                high_24h: t.high24h.parse().unwrap_or(0.0),
                low_24h: t.low24h.parse().unwrap_or(0.0),
                timestamp: t.ts.parse().unwrap_or(0),
            })
            .collect())
    }
    
    async fn get_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<CandleData>> {
        // 实现K线获取和转换
        todo!()
    }
    
    fn name(&self) -> &str {
        "okx"
    }
}
```

**文件**: `crates/infrastructure/src/exchanges/binance_adapter.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::traits::ExchangeMarketData;
use rust_quant_domain::entities::{TickerData, CandleData};

/// Binance交易所适配器
pub struct BinanceMarketDataAdapter {
    // binance client
}

impl BinanceMarketDataAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ExchangeMarketData for BinanceMarketDataAdapter {
    async fn get_ticker(&self, symbol: &str) -> Result<TickerData> {
        // 调用Binance API，转换为统一模型
        todo!()
    }
    
    async fn get_tickers(&self, inst_type: &str) -> Result<Vec<TickerData>> {
        // Binance实现
        todo!()
    }
    
    async fn get_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<CandleData>> {
        // Binance实现
        todo!()
    }
    
    fn name(&self) -> &str {
        "binance"
    }
}
```

---

### 步骤4：Core层提供交易所工厂

**文件**: `crates/core/src/exchange/factory.rs`

```rust
use rust_quant_domain::traits::ExchangeMarketData;
use rust_quant_infrastructure::exchanges::{
    OkxMarketDataAdapter,
    BinanceMarketDataAdapter,
};

/// 交易所工厂
pub struct ExchangeFactory;

impl ExchangeFactory {
    /// 创建市场数据客户端
    ///
    /// # Arguments
    /// * `exchange_name` - 交易所名称（"okx", "binance", "bybit"）
    pub fn create_market_data(exchange_name: &str) -> Box<dyn ExchangeMarketData> {
        match exchange_name.to_lowercase().as_str() {
            "okx" => Box::new(OkxMarketDataAdapter::new()),
            "binance" => Box::new(BinanceMarketDataAdapter::new()),
            // "bybit" => Box::new(BybitMarketDataAdapter::new()),
            _ => panic!("不支持的交易所: {}", exchange_name),
        }
    }
    
    /// 从环境变量创建（读取 DEFAULT_EXCHANGE）
    pub fn create_default_market_data() -> Box<dyn ExchangeMarketData> {
        let exchange = std::env::var("DEFAULT_EXCHANGE")
            .unwrap_or_else(|_| "okx".to_string());
        Self::create_market_data(&exchange)
    }
    
    /// 创建多个交易所客户端（用于套利）
    pub fn create_multiple_market_data(
        exchanges: &[&str],
    ) -> Vec<Box<dyn ExchangeMarketData>> {
        exchanges
            .iter()
            .map(|name| Self::create_market_data(name))
            .collect()
    }
}
```

---

### 步骤5：Services层重构为依赖接口

**文件**: `crates/services/src/market/ticker_service_v2.rs` (新版本)

```rust
use anyhow::Result;
use rust_quant_domain::traits::ExchangeMarketData;
use rust_quant_domain::entities::TickerData;
use rust_quant_market::models::tickers::TicketsModel;
use tracing::{debug, info};

/// Ticker数据服务（多交易所版本）
///
/// 通过依赖注入支持任意交易所
pub struct TickerServiceV2 {
    exchange: Box<dyn ExchangeMarketData>,
}

impl TickerServiceV2 {
    /// 创建服务实例
    ///
    /// # Arguments
    /// * `exchange` - 交易所实现（通过依赖注入）
    pub fn new(exchange: Box<dyn ExchangeMarketData>) -> Self {
        Self { exchange }
    }
    
    /// 从默认交易所创建
    pub fn from_default() -> Self {
        use rust_quant_core::exchange::ExchangeFactory;
        Self {
            exchange: ExchangeFactory::create_default_market_data(),
        }
    }
    
    /// 从指定交易所创建
    pub fn from_exchange(exchange_name: &str) -> Self {
        use rust_quant_core::exchange::ExchangeFactory;
        Self {
            exchange: ExchangeFactory::create_market_data(exchange_name),
        }
    }
    
    /// 同步Ticker（从交易所获取并保存）
    ///
    /// # 优点
    /// - 支持任意交易所（通过依赖注入）
    /// - 统一的数据模型
    pub async fn sync_ticker_from_exchange(&self, symbol: &str) -> Result<bool> {
        info!("同步Ticker: exchange={}, symbol={}", self.exchange.name(), symbol);
        
        // 1. 从交易所获取（统一接口）
        let ticker = self.exchange.get_ticker(symbol).await?;
        
        // 2. 保存到数据库（统一数据模型）
        let model = TicketsModel::new();
        let existing = model.find_one(symbol).await?;
        
        if existing.is_empty() {
            debug!("插入新Ticker: {}", symbol);
            // 将TickerData转换为OkxDTO保存（暂时保持兼容）
            // TODO: 后续可以重构为统一的TickerEntity
            Ok(true)
        } else {
            debug!("更新Ticker: {}", symbol);
            Ok(false)
        }
    }
}
```

---

### 步骤6：Orchestration层使用（支持配置切换）

**文件**: `crates/orchestration/src/workflow/ticker_sync_multi_exchange.rs`

```rust
use anyhow::Result;
use rust_quant_services::market::TickerServiceV2;
use tracing::info;

/// 同步Ticker（支持多交易所）
///
/// 通过环境变量 DEFAULT_EXCHANGE 选择交易所
pub async fn sync_ticker_multi_exchange(symbol: &str) -> Result<()> {
    // 从默认交易所创建service
    let service = TickerServiceV2::from_default();
    
    service.sync_ticker_from_exchange(symbol).await?;
    Ok(())
}

/// 多交易所套利：同时从多个交易所获取数据
pub async fn fetch_ticker_from_multiple_exchanges(
    symbol: &str,
    exchanges: &[&str],
) -> Result<Vec<TickerData>> {
    use futures::future::join_all;
    
    let mut tasks = Vec::new();
    
    for exchange_name in exchanges {
        let service = TickerServiceV2::from_exchange(exchange_name);
        let symbol = symbol.to_string();
        
        tasks.push(tokio::spawn(async move {
            service.exchange.get_ticker(&symbol).await
        }));
    }
    
    let results = join_all(tasks).await;
    
    let mut tickers = Vec::new();
    for result in results {
        if let Ok(Ok(ticker)) = result {
            tickers.push(ticker);
        }
    }
    
    info!("从 {} 个交易所获取了Ticker数据", tickers.len());
    Ok(tickers)
}

/// 价差套利示例
pub async fn find_arbitrage_opportunities(symbol: &str) -> Result<()> {
    let tickers = fetch_ticker_from_multiple_exchanges(
        symbol,
        &["okx", "binance", "bybit"],
    ).await?;
    
    // 分析价差
    if tickers.len() >= 2 {
        let max_price = tickers.iter().map(|t| t.last_price).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let min_price = tickers.iter().map(|t| t.last_price).min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let spread = (max_price - min_price) / min_price * 100.0;
        
        info!("价差: {:.4}%", spread);
        
        if spread > 0.5 {
            info!("发现套利机会：价差 {:.4}%", spread);
            // 执行套利策略
        }
    }
    
    Ok(())
}
```

---

## 配置文件示例

**文件**: `config/exchange.toml`

```toml
# 默认交易所
default_exchange = "okx"

# 启用的交易所列表（用于套利）
enabled_exchanges = ["okx", "binance"]

[exchanges.okx]
api_key = "${OKX_API_KEY}"
api_secret = "${OKX_API_SECRET}"
passphrase = "${OKX_PASSPHRASE}"
base_url = "https://www.okx.com"
ws_url = "wss://ws.okx.com:8443/ws/v5/public"

[exchanges.binance]
api_key = "${BINANCE_API_KEY}"
api_secret = "${BINANCE_API_SECRET}"
base_url = "https://api.binance.com"
ws_url = "wss://stream.binance.com:9443"

[exchanges.bybit]
api_key = "${BYBIT_API_KEY}"
api_secret = "${BYBIT_API_SECRET}"
base_url = "https://api.bybit.com"
ws_url = "wss://stream.bybit.com"
```

---

## 迁移路径建议

### 当前阶段（Phase 1）- 快速迁移 ✅ 已完成
- services层直接依赖okx SDK
- 优点：快速完成架构重构，orchestration层解耦
- 缺点：难以扩展多交易所

### 优化阶段（Phase 2）- 引入抽象
**何时执行**：当需要接入第二个交易所时

步骤：
1. 在domain层定义交易所接口
2. 在infrastructure层实现okx adapter
3. 重构services层为依赖接口
4. 通过依赖注入选择交易所

### 扩展阶段（Phase 3）- 多交易所支持
**何时执行**：需要同时支持多个交易所或套利

步骤：
1. 实现其他交易所adapter（binance、bybit）
2. 实现交易所工厂（Factory Pattern）
3. 支持配置化选择交易所
4. 实现多交易所套利策略

---

## 设计原则

### 1. 依赖倒置原则（DIP）
- services层依赖domain接口，而不是infrastructure具体实现
- infrastructure实现domain接口
- 通过依赖注入连接

### 2. 开闭原则（OCP）
- 对扩展开放：添加新交易所只需实现adapter
- 对修改关闭：services层代码无需修改

### 3. 单一职责原则（SRP）
- Adapter只负责协议转换
- Service只负责业务逻辑
- Repository只负责数据持久化

### 4. YAGNI原则
- 当前不需要多交易所，保持简单
- 需要时再重构

---

## 目录结构

```
crates/
├── domain/
│   ├── src/
│   │   ├── traits/
│   │   │   ├── exchange_trait.rs        # 交易所接口定义
│   │   │   └── ...
│   │   └── entities/
│   │       ├── exchange_data.rs         # 统一数据模型
│   │       └── ...
│
├── infrastructure/
│   ├── src/
│   │   ├── exchanges/                    # 交易所适配器
│   │   │   ├── mod.rs
│   │   │   ├── okx_adapter.rs           # OKX适配器
│   │   │   ├── binance_adapter.rs       # Binance适配器
│   │   │   └── bybit_adapter.rs         # Bybit适配器
│   │   └── ...
│
├── core/
│   ├── src/
│   │   ├── exchange/                     # 交易所工厂
│   │   │   ├── mod.rs
│   │   │   └── factory.rs               # 交易所工厂
│   │   └── ...
│
└── services/
    ├── src/
    │   ├── market/
    │   │   ├── ticker_service.rs         # 当前版本（直接依赖okx）
    │   │   ├── ticker_service_v2.rs      # 多交易所版本（依赖trait）
    │   │   └── ...
    │   └── ...
```

---

## 使用示例

### 单交易所模式（当前）

```rust
// 使用默认交易所（从环境变量读取）
let service = TickerServiceV2::from_default();
service.sync_ticker_from_exchange("BTC-USDT").await?;

// 使用指定交易所
let service = TickerServiceV2::from_exchange("binance");
service.sync_ticker_from_exchange("BTCUSDT").await?;
```

### 多交易所套利模式

```rust
use rust_quant_core::exchange::ExchangeFactory;

// 创建多个交易所客户端
let exchanges = ExchangeFactory::create_multiple_market_data(&["okx", "binance", "bybit"]);

// 并发获取所有交易所的ticker
let mut tasks = Vec::new();
for exchange in exchanges {
    tasks.push(exchange.get_ticker("BTC-USDT"));
}

let tickers = futures::future::join_all(tasks).await;

// 分析价差
let prices: Vec<f64> = tickers.iter().map(|t| t.last_price).collect();
let spread = (prices.max() - prices.min()) / prices.min();

if spread > 0.005 { // 0.5%套利空间
    info!("发现套利机会");
}
```

---

## 建议

### 当前建议（短期）
保持现状，services层直接依赖okx SDK：
- ✅ 符合YAGNI原则
- ✅ 架构已符合DDD（orchestration已解耦）
- ✅ 后续扩展时再重构

### 未来规划（中期）
当需要接入第二个交易所时：
1. 定义domain层接口
2. 实现infrastructure层adapter
3. 重构services层为依赖接口
4. 保持向后兼容

### 长期规划
支持多交易所套利：
1. 实现多个交易所adapter
2. 实现套利策略引擎
3. 支持跨交易所订单路由
4. 实现风险对冲机制

---

## 参考资料

- **设计模式**: Adapter Pattern (适配器模式)
- **架构原则**: SOLID原则（特别是DIP和OCP）
- **重构书籍**: Martin Fowler - Refactoring

