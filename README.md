[English Version](./README.en.md)

# rust_quant

## 项目简介

`rust_quant` 是一个基于 Rust 的量化交易与策略回测框架，支持 OKX 交易所历史数据获取、实时行情推送、策略回测、自动化策略调度与下单。  
通过灵活的配置和模块化设计，既适合策略开发者回测，也适合实盘自动化交易。

---

## 主要特性

- **API 获取历史数据**：便捷拉取 OKX 历史K线、行情、深度等数据，支持策略回测。
- **环境变量配置**：通过 `.env` 文件灵活切换回测/实盘/数据同步/实时策略等模式。
- **WebSocket 实时行情**：高性能异步推送，支持多品种多周期订阅。
- **策略回测与实盘调度**：支持批量回测、定时调度、实时信号生成与自动下单。
- **任务调度与监控**：内置任务调度器，支持策略任务的动态添加、取消与监控。

---

## 目录结构简述

```
src/
├── main.rs                        # 程序入口，模式切换与调度
├── config.rs                      # 环境变量与配置管理
├── error.rs                       # 错误类型
├── socket/                        # WebSocket 实时行情
│   └── mod.rs
├── trading/                       # 策略与回测
│   ├── strategy/
│   │   ├── order/vagas_order.rs   # Vegas策略调度与下单
│   │   └── ...                    # 其它策略
│   └── task/
│       ├── basic.rs               # 策略回测与信号生成
│       └── tickets_job.rs         # 行情数据更新
└── ...
```

---

## 环境变量配置

`.env` 示例：

```
MODE=backtest                # backtest/real
IS_BACK_TEST=true            # 是否回测
IS_RUN_REAL_STRATEGY=false   # 是否实盘策略调度
IS_RUN_SYNC_DATA_JOB=false   # 是否同步历史数据
IS_OPEN_SOCKET=true          # 是否启动WebSocket实时行情
OKX_API_KEY=xxx
OKX_API_SECRET=xxx
OKX_PASSPHRASE=xxx
```

---

## 典型业务流程

### 1. 通过 API 获取历史数据

```rust
use okx::api::market::market_api::OkxMarket;
let market = OkxMarket::from_env().unwrap();
let candles = market.get_history_candles("BTC-USDT", "1D", None, None, Some("100")).await.unwrap();
println!("{:?}", candles);
```

### 2. 策略回测（批量回测）

main.rs 片段（自动遍历品种和周期）：

```rust
if env::var("IS_BACK_TEST").unwrap() == "true" {
    for inst_id in inst_ids {
        for time in period {
            let res = task::basic::vegas_test(inst_id, &time).await;
            // 错误处理...
        }
    }
}
```

### 3. 实盘策略调度与自动下单

main.rs 片段（定时调度，自动下单）：

```rust
if env::var("IS_RUN_REAL_STRATEGY").unwrap_or("false".to_string()) == "true" {
    // 获取策略配置
    let strategy_list = StrategyConfigEntityModel::new().await.get_list().await;
    for strategy in strategy_list.unwrap().iter() {
        if strategy.strategy_type == StrategyType::Vegas.to_string() {
            let strategy_config: VegasStrategy = serde_json::from_str(&strategy.value).unwrap();
            VagasOrder::new().order(strategy_config, inst_id, time).await?;
        }
    }
}
```

- 支持多策略、多品种批量调度
- 任务调度器自动管理策略的定时执行、监控与取消

### 4. WebSocket 实时行情推送

main.rs 片段：

```rust
if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
    socket::websocket_service::run_socket(inst_ids, period).await;
}
```

- 支持多品种多周期订阅
- 实时推送行情/K线到策略模块

---

## 策略调度与下单（Vegas 策略为例）

- `VagasOrder::order` 会自动初始化历史数据、注册定时任务、定时运行策略、自动下单
- 支持任务的动态添加、取消与监控
- 详见 `src/trading/strategy/order/vagas_order.rs`

---

## 一句话流程总结

- **回测模式**：通过 API 拉取历史数据，批量执行策略回测，输出绩效。
- **实盘模式**：WebSocket 实时推送行情，策略模块定时/实时生成信号，自动下单。
- **数据同步**：可选同步历史行情，便于数据分析与回测。

---

## 启动项目

```bash
# 配置好 .env 后
cargo run
```

---

## 开发建议

- 策略回测与实盘调度解耦，便于独立测试和优化。
- 支持多策略、多品种批量回测与实盘调度。
- 任务调度器支持动态添加、取消、监控任务。
- WebSocket 实时行情与策略信号解耦，便于扩展更多策略。

---

## License

MIT

---

## 系统流程图

以下流程图展示了系统的核心业务流程：
### 回测执行逻辑流程
下图展示了系统回测执行的详细逻辑流程，包括各种交易条件判断和仓位管理：


![回测执行逻辑流程](uml/image/back_test_logic.png)

主要包括：
- 初始化交易状态（资金、持仓、入场价等）
- 循环处理K线数据
- 根据信号执行开仓、平仓操作
- 计算并记录每笔交易和最终回测结果

这些流程图直观地展示了系统的工作原理和执行路径，有助于理解代码的设计思路和业务逻辑。
### Vegas 策略信号生成流程

下图展示了 Vegas 指标信号获取全流程，从获取历史K线数据，到指标计算、信号判定，再到风控与持仓管理的完整流程：

![Vegas 策略信号生成流程](uml/image/vegas_signal.png)

主要包括：
- 获取历史K线数据并转换为 `Vec<CandleItem>`
- 进行多指标计算（EMA、成交量比率、RSI、布林带等）
- 根据指标信号和权重生成交易信号
- 实施风控和持仓管理逻辑

### Vegas 订单自动化处理流程

下图展示了 Vegas 订单的自动化处理流程，包括初始化、数据准备、任务调度和实际下单执行：

![Vegas 订单自动化处理流程](uml/image/place_order.png)

主要包括：
- 初始化历史数据和策略参数
- 注册策略任务到调度器
- 数据收集与指标计算
- 判断交易信号并执行下单
