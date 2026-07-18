# Vegas 与 NWE 策略回测问题分析

## 概述

Vegas 与 NWE 是系统中最老的两组策略,已经在生产跑了较长时间。它们都接入了统一的 `run_indicator_strategy_backtest` pipeline,但在**止盈目标硬编码、live/backtest 口径、动态调整可审计性**方面存在与新策略相同甚至更严重的问题。

## Vegas 策略回测链路

```
VegasBacktestAdapter (vegas_backtest.rs)
  └─ impl IndicatorStrategyBacktest
      ├─ min_data_length: strategy.min_k_line_num (默认 7000!)
      ├─ init_indicator_combine: strategy.get_indicator_combine()
      ├─ build_indicator_values: get_multi_indicator_values(combine, candle)
      └─ generate_signal: strategy.get_trade_signal(candles, values, weights, risk)
           └─ indicators/trend/vegas/strategy/trade_signal.rs
```

**live 执行**走 `vegas_executor.rs::execute`,也调用同一个 `strategy.get_trade_signal(...)`。核心信号生成**完全统一**,但:
- live 用全局 `IndicatorManager` 滚动更新指标
- backtest 用 pipeline 滑窗 + 每根 K 线重算

## NWE 策略回测链路

```
NweStrategy (nwe_strategy.rs)
  └─ impl IndicatorStrategyBacktest
      ├─ min_data_length: config.min_k_line_num.max(config.nwe_period)
      ├─ init_indicator_combine: combine_indicator.clone()
      ├─ build_indicator_values: combine.next(candle).into()
      └─ generate_signal: self.get_trade_signal(candles, values, risk)
```

live 执行走 `nwe_executor.rs`,同样调 `strategy.get_trade_signal`,口径一致。

## 已发现的关键问题

### 1. **NWE 硬编码三档止盈 R 倍数为 1.5 / 2.0 / 3.0,与 `atr_take_profit_ratio` 完全解耦**

`nwe_strategy.rs:524-548`:

```rust
if let Some(atr_ratio) = risk_config.atr_take_profit_ratio {
    if atr_ratio > 0.0 {
        // 第一级：1.5倍ATR
        let level_1 = current_price + stop_distance * 1.5;
        signal_result.atr_take_profit_level_1 = Some(level_1);
        // 第二级：2倍ATR
        let level_2 = current_price + stop_distance * 2.0;
        signal_result.atr_take_profit_level_2 = Some(level_2);
        // 第三级：3倍ATR
        let level_3 = current_price + stop_distance * 3.0;
        signal_result.atr_take_profit_level_3 = Some(level_3);
    }
}
```

**问题**:
- `atr_take_profit_ratio` 只用来判断"是否启用止盈",**不控制 R 倍数**——与字段语义矛盾。
- 回测想调整目标 R 时无法通过 `BasicRiskStrategyConfig` 改动,必须直接改代码。
- 做空分支同样硬编码 `-1.5 / -2.0 / -3.0`(line 552-568)。

**影响**:回测找到的最优 R 倍数无法落地到 live,与 scalper 旧版完全相同的双口径风险。

### 2. **NWE 的 `volatility_sensitivity` 与 `dynamic_atr_adjustment` 在回测中只能改代码**

`nwe_strategy.rs:321`:

```rust
let scalar = (1.0 + (volatility_ratio - 1.0) * self.config.volatility_sensitivity).clamp(0.6, 2.0);
```

这两个参数控制 NWE 带宽与 ATR 止损的动态调整,但:
- 它们在 `NweStrategyConfig` 里有字段,但回测时想扫描不同值**没有 tuning 结构体**。
- 唯一的回测入口 `NweStrategy::run_test` 只接受 `BasicRiskStrategyConfig`,NWE 专属参数无法注入。
- 要改这两个值,必须改 `NweStrategyConfig::default()`,影响所有调用方。

**对比** bear_short / scalper:它们有专属的 `*BacktestTuning` 结构体,扫参数时不改默认配置。

### 3. **Vegas 的 `min_k_line_num` 默认 7000,远超 bear/scalper/nwe 的预热窗口**

`indicators/trend/vegas/strategy.rs:122`:

```rust
min_k_line_num: 7000,
```

加上 pipeline 强制的 500 根预热,Vegas 回测**实际需要至少 7500 根 K 线才开始产出信号**。这是:
- 5m K 线: 7500 * 5 / 60 = 625 小时 = **26 天**
- 15m K 线: 7500 * 15 / 60 = 1875 小时 = **78 天**

**问题**:
- Vegas 回测 fixture 如果不够长,永远不会开仓——但测试不会失败,只是 `open_trades=0`。
- 7000 这个值来自早期保守设定,现在的 pipeline 已经用滑窗 + 容量上限管理预热,不需要策略自己强制这么长的窗口。
- bear/scalper 都是 96 根(bear)或 48 根(scalper trend_slow_window),差了近 100 倍。

**影响**:Vegas 回测成本极高,且"回测通过但 live 无信号"的风险被预热窗口掩盖。

### 4. **Vegas 的动态止损调整(`dynamic_adjustments`)写入 `signal_result`,但回测 pipeline 不消费它**

`trade_signal.rs:685-687`:

```rust
signal_result
    .dynamic_adjustments
    .push("MACD_NEAR_ZERO_TIGHTEN_SHORT_STOP".to_string());
```

Vegas 会在信号里塞 `MACD_NEAR_ZERO_TIGHTEN_SHORT_STOP`、`MACD_NEAR_ZERO_TIGHTEN_LONG_STOP`、`LARGE_ENTITY_RETRACEMENT_SL` 等动态调整标记,但:
- `deal_signal` / `open_long_position` / `open_short_position` 只读 `signal_kline_stop_loss_price` 与 `atr_stop_loss_price`,**不解析 `dynamic_adjustments`**。
- 这些标记只在 `audit_trail.signal_snapshots` 里留痕,回测本身的 `TradeRecord` 不反映"是否因 MACD near zero 而收紧止损"。
- live 执行同样只看 `signal_kline_stop_loss_price`,动态调整的**审计价值有,但实际约束力为零**。

**对比** bear/scalper:它们的 `dynamic_adjustments` 包含 `HALF_RISK` / `REDUCE_SIZE_NO_OI_CONFIRMATION`,且 live 执行层(`execution_worker`)会读取并应用——这是真约束,不仅仅审计。

### 5. **Vegas 的 Fib 过滤、EMA 距离过滤、large entity 止损都不可回测扫参**

Vegas 有大量配置字段:
- `fib_retracement_signal: FibRetracementSignalConfig`
- `entry_block_config: EntryBlockConfig`
- `ema_distance_config: EmaDistanceConfig`
- `large_entity_stop_loss_config: LargeEntityStopLossConfig`

它们都在 `VegasStrategy` 结构体里,但:
- 没有对应的 `VegasBacktestTuning` 让回测扫描这些参数。
- 要改 Fib 的 `strict_major_trend_block_counter_enable`,只能改 `VegasStrategy::default()`。
- 回测想评估"关闭 EMA 距离过滤后频率变化",必须手动构造不同 `VegasStrategy` 实例,无法批量扫。

**对比** bear/scalper:tuning 结构体让"研究参数"与"live 配置"分离,改 tuning 不影响默认配置。

### 6. **NWE 的 Vegas EMA 过滤固定 169 周期,不可配置**

`nwe_strategy.rs:171`:

```rust
vegas_ema_indicator: Some(EmaIndicator::new(169, 576, 676, 2304, 2704, 2704, 2704)),
```

这是 NWE 借用 Vegas 的 EMA 趋势过滤,但参数写死在构造函数里,想改只能改代码。与 Vegas 的 `ema_signal: Option<EmaSignalConfig>` 不一致(Vegas 可以通过配置关掉或调周期)。

### 7. **Market Velocity event backtest 有自己的 500 根预热硬编码,与 pipeline 的 500 重复**

`market_velocity_event_backtest/equity.rs:19`:

```rust
const FRAMEWORK_SIGNAL_WARMUP_CANDLES: usize = 500;
```

Line 1992-1993:

```rust
let mut items = Vec::with_capacity(candles.len() + FRAMEWORK_SIGNAL_WARMUP_CANDLES);
for offset in (1..=FRAMEWORK_SIGNAL_WARMUP_CANDLES).rev() {
```

这是在调用 `run_indicator_strategy_backtest` **之前**手动复制前 500 根 K 线,但 pipeline 内部 `SignalStage` 又有一遍 `i < 500` 的预热跳过——**重复预热,实际需要 1000 根**。

**问题**:
- 如果 event backtest fixture 只有 600 根,pipeline 内部跳过前 500,只剩 100 根真跑——但调用方以为已经预热过了。
- market_velocity 的回测逻辑与 bear/scalper/nwe 走的是同一套 pipeline,**不应该有自己的预热逻辑**。

### 8. **NWE 在回测与 live 都用 `combine_indicator.clone()`,但 clone 成本未量化**

`nwe_strategy.rs:665`:

```rust
fn init_indicator_combine(&self) -> Self::IndicatorCombine {
    self.combine_indicator.clone()
}
```

NWE 的 `IndicatorCombine` 包含 `StcIndicator`、`ATR`、`VolumeRatioIndicator`、`KlineHammerIndicator` 等多个有状态指标,每次 backtest 都 clone 一份。

**问题**:
- Vegas 的 `get_indicator_combine` 是**现场构造**新实例,不依赖字段。
- NWE 把 `combine_indicator` 当字段存,每次 clone 会拷贝所有指标的内部队列/缓冲。
- 如果回测跑 10k 根 K 线,clone 10k 次,成本不可忽略。

**改进方向**:要么像 Vegas 一样现场构造,要么让 `NweIndicatorCombine` 实现 `Default` 并在 adapter 内每次 `new`。

## 与新策略(bear/scalper)的对比

| 维度 | Vegas / NWE (老策略) | bear_short / scalper (新策略) |
|---|---|---|
| R 倍数配置 | 硬编码在 `get_trade_signal` | 通过 tuning → reasons → apply_*_signal,live/backtest 统一 |
| 专属参数回测扫描 | 无 tuning 结构体,改默认配置 | 有 `*BacktestTuning`,研究/live 分离 |
| 预热窗口 | Vegas 7000(过长),NWE 合理 | 96 / 48,合理 |
| dynamic_adjustments | 只审计,不约束 live 执行 | live 执行层读取并应用(如 HALF_RISK) |
| 合成市场上下文 | 无显式开关 | `allow_synthetic_market_context` 锁定 |
| 止损参数 | 散落在 config 字段 | 集中在 thresholds / tuning |

## 推荐改造优先级

按影响面从高到低:

| # | 改动 | 风险 | 收益 |
|---|---|---|---|
| A | NWE 的三档 R 倍数改成从 `risk_config` 或新增 tuning 读取 | 中,需加 `NweBacktestTuning` | 消除双口径,回测结果可落地 |
| B | Vegas `min_k_line_num` 从 7000 降到合理值(如 200) | 低-中,需验证既有回测 | 大幅降低回测成本,提升迭代速度 |
| C | NWE / Vegas 新增 `*BacktestTuning`,把配置字段迁移进去 | 中高,API 变更 | 研究/live 配置分离,扫参不污染默认 |
| D | market_velocity 删掉自己的 500 根预热,信任 pipeline | 低,只删冗余代码 | 消除重复预热,fixture 更短 |
| E | NWE `combine_indicator.clone()` 改成现场构造或 Default | 低,性能优化 | 降低回测内存/CPU |
| F | Vegas / NWE 的 `dynamic_adjustments` 接入 live 执行层 | 高,触及 execution_worker | 动态调整变成真约束 |

**F 项风险最高**,因为 Vegas / NWE 已经在生产跑,它们的 `dynamic_adjustments` 可能只是"审计标记",并非设计成 execution 层的强制约束。改动前需要先确认:现有 live 持仓有没有依赖"不读 dynamic_adjustments"的行为。

## 下一步

1. **NWE R 倍数双口径**(问题 A)与 scalper 旧版完全相同,应优先修复——这是最明确的 bug。
2. **Vegas 预热窗口**(问题 B)可以先在测试环境验证降到 200 后信号频率变化,确认不破坏既有策略逻辑。
3. 其他问题(C/D/E/F)可以在新一轮迭代中逐个处理,不阻塞当前 bear/scalper 的改造交付。
