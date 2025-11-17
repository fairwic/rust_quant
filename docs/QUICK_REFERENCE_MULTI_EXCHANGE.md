# 多交易所快速参考

## 当前状态

✅ 架构已完成，支持多交易所扩展
✅ OKX交易所已实现
✅ 所有代码编译通过
✅ Orchestration层无需修改

---

## 如何添加新交易所（仅需3步）

### 步骤1：实现Adapter（15分钟）

创建 `crates/infrastructure/src/exchanges/binance_adapter.rs`：

```rust
use anyhow::Result;
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
        let url = format!("https://api.binance.com/api/v3/ticker/24hr?symbol={}", symbol);
        let response = reqwest::get(&url).await?;
        let data = response.json().await?;
        Ok(serde_json::json!([data])) // 包装为数组格式
    }
    
    async fn fetch_tickers(&self, _inst_type: &str) -> Result<Vec<serde_json::Value>> {
        let url = "https://api.binance.com/api/v3/ticker/24hr";
        let response = reqwest::get(url).await?;
        let data: Vec<serde_json::Value> = response.json().await?;
        Ok(data)
    }
    
    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        // 实现Binance K线获取
        todo!()
    }
    
    async fn fetch_latest_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        // 实现Binance最新K线获取
        todo!()
    }
}
```

### 步骤2：注册到Factory（1分钟）

修改 `crates/infrastructure/src/exchanges/factory.rs`：

```rust
pub fn create_market_data(exchange_name: &str) -> Result<Box<dyn ExchangeMarketData>> {
    match exchange_name.to_lowercase().as_str() {
        "okx" => Ok(Box::new(OkxMarketDataAdapter::new()?)),
        "binance" => Ok(Box::new(BinanceMarketDataAdapter::new()?)),  // 添加这行
        _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
    }
}
```

修改 `crates/infrastructure/src/exchanges/mod.rs`：

```rust
mod binance_adapter;  // 添加这行
pub use binance_adapter::*;  // 添加这行
```

### 步骤3：使用（1秒）

```bash
export DEFAULT_EXCHANGE=binance
cargo run
```

**完成！无需修改任何业务代码。**

---

## 当前使用（OKX）

### 默认配置
```bash
# 默认使用OKX，无需设置
cargo run
```

### 显式指定
```bash
export DEFAULT_EXCHANGE=okx
cargo run
```

---

## 核心代码示例

### Services层代码（已完成）

```rust
// services层自动使用默认交易所
pub async fn fetch_ticker_from_exchange(&self, inst_id: &str) -> Result<Option<TickerOkxResDto>> {
    use rust_quant_infrastructure::ExchangeFactory;

    let exchange = ExchangeFactory::create_default_market_data()?;  // 自动使用DEFAULT_EXCHANGE
    let ticker_json = exchange.fetch_ticker(inst_id).await?;
    
    // 转换为OKX格式（保持向后兼容）
    // ...
}
```

### Orchestration层代码（无需修改）

```rust
// orchestration层代码完全不变
pub async fn sync_tickers(inst_ids: &[String]) -> Result<()> {
    let ticker_service = TickerService::new();
    
    for inst_id in inst_ids {
        ticker_service.sync_ticker_from_exchange(inst_id).await?;
    }
    
    Ok(())
}
```

---

## 多交易所套利（未来）

```rust
use rust_quant_infrastructure::ExchangeFactory;

pub async fn arbitrage_btc_usdt() -> Result<()> {
    // 创建多个交易所客户端
    let okx = ExchangeFactory::create_market_data("okx")?;
    let binance = ExchangeFactory::create_market_data("binance")?;
    let bybit = ExchangeFactory::create_market_data("bybit")?;
    
    // 并发获取价格
    let (okx_ticker, binance_ticker, bybit_ticker) = tokio::join!(
        okx.fetch_ticker("BTC-USDT"),
        binance.fetch_ticker("BTCUSDT"),
        bybit.fetch_ticker("BTCUSDT"),
    );
    
    // 提取价格并分析套利机会
    let prices = vec![
        extract_price(&okx_ticker?)?,
        extract_price(&binance_ticker?)?,
        extract_price(&bybit_ticker?)?,
    ];
    
    let max = prices.iter().max().unwrap();
    let min = prices.iter().min().unwrap();
    let spread = (max - min) / min * 100.0;
    
    if spread > 0.5 {
        info!("⚡ 发现套利机会！价差: {:.2}%", spread);
        // 执行套利交易
    }
    
    Ok(())
}
```

---

## 架构验证清单

- [x] Domain层定义交易所接口
- [x] Infrastructure层实现OKX adapter
- [x] Infrastructure层实现ExchangeFactory
- [x] Services层重构为依赖接口
- [x] Orchestration层无需修改
- [x] 编译通过无错误
- [x] 无循环依赖
- [x] 文档完善

---

## 依赖图

```
orchestration (不依赖okx)
    ↓
services (不直接依赖okx，通过ExchangeFactory)
    ↓
infrastructure (实现domain接口 + ExchangeFactory)
    ├── domain (接口定义)
    └── okx (SDK)
```

---

## 关键文件

| 层级 | 文件 | 作用 |
|------|------|------|
| Domain | `domain/src/traits/exchange_trait.rs` | 定义接口 |
| Infrastructure | `infrastructure/src/exchanges/okx_adapter.rs` | OKX实现 |
| Infrastructure | `infrastructure/src/exchanges/factory.rs` | 交易所工厂 |
| Services | `services/src/market/mod.rs` | 使用ExchangeFactory |
| Orchestration | `orchestration/src/workflow/tickets_job.rs` | 调用services（无需修改） |

---

## 总结

多交易所架构已完成：
- **当前**：完美支持OKX
- **未来**：添加新交易所仅需3步，无需修改业务代码
- **套利**：支持多交易所同时连接
- **测试**：支持Mock交易所

架构符合SOLID原则，为长远发展打下坚实基础。

