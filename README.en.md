[中文版](./README.md)

# rust_quant

## Project Introduction

`rust_quant` is a Rust-based quantitative trading and backtesting framework, supporting OKX historical data acquisition, real-time market push, strategy backtesting, and automated strategy scheduling and order execution.
With flexible configuration and modular design, it is suitable for both strategy developers for backtesting and for live automated trading.

---

## Main Features

- **API for Historical Data**: Easily fetch OKX historical candles, market data, and depth for strategy backtesting.
- **Environment Variable Configuration**: Switch between backtest/live/data sync/real-time strategy modes via `.env` file.
- **WebSocket Real-time Market**: High-performance async push, supports multi-symbol and multi-timeframe subscription.
- **Strategy Backtest & Live Scheduling**: Batch backtest, scheduled execution, real-time signal generation and auto order placement.
- **Task Scheduling & Monitoring**: Built-in scheduler for dynamic add/cancel/monitoring of strategy tasks.

---

## Directory Structure

```
src/
├── main.rs                        # Entry, mode switch & scheduling
├── config.rs                      # Env & config management
├── error.rs                       # Error types
├── socket/                        # WebSocket real-time market
│   └── mod.rs
├── trading/                       # Strategy & backtest
│   ├── strategy/
│   │   ├── order/vagas_order.rs   # Vegas strategy scheduling & order
│   │   └── ...                    # Other strategies
│   └── task/
│       ├── basic.rs               # Strategy backtest & signal
│       └── tickets_job.rs         # Market data update
└── ...
```

---

## Environment Variable Example

`.env` example:

```
MODE=backtest                # backtest/real
IS_BACK_TEST=true            # Enable backtest
IS_RUN_REAL_STRATEGY=false   # Enable live strategy scheduling
IS_RUN_SYNC_DATA_JOB=false   # Enable historical data sync
IS_OPEN_SOCKET=true          # Enable WebSocket real-time market
OKX_API_KEY=xxx
OKX_API_SECRET=xxx
OKX_PASSPHRASE=xxx
```

---

## Typical Workflow

### 1. Fetch Historical Data via API

```rust
use okx::api::market::market_api::OkxMarket;
let market = OkxMarket::from_env().unwrap();
let candles = market.get_history_candles("BTC-USDT", "1D", None, None, Some("100")).await.unwrap();
println!("{:?}", candles);
```

### 2. Strategy Backtest (Batch)

main.rs snippet (auto loop symbols & timeframes):

```rust
if env::var("IS_BACK_TEST").unwrap() == "true" {
    for inst_id in inst_ids {
        for time in period {
            let res = task::basic::vegas_test(inst_id, &time).await;
            // Error handling ...
        }
    }
}
```

### 3. Live Strategy Scheduling & Auto Order

main.rs snippet (scheduled, auto order):

```rust
if env::var("IS_RUN_REAL_STRATEGY").unwrap_or("false".to_string()) == "true" {
    // Get strategy config
    let strategy_list = StrategyConfigEntityModel::new().await.get_list().await;
    for strategy in strategy_list.unwrap().iter() {
        if strategy.strategy_type == StrategyType::Vegas.to_string() {
            let strategy_config: VegasStrategy = serde_json::from_str(&strategy.value).unwrap();
            VagasOrder::new().order(strategy_config, inst_id, time).await?;
        }
    }
}
```

- Supports batch scheduling for multiple strategies & symbols
- Scheduler auto manages scheduled execution, monitoring, and cancel

### 4. WebSocket Real-time Market Push

main.rs snippet:

```rust
if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
    socket::websocket_service::run_socket(inst_ids, period).await;
}
```

- Supports multi-symbol, multi-timeframe subscription
- Real-time push of market/candle to strategy module

---

## Strategy Scheduling & Order (Vegas Example)

- `VagasOrder::order` auto initializes historical data, registers scheduled task, runs strategy on schedule, and auto orders
- Supports dynamic add/cancel/monitoring of tasks
- See `src/trading/strategy/order/vagas_order.rs` for details

---

## One-sentence Workflow Summary

- **Backtest Mode**: Fetch historical data via API, batch run strategy backtest, output performance.
- **Live Mode**: WebSocket real-time market push, strategy module generates signals on schedule/real-time, auto order.
- **Data Sync**: Optionally sync historical market data for analysis & backtest.

---

## Start Project

```bash
# After configuring .env
cargo run
```

---

## Development Advice

- Decoupled backtest & live scheduling for easy testing and optimization.
- Supports batch backtest & live scheduling for multiple strategies & symbols.
- Scheduler supports dynamic add/cancel/monitoring of tasks.
- WebSocket real-time market and strategy signal decoupled for easy extension.

---

## License

MIT

---

For more details on strategy development, backtest, or live trading, feel free to ask! 