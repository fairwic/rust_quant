# Vegas 策略迭代日志

说明：本文件内容来自仓库 `docs/VEGAS_ITERATION_LOG.md`。

- 为避免单行超长（Read 工具会截断 >2000 字符的行），少量包含超长 INSERT/JSON 的行在此版本中已用“（省略）”替代。
- 需要完整原文时，直接读取：`docs/VEGAS_ITERATION_LOG.md`。

---

## 2026-04-08 迭代闸门规则（新增）

- 默认直接执行编译、回测、结果对比和日志记录；只有遇到判断走不通或需要切换大方向时，才停下来确认。
- 先在 `ETH` 当前基线里确认新规则/止损/形态修正是否**净正向**，再做跨币种复核。
- 只有在 `ETH` 正向且 `BTC / SOL / BCH` 未被明显拖坏后，才允许升级正式基线与默认配置。
- 只修正单个样本、但整体回测退化的改动，不得写成“推荐基线 / 已确认基线”。
- ETH 尚未确认正向时，不允许直接扩到其他币种继续调优并升级结论。

## 2026-04-15 波动性分层普适性规则（新增）

- 跨币种普适性验证默认按 `BTC / ETH / 其他币种` 三层处理，因为 `BTC 波动性 < ETH 波动性 < 其他币种`。
- 允许同一结构因子按三层做参数微调，不再强求一套参数跨所有币种同时最优。
- 但结论必须分开写：
  - `单参数通用`
  - `分层参数通用`
  - `仅 ETH 有效`
- 若只有 `ETH` 有效，而其他层在合理分层调参后仍不成立，则不得升级为跨币种基线或写成“通用因子”。
- 后续日志若涉及普适性，必须同时记录：
  - 各层参数差异
  - 各层回测结果
  - 最终属于哪一级结论

---

### 2026-04-08: 吞没形态最小回滚版 A/B（仅保留 `body/range < 0.5` 视为非吞没）

- 本轮已回滚吞没链路的大改动，只保留一条最小规则：若吞没 candle 的 `body / range < 0.5`，则直接视为非吞没。
- ETH 对照结果：
  - `1279`: `52.39% / 7127.48 / 2.9465 / 35.77%`
  - `1302`: `52.39% / 4569.84 / 2.4988 / 36.53%`
- 目标样本 `2026-03-25 20:00:00` 的 ETH short 在 `1302` 中已不存在，说明该规则确实过滤掉了该小实体长影线吞没。
- 但 ETH 整体仍非净正向，因此：
  - 不升级为正式 ETH 基线
  - 不进入跨币种晋级
  - 只记录为“已验证但拒绝晋级”的候选规则

---

### 2026-04-08: 吞没缩量 50% 不作为止损信号（已验证，拒绝晋级）

- 本轮恢复吞没基线识别，只测试一条更窄规则：
  - 若当前吞没 K 线成交量相对前一根 K 线缩到 `50%` 或以下，则不允许它作为 `Engulfing_*` 止损信号。
- ETH 对照结果完全一致：
  - `1279`: `52.39% / 7127.48 / 2.9465 / 35.77%`
  - `1306`: `52.39% / 7127.48 / 2.9465 / 35.77%`
- 目标样本东八区 `2026-03-25 20:00:00` 的 ETH short 没有变化，仍在 `2026-03-26 00:00:00` 以 `Engulfing_Volume_Confirmed` 止损离场。
- 新规则在 `1306` 中只命中了 1 笔交易，且该笔本来是盈利单（`+16.36`），因此没有形成净正向优化。
- 结论：
  - 撤销实验代码，回到原基线实现
  - 不升级 ETH 基线
  - 不进入跨币种晋级

---

### 2026-04-08: 吞没缩量超过 40% 不作为止损信号（已验证，拒绝晋级）

- 在 `50%` 阈值未命中目标样本后，本轮把阈值放宽到 `current_volume / prev_volume <= 0.6`。
- ETH 对照结果仍完全一致：
  - `1279`: `52.39% / 7127.48 / 2.9465 / 35.77%`
  - `1310`: `52.39% / 7127.48 / 2.9465 / 35.77%`
- 目标样本东八区 `2026-03-25 20:00:00` 的 ETH short 仍未变化，继续在 `2026-03-26 00:00:00` 以 `Engulfing_Volume_Confirmed` 止损离场。
- 新增命中的仍然只有 1 笔，且依旧是盈利单 `+16.36`，所以这轮没有形成净正向优化；`BCH` 还轻微变差。
- 结论：
  - 撤销实验代码，回到原基线实现
  - 不升级 ETH 基线
  - 不进入跨币种晋级

---

### 2026-01-26: strict_major_trend=false A/B（验证收益是否变差）

#### 实验记录（ETH-USDT-SWAP 4H, min_trend_move_pct=0.2）

| Backtest ID | strict_major_trend | 胜率   | 利润     | Sharpe | 最大回撤 |
| ----------- | ------------------ | ------ | -------- | ------ | -------- |
| 111         | true               | 47.44% | $1801.49 | 1.626  | 38.03%   |
| 114         | false              | 47.44% | $1787.18 | 1.620  | 38.03%   |

#### 结论

- strict_major_trend=false 利润小幅下降（-14.31），当前仍以 strict_major_trend=true 为优先。
- 已将 DB 配置恢复为 `strict_major_trend=true`。

---

### 2026-01-26: Fib 大趋势过滤波动阈值优化（修复 ID 71 逆势做多）

#### 背景

- **问题**：回测 `back_test_id=71` 在 `2026-01-22 00:00:00` 出现逆势做多，EMA 明显空头排列，MACD 负值，腿部检测显示下跌腿，但策略权重系统仍触发做多（亏损 $86.23）。
- **根因**：权重系统缺少"大趋势否决权"，Fib 回撤模块未启用。

#### 方案

- **新增 `min_trend_move_pct` 参数**：只有当 swing 范围 `(high - low) / low` 超过该阈值时，才应用 `strict_major_trend` 过滤。
- **目的**：避免过滤小波动行情中的盈利单，同时精准拦截大趋势波动中的逆势信号。

#### 实验记录（ETH-USDT-SWAP 4H）

| Backtest ID | 阈值         | 胜率   | 利润     | Sharpe | 最大回撤 |
| ----------- | ------------ | ------ | -------- | ------ | -------- |
| 71          | 无过滤(基线) | 47.00% | $1835.85 | 1.672  | 38.03%   |
| 73-78       | 5%-20%       | 47.44% | $1862.34 | 1.651  | 38.03%   |

#### 关键发现

- 2026-01-22 的 swing 范围为 **18.85%**，因此所有测试阈值（5%-20%）都能触发过滤
- 该交易从**错误做多(-$86.23)**改为**正确做空(+$65.89)**，单笔差异 **+$152**
- 整体利润提升 **+$26.49**

#### 代码变更

1. `config.rs`：`FibRetracementSignalConfig` 新增 `min_trend_move_pct: f64`（默认 0.08）
2. `strategy.rs`：Fib 严格大趋势过滤逻辑增加波动幅度判断

#### 推荐配置

```json
"fib_retracement_signal": {
    "is_open": true,
    "strict_major_trend": true,
    "min_trend_move_pct": 0.08
}
```

---

### 2026-01-25: Fib 回撤入场 + 顺大趋势过滤（修复 15655 逆势开仓）

#### 背景

- **问题**：回测 `back_test_id=15655` 在 `2026-01-22 00:00:00` 出现逆势开多（long），与预期“**大趋势跌 + 小趋势跌 + 反弹到 Fib 位放量做空**”不符。
- **备注**：该时间为 K 线起始时间（4H），图表常见显示为上海时间 `00:00`；若用 UTC 展示会出现时区差异，但本质问题是**方向不对**。

#### 方案

- **新增 `fib_retracement_signal`**：基于 Swing 高低点 + Fib 回撤区间（默认 `0.328~0.618`）+ 放量确认 + 大趋势/腿部方向一致，生成回撤入场信号。
- **严格大趋势过滤（只禁开仓）**：`strict_major_trend=true` 时，仅记录 `filter_reasons`（禁开反向仓），不在策略层强行清空 `should_buy/should_sell`，避免影响“反向信号平仓”链路。
- **回测引擎行为**：当反向信号被趋势过滤时，允许用于平仓，但不反手开新仓（避免被逆势信号来回打脸）。

#### 实验记录（ETH-USDT-SWAP 4H）

| Backtest ID | 关键变更                                                     | 胜率       | 利润        | Sharpe    | 最大回撤   |
| ----------- | ------------------------------------------------------------ | ---------- | ----------- | --------- | ---------- |
| 15655       | 基线（无 Fib 回撤入场）                                      | 47.13%     | 1923.02     | 1.710     | 38.03%     |
| 15666       | 关闭信号 K 线止损（`is_used_signal_k_line_stop_loss=false`） | 52.09%     | 120.10      | 0.403     | 40.71%     |
| 15672       | Fib：`swing_lookback=120`、`min_volume_ratio=2.0`            | 53.12%     | 175.25      | 0.542     | 40.16%     |
| 15673       | 同 15672，但 `strict_major_trend=false`                      | 53.16%     | 1129.08     | 1.217     | 37.31%     |
| 15674       | 同 15673，且恢复 `is_used_signal_k_line_stop_loss=true`      | 47.91%     | 2275.59     | 1.861     | 38.03%     |

#### 结果

- `15655 @ 2026-01-22 00:00:00`：开仓方向为 **long**（错误）。
- `15672 @ 2026-01-22 00:00:00`：开仓方向为 **short**（符合预期），平仓类型为 `反向信号触发平仓(趋势过滤)`。
- **盈利下降原因定位**：`strict_major_trend=true` 会硬过滤大量“逆大趋势”信号（约 748 次），导致开仓次数从 664 降到 369，且主要盈利来源（反向平仓/ATR 止盈）显著减少，从而出现“胜率提升但利润大幅下降”。
- **当前利润优先推荐**：采用 `15674`（`strict_major_trend=false` + `is_used_signal_k_line_stop_loss=true`），在修复该处方向错误的同时，利润已超过基线 `15655`。

---

### 2026-01-22: 成交量确认形态止损(重大突破)

#### 背景与目标

- **问题**: 形态止损(吞没/锤子线)胜率低(17%~25%),被正常回调频繁触发
- **假设**: 成交量放大时形态更可靠,无量形态多为假突破
- **目标**: 只在成交量 > 1.5 倍均量时启用形态止损

#### 实验记录

| Backtest ID | 配置           | 胜率       | 利润     | Sharpe    | 年化       | 最大回撤  |
| ----------- | -------------- | ---------- | -------- | --------- | ---------- | --------- |
| 61          | 全部形态止损   | 42.67%     | 1517     | 1.585     | 87.4%      | 38.5%     |
| 62          | 关闭形态止损   | 52.75%     | 1982     | 1.559     | 98.4%      | 38.0%     |
| 66          | 成交量确认     | 47.28%     | 2480     | 1.809     | 108.3%     | 31.7%     |

#### 关键发现

- 利润: 2480 vs 1517 (+63%)
- Sharpe: 1.809 vs 1.585 (+14%)
- 最大回撤: 31.7% vs 38.5% (-18%)

#### 代码实现（摘录）

```rust
// strategy.rs: 成交量确认形态止损
let volume_confirmed = vegas_indicator_signal_values.volume_value.volume_ratio > 1.5;

if is_engulfing {
    if volume_confirmed {
        signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
        signal_result.stop_loss_source = Some("Engulfing_Volume_Confirmed".to_string());
    } else {
        signal_result.stop_loss_source = Some("Engulfing_Volume_Rejected".to_string());
    }
}
```

#### 当前基线

推荐基线: Backtest ID 66

- 胜率: 47.28%
- 利润: 2480 USDT
- Sharpe: 1.809
- 年化收益: 108.3%
- 最大回撤: 31.7%
- 配置: `is_used_signal_k_line_stop_loss=true` + 成交量确认

---

### 2026-01-22: 信号 K 线止损开关对比实验 + 止损更新历史功能实现

#### 实验记录

| Backtest ID | is_used_signal_k_line_stop_loss | 胜率       | 利润     | Sharpe    | 年化收益   | 最大回撤   |
| ----------- | ------------------------------- | ---------- | -------- | --------- | ---------- | ---------- |
| 61          | 开启                            | 42.67%     | 1517     | 1.585     | 87.44%     | 38.46%     |
| 62          | 关闭                            | 52.75%     | 1982     | 1.559     | 98.44%     | 38.03%     |

#### 当前基线

推荐基线: Backtest ID 62

- 胜率: 52.75%
- 利润: 1982 USDT
- Sharpe: 1.559
- 年化收益: 98.44%
- 最大回撤: 38.03%
- 配置: `is_used_signal_k_line_stop_loss=false`

---

### 2026-01-21: 吞没形态信号线止损优化 + KlineHammer 止损探索

#### 实验记录

| Backtest ID | 配置                       | Profit     | WR        | Sharpe   | MaxDD     | 结论             |
| ----------- | -------------------------- | ---------- | --------- | -------- | --------- | ---------------- |
| 35          | 基线（无专项止损）         | 3126 U     | 51.2%     | 1.88     | ~44%      | 利润高但回撤大   |
| 36          | 吞没+开盘价止损            | 2035 U     | 52.8%     | 1.58     | ~44%      | 止损生效但利润降 |
| 51          | 所有信号用开盘价止损       | 1335 U     | 42.0%     | 1.56     | -         | 过度止损         |
| 52          | 只吞没形态用开盘价止损     | 2002 U     | 48.1%     | 1.69     | 32.8%     | 最优解            |

---

### 2026-01-15: 从回测切到实盘（基线 5692）- 信号/止盈止损/交易所链路对齐

#### 目标

- 基线回测：`back_test_log.id = 5692`（Vegas / ETH-USDT-SWAP / 4H）。
- 开始实盘前，把“回测 → 实盘”关键差异收敛到可控开关，并补齐 OKX 真实下单路径。

---

### 2026-01-09: Shadow Trading + 风控优化 + ATR 止盈修复

（略，详见原文 `docs/VEGAS_ITERATION_LOG.md`）

---

### 2026-01-08: 极端 K 线过滤分布试验

（略，详见原文 `docs/VEGAS_ITERATION_LOG.md`）

---

### 2026-01-07: 回撤/胜率平衡与高波动探索

#### 当前基线（ID 5576）

- 配置：`ema_breakthrough_threshold=0.0026`，price_high=1.0016，price_low=0.998，RSI 14/86，min_total_weight=2.0；stop_loss（信号 K 线止损）关闭；`max_loss_percent=0.05`。
- 绩效：win_rate≈56.6%，profit≈1556.74，Sharpe≈1.4355，max_dd≈52.6%。
- 详情：包含超长 INSERT/JSON（省略，见原文 `docs/VEGAS_ITERATION_LOG.md`）。

#### 下一步优化方向（待验证）

1. 高波动自适应止损：布林带宽或 ATR/价 超阈值时临时把 `max_loss_percent` 下调到 0.045，其余时段保持 0.05。
2. 连续亏损冷却：同方向/短窗口内连续 N 次触发“最大亏损止损”后，降低仓位或冷却。
3. 长周期趋势确认（温和版）：要求日线 EMA 斜率与 4H 同向才放行。
4. 出场分层：浮盈达 1R/1.5R 减半仓，剩余仓位继续用反向信号平仓。
5. 极端波动过滤：在极端 K（>5%-8%涨跌）后一根内拒绝入场。

---

### 2026-04-08: 低 body 吞没仅禁止止损资格（已验证，拒绝晋级）

- 目标样本：`ETH 1279`，东八区 `2026-03-25 20:00:00 short`
- 先查 close 行 `stop_loss_update_history`，确认：
  - `signal_ts=1774440000000`
  - 东八区对应 `2026-03-25 20:00:00`
  - 说明这笔单是在开仓当根就挂上了 `Engulfing_Volume_Confirmed` 止损

实验内容：

- 只提高 `Engulfing` 的止损资格门槛
- 要求 `body_ratio > max(config.body_ratio, 0.5)` 才允许初始化 `Engulfing_Volume_Confirmed`
- 不改 `Engulfing` 入场识别

结果：

- `1279`：`52.3901% / 7127.48 / 2.94653 / 35.7654%`
- `1314`：`52.6419% / 5166.46 / 2.58425 / 34.6838%`

目标样本：

- `1279`: `-84.49`
- `1314`: `+199.13`

结论：

- 命中了目标样本，局部方向正确
- 但 ETH 总体 `profit`、`sharpe` 下滑，不满足 ETH 单币种净正向闸门
- 实验代码已撤回，不更新基线

新增规则：

- 分析“为什么会止损”时，必须先看 close 行 `stop_loss_update_history`
- 不能只看开仓行 `signal_value`，否则容易把止损来源误判成别的 K 线或别的信号链

---

### 2026-04-08: TooFar 弱实体吞没 short 过滤（ETH 通过，跨币种拒绝晋级）

- 目标：只拦 `TooFar + Bolling short + Engulfing + LegDetection + VolumeTrend + RSI` 且 `body_ratio < 0.48`、`fib >= 0.5`、`volume up` 的弱空头吞没。
- 回测结果：
  - `1279 -> 1318 (ETH)`: `52.39% / 7127.48 / 2.94653 / 35.77%` -> `52.59% / 7311.66 / 2.97433 / 35.77%`
  - `1280 -> 1319 (BTC)`: `54.40% / 334.79 / 1.03208 / 36.83%` -> `54.34% / 328.23 / 1.01394 / 36.83%`
  - `1281 -> 1320 (SOL)`: `43.69% / 500.31 / 1.15575 / 41.76%` -> `43.15% / 444.84 / 1.07612 / 41.76%`
  - `1282 -> 1321 (BCH)`: `36.13% / -75.39 / -0.53237 / 79.76%` -> `35.97% / -77.74 / -0.56373 / 81.70%`
- 目标 ETH 问题单 `2026-03-25 20:00:00 short` 已被过滤掉。
- 新过滤理由 `WEAK_TOO_FAR_ENGULFING_SHORT` 命中：
  - `ETH`: `2` 笔
  - `BTC`: `2` 笔
  - `SOL`: `2` 笔
  - `BCH`: `3` 笔
- 结论：
  - ETH 单币种闸门通过
  - 跨币种晋级闸门不通过
  - 实验代码已撤回，不更新正式基线
  - 后续继续收窄 ETH-only 子模式，不再直接推广全币种规则
- 补充规则：
  - `back_test_detail` 的 close 行 `signal_value` 可能为空
  - 入场形态与指标快照要查同笔的 open/long/short 行
  - close 行只看 `close_type / stop_loss_source / stop_loss_update_history`

---

### 2026-04-08: TooFar 放量锤子线 + MACD 多头状态豁免（已验证，拒绝晋级）

- 目标样本：`ETH 1279` 东八区 `2026-03-30 04:00:00`
- 样本本身：
  - `volume_ratio=2.8554`
  - `rsi=36.06`
  - `macd_line > signal_line`
  - `histogram > 0`
  - 但被 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG` 拦掉
- 实验内容：
  - 对 `TooFar` 下的锤子线 long 增加一个窄豁免
  - 用 `macd_line > signal_line && histogram > 0` 表示 `MACD` 已处于多头状态
- 回测结果：
  - `1279`: `52.3901% / 7127.48 / 2.94653 / 35.7654%`
  - `1326`: `52.1822% / 6661.46 / 2.86132 / 35.7654%`
- 目标样本变化：
  - `1279` 未开多
  - `1326` 在 `2026-03-30 04:00:00` 开多，并在 `2026-03-30 08:00:00` 以 `Signal_Kline_Stop_Loss / Engulfing_Volume_Confirmed` 盈利出场，`profit_loss=+34.8004`
- 结论：
  - 目标样本修对了
  - 但 ETH 总体退化
  - 实验代码已撤回，不更新基线

---

### 2026-04-09: TooFar 非横盘非破位下跌放量锤子线 long 豁免（ETH 通过，跨币种拒绝晋级）

- 目标样本：`ETH 1279` 东八区 `2026-03-30 04:00:00`
- 命中形态：
  - `TooFar`
  - `is_ranging_market=false`
  - `hammer_long=true`
  - `volume_ratio=2.8554`
  - `macd_line > signal_line`
  - `histogram > 0`
  - `leg_detection.is_new_leg=true`
  - 四个 bearish `BOS/CHOCH` 全为 `false`
- 实验内容：
  - 只在 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG` 这条过滤前增加极窄豁免
  - 用 TDD 先写失败测试，再补最小 helper
- 回测结果：
  - `1279 -> 1327 (ETH)`: `52.39% / 7127.48 / 2.94653 / 35.77%` -> `52.48% / 7164.95 / 2.95204 / 35.77%`
  - `1280 -> 1328 (BTC)`: `54.40% / 334.79 / 1.03208 / 36.83%` -> `54.31% / 314.92 / 0.98976 / 39.72%`
  - `1281 -> 1329 (SOL)`: `43.69% / 500.31 / 1.15575 / 41.76%` -> `43.69% / 500.31 / 1.15575 / 41.76%`
  - `1282 -> 1330 (BCH)`: `36.13% / -75.39 / -0.53237 / 79.76%` -> `36.07% / -75.80 / -0.53746 / 80.11%`
- 目标样本变化：
  - `1279` 未开多
  - `1327` 在 `2026-03-30 04:00:00` 开多，并在 `2026-03-30 08:00:00` 以 `Signal_Kline_Stop_Loss / Engulfing_Volume_Confirmed` 盈利出场，`profit_loss=+37.39`
  - `filtered_signal_log 1327` 中该时间点已不再出现 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG`
- 结论：
  - ETH 单币种闸门通过
  - 跨币种晋级闸门不通过
  - 实验代码已撤回，不更新正式基线
  - 保留为 ETH 有效但不可直接跨币种推广的候选模式

---

### 2026-04-09: 外部市场数据接入基础设施第一阶段落地

- 目标：
  - 先把策略外部特征的数据底座搭起来
  - 优先 Hyperliquid funding/meta，其次 Dune 模板，再预留 OKX/Binance
- 已落地：
  - `ExternalMarketSnapshot` 领域实体与仓储抽象
  - `external_market_snapshots` 表，唯一键 `(source, symbol, metric_type, metric_time)`
  - `HyperliquidPublicAdapter`
  - `SqlxExternalMarketSnapshotRepository`
  - `ExternalMarketSyncService`
  - `ExternalMarketDataProvider`
  - `ExternalMarketSource`
  - `normalize_external_market_symbol`
  - `ExternalMarketSyncJob`
  - `docs/external_market_data/README.md` 与 3 个 Dune SQL 模板
- 验证：
  - `cargo test -p rust-quant-domain external_market_snapshot -- --nocapture`
  - `cargo test -p rust-quant-infrastructure --test hyperliquid_adapter -- --nocapture`
  - `cargo test -p rust-quant-services --test external_market_sync -- --nocapture`
  - `cargo build --bin rust_quant`
- 当前边界：
  - Hyperliquid 已经能抓、能转、能存
  - Dune 已有模板和最小 API 执行链，但尚未挂到调度入口
  - OKX/Binance 目前只保留扩展点，尚未实现 provider
  - Hyperliquid 已切到官方 Rust SDK；当前使用官方 GitHub 仓库版本而不是 crates.io `0.6.0`，因为发布版还缺 `metaAndAssetCtxs`

### 2026-04-09: Dune 真实执行链打通并确认数据边界

- 已用真实 `DUNE_API_KEY` 跑通 `run_dune_external_sync`
- 修复项：
  - `hyperliquid_funding_basis.sql` 改为基于 `hyperliquid.market_data`
  - `results` 解析兼容缺失 `query_id`
  - 时间过滤改为 `from_iso8601_timestamp`
  - 服务层兼容 `2026-02-21 20:00:00.000 UTC` 格式
  - orchestration job 失败时不再吞错，CLI 会返回非 0
- 验证：
  - `cargo test -p rust-quant-infrastructure --test dune_client -- --nocapture`
  - `cargo test -p rust-quant-services --test dune_market_sync -- --nocapture`
  - `cargo test -p rust-quant-orchestration external_market_sync_job -- --nocapture`
  - `cargo build -p rust-quant-cli --example run_dune_external_sync`
- 实际入库：
  - `external_market_snapshots` 中新增 `4` 条 `source=dune, symbol=ETH, metric_type=hyperliquid_basis`
  - 小时点为 `2026-02-21 20:00:00 UTC` 到 `2026-02-21 23:00:00 UTC`
- 数据边界：
  - `hyperliquid.market_data` 当前 `ETH` 最晚只到 `2026-02-21 23:59:00 UTC`
  - 所以 `2026-03-30` 这类窗口返回 0 行是数据覆盖问题，不是程序问题

### 2026-04-09: Dune 模板同步已挂入主程序数据同步入口

- 新入口位于 `rust_quant_cli::app::bootstrap::run_modes`
- 开关：
  - `IS_RUN_SYNC_DATA_JOB=1`
  - `IS_RUN_DUNE_SYNC_JOB=1`
- 新增：
  - `SYNC_SKIP_MARKET_DATA=1`，仅跑外部数据同步
- 配置：
  - 单任务环境变量，或
  - `DUNE_TEMPLATE_JOBS=metric_type|symbol|template_path|start_time|end_time|performance|[min_usd]`
- 当前验证：
  - `cargo test -p rust-quant-cli parse_dune_sync_requests_from_map -- --nocapture`
  - `cargo test -p rust-quant-cli should_skip_market_data_sync_from_map -- --nocapture`
  - 主程序日志已确认会在市场数据同步后执行 `📊 执行Dune模板同步`
  - 开启 `SYNC_SKIP_MARKET_DATA=1` 后，主程序日志已确认会输出 `⏭️ 跳过市场数据同步（SYNC_SKIP_MARKET_DATA=true）`
## 2026-04-09 资金费率同步入口与一年窗口校验

- 在主程序同步入口增加 `IS_RUN_FUNDING_RATE_JOB=1`
- 支持与 `SYNC_SKIP_TICKERS=1`、`SYNC_SKIP_MARKET_DATA=1` 组合，只跑资金费率同步
- 实跑命令：
  - `IS_RUN_SYNC_DATA_JOB=1`
  - `IS_RUN_FUNDING_RATE_JOB=1`
  - `IS_RUN_DUNE_SYNC_JOB=0`
  - `SYNC_SKIP_TICKERS=1`
  - `SYNC_SKIP_MARKET_DATA=1`
  - `EXIT_AFTER_SYNC=1`
  - `SYNC_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP,BCH-USDT-SWAP'`
- 结果：
  - `ETH / BTC / SOL / BCH` 全部同步成功
  - `funding_rates` 每个交易对落库 `273` 条
  - 东八区时间范围统一为 `2026-01-08 16:00:00` 到 `2026-04-09 08:00:00`
- 结论：
- 当前 OKX `funding-rate-history` 实际只拿到近 `91` 天
- “最近一年资金费率” 不能只依赖当前 OKX 历史接口，需要后续用 Hyperliquid / Binance / Dune 补齐

### 2026-04-14: 冲突型 TooFar 新空头腿 short 低量过滤（正式基线）

- 目标样本：`ETH 1343` 东八区 `2026-03-27 00:00:00`
- 拦截条件：
  - `Bolling long`
  - `Engulfing short`
  - `LegDetection bearish && is_new_leg`
  - `ema_distance.state = TooFar`
  - `fib.in_zone = true`
  - `volume_ratio < 1.5`
- 过滤理由：
  - `CONFLICTING_TOO_FAR_NEW_BEAR_LEG_SHORT`
- 样本变化：
  - `1343` 该笔开空，`profit_loss=-81.8008`
  - `1351` 该笔不再开空，`filtered_signal_log` 已记录新过滤理由
- 回测结果：
  - `1343 -> 1351 (ETH)`: `52.2901% / 7010.05 / 2.91022 / 35.7654%` -> `52.3901% / 7090.69 / 2.92250 / 35.7654%`
  - `1344 -> 1352 (BTC)`: `54.3974% / 334.79 / 1.03208 / 36.8281%` -> `54.4861% / 340.10 / 1.04279 / 36.8281%`
  - `1345 -> 1353 (SOL)`: 无变化
  - `1346 -> 1354 (BCH)`: `36.1301% / -75.39 / -0.53237 / 79.7638%` -> `36.1921% / -75.72 / -0.53668 / 80.0387%`
- 结论：
  - ETH、BTC 正向
  - SOL 不变
  - BCH 仅边际变差
  - 升级为当前正式基线
  - 当前正式基线 ID：`1351 / 1352 / 1353 / 1354`
  - 后续若 BCH 再被同类 short 规则拖差，再单独收窄

### 2026-04-14: TooFar 反趋势锤子线 long 广义放行（已验证，拒绝晋级）

- 候选放行条件：
  - `!is_ranging_market`
  - `volume_ratio >= 2.8`
  - `RSI >= 36`
- 目标样本：
  - `ETH 1351` 东八区 `2026-03-30 04:00:00`
  - 在 `1355` 中被放行，单笔 `+35.8730`
- 回测结果：
  - `1351 -> 1355 (ETH)`: `52.3901% / 7090.69 / 2.92250 / 35.7654%` -> `52.3810% / 6756.63 / 2.88018 / 35.7654%`
  - `1352 -> 1356 (BTC)`: 边际改善
  - `1353 -> 1357 (SOL)`: 无变化
  - `1354 -> 1358 (BCH)`: 边际改善
- 结论：
  - 虽然目标样本修对，但 ETH 总体退化
  - 不通过单币种闸门
  - 实验代码已撤回
  - 正式基线保持 `1351 / 1352 / 1353 / 1354`

### 2026-04-14: 放量反转 long 与黑天鹅过滤（已验证，无效，拒绝晋级）

- 目标：
  - 验证“`volume_ratio >= 2.5` 的 TooFar 反趋势锤子线 long 应提高权重”
  - 同时过滤“极端快速下跌/黑天鹅”类样本
- 实验过程：
  - 先分析 `ETH 1351` 中 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG` 母集，确认高量样本并不天然为正
  - 两次实现过窄豁免：
    - 基于前后两根 K 线修复路径的 helper
    - 基于稳定快照字段（`new_leg / hammer body / volume / MACD`）的 helper
  - 两轮都完成：
    - 定向单测
    - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
    - `cargo build --bin rust_quant`
    - 全量 4H 回测
- 回测结果：
  - 第一轮：`1359 / 1360 / 1361 / 1362`
  - 第二轮：`1363 / 1364 / 1365 / 1366`
  - 两轮结果都与正式基线完全一致：
    - `ETH`: `52.3901% / 7090.689 / 2.92250 / 35.7654%`
    - `BTC`: `54.4861% / 340.1024 / 1.04279 / 36.8281%`
    - `SOL`: `43.6860% / 500.3075 / 1.15575 / 41.7609%`
    - `BCH`: `36.1921% / -75.7198 / -0.53668 / 80.0387%`
- 目标样本核对：
  - `filtered_signal_log 1363` 中东八区 `2026-03-30 04:00:00` 仍然只有：
    - `["EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG"]`
  - `back_test_detail 1363` 中该时间点没有开多记录
- 结论：
  - 这条“放量反转 + 黑天鹅过滤”实验没有真正命中目标样本
  - 即使逻辑在单测里成立，也没有改变回测路径
  - 按 runbook 规则，这类实验视为“已验证但无效”，必须撤回代码
  - 当前正式基线仍保持 `1351 / 1352 / 1353 / 1354`

### 2026-04-14: 全局多空冲突惩罚（已验证，拒绝晋级）

- 目标：
  - 解决 `2026-03-30 04:00:00` 这类 `LONG` 候选依赖反向 `SHORT` 权重抬分过线的问题
  - 至少在多空冲突时，不再只靠多数决决定方向
- 实验内容：
  - 在 `signal_weight` 中增加 `conflict_penalty_ratio`
  - 当 `LONG` 与 `SHORT` 同时出现时，对总分做全局扣减
  - 先试 `ETH signal_weights.conflict_penalty_ratio = 1.0`，后收窄到 `0.63`
- 结果：
  - `1371 (ratio=1.0)`：目标样本被压成 `direction=None`，但 `ETH` 跌到 `203.18 / 0.74011 / 43.6496%`
  - `1375 (ratio=0.63)`：目标样本仍被压掉，但 `ETH` 也只有 `721.21 / 1.23207 / 37.5598%`
  - 删除交易归因显示：被删 `short` 合计 `-511.3948`，被删 `long` 合计 `+1938.8090`
- 结论：
  - 这条线的根因判断是对的，但“全局冲突惩罚”作用面过大
  - 它不是只拦问题样本，而是在大量误伤盈利 `long`
  - 实验代码与 DB 配置都已撤回
  - 当前正式基线仍保持 `1351 / 1352 / 1353 / 1354`

### 2026-04-15: TooFar 反趋势锤子线 long 的 target-like 窄豁免（正式基线）

- 目标：
  - 继续处理东八区 `2026-03-30 04:00:00` 这类 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG` 被误拦的例外样本
  - 不再做宽放行，只放过更像目标样本的 `target-like` 子集
- 分析结论：
  - 上一版 `new_leg + macd_hist >= 0` 豁免虽然让 `ETH` 受益，但会把 `BTC/BCH` 的差样本一起放出来
  - 跨币种对比后，可接受的最窄公共条件收敛到：
    - `leg_detection.is_bearish_leg = true`
    - `leg_detection.is_new_leg = true`
    - `0.0 <= macd.histogram <= 3.0`
    - `kline_hammer.body_ratio >= 0.15`
    - `1.5 <= volume_ratio <= 3.0`
- 实现：
  - 在 `EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG` 之前增加 `target-like` long 豁免 helper
  - 仅对 `TooFar + short_trend + !fib.in_zone + hammer long` 的极窄子集生效
- 验证：
  - `cargo test -p rust-quant-indicators counter_trend_hammer_long_new_leg_positive_macd_candidate -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测结果：
  - `1351 -> 1383 (ETH)`: `52.3901% / 7090.689 / 2.92250 / 35.7654%` -> `52.4809% / 7127.9673 / 2.92796 / 35.7654%`
  - `1352 -> 1384 (BTC)`: 完全不变，仍为 `54.4861% / 340.1024 / 1.04279 / 36.8281%`
  - `1353 -> 1385 (SOL)`: `43.6860% / 500.3075 / 1.15575 / 41.7609%` -> `44.0273% / 549.3441 / 1.22202 / 41.7609%`
  - `1354 -> 1386 (BCH)`: 完全不变，仍为 `36.1921% / -75.7198 / -0.53668 / 80.0387%`
- 样本核对：
  - `ETH 1383` 中东八区 `2026-03-30 04:00:00` 已开多
  - `2026-03-30 08:00:00` 以 `Signal_Kline_Stop_Loss / Engulfing_Volume_Confirmed` 出场
  - `profit_loss = +37.81585584`
  - `filtered_signal_log 1383` 该时间点记录数为 `0`，说明已经被真正放行
  - 与基线对比，`ETH 1383` 仅新增这一笔；`SOL 1385` 仅新增一笔 `2025-11-07 20:00:00 -> 2025-11-11 12:00:00, +37.12344533`
- 结论：
  - 这是当前可接受的最小正向豁免
  - `ETH` 单币种过闸门，`BTC/BCH` 不受影响，`SOL` 同向改善
  - 正式基线升级为 `1383 / 1384 / 1385 / 1386`

### 2026-04-15: EMA576 牛熊反向过滤实验（已验证，无效，拒绝晋级）

- 目标：
  - 验证事件研究提炼出的 `EMA576` 因子，能否先作为反向过滤器提升基线
  - 具体尝试：
    - `bull + touch EMA576 + close above EMA576 + green + vol>=1.2` 时拦截 `short`
    - `bear + touch EMA576 + close below EMA576 + red + vol>=1.2` 时拦截 `long`
- 实现：
  - 在过滤链中新增 `BULL_EMA576_RECLAIM_BLOCK_SHORT`
  - 在过滤链中新增 `BEAR_EMA576_REJECT_BLOCK_LONG`
  - 配套 helper 与单测均通过
- 验证：
  - `cargo test -p rust-quant-indicators ema576_ -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测结果：
  - `1391 / 1392 / 1393 / 1394` 与正式基线 `1383 / 1384 / 1385 / 1386` 完全一致
  - 所有指标无变化
- 根因分析：
  - `filtered_signal_log` 中新增过滤理由命中数均为 `0`
  - 说明这两个因子虽然在全市场事件研究里有统计优势，但在当前 Vegas 候选链上没有形成“需要被拦的反向交易”
  - 进一步核对后，按同样条件筛选当前 `filtered_signal_log` 的同向候选，`ETH/BTC` 的样本反而是负值，说明“事件优势”并不能直接平移到现有候选信号
- 结论：
  - 这条线不是坏方向，而是切入点不对
  - 若继续利用 `EMA576/696` 因子，下一步应该考虑“新增独立入场因子/加权”，而不是继续做反向过滤
  - 实验代码已撤回，正式基线保持 `1383 / 1384 / 1385 / 1386`

### 2026-04-15: MACD 零轴附近 short 形态止损收紧（正式基线）

- 目标：
  - 回到“MACD 附近是否应加严/放松止损”的原始方向，先用现有基线 `1383 / 1384 / 1385 / 1386` 做归因
  - 只动止损，不动开仓，避免把问题扩散到候选链
- 基线分析：
  - `ETH 1383` 的 `Signal_Kline_Stop_Loss` 中，`abs(macd.histogram) < 2` 的 `short` 样本整体最差
  - 该子集共 `22` 笔，合计 `-441.3943`，平均 `-20.0634`
  - 同条件 `long` 仅 `30` 笔，合计 `-11.0826`，没有同级别坏簇
  - 坏单主要集中在 `Engulfing_Volume_Confirmed` / `KlineHammer_Volume_Confirmed` 两条 short 形态止损链
- 实现：
  - 新增 helper：当 `short` 信号的形态止损已经生成，且 `abs(macd.histogram) < 2.0` 时，把初始信号止损向开仓价收紧到中点
  - 只影响：
    - `Engulfing_Volume_Confirmed`
    - `KlineHammer_Volume_Confirmed`
  - 不影响：
    - 开仓条件
    - long 止损
    - `LargeEntity` / `Fib` / `max_loss_percent`
  - 动态标记：`MACD_NEAR_ZERO_TIGHTEN_SHORT_STOP`
- 验证：
  - `cargo test -p rust-quant-indicators macd_near_zero_short_stop_should -- --nocapture`
  - `cargo test -p rust-quant-indicators macd_far_from_zero_short_stop_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测结果：
  - `1383 -> 1395 (ETH)`: `52.4809% / 7127.9673 / 2.92796 / 35.7654%` -> `52.0000% / 7660.1900 / 3.00861 / 35.0947%`
  - `1384 -> 1396 (BTC)`: `54.4861% / 340.1024 / 1.04279 / 36.8281%` -> `54.3230% / 336.7440 / 1.03625 / 37.3102%`
  - `1385 -> 1397 (SOL)`: `44.0273% / 549.3441 / 1.22202 / 41.7609%` -> `42.6174% / 680.0210 / 1.39990 / 41.4416%`
  - `1386 -> 1398 (BCH)`: `36.1921% / -75.7198 / -0.53668 / 80.0387%` -> `34.4652% / -78.5089 / -0.58197 / 82.1567%`
- 归因：
  - `ETH` 明显受益，`Signal_Kline_Stop_Loss / Engulfing_Volume_Confirmed` 总 PnL 从 `+675.3514` 升到 `+886.3627`
  - `ETH` 的 `abs(histogram) < 2` short 止损子集从 `-441.3943` 改善到 `-260.2396`
  - `SOL` 同向改善更明显，利润和 Sharpe 都抬升，回撤还略降
  - `BTC/BCH` 有轻微退化，但 near-zero short 在这两个币种里样本很少、影响有限，不构成“明显拖坏”
- 结论：
  - 这是一次“只改止损，不改开仓”的正向优化
  - `ETH` 单币种明显过闸门，`SOL` 同向改善，`BTC/BCH` 未出现足以否决的副作用
  - 正式基线升级为 `1395 / 1396 / 1397 / 1398`

### 2026-04-15: MACD 零轴附近弱锤子线 long 过滤（命中坏簇，但拒绝晋级）

- 目标：
  - 继续拆 `MACD` 零轴附近哪些 long 本身不应该开仓
  - 优先处理 `ETH 1395` 中最差的 near-zero long 簇：
    - `TooFar`
    - 无明确趋势（`!long_trend && !short_trend`）
    - `hammer long`
    - `leg` 参与
    - `!valid_engulfing`
    - `volume_ratio < 2.5`
    - `abs(macd.histogram) < 2.0`
- 实现：
  - 新增候选过滤 `MACD_NEAR_ZERO_WEAK_HAMMER_LONG_BLOCK`
  - 只拦 long，不改 short，不改止损
  - 单测验证 helper 命中与放行边界
- 验证：
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测结果：
  - `1395 -> 1399 (ETH)`: `52.0000% / 7660.1929 / 3.00861 / 35.0947%` -> `52.1154% / 7657.1768 / 3.00904 / 35.0947%`
  - `1396 -> 1400 (BTC)`: 完全不变
  - `1397 -> 1401 (SOL)`: `42.6174% / 680.0208 / 1.39990 / 41.4416%` -> `42.7119% / 859.8007 / 1.48841 / 41.4416%`
  - `1398 -> 1402 (BCH)`: `34.4652% / -78.5089 / -0.58197 / 82.1567%` -> `33.8596% / -79.4821 / -0.59854 / 83.6242%`
- 命中与归因：
  - `ETH 1395` 目标坏簇共 `5` 笔，原始总 PnL `-138.3642`
  - 新过滤在 `ETH 1399` 命中 `8` 笔，shadow PnL `-0.1039`
  - `SOL 1401` 命中 `4` 笔，shadow PnL `-0.3502`，因此 `SOL` 同向改善
  - `BCH 1402` 命中 `30` 笔，但 shadow PnL `+0.6789`，说明开始大面积误伤盈利 long
- 结论：
  - 这条规则并非无效，它确实命中了 `ETH` 的 near-zero 坏簇
  - 但 `ETH` 总利润没有净增，且 `BCH` 出现明显误伤盈利样本
  - 按当前“ETH 先净正向，再看跨币种”的闸门，这轮实验拒绝晋级
  - 代码已回滚，正式基线保持 `1395 / 1396 / 1397 / 1398`

### 2026-04-15: MACD 零轴附近弱锤子线 short 过滤（正式基线）

- 目标：
  - 继续迭代 `MACD` 零轴附近不该开的 short
  - 优先处理跨币都偏负的一类：
    - `hammer short`
    - `short_trend=true`
    - `ema_distance.state=TooFar`
    - `!valid_engulfing`
    - `abs(macd.histogram) < 2.0`
    - `volume_ratio < 1.0`
- 基线分析：
  - `ETH 1395` 中该簇 `2` 笔，合计 `-76.3545`
  - `SOL 1397` 中该簇 `1` 笔，合计 `-2.4525`
  - `BCH 1398` 中该簇 `3` 笔，合计 `-4.3203`
  - `BTC 1396` 没有命中样本
  - 目标坏单已确认命中：`ETH 2024-09-13 16:00:00`，原始 `profit_loss=-75.5353`
- 实现：
  - 新增过滤：`MACD_NEAR_ZERO_WEAK_HAMMER_SHORT_BLOCK`
  - 只拦 short，不改开仓评分，不改 long，不改止损链
  - 仅在 `TooFar + short_trend + hammer_short + near-zero MACD + low volume + 非有效 engulfing` 时生效
- 验证：
  - `cargo test -p rust-quant-indicators macd_near_zero_weak_hammer_short_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - 全量回测：
    - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
    - 因主进程在补跑时卡住，随后使用 `BACKTEST_ONLY_INST_IDS='SOL-USDT-SWAP,BCH-USDT-SWAP'` 单独补跑 `SOL/BCH`
- 回测结果：
  - `1395 -> 1403 (ETH)`: `52.0000% / 7660.1929 / 3.00861 / 35.0947%` -> `52.0992% / 7989.4326 / 3.05633 / 35.0947%`
  - `1396 -> 1404 (BTC)`: 完全不变
  - `1397 -> 1405 (SOL)`: `42.6174% / 680.0208 / 1.39990 / 41.4416%` -> `42.6174% / 701.0798 / 1.42464 / 41.4416%`
  - `1398 -> 1406 (BCH)`: `34.4652% / -78.5089 / -0.58197 / 82.1567%` -> `34.5299% / -77.2753 / -0.56594 / 81.5425%`
- 命中与归因：
  - `1403` 命中 `1` 笔 short，shadow PnL `-0.0246`
  - `1405` 命中 `2` 笔 short，shadow PnL `-0.1145`
  - `1406` 命中 `8` 笔 short，shadow PnL `-0.2610`
  - 命中的三个币种 shadow PnL 全为负，说明这条过滤在实际回测里确实是在挡亏损单
- 结论：
  - `ETH` 明显正向，利润和 Sharpe 都抬升
  - `BTC` 不受影响
  - `SOL/BCH` 同向改善，没有出现新的拖坏
  - 这条 near-zero short 过滤通过闸门，正式基线升级为 `1403 / 1404 / 1405 / 1406`

### 2026-04-15: MACD 零轴附近无趋势锤子线 short 过滤（仅命中 SOL，拒绝晋级）

- 目标：
  - 继续从 near-zero short 坏簇里筛一个更通用的 no-trend 子集
  - 候选条件：
    - `ema_distance.state = Normal`
    - `!short_trend && !long_trend`
    - `hammer_short = true`
    - `bear_leg = true`
    - `!valid_engulfing`
    - `!boll_long && !boll_short`
    - `abs(macd.histogram) < 2.0`
    - `volume_ratio < 2.5`
- 实现：
  - 新增候选过滤 `MACD_NEAR_ZERO_NO_TREND_HAMMER_SHORT_BLOCK`
  - 只拦 short，不改打分、不改止损
- 验证：
  - `cargo test -p rust-quant-indicators macd_near_zero_no_trend_hammer_short_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - 使用分批回测：
    - `BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP' ...`
    - `BACKTEST_ONLY_INST_IDS='SOL-USDT-SWAP,BCH-USDT-SWAP' ...`
- 回测结果：
  - `1403 -> 1408 (ETH)`: 完全不变
  - `1404 -> 1410 (BTC)`: 完全不变
  - `1405 -> 1407 (SOL)`: `42.6174% / 701.0798 / 1.42464 / 41.4416%` -> `42.7609% / 936.2532 / 1.63586 / 29.8279%`
  - `1406 -> 1409 (BCH)`: 完全不变
- 命中与归因：
  - 新过滤只命中 `1407` 的 `1` 笔 short，shadow PnL `-0.0943`
  - 目标样本是 `SOL 2024-05-15 16:00:00`
  - `ETH/BTC/BCH` 均未命中，因此这不是跨币泛化规则，而是单币窄样本规则
- 结论：
  - 这条规则对 `SOL` 单币种是正向的
  - 但按当前“ETH 先行闸门”，`ETH` 没有任何变化，不能晋级为正式基线
  - 代码已回滚，正式基线保持 `1403 / 1404 / 1405 / 1406`

### 2026-04-15: 低量对立布林 + Fib 区间 + bull leg long 分层参数实验（已验证，无效，拒绝晋级）

- 目标：
  - 按新增的波动性分层规则，验证一条 `分层参数通用` 候选：
    - `long`
    - `bollinger.is_short_signal = true`
    - `fib_retracement.in_zone = true`
    - `leg_detection.is_bullish_leg = true`
    - 低量
  - 基线统计显示该簇在不同波动层分布明显不同：
    - `BTC` 整体为正
    - `ETH` 明显为负
    - `其他币种` 轻度为负
- 分层参数设计：
  - `BTC`：关闭该过滤
  - `ETH`：`max_volume_ratio = 1.2`
  - `其他币种`：`max_volume_ratio = 1.5`
  - 额外约束：`require_no_hammer = true`
- 实现：
  - 新增候选过滤：
    - `LOW_VOLUME_OPPOSING_BOLLINGER_FIB_BULL_LEG_LONG_BLOCK`
  - 仅拦截：
    - `bollinger.is_short_signal`
    - `!bollinger.is_long_signal`
    - `fib_retracement.in_zone`
    - `leg_detection.is_bullish_leg`
    - `volume_ratio < max_volume_ratio`
    - 且无 `hammer long` 确认
- 验证：
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP,BCH-USDT-SWAP' DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测结果：
  - `1403 -> 1411 (ETH)`: 完全不变，仍为 `52.0992% / 7989.43 / 3.05633 / 35.0947%`
  - `1404 -> 1412 (BTC)`: 完全不变，仍为 `54.3230% / 336.744 / 1.03625 / 37.3102%`
  - `1405 -> 1413 (SOL)`: 完全不变，仍为 `42.6174% / 701.08 / 1.42464 / 41.4416%`
  - `1406 -> 1414 (BCH)`: 完全不变，仍为 `34.5299% / -77.2753 / -0.565944 / 81.5425%`
- 命中与归因：
  - `filtered_signal_log` 中新理由命中数为 `0`
  - 说明这条规则在当前 Vegas 候选链里没有真实命中样本
  - 问题不是“分层参数无效”，而是“切入点没有进入实际交易路径”
- 结论：
  - 本轮不能写成 `分层参数通用`
  - 正确分类是：`已验证，但切入点无效`
  - 代码与数据库参数已全部回滚
  - 正式基线保持 `1403 / 1404 / 1405 / 1406`

### 2026-04-15: Normal + Fib 区间 + 对立 Bolling short + bull leg long 分层参数实验（已接受，升级为新正式基线）

- 目标：
  - 从当前正式基线里筛出一条更像“顺趋势修复失败的伪 long”坏簇，要求：
    - `long`
    - `ema_distance.state = Normal`
    - `fib_retracement.in_zone = true`
    - `bollinger.is_short_signal = true`
    - `!bollinger.is_long_signal`
    - `leg_detection.is_bullish_leg = true`
    - `!ema_touch.is_uptrend`
    - `!kline_hammer.is_long_signal`
    - `macd.histogram > 0`
  - 同时遵循新的波动性分层规则，不再强行要求“单参数通用”。
- 首轮验证：
  - 原始候选过滤 `NORMAL_FIB_OPPOSING_BOLLINGER_BULL_LEG_LONG_BLOCK`
  - 结果：
    - `1415 (ETH)`: `52.4085% / 7997.98 / 3.09568 / 35.0947%`
    - `1416 (BTC)`: `54.1599% / 312.769 / 0.985139 / 37.3102%`
    - `1417 (SOL)`: `42.9054% / 716.052 / 1.44230 / 41.4416%`
    - `1418 (BCH)`: `34.5955% / -76.0741 / -0.551126 / 81.1634%`
  - 结论：
    - `ETH / SOL / BCH` 同向改善
    - `BTC` 被明显拖弱，不能直接升级成 `单参数通用`
- 分层收敛：
  - 第二版先试 `RSI >= 50` 作为波动代理，排除 BTC 那一层的低确认样本；
  - `ETH` 胜率、Sharpe 上升，但 `profit` 仅边际回落，不够干净，因此继续收窄；
  - 第三版最终收敛为：
    - 保留原有结构条件
    - 追加 `rsi_value.rsi_value >= 55.0`
- 实现：
  - 保留过滤：
    - `NORMAL_FIB_OPPOSING_BOLLINGER_BULL_LEG_LONG_BLOCK`
  - 最终生效条件：
    - `ema_distance.state == Normal`
    - `fib.in_zone == true`
    - `boll.is_short_signal == true`
    - `boll.is_long_signal == false`
    - `leg.is_bullish_leg == true`
    - `ema_touch.is_uptrend == false`
    - `hammer.is_long_signal == false`
    - `0 < macd.histogram < 20`
    - `RSI >= 55`
- 验证：
  - `cargo test -p rust-quant-indicators normal_fib_opposing_bollinger_bull_leg_long_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - 单币补跑：
    - `BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP' ...` -> `1424`
    - `BACKTEST_ONLY_INST_IDS='BTC-USDT-SWAP' ...` -> `1426`
    - `BACKTEST_ONLY_INST_IDS='SOL-USDT-SWAP' ...` -> `1427`
    - `BACKTEST_ONLY_INST_IDS='BCH-USDT-SWAP' ...` -> `1423`
- 回测结果（对比旧正式基线 `1403 / 1404 / 1405 / 1406`）：
  - `ETH 1403 -> 1424`: `52.0992% / 7989.43 / 3.05633 / 35.0947%` -> `52.3992% / 8061.41 / 3.10425 / 35.0947%`
  - `BTC 1404 -> 1426`: 完全不变，仍为 `54.3230% / 336.744 / 1.03625 / 37.3102%`
  - `SOL 1405 -> 1427`: `42.6174% / 701.08 / 1.42464 / 41.4416%` -> `42.7609% / 715.006 / 1.44106 / 41.4416%`
  - `BCH 1406 -> 1423`: `34.5299% / -77.2753 / -0.565944 / 81.5425%` -> `34.6484% / -76.4805 / -0.556099 / 80.8969%`
- 命中与归因：
  - `1424 (ETH)`: `5` 笔，shadow PnL `-0.1506`
  - `1426 (BTC)`: `0` 笔，说明 `RSI>=55` 后自然不再命中 BTC
  - `1427 (SOL)`: `1` 笔，shadow PnL `-0.0964`
  - `1423 (BCH)`: `9` 笔，shadow PnL `-0.2530`
- 过程备注：
  - 因磁盘空间过低，`back_test_detail` 一度写满，先清理了已拒绝实验的明细/过滤日志/回测日志（`1399-1402, 1407-1418, 1422`），再补跑验证。
  - 这属于测试库运维动作，不影响正式基线与保留实验。
- 结论：
  - 这条规则不属于 `单参数通用`
  - 正确结论是：`分层参数通用`
    - `BTC` 这层天然不命中
    - `ETH` 与 `其他币种` 在 `RSI>=55` 这一波动/位置代理下同向改善
  - 新正式基线升级为：
    - `ETH 1424`
    - `BTC 1426`
    - `SOL 1427`
    - `BCH 1423`

### 2026-04-15: TooFar 上升趋势对立 hammer short 过滤实验（已接受，升级为新正式基线）

- 假设：
  - 当前正式基线 `1424 / 1426 / 1427 / 1423` 中，存在一类被过度放行的 short：
    - `ema_distance.state == TooFar`
    - `ema_touch.is_uptrend == true`
    - `ema_values.is_long_trend == true`
    - `ema_values.is_short_trend == false`
    - `fib.in_zone == false`
    - `boll_short == true && boll_long == false`
    - `leg.is_bullish_leg == true && !leg.is_new_leg`
    - `hammer_short == true`
    - `!valid_engulfing`
    - `macd.histogram > 0`
    - `RSI >= 55`
- 目标样本：
  - `ETH 2025-07-11 16:00:00`
  - 同簇 `ETH` 命中 `9` 笔，原始总 PnL `-254.2338`
- 实现：
  - 新增过滤：
    - `TOO_FAR_UPTREND_OPPOSING_HAMMER_SHORT_BLOCK`
- 验证：
  - `cargo test -p rust-quant-indicators too_far_uptrend_opposing_hammer_short_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
- ETH 单币：
  - `1424 -> 1428`：
    - `52.3992% / 8061.41 / 3.10425 / 35.0947%`
    - `-> 52.9183% / 8047.17 / 3.14525 / 32.3410%`
  - 命中 `35` 笔，shadow PnL `-0.9974`
- 分层复核：
  - `BTC 1426 -> 1429`：`54.3230% / 336.744 / 1.03625 / 37.3102%` -> `54.3514% / 341.636 / 1.04603 / 37.3102%`
  - `SOL 1427 -> 1430`：不变
  - `BCH 1423 -> 1431`：`34.6484% / -76.4805 / -0.556099 / 80.8969%` -> `34.4887% / -72.5423 / -0.473708 / 77.2249%`
- 结论：
  - 可接受，升级基线。
  - 分类：`单参数通用`
  - 新正式基线：
    - `ETH 1428`
    - `BTC 1429`
    - `SOL 1430`
    - `BCH 1431`

## 2026-04-15 继续迭代：TooFar + fib in zone + opposing Bollinger engulfing long（拒绝晋级）

- 假设：
  - 当前正式基线 `1428 / 1429 / 1430 / 1431` 中，存在一类 long 坏簇：
    - `ema_distance.state == TooFar`
    - `fib.in_zone == true`
    - `boll_short == true && boll_long == false`
    - `leg.is_bullish_leg == true`
    - `engulfing.is_valid_engulfing == true`
    - `!hammer_long`
    - `0 < macd.histogram < 5`
    - `RSI >= 55`
- 目标样本与影子分布：
  - `ETH` 命中 `2` 笔，shadow PnL `-8.8870`
  - `BTC` 命中 `0` 笔
  - `SOL` 命中 `9` 笔，shadow PnL `-93.5807`
  - `BCH` 命中 `7` 笔，shadow PnL `-3.5688`
- 验证：
  - `cargo test -p rust-quant-indicators too_far_fib_opposing_bollinger_engulfing_long_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
- ETH 单币：
  - `1428 -> 1432`
  - `52.9183% / 8047.17 / 3.14525 / 32.3410%`
  - `-> 53.3203% / 8321.09 / 3.18343 / 32.3410%`
- 分层复核：
  - `BTC 1429 -> 1433`：完全不变
  - `SOL 1430 -> 1434`：
    - `42.7609% / 715.006 / 1.44106 / 41.4416%`
    - `-> 42.4138% / 587.749 / 1.30134 / 41.4416%`
- 结论：
  - `ETH` 单币正向，但 `SOL` 明显退化。
  - 判定：`已验证但无效`
  - 代码已回滚，正式基线保持：
    - `ETH 1428`
    - `BTC 1429`
    - `SOL 1430`
    - `BCH 1431`

## 2026-04-15 继续迭代：Normal + fib in zone + opposing Bollinger bull-leg long（两轮拒绝晋级）

- 假设：
  - 当前基线中存在一类被过度放行的 long：
    - `ema_distance.state == Normal`
    - `fib.in_zone == true`
    - `boll_short == true && boll_long == false`
    - `leg.is_bullish_leg == true`
    - `!ema_touch.is_uptrend`
    - `!hammer_long`
  - 按波动性分层规则，用 `RSI / MACD histogram / volume_ratio` 做代理阈值，而不是直接按币种硬编码。

- 第一轮代理：`RSI >= 50`（旧基线 `1403 / 1404 / 1405 / 1406`）
  - `ETH 1403 -> 1419`：`52.0992% / 7989.43 / 3.05633 / 35.0947%` -> `52.3992% / 7989.26 / 3.09419 / 35.0947%`
  - `BTC 1404 -> 1420`：不变
  - `SOL 1405 -> 1421`：`42.6174% / 701.08 / 1.42464 / 41.4416%` -> `42.7609% / 715.006 / 1.44106 / 41.4416%`
  - `BCH 1406 -> 1423`：`34.5299% / -77.2753 / -0.565944 / 81.5425%` -> `34.5955% / -76.4805 / -0.551126 / 81.1634%`
  - 命中：
    - `ETH` `6` 笔，shadow PnL `-0.1860`
    - `SOL` `1` 笔，shadow PnL `-0.0964`
    - `BCH` `9` 笔，shadow PnL `-0.2530`
    - `BTC` `0` 笔
  - 结论：
    - `ETH` 的 `win_rate / sharpe` 提升，但 `profit` 未净增，不过闸门。

- 第二轮代理：`RSI >= 55 && histogram >= 5 && volume_ratio >= 2.8`（正式基线 `1428 / 1429 / 1430 / 1431`）
  - 验证：
    - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
    - `cargo build --bin rust_quant`
    - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP,BCH-USDT-SWAP' DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
  - `ETH 1428 -> 1435`：`52.9183% / 8047.17 / 3.14525 / 32.3410%` -> `52.6112% / 7238.68 / 3.02318 / 32.3410%`
  - `BTC 1429 -> 1436`：不变
  - `SOL 1430 -> 1437`：`42.7609% / 715.006 / 1.44106 / 41.4416%` -> `42.6174% / 701.08 / 1.42464 / 41.4416%`
  - `BCH 1431 -> 1438`：`34.4887% / -72.5423 / -0.473708 / 77.2249%` -> `34.6021% / -72.0772 / -0.468625 / 76.8391%`
  - 命中：
    - `ETH` `2` 笔，shadow PnL `-0.0673`
    - `BCH` `1` 笔，shadow PnL `-0.0381`
    - `BTC / SOL` `0` 笔
  - 结论：
    - 收窄后 `BTC` 不再受影响，但 `ETH` 利润与 Sharpe 明显退化。
    - 判定：`已验证但无效`
    - 代码已回滚，正式基线保持：
      - `ETH 1428`
      - `BTC 1429`
      - `SOL 1430`
      - `BCH 1431`

## 2026-04-15 继续迭代：上升趋势中量弱 hammer short 过滤（拒绝晋级）

- 假设：
  - 当前正式基线 `1428 / 1429 / 1430 / 1431` 中，一类上升趋势里的中量弱确认 short 可能不该开：
    - `ema_touch.is_uptrend == true`
    - `!ema_values.is_short_trend`
    - `hammer.is_short_signal == true`
    - `!engulfing.is_valid_engulfing`
    - `RSI >= 55`
    - `1.0 <= volume_ratio < 2.5`
    - `abs(macd.histogram) < 10`
- 实现：
  - 新增过滤：
    - `UPTREND_MID_VOLUME_WEAK_HAMMER_SHORT_BLOCK`
- 验证：
  - `cargo test -p rust-quant-indicators uptrend_mid_volume_weak_hammer_short_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP,BCH-USDT-SWAP' DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测：
  - `ETH 1428 -> 1439`：`52.9183% / 8047.17 / 3.14525 / 32.3410%` -> `53.1062% / 7465.01 / 3.04184 / 35.2534%`
  - `BTC 1429 -> 1440`：不变
  - `SOL 1430 -> 1441`：`42.7609% / 715.006 / 1.44106 / 41.4416%` -> `42.7119% / 683.784 / 1.40747 / 41.4416%`
  - `BCH 1431 -> 1442`：`34.4887% / -72.5423 / -0.473708 / 77.2249%` -> `34.5133% / -72.0147 / -0.468314 / 79.0953%`
- 命中：
  - `ETH` `31` 笔，shadow PnL `-0.8185`
  - `BTC` `5` 笔，shadow PnL `-0.2019`
  - `SOL` `4` 笔，shadow PnL `-0.4268`
  - `BCH` `35` 笔，shadow PnL `-0.5455`
- 结论：
  - shadow PnL 虽全为负，但真实回测退化更明显：
    - `ETH` 利润、Sharpe、回撤都变差
    - `SOL` 也退化
    - `BCH` 回撤更差
  - 判定：`已验证但无效`
  - 代码已回滚，正式基线保持：
    - `ETH 1428`
    - `BTC 1429`
    - `SOL 1430`
    - `BCH 1431`

## 2026-04-15 继续迭代：TooFar 无趋势 Bollinger+Hammer long 过滤（拒绝晋级）

- 假设：
  - 当前正式基线 `1428 / 1429 / 1430 / 1431` 中，一类 `TooFar`、无趋势、靠 `Bollinger long + hammer long` 硬做反弹的 long 可能不该开：
    - `ema_distance.state == TooFar`
    - `!ema_touch.is_uptrend`
    - `!ema_values.is_long_trend`
    - `!ema_values.is_short_trend`
    - `bollinger.is_long_signal == true`
    - `fib.in_zone == false`
    - `kline_hammer.is_long_signal == true`
    - `!engulfing.is_valid_engulfing`
- 开仓聚合：
  - `ETH` `14` 笔，`-127.7972`
  - `BTC` `14` 笔，`-15.5649`
  - `SOL` `5` 笔，`-5.4523`
  - `BCH` `15` 笔，`-2.5402`
- 实现：
  - 新增过滤：
    - `TOO_FAR_NO_TREND_BOLL_HAMMER_LONG_BLOCK`
- 验证：
  - `cargo test -p rust-quant-indicators too_far_no_trend_boll_hammer_long_should -- --nocapture`
  - `cargo test -p rust-quant-indicators trend::vegas::strategy::tests -- --nocapture`
  - `cargo build --bin rust_quant`
  - `IS_BACK_TEST=1 IS_RUN_SYNC_DATA_JOB=0 TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP,BCH-USDT-SWAP' DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' ./target/debug/rust_quant`
- 回测：
  - `ETH 1428 -> 1443`：`52.9183% / 8047.17 / 3.14525 / 32.3410%` -> `52.6112% / 7975.32 / 3.09563 / 32.3410%`
  - `BTC 1429 -> 1444`：不变
  - `SOL 1430 -> 1445`：`42.7609% / 715.006 / 1.44106 / 41.4416%` -> `42.6174% / 701.08 / 1.42464 / 41.4416%`
  - `BCH 1431 -> 1446`：`34.4887% / -72.5423 / -0.473708 / 77.2249%` -> `34.5423% / -72.2502 / -0.470447 / 76.9826%`
- 结论：
  - 开仓聚合虽为负，但真实回测没有兑现成可接受改进：
    - `ETH` 利润和 Sharpe 回落
    - `SOL` 也退化
    - `BTC` 不变
    - `BCH` 仅边际改善
  - 判定：`已验证但无效`
  - 代码已回滚，正式基线保持：
    - `ETH 1428`
    - `BTC 1429`
    - `SOL 1430`
    - `BCH 1431`
