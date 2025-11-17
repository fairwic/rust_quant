# 交易所使用示例

## 当前架构（已完成）

### 架构图

```
orchestration 层（任务编排）
    ↓ 无需修改
services 层（业务逻辑）
    ↓ 通过 ExchangeFactory 创建
domain 层（接口定义）
    ↑ 实现接口
infrastructure 层（OKX Adapter）
    ↓ 调用
OKX SDK
```

---

## 使用方式

### 1. 使用默认交易所（OKX）

**无需任何代码修改**，默认使用OKX：

```rust
// orchestration 层代码（无需修改）
use rust_quant_services::market::TickerService;

let service = TickerService::new();
service.sync_ticker_from_exchange("BTC-USDT").await?;
```

### 2. 切换交易所（通过环境变量）

```bash
# 设置默认交易所
export DEFAULT_EXCHANGE=okx

# 运行程序（无需修改代码）
cargo run
```

### 3. 查看当前使用的交易所

services层会自动打印交易所名称：

```
✅ 从交易所 okx 获取了 10 个币种余额
✅ 从交易所 okx 获取了账户余额
```

---

## 未来扩展：添加新交易所

### 步骤1：实现Binance Adapter

创建 `crates/infrastructure/src/exchanges/binance_adapter.rs`：

```rust
use async_trait::async_trait;
use rust_quant_domain::traits::ExchangeMarketData;

pub struct BinanceMarketDataAdapter;

impl BinanceMarketDataAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl ExchangeMarketData for BinanceMarketDataAdapter {
    fn name(&self) -> &'static str {
        "binance"
    }
    
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value> {
        // 调用Binance API
        // HTTP GET https://api.binance.com/api/v3/ticker/24hr?symbol=BTCUSDT
        todo!()
    }
    
    // ... 实现其他方法
}
```

### 步骤2：注册到工厂

修改 `crates/infrastructure/src/exchanges/factory.rs`：

```rust
pub fn create_market_data(exchange_name: &str) -> Result<Box<dyn ExchangeMarketData>> {
    match exchange_name.to_lowercase().as_str() {
        "okx" => Ok(Box::new(OkxMarketDataAdapter::new()?)),
        "binance" => Ok(Box::new(BinanceMarketDataAdapter::new()?)),  // 添加这一行
        _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
    }
}
```

### 步骤3：使用新交易所

```bash
# 切换到Binance
export DEFAULT_EXCHANGE=binance

# 运行程序（无需修改代码）
cargo run
```

**orchestration层和services层代码完全无需修改！**

---

## 多交易所套利示例

### 场景：同时从多个交易所获取价格，寻找套利机会

创建 `crates/orchestration/src/workflow/arbitrage_job.rs`：

```rust
use anyhow::Result;
use rust_quant_infrastructure::ExchangeFactory;
use tracing::info;

/// 跨交易所套利任务
pub async fn find_arbitrage_opportunities(symbol: &str) -> Result<()> {
    info!("🔍 检查套利机会: {}", symbol);
    
    // 1. 创建多个交易所客户端
    let okx = ExchangeFactory::create_market_data("okx")?;
    let binance = ExchangeFactory::create_market_data("binance")?;
    
    // 2. 并发获取ticker
    let (okx_ticker, binance_ticker) = tokio::join!(
        okx.fetch_ticker(symbol),
        binance.fetch_ticker(symbol),
    );
    
    let okx_data = okx_ticker?;
    let binance_data = binance_ticker?;
    
    // 3. 提取价格（需要解析JSON）
    let okx_price = extract_last_price(&okx_data)?;
    let binance_price = extract_last_price(&binance_data)?;
    
    // 4. 计算价差
    let spread = ((okx_price - binance_price).abs() / okx_price.min(binance_price)) * 100.0;
    
    info!("价差: {:.4}% (OKX: {}, Binance: {})", spread, okx_price, binance_price);
    
    if spread > 0.5 {
        info!("⚡ 发现套利机会！价差 {:.4}%", spread);
        // 执行套利策略
    }
    
    Ok(())
}

fn extract_last_price(ticker_json: &serde_json::Value) -> Result<f64> {
    // 从JSON中提取价格（需要根据不同交易所的格式）
    if let Some(arr) = ticker_json.as_array() {
        if let Some(first) = arr.first() {
            if let Some(last_str) = first.get("last").and_then(|v| v.as_str()) {
                return Ok(last_str.parse()?);
            }
        }
    }
    Err(anyhow::anyhow!("无法提取价格"))
}
```

---

## 配置管理

### 环境变量方式

```bash
# .env 文件
DEFAULT_EXCHANGE=okx

# OKX配置
OKX_API_KEY=your_key
OKX_API_SECRET=your_secret
OKX_PASSPHRASE=your_passphrase

# Binance配置（未来使用）
BINANCE_API_KEY=your_binance_key
BINANCE_API_SECRET=your_binance_secret
```

### 配置文件方式（未来扩展）

```toml
# config/exchange.toml
default_exchange = "okx"
enabled_exchanges = ["okx"]

[exchanges.okx]
enabled = true
api_key = "${OKX_API_KEY}"
api_secret = "${OKX_API_SECRET}"
passphrase = "${OKX_PASSPHRASE}"

[exchanges.binance]
enabled = false
api_key = "${BINANCE_API_KEY}"
api_secret = "${BINANCE_API_SECRET}"
```

---

## 测试不同交易所

```rust
#[tokio::test]
#[ignore]
async fn test_okx_exchange() {
    std::env::set_var("DEFAULT_EXCHANGE", "okx");
    
    let service = TickerService::new();
    let result = service.sync_ticker_from_exchange("BTC-USDT").await;
    
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore]
async fn test_binance_exchange() {
    std::env::set_var("DEFAULT_EXCHANGE", "binance");
    
    let service = TickerService::new();
    let result = service.sync_ticker_from_exchange("BTCUSDT").await;
    
    assert!(result.is_ok());
}
```

---

## 架构优势

### 1. 零成本抽象
- services层代码完全相同
- orchestration层代码完全相同
- 只需修改环境变量即可切换交易所

### 2. 易于测试
```rust
// 创建Mock交易所用于测试
struct MockExchange;

#[async_trait]
impl ExchangeMarketData for MockExchange {
    fn name(&self) -> &'static str { "mock" }
    
    async fn fetch_ticker(&self, _symbol: &str) -> Result<serde_json::Value> {
        Ok(serde_json::json!([{
            "last": "50000",
            "bid_px": "49999",
            "ask_px": "50001",
            // ... mock数据
        }]))
    }
    
    // ... 其他方法
}

// 在测试中使用
let service = TickerService::new_with_exchange(Box::new(MockExchange));
```

### 3. 支持多交易所并发
```rust
// 同时连接3个交易所
let exchanges = vec!["okx", "binance", "bybit"];
let clients = ExchangeFactory::create_multiple_market_data(&exchanges);

// 并发获取价格
let tasks: Vec<_> = clients
    .into_iter()
    .map(|exchange| async move {
        exchange.fetch_ticker("BTC-USDT").await
    })
    .collect();

let results = futures::future::join_all(tasks).await;
```

---

## 当前状态

✅ **已完成**：
- Domain层：交易所接口定义
- Infrastructure层：OKX adapter实现
- Infrastructure层：ExchangeFactory工厂
- Services层：依赖domain接口（支持多交易所扩展）
- Orchestration层：无需修改

⏳ **待添加**（当需要时）：
- Binance adapter
- Bybit adapter
- Coinbase adapter
- 其他交易所...

---

## 如何添加新交易所

### 三步完成

1. **实现adapter**：创建 `infrastructure/src/exchanges/xxx_adapter.rs`
2. **注册工厂**：在 `ExchangeFactory` 中添加case分支
3. **设置环境变量**：`export DEFAULT_EXCHANGE=xxx`

**services层和orchestration层代码完全不需要修改！**

---

## 总结

当前架构已支持多交易所扩展：
- ✅ 接口定义完成
- ✅ OKX adapter实现完成
- ✅ 工厂模式完成
- ✅ Services层解耦完成
- ✅ Orchestration层保持不变

未来添加新交易所时，只需实现对应的adapter，无需修改业务代码。

