# Vegas 回测策略完整逻辑（当前代码版）& 实盘落地注意事项

> 目标：把“回测里到底怎么开仓/过滤/止盈止损/平仓”的完整链路，用图表方式写清楚，方便你把同一套逻辑搬到实盘执行器里。

## 代码来源（建议对照阅读）

- 信号生成（指标→条件→打分→过滤器链）：`crates/indicators/src/trend/vegas/strategy.rs`
- 默认参数（EMA/RSI/追涨确认/极端K/MACD 等）：`crates/indicators/src/trend/vegas/config.rs`
- 信号打分与方向判定：`crates/indicators/src/trend/signal_weight.rs`
- 回测适配器：`crates/strategies/src/implementations/vegas_backtest.rs`
- 回测撮合与开/平仓（含 best_open_price 延迟开仓逻辑）：`crates/strategies/src/framework/backtest/signal.rs`
- 仓位与止盈止损落地（开仓时写入仓位字段）：`crates/strategies/src/framework/backtest/position.rs`
- 风控/出场优先级（同一根K线“先止损后止盈”）：`crates/strategies/src/framework/backtest/risk.rs`
- 回测执行器（Vegas 风控收紧已配置化为 `risk_config.tighten_vegas_risk`）：`crates/orchestration/src/backtest/executor.rs`

---

## 一图看懂：从 K 线到交易的总流程

```mermaid
flowchart TD
  A[输入: 已确认K线 candle(ts,o,h,l,c,v)] --> B[IndicatorCombine 更新指标值]
  B --> C[VegasStrategy::get_trade_signal]

  C --> C1[采集 conditions<br/>VolumeTrend / PriceBreakout / EmaTrend / RSI / Bollinger / Engulfing / Hammer<br/>（SMC/假突破等默认多为关闭，仅在 is_open 时加入）]
  C1 --> D[SignalWeightsConfig::calculate_score]
  D --> E{total_weight >= min_total_weight(默认2.0)<br/>且 Long信号数 > Short信号数 ?}
  E -- 否 --> X[无开仓信号]
  E -- 是 --> F[产生 should_buy/should_sell + direction<br/>并给出 ATR 止损(1.5xATR)]

  F --> G[过滤器链（可能撤销信号/改止盈止损）<br/>EMA_DISTANCE_FILTER → CHASE_CONFIRM<br/>贴线极小止损 → EXTREME_K_FILTER → RANGE_TP(可选) → MACD动量过滤]
  G --> I[deal_signal（撮合/持仓管理）]

  I --> J{当前有持仓?}
  J -- 无 --> K[开仓（默认全仓：funds/open_price）<br/>若 best_open_price 有值：进入“等待触发价”]
  J -- 有/同向 --> L[更新仓位里的止盈止损参数]
  J -- 有/反向 --> M[先跑一次风控检查/平仓<br/>再反向开仓（可被 BTC veto 拦截）]

  K --> N[每根K线：check_risk_config（先止损后止盈）]
  L --> N
  M --> N
  N --> O{触发任一出场条件?}
  O -- 是 --> P[close_position 记录交易 + 更新资金/胜率/手续费]
  O -- 否 --> Q[继续持仓等待下一根K线]
```

---

## 信号“打分→方向”规则（非常关键）

### 方向判定（不是按权重，而是按“信号数量”投票）

`SignalWeightsConfig::calculate_score` 的方向规则是：

- `is_long_nums > is_short_nums` → `direction = Long`
- `is_long_nums < is_short_nums` → `direction = Short`
- 相等 → `direction = None`（即使总分够，也不开仓）

### 开仓阈值

- `total_weight >= min_total_weight` 才会进入“可能开仓”的分支  
- 默认 `min_total_weight = 2.0`（意味着通常需要至少两个方向一致的条件成立）

---

## 当前回测信号的“条件来源”与默认参数（表格）

> 说明：是否参与打分/过滤以配置为准（`VegasStrategy` / `BasicRiskStrategyConfig` 的 JSON）；`VegasStrategy::new` 只是“缺省值”。

| 模块 | 在 `get_trade_signal` 的作用 | 默认关键参数（当前代码） | 默认是否参与本策略 |
|---|---|---:|---|
| `VolumeTrend` | 放量判断 → 作为一个条件进打分 | `volume_bar_num=4`, `volume_increase_ratio=2.0` | 参与 |
| `SimpleBreakEma2through` | 价格突破 EMA2（默认EMA144）| `ema2=144`, `ema_breakthrough_threshold=0.003` | 参与 |
| `EmaTrend` | 均线排列 + 回调触碰趋势位 | EMA 组：`12/144/169/576/676/2304/2704` | 参与 |
| `Rsi` | 只要 RSI 有效就入条件；若识别到极端事件则直接“不交易”返回 | `rsi_length=9`, `oversold=15`, `overbought=85` | 参与 |
| `Bolling` | 触碰上下轨给出多/空候选；含“小实体大上下影”过滤 | 见 `BollingBandsSignalConfig::default()` | 参与 |
| `Engulfing` | 满足吞没+实体占比 → 作为方向条件 | `body_ratio>0.4` | 参与 |
| `KlineHammer` | 锤子/吊人形态（并带趋势/低量过滤）→ 作为方向条件 | `up_shadow_ratio=0.6`, `down_shadow_ratio=0.6` | 参与 |
| `RangeFilter` | **不改开仓**，只在震荡时把止盈目标压小（更快落袋） | `bb_width_threshold=0.03`, `tp_kline_ratio=0.6` | 可选（`range_filter_signal.is_open`） |
| `LegDetection` | 腿部识别 → 作为方向条件进打分 | `size=5` | 默认不参与（`is_open=false`） |

---

## 过滤器链（开仓后还可能被“撤销”）

下表按 `get_trade_signal` 中的执行顺序梳理（撤销信号时会写 `filter_reasons`）：

| 过滤器 | 作用 | 默认开关/参数 |
|---|---|---|
| `EMA_DISTANCE_FILTER_SHORT` | 根据价格与均线距离状态过滤空头方向 | 始终计算；触发时撤销 `should_sell` |
| `CHASE_CONFIRM_FILTER` | 当价格远离 EMA144 时，要求额外确认（回调触线/大实体/吞没） | `chase_confirm_config.enabled=true`（阈值见配置） |
| `EXTREME_K_FILTER_*` | 大实体跨多条 EMA：只允许顺势；反向信号直接撤销 | `extreme_k_filter_signal.is_open=true`（阈值见配置） |
| `RANGE_TP` | 震荡时压缩 TP（仅调整止盈目标） | `range_filter_signal.is_open=true` 且有 Bolling |
| `MACD_*` | 允许逆势，但禁止“接飞刀/摸顶”：动量还在恶化就撤销 | `macd_signal.is_open=true`（默认 `fast=12、slow=26、signal=9`，`filter_falling_knife=true`） |

---

## 出场（止损/止盈）优先级图：同一根K线“先止损后止盈”

```mermaid
flowchart TD
  A[有持仓] --> B[check_risk_config]
  B --> S1{1 最大亏损止损?}
  S1 -- 是 --> X[平仓]
  S1 -- 否 --> S2[2 保本止损激活（不一定平仓）]
  S2 --> S3{3 单K振幅止损(1R)?}
  S3 -- 是 --> X
  S3 -- 否 --> S4{4 移动止损(三级ATR/保本)?}
  S4 -- 是 --> X
  S4 -- 否 --> S5{5 信号K线止损?}
  S5 -- 是 --> X
  S5 -- 否 --> T1{6 三级ATR止盈(5xATR)?}
  T1 -- 是 --> X
  T1 -- 否 --> T2{7 ATR比例止盈?}
  T2 -- 是 --> X
  T2 -- 否 --> T3{8 固定信号线比例止盈?}
  T3 -- 是 --> X
  T3 -- 否 --> T4{9 指标动态止盈?}
  T4 -- 是 --> X
  T4 -- 否 --> T5{10 逆势回调止盈?}
  T5 -- 是 --> X
  T5 -- 否 --> C[继续持仓]
```

### 两个“回测实现细节”会影响实盘一致性

1. **手续费模型**：`close_position` 里按 `quantity * open_price * 0.0007` 扣一次手续费（注释写“开/平各一次”，但实现是一次）。实盘需要用交易所实际费率 + 双边扣费（以及滑点）。
2. **开仓价格语义**：默认信号的 `open_price = last.c`（收盘价）；若 `best_open_price` 有值会等待下一根K线触发到该价位才开仓。实盘必须决定：是“收盘市价立即成交”还是“挂单等待”。

---

## 实盘落地注意事项（建议按清单逐条对齐回测）

### 数据与时序

- **只用已确认K线**：回测用的是 confirmed candles；实盘必须等 4H K 线收盘确认后再计算信号，否则会出现“回测有、实盘没有/反复变”的信号漂移。
- **时间戳/时区一致**：确保 `ts` 的粒度与对齐方式一致（本工程用毫秒时间戳），避免少/多一根K线导致最后“结束平仓”差异。

### 交易执行

- **成交模型**：回测默认“用收盘价开仓”近似；实盘需要处理滑点、盘口深度、部分成交、撤单重挂。
- **仓位管理**：回测是 `position_nums = funds/open_price`（全仓单向）；实盘建议改为“固定风险/固定仓位比例”，并明确杠杆与保证金模式。

### 风控一致性

- **同一根K线触发顺序**：回测严格“先止损后止盈”；实盘要用“极值触发”来模拟（Long 用 low 判断止损，用 high 判断止盈；Short 相反）。
- **开关统一走配置**：上线前把 `VegasStrategy` 与 `risk_config` 固化到同一套配置源（DB/文件/远端配置），避免依赖环境变量导致线上漂移。

### 推荐：实盘最小一致性开关集合

> 先追求“实盘复刻回测”，再做增强。

- `risk_config.tighten_vegas_risk=true/false`（是否统一收紧风控）
- `risk_config.validate_signal_tp=true/false`（止盈价有效性校验）
- `risk_config.dynamic_max_loss=true/false`（高波动时动态收紧最大亏损）
- `vegas_strategy.chase_confirm_config.enabled=true/false`（追涨追跌确认）
- `vegas_strategy.extreme_k_filter_signal.is_open=true/false`（极端K过滤）
- `vegas_strategy.range_filter_signal.is_open=true/false`（震荡 TP 压缩）

---

## 基线回测验证（本次改动后）

- 运行命令：`IS_BACK_TEST=true ENABLE_SPECIFIED_TEST_VEGAS=true IS_RUN_SYNC_DATA_JOB=false cargo run -p rust-quant-cli`
- 最新回测：
  - `back_test_id=5686`（ETH-USDT-SWAP 4H）：Sharpe 1.1474，年化 69.06%，MaxDD 41.91%
  - `back_test_id=5687`（BTC-USDT-SWAP 4H）：Sharpe -0.5280，年化 -21.14%，MaxDD 78.09%



检查顺序（同一K线内止损优先于止盈）:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
止损（优先级高）:
  1. 最大亏损止损 (max_loss_percent)         ← 实盘 ✅
  2. 保本移动止损激活 (1.5R触发)             ← 实盘 ✅ (RealtimeRiskEngine)
  3. 单K振幅固定止损(1R)                    ← 实盘 ❌
  4. 移动止损 (三级ATR系统)                 ← 实盘 ❌
  5. 信号K线止损                           ← 实盘 ✅ (开仓时附带)
止盈:
  6. 三级ATR止盈 (5倍ATR完全平仓)            ← 实盘 ❌
  7. ATR比例止盈                           ← 实盘 ✅ (LIVE_ATTACH_TP)
  8. 固定信号线比例止盈                     ← 实盘 ❌
  9. 动态止盈 (指标动态止盈)                 ← 实盘 ❌
 10. 逆势回调止盈                          ← 实盘 ❌
