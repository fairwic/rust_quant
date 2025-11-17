# 多交易所实施指南

## 快速开始

如果你现在需要接入新交易所，按照以下步骤操作：

---

## Step 1: 定义 Domain 层接口

### 1.1 创建交易所接口

**文件**: `crates/domain/src/traits/exchange_trait.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;

/// 交易所市场数据接口
#[async_trait]
pub trait ExchangeMarketData: Send + Sync {
    /// 交易所名称
    fn name(&self) -> &'static str;
    
    /// 获取单个Ticker
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value>;
    
    /// 批量获取Ticker
    async fn fetch_tickers(&self, inst_type: &str) -> Result<Vec<serde_json::Value>>;
    
    /// 获取K线
    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>>;
}
```

### 1.2 更新 Domain 导出

**文件**: `crates/domain/src/traits/mod.rs`

```rust
mod exchange_trait;
pub use exchange_trait::*;
```

---

## Step 2: Infrastructure 层实现 Adapter

### 2.1 创建 OKX Adapter

**文件**: `crates/infrastructure/src/exchanges/okx_adapter.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use okx::api::{api_trait::OkxApiTrait, market::OkxMarket};
use rust_quant_domain::traits::ExchangeMarketData;

pub struct OkxExchangeAdapter {
    client: OkxMarket,
}

impl OkxExchangeAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: OkxMarket::from_env()?,
        })
    }
}

#[async_trait]
impl ExchangeMarketData for OkxExchangeAdapter {
    fn name(&self) -> &'static str {
        "okx"
    }
    
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value> {
        let tickers = self.client.get_ticker(symbol).await?;
        Ok(serde_json::to_value(&tickers)?)
    }
    
    async fn fetch_tickers(&self, inst_type: &str) -> Result<Vec<serde_json::Value>> {
        let tickers = self.client.get_tickers(inst_type).await?;
        Ok(tickers.into_iter().map(|t| serde_json::to_value(t).unwrap()).collect())
    }
    
    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        let after = start.map(|s| s.to_string());
        let before = end.map(|e| e.to_string());
        let limit_str = limit.map(|l| l.to_string());
        
        let candles = self.client
            .get_history_candles(
                symbol,
                timeframe,
                after.as_deref(),
                before.as_deref(),
                limit_str.as_deref(),
            )
            .await?;
            
        Ok(candles.into_iter().map(|c| serde_json::to_value(c).unwrap()).collect())
    }
}
```

### 2.2 创建 Binance Adapter（示例）

**文件**: `crates/infrastructure/src/exchanges/binance_adapter.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_domain::traits::ExchangeMarketData;

pub struct BinanceExchangeAdapter {
    // Binance client
    api_key: String,
    api_secret: String,
}

impl BinanceExchangeAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            api_key: std::env::var("BINANCE_API_KEY")?,
            api_secret: std::env::var("BINANCE_API_SECRET")?,
        })
    }
}

#[async_trait]
impl ExchangeMarketData for BinanceExchangeAdapter {
    fn name(&self) -> &'static str {
        "binance"
    }
    
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value> {
        // 调用Binance API
        // 1. 构建请求
        // 2. 发送HTTP请求到 https://api.binance.com/api/v3/ticker/24hr?symbol=BTCUSDT
        // 3. 解析响应
        // 4. 转换为JSON
        
        todo!("实现Binance API调用")
    }
    
    async fn fetch_tickers(&self, _inst_type: &str) -> Result<Vec<serde_json::Value>> {
        todo!("实现Binance批量ticker获取")
    }
    
    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        todo!("实现Binance K线获取")
    }
}
```

### 2.3 创建 Adapter 模块

**文件**: `crates/infrastructure/src/exchanges/mod.rs`

```rust
mod okx_adapter;
mod binance_adapter;

pub use okx_adapter::OkxExchangeAdapter;
pub use binance_adapter::BinanceExchangeAdapter;
```

**文件**: `crates/infrastructure/src/lib.rs`

```rust
pub mod exchanges;  // 添加这一行
```

---

## Step 3: Core 层提供工厂

**文件**: `crates/core/src/exchange/factory.rs`

```rust
use rust_quant_domain::traits::ExchangeMarketData;
use rust_quant_infrastructure::exchanges::{OkxExchangeAdapter, BinanceExchangeAdapter};

pub struct ExchangeFactory;

impl ExchangeFactory {
    pub fn create_market_data(name: &str) -> anyhow::Result<Box<dyn ExchangeMarketData>> {
        match name.to_lowercase().as_str() {
            "okx" => Ok(Box::new(OkxExchangeAdapter::new()?)),
            "binance" => Ok(Box::new(BinanceExchangeAdapter::new()?)),
            _ => Err(anyhow::anyhow!("不支持的交易所: {}", name)),
        }
    }
    
    pub fn create_default() -> anyhow::Result<Box<dyn ExchangeMarketData>> {
        let name = std::env::var("DEFAULT_EXCHANGE").unwrap_or_else(|_| "okx".to_string());
        Self::create_market_data(&name)
    }
}
```

**文件**: `crates/core/src/exchange/mod.rs`

```rust
mod factory;
pub use factory::*;
```

**文件**: `crates/core/src/lib.rs`

```rust
pub mod exchange;  // 添加这一行
```

---

## Step 4: Services 层重构

### 4.1 重构 TickerService

**文件**: `crates/services/src/market/ticker_service.rs` (替换现有版本)

```rust
use anyhow::Result;
use rust_quant_domain::traits::ExchangeMarketData;
use tracing::info;

pub struct TickerService {
    exchange: Box<dyn ExchangeMarketData>,
}

impl TickerService {
    /// 通过依赖注入创建
    pub fn new(exchange: Box<dyn ExchangeMarketData>) -> Self {
        Self { exchange }
    }
    
    /// 从默认交易所创建
    pub fn from_default() -> Result<Self> {
        use rust_quant_core::exchange::ExchangeFactory;
        Ok(Self {
            exchange: ExchangeFactory::create_default()?,
        })
    }
    
    /// 从指定交易所创建
    pub fn from_exchange(name: &str) -> Result<Self> {
        use rust_quant_core::exchange::ExchangeFactory;
        Ok(Self {
            exchange: ExchangeFactory::create_market_data(name)?,
        })
    }
    
    /// 同步Ticker
    pub async fn sync_ticker_from_exchange(&self, symbol: &str) -> Result<()> {
        info!("同步Ticker: exchange={}, symbol={}", self.exchange.name(), symbol);
        
        // 从交易所获取（统一接口）
        let ticker = self.exchange.fetch_ticker(symbol).await?;
        
        // 保存到数据库
        // TODO: 保存ticker
        
        Ok(())
    }
}
```

---

## Step 5: 更新 Orchestration 层

无需修改！orchestration 层代码保持不变：

```rust
// 自动使用默认交易所
let service = TickerService::from_default()?;
service.sync_ticker_from_exchange("BTC-USDT").await?;

// 或者指定交易所
let service = TickerService::from_exchange("binance")?;
service.sync_ticker_from_exchange("BTCUSDT").await?;
```

---

## 配置切换交易所

### 环境变量方式

```bash
# 使用OKX
export DEFAULT_EXCHANGE=okx

# 切换到Binance
export DEFAULT_EXCHANGE=binance

# 运行程序（无需修改代码）
cargo run
```

### 配置文件方式

```toml
# config/exchange.toml
default_exchange = "okx"

[exchanges.okx]
enabled = true

[exchanges.binance]
enabled = false
```

---

## 套利策略示例

```rust
use rust_quant_core::exchange::ExchangeFactory;

pub async fn arbitrage_strategy(symbol: &str) -> Result<()> {
    // 1. 创建多个交易所客户端
    let okx = ExchangeFactory::create_market_data("okx")?;
    let binance = ExchangeFactory::create_market_data("binance")?;
    
    // 2. 并发获取价格
    let (okx_ticker, binance_ticker) = tokio::join!(
        okx.fetch_ticker(symbol),
        binance.fetch_ticker(symbol),
    );
    
    // 3. 分析价差
    let okx_price = extract_price(&okx_ticker?)?;
    let binance_price = extract_price(&binance_ticker?)?;
    
    let spread = (okx_price - binance_price).abs() / okx_price.min(binance_price);
    
    if spread > 0.005 {
        info!("套利机会：价差 {:.2}%", spread * 100.0);
        // 4. 执行套利交易
    }
    
    Ok(())
}
```

---

## 测试不同交易所

```rust
#[tokio::test]
async fn test_okx_adapter() {
    let adapter = OkxExchangeAdapter::new().unwrap();
    let ticker = adapter.fetch_ticker("BTC-USDT").await.unwrap();
    assert!(!ticker.is_null());
}

#[tokio::test]
async fn test_binance_adapter() {
    let adapter = BinanceExchangeAdapter::new().unwrap();
    let ticker = adapter.fetch_ticker("BTCUSDT").await.unwrap();
    assert!(!ticker.is_null());
}

#[tokio::test]
async fn test_exchange_switching() {
    // 通过工厂切换交易所
    let okx_service = TickerService::from_exchange("okx").unwrap();
    let binance_service = TickerService::from_exchange("binance").unwrap();
    
    // 两者使用相同的接口
    okx_service.sync_ticker_from_exchange("BTC-USDT").await.unwrap();
    binance_service.sync_ticker_from_exchange("BTCUSDT").await.unwrap();
}
```

---

## 实施检查清单

接入新交易所时的检查项：

- [ ] 在 domain 层定义交易所接口（如果还没有）
- [ ] 在 infrastructure/exchanges/ 创建新交易所 adapter
- [ ] 实现 ExchangeMarketData trait
- [ ] 实现数据格式转换（交易所DTO → domain统一模型）
- [ ] 在 ExchangeFactory 中注册新交易所
- [ ] 添加配置支持（API密钥等）
- [ ] 编写单元测试
- [ ] 更新文档

---

## 关键设计决策

### Q: 为什么不直接在services层支持多个交易所？
A: 违反单一职责原则，services层会变得臃肿。通过adapter模式，每个交易所的实现独立、可测试。

### Q: 数据模型要统一吗？
A: 建议统一。定义domain层的标准数据模型，各adapter负责转换。

### Q: 如何处理交易所特有功能？
A: 
- 通用功能：定义在trait中
- 特有功能：扩展trait或使用trait的方法返回JSON

### Q: 性能会受影响吗？
A: trait的动态分发有微小开销（纳秒级），相比网络IO可忽略不计。

---

## 迁移检查

当前代码中需要修改的位置：

```bash
# 查找所有直接使用okx的地方
grep -r "okx::" crates/services/src/

# 需要重构为：
# 1. 依赖 domain::traits::ExchangeXxx
# 2. 通过依赖注入获取实现
```

---

## 总结

### 当前架构（Phase 1）✅
```
orchestration → services → okx SDK
```
- 优点：简单快速
- 缺点：单交易所

### 推荐架构（Phase 2）
```
orchestration → services → domain::trait ← infrastructure::adapter → okx SDK
```
- 优点：可扩展、可测试、符合SOLID
- 缺点：稍复杂

选择时机：**当需要接入第二个交易所时，再进行架构升级**。

