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
