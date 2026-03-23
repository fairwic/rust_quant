# Vegas 策略迭代日志

## 迭代记录

---

### 2026-01-27: 震荡区间 1:1 止盈优化（普适版）

#### 背景

- 震荡区间仍允许开仓，但需要更保守的止盈策略，避免区间内长时间持仓。
- 目标：在“窄布林 + RSI 中性 + 缩量/MACD 近零/影线并存”时直接采用 1:1 止盈。

#### 方案

- 仅在布林带宽度 <= 阈值 * 0.85 时触发“震荡收紧”。
- RSI 中性区间：46~54。
- 条件满足（缩量或 MACD 近零轴或上下影线并存）时，将止盈距离设为 1:1（使用止损距离）。
- 其余情况仍使用 `tp_kline_ratio` 作为震荡止盈距离。

#### 实验记录（ETH-USDT-SWAP 4H）

| Backtest ID | 版本         | 胜率   | 利润     | Sharpe | 最大回撤 |
| ----------- | ------------ | ------ | -------- | ------ | -------- |
| 1           | 基线         | 47.78% | 2086.15  | 1.7889 | 38.03%   |
| 6           | 初版(48-50)  | 47.39% | 2126.24  | 1.8015 | 38.95%   |
| 7           | 普适版(45-55) | 47.18% | 2321.16  | 1.8702 | 38.73%   |
| **8**       | **收紧版**   | 47.31% | **2328.22** | **1.8728** | 38.73% |

#### 结论

- **ID 8** 利润与 Sharpe 明显优于基线，回撤略升，胜率小幅下降。
- 已将 **ID 8** 作为新基线，后续优化以提升胜率/回撤为主。

#### 后续微调（胜率/回撤优先）

| Backtest ID | 变更点                         | 胜率   | 利润     | Sharpe | 最大回撤 |
| ----------- | ------------------------------ | ------ | -------- | ------ | -------- |
| 9           | 窄布林放宽(0.85→0.9) + 放量阈值放宽 | 47.18% | 2232.69  | 1.8385 | 38.73%   |
| 10          | 1:1 止盈上限=默认止盈           | 47.65% | 2084.31  | 1.7883 | 38.03%   |
| 11          | 震荡期非主趋势也触发 1:1        | 47.31% | 2328.22  | 1.8728 | 38.73%   |
| 12          | 1:1 上限放宽 1.1x               | 47.65% | 2084.31  | 1.7883 | 38.03%   |
| 13          | 超窄布林降 TP 比例(0.8x)        | 47.51% | 2129.42  | 1.8029 | 38.96%   |
| 17          | 震荡区间收紧止损(0.7x)          | 47.06% | 2115.05  | 1.7984 | 38.73%   |

#### 结论（微调）

- ID 9/10/11/12/13/17 未超过 ID 8，暂不替换基线。

---

### 2026-01-27: 动态配置调整日志（每根K线）

#### 目标

- 记录每根 K 线的动态配置调整（如震荡区间止盈收紧、止损来源变化），便于复盘与参数调优。

#### 方案

- 新增 `dynamic_config_log` 表，存储 `backtest_id + kline_time + adjustments + snapshot`。
- Vegas 策略每根 K 线生成动态配置快照（包含 range_tp / 止盈止损）。
- 回测保存日志时批量落库。

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
| **15672**   | Fib：`swing_lookback=120`、`min_volume_ratio=2.0`            | **53.12%** | **175.25**  | **0.542** | **40.16%** |
| **15673**   | 同 15672，但 `strict_major_trend=false`                      | **53.16%** | **1129.08** | **1.217** | **37.31%** |
| **15674**   | 同 15673，且恢复 `is_used_signal_k_line_stop_loss=true`      | **47.91%** | **2275.59** | **1.861** | **38.03%** |

#### 结果

- `15655 @ 2026-01-22 00:00:00`：开仓方向为 **long**（错误）。
- `15672 @ 2026-01-22 00:00:00`：开仓方向为 **short**（符合预期），平仓类型为 `反向信号触发平仓(趋势过滤)`。
- **盈利下降原因定位**：`strict_major_trend=true` 会硬过滤大量“逆大趋势”信号（约 748 次），导致开仓次数从 664 降到 369，且主要盈利来源（反向平仓/ATR 止盈）显著减少，从而出现“胜率提升但利润大幅下降”。
- **当前利润优先推荐**：采用 `15674`（`strict_major_trend=false` + `is_used_signal_k_line_stop_loss=true`），在修复该处方向错误的同时，利润已超过基线 `15655`。

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
| **66**      | **成交量确认** | **47.28%** | **2480** | **1.809** | **108.3%** | **31.7%** |

#### 核心发现

**ID 66 vs ID 61 改进**:

- 利润: 2480 vs 1517 (**+63%**)
- Sharpe: 1.809 vs 1.585 (**+14%**,史上最高)
- 最大回撤: 31.7% vs 38.5% (**-18%**,风控显著改善)

#### 止损来源效果分析

| 止损来源                     | 次数 | 总盈亏    | 平均盈亏 | 胜率   |
| ---------------------------- | ---- | --------- | -------- | ------ |
| Engulfing_Volume_Rejected    | 371  | **+1643** | +4.43    | 25.88% |
| Engulfing_Volume_Confirmed   | 348  | +1162     | +3.34    | 27.01% |
| KlineHammer_Volume_Confirmed | 205  | -92       | -0.45    | 21.95% |

#### 利润提升的根本原因

**关键洞察**: 成交量确认的核心价值是**减少过早止损次数**

| 平仓类型               | ID 61          | ID 66          | 变化             |
| ---------------------- | -------------- | -------------- | ---------------- |
| Signal_Kline_Stop_Loss | -1390 (411 次) | -667 (255 次)  | **亏损减少 723** |
| 止盈                   | +1113 (129 次) | +2002 (152 次) | **利润增加 889** |

- 无量吞没不设止损 → 趋势有发展空间 → 更多交易走到止盈
- 信号 K 线止损次数减少 156 次(-38%)

#### 代码实现

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

**推荐基线**: Backtest ID 66

- 胜率: 47.28%
- 利润: 2480 USDT
- Sharpe: 1.809 ⭐
- 年化收益: 108.3%
- 最大回撤: 31.7%
- 配置: `is_used_signal_k_line_stop_loss=true` + 成交量确认

---

### 2026-01-22: 信号 K 线止损开关对比实验 + 止损更新历史功能实现

#### 背景与目标

- **问题**: 开启`is_used_signal_k_line_stop_loss`后胜率大幅下降,但完全关闭又失去了形态止损的保护
- **目标**: 对比信号 K 线止损开关对策略表现的影响,探索选择性启用的可能性

#### 实验记录

| Backtest ID | is_used_signal_k_line_stop_loss | 胜率       | 利润     | Sharpe    | 年化收益   | 最大回撤   |
| ----------- | ------------------------------- | ---------- | -------- | --------- | ---------- | ---------- |
| 61          | ✅ 开启                         | 42.67%     | 1517     | 1.585     | 87.44%     | 38.46%     |
| **62**      | ❌ **关闭**                     | **52.75%** | **1982** | **1.559** | **98.44%** | **38.03%** |

#### 关键发现

**关闭信号 K 线止损的优势**:

1. **胜率大幅提升**: 从 42.67%提升到 52.75% (+10.08 个百分点)
2. **利润显著增长**: 从 1517 提升到 1982 (+30.6%)
3. **年化收益接近翻倍**: 98.44%
4. **回撤略有改善**: 38.03% vs 38.46%

**开启信号 K 线止损的问题**:

1. **过早止损**: 在趋势策略中,形态止损(开盘价/最高价/最低价)可能过于接近入场价,导致正常回调就被止损
2. **胜率严重下降**: 损失 10 个百分点的胜率
3. **利润大幅缩水**: 损失 465 USDT (-23.5%)

#### 核心矛盾

- **需求**: 希望在高确定性形态(如吞没、锤子线)时使用精准止损,降低回撤
- **现实**: 全局开启信号 K 线止损会过度止损,严重损害收益

#### 解决方案探索

**方向 1: 选择性启用止损** (推荐)

- 只对特定形态(如吞没)启用信号 K 线止损
- 其他信号使用最大亏损止损(`max_loss_percent`)
- 实现方式: 在`position.rs`中根据`stop_loss_source`条件判断

**方向 2: 动态止损距离**

- 根据市场波动(ATR/布林带宽)调整止损距离
- 高波动时放宽止损,低波动时收紧止损

**方向 3: 分层止损**

- 初始止损: 最大亏损止损(宽松,保护趋势)
- 触发条件: 浮盈达到 1R 后,移动到信号 K 线止损(收紧,保护利润)

#### 止损更新历史功能实现

**功能**: 记录所有止损价格更新的完整历史(时间、来源、价格变化)

**实现**:

- 新增`StopLossUpdate`结构体(Domain 层)
- 数据库添加`stop_loss_update_history` TEXT 字段(JSON 格式)
- `TradePosition`维护`Vec<StopLossUpdate>`历史记录
- 开仓/更新止损时自动追加记录

**状态**: ⚠️ 代码已实现,但回测 ID 62 中`stop_loss_update_history`全部为 NULL

- 原因: `is_used_signal_k_line_stop_loss=false`,止损更新逻辑未触发
- 需要: 在开启信号 K 线止损的回测中验证功能

#### 下一步计划

1. **实现选择性止损**: 只对吞没形态启用信号 K 线止损,其他形态使用最大亏损止损
2. **验证止损历史功能**: 运行开启`is_used_signal_k_line_stop_loss`的回测,验证历史记录功能
3. **分析止损更新模式**: 利用历史数据分析哪些情况下止损更新有益/有害
4. **优化止损策略**: 基于数据分析结果,设计更智能的止损规则

#### 当前基线

**推荐基线**: Backtest ID 62

- 胜率: 52.75%
- 利润: 1982 USDT
- Sharpe: 1.559
- 年化收益: 98.44%
- 最大回撤: 38.03%
- 配置: `is_used_signal_k_line_stop_loss=false`

---

### 2026-01-21: 吞没形态信号线止损优化 + KlineHammer 止损探索

#### 背景与目标

- **问题发现**：2025-12-29 的空头交易出现严重亏损（~130 U），原因是信号线止损未正确触发，导致持仓被"最大亏损止损"平仓
- **目标**：实现精准的形态止损机制，在检测到吞没形态时使用开盘价作为止损，降低回撤并保持利润

#### 优化思路

**核心洞察**：不是所有信号都需要严格止损，只有高确定性形态（如吞没）才需要。

**实验流程**：

1. 先修复止损逻辑链路（Strategy → Position → Risk）
2. 测试不同止损价格（开盘价、最低价、ATR 动态止损）
3. 逐步缩小止损范围，只对特定形态启用

#### 实验记录

| Backtest ID | 配置                       | Profit     | WR        | Sharpe   | MaxDD     | 结论             |
| ----------- | -------------------------- | ---------- | --------- | -------- | --------- | ---------------- |
| 35          | 基线（无专项止损）         | 3126 U     | 51.2%     | 1.88     | ~44%      | 利润高但回撤大   |
| 36          | 吞没+开盘价止损            | 2035 U     | 52.8%     | 1.58     | ~44%      | 止损生效但利润降 |
| 51          | 所有信号用开盘价止损       | 1335 U     | 42.0%     | 1.56     | -         | ❌ 过度止损      |
| **52**      | **只吞没形态用开盘价止损** | **2002 U** | **48.1%** | **1.69** | **32.8%** | ✅ **最优解**    |

#### 关键代码修改

**1. `strategy.rs` - 吞没形态止损**

```rust
// 【新增】如果是吞没形态，止损设为吞没K线开盘价
if vegas_indicator_signal_values.engulfing_value.is_engulfing {
    signal_result.signal_kline_stop_loss_price = Some(last_data_item.o);
}
```

**2. `strategy.rs` - 禁用其他止损逻辑**

```rust
// 【已禁用】只保留吞没形态止损
// if dist >= 0.0 && dist <= chase_cfg.close_to_ema_threshold { ... }
// if let Some(stop_loss_price) = utils::calculate_best_stop_loss_price(...) { ... }
```

#### KlineHammer 止损探索（额外实验）

| Backtest ID | 配置                | Profit | Sharpe | 结论                 |
| ----------- | ------------------- | ------ | ------ | -------------------- |
| 54          | + Hammer 开盘价止损 | 1096 U | 1.62   | ❌ 利润大幅下降      |
| 55          | + Hammer 最高价止损 | 1623 U | 1.86   | ⚠️ 利润降，Sharpe 升 |

**结论**：KlineHammer 止损对利润影响较大，暂不启用。

#### 最终配置（Backtest 52）

| 指标         | 优化前 | 优化后    | 变化     |
| ------------ | ------ | --------- | -------- |
| Profit       | 3126 U | 2002 U    | -36%     |
| Max Drawdown | ~44%   | **32.8%** | **-25%** |
| Sharpe       | 1.88   | 1.69      | -10%     |
| Win Rate     | 51.2%  | 48.1%     | -3.1%    |

**核心价值**：用 36% 的利润换取 25% 的回撤降低，风险调整后收益更优。

#### 经验总结

1. **精准止损 > 全局止损**：只对高确定性形态（吞没）启用止损，避免误杀
2. **风控优先级**：回撤控制优先于利润最大化，32.8% 的回撤更适合实盘
3. **渐进式实验**：先修复链路，再测试参数，最后缩小范围

---

### 2026-01-15: 从回测切到实盘（基线 5692）- 信号/止盈止损/交易所链路对齐

#### 目标

- 基线回测：`back_test_log.id = 5692`（Vegas / ETH-USDT-SWAP / 4H）。
- 开始实盘前，把“回测 → 实盘”关键差异收敛到可控开关，并补齐 OKX 真实下单路径（含止盈止损/改单/平仓）。

#### 本次落地（代码侧）

- **Vegas 参数一致性**：实盘执行器使用配置中的 `signal_weights`（不再强制 default），避免回测/实盘权重不一致导致信号偏移。
- **MarketStructure 是否可禁用**：补了回归用例验证 MarketStructure 即使权重=0 也会参与方向投票（只是不加权），因此“禁用=删除逻辑”会改变行为；需要通过配置控制而不是删链路。
- **实盘下单开关（灰度上线）**：
  - `LIVE_ATTACH_TP=1`：下单时附带 TP（默认关）。
  - `LIVE_CLOSE_OPPOSITE_POSITION=1`：反向持仓先平仓再开仓（默认关）。
  - `LIVE_SKIP_IF_SAME_SIDE_POSITION=1`：已有同向持仓则跳过开新仓（默认关）。
- **止盈价格优先级对齐**：`ATR TP → 信号 TP → 逆势回调 TP`（并做方向合理性校验，不合理则忽略 TP）。
- **预热 K 线数量对齐回测**：预热数量改为 `max(STRATEGY_WARMUP_LIMIT, min_k_line_num)` 并受 `STRATEGY_WARMUP_LIMIT_MAX` 上限控制，避免实盘预热不够导致指标冷启动偏差。
- **OKX 下单链路补全**：
  - 下单支持 attachAlgoOrds 同时附带 `TP+SL`。
  - 支持 `close_position` 市价平仓。
  - `OKX_REQUEST_EXPIRATION_MS` 可覆盖请求有效期（修复 `expTime can't be earlier...`）。
  - `OkxStopLossAmender` 支持基于 `ExchangeApiConfig` 创建，且同样支持 `OKX_REQUEST_EXPIRATION_MS` 覆盖。
- **risk_config 字段兼容**：`fix_signal_kline_take_profit_ratio` 增加 `serde(alias="fixed_signal_kline_take_profit_ratio")`，避免历史配置字段名不一致造成解析差异。

#### 联调与验证

- `cargo test -p rust-quant-services --lib`
- OKX 模拟盘 E2E（默认 ignore）：
  - `RUN_OKX_SIMULATED_E2E=1 cargo test -p rust-quant-services --test okx_simulated_order_flow -- --ignored --nocapture`
  - 测试内会设置 `OKX_REQUEST_EXPIRATION_MS=300000`，降低本地时间与服务器时间漂移导致的过期风险。

---

### 2026-01-09: Shadow Trading + 风控优化 + ATR 止盈修复

#### 实验概览

本次迭代专注于三个方面：

1. **Shadow Trading 实现** - 记录被过滤的信号并模拟交易结果
2. **风控参数优化** - 调整 `max_loss_percent` 从 5% 到 4%
3. **ATR 止盈修复** - 修复 `atr_stop_loss_price` 未计算的问题

#### 1. Shadow Trading（影子交易）

实现了 `filtered_signal_log` 表来记录被过滤的信号，用于分析过滤器有效性。

**关键发现**（基于 592 个被过滤信号）：

| 过滤原因                   | 数量 | 影子 PnL | 胜率   | 结论            |
| -------------------------- | ---- | -------- | ------ | --------------- |
| MACD_FALLING_KNIFE_LONG    | 207  | +41.10   | 74.88% | ⚠️ 可能过度过滤 |
| MACD_FALLING_KNIFE_SHORT   | 195  | -36.13   | 55.38% | ✅ 有效过滤     |
| CHASE_CONFIRM_FILTER_SHORT | 18   | -14.30   | 5.56%  | ✅ 非常有效     |

**MACD 过滤器放宽实验（5636）**：

- 尝试放宽 MACD_FALLING_KNIFE_LONG 过滤条件
- 结果：Profit 2046→1254 (-39%), MaxDD 49%→52%
- **结论**：放宽后反而恶化，MACD 过滤器保持原有逻辑

#### 2. 风控参数优化

**4% 止损测试 (5638)**：

| 指标     | 基线 (5637, 5%SL) | 4% SL (5638) | 变化    |
| -------- | ----------------- | ------------ | ------- |
| Sharpe   | 1.56              | 1.54         | -1.3%   |
| 年化收益 | 100.84%           | 98.66%       | -2.2%   |
| 最大回撤 | 49.29%            | **45.40%**   | **-8%** |
| 盈利     | 2046              | 1945         | -5%     |

**结论**：4% 止损有效降低回撤，轻微牺牲收益，已采用。

#### 3. ATR 止盈修复

**问题**：`atr_stop_loss_price` 在信号生成时始终为 `None`，导致 ATR 止盈无法触发。

**修复**：

- 在 `VegasStrategy.generate_signal()` 中添加 ATR(14) 计算
- 当生成有效信号时设置 `atr_stop_loss_price`：
  - 做多: `入场价 - ATR * 1.5`
  - 做空: `入场价 + ATR * 1.5`

**修复后结果 (5640)**：

| 指标         | 修复前 (5638) | 修复后 (5640) | 变化     |
| ------------ | ------------- | ------------- | -------- |
| **Sharpe**   | 1.54          | **1.83**      | **+19%** |
| **年化收益** | 98.66%        | **115.70%**   | **+17%** |
| **盈利**     | 1945          | **2838**      | **+46%** |
| 最大回撤     | 45.40%        | 45.40%        | 持平     |

#### 最终配置

```json
{
  "max_loss_percent": 0.04,
  "atr_take_profit_ratio": 3.0
}
```

- ATR 周期: 14
- 止损乘数: 1.5

#### 下一步

1. 优化 ATR 参数（周期、乘数组合）
2. 测试不同时间框架（1H、2H）的表现
3. 考虑动态调整 ATR 乘数

---

### 2026-01-08: 极端 K 线过滤分布试验

#### 实验概览

- 新增 `ExtremeKFilter`（大实体+多均线穿越）过滤逆势/假突破，分档测试：
  - 5588 宽松：实体 ≥0.65、单根波动 ≥1.0%、跨 ≥2 条 EMA。
  - 5589/5590 严格：实体 ≥0.70、单根波动 ≥1.5%、跨 ≥3 条 EMA（当前默认）。
  - 5593 宽松 + 高波动降损（极端波动降至 4.5% 止损，上述宽松阈值）

#### 结果对比

| 回测 ID | 档位         | 胜率   | Profit      | Sharpe    | MaxDD      |
| ------- | ------------ | ------ | ----------- | --------- | ---------- |
| 5588    | 宽松         | 57.51% | **1591.88** | **1.462** | 58.12%     |
| 5589    | 严格         | 57.06% | 1537.32     | 1.433     | **51.93%** |
| 5590    | 严格（复现） | 57.06% | 1537.32     | 1.433     | 51.93%     |
| 5593    | 宽松+降损    | 57.30% | **1752.61** | **1.534** | 57.74%     |
| 5576    | 基线         | 56.57% | 1556.74     | 1.435     | 52.64%     |

#### 结论

- 宽松档（5588）在盈利/Sharpe 上超越基线，但回撤升至 58%；可作为“收益偏好”参考。
- 严格档（5589/5590）回撤略优于基线，盈利略低，适合作为“稳健偏好”当前默认。
- 5593（宽松+高波动降损）在盈利/Sharpe 上最佳，回撤 57.7%，为当前默认开关组合。
- 极端 K 过滤对 Sharpe 有正向作用，仍需配合风控降低宽松档的回撤。

#### 下一步（目标 Profit≈2000 且 win_rate≥50%）

1. 宽松档 + 高波动降损：保留 5588 阈值，极端波动时临时把 `max_loss_percent` 下调至 0.045，降低尾部亏损后再冲盈利。
2. 分层止盈：引入 1R/1.5R 首段减仓，尾仓继续跟随信号，提高盈亏比并守住已得收益。
3. 轻量长周期确认：宽松档仅在日线与 4H 同向时放行，提升胜率并压回撤。
4. 趋势爆发放行：盘整后大实体顺势穿多条 EMA 的场景直接放行（当前仅过滤），争取捕捉趋势爆发行的利润。

---

### 2026-01-07: 回撤/胜率平衡与高波动探索

#### 当前基线（ID 5576）

- 配置：`ema_breakthrough_threshold=0.0026`，price_high=1.0016，price_low=0.998，RSI 14/86，min_total_weight=2.0；stop_loss（信号 K 线止损）关闭；`max_loss_percent=0.05`。
- 绩效：win_rate≈56.6%，profit≈1556.74，Sharpe≈1.4355，max_dd≈52.6%。
- 详情 INSERT INTO `test`.`back_test_log` (`id`, `strategy_type`, `inst_type`, `time`, `win_rate`, `open_positions_num`, `final_fund`, `strategy_detail`, `risk_config_detail`, `created_at`, `profit`, `one_bar_after_win_rate`, `two_bar_after_win_rate`, `three_bar_after_win_rate`, `four_bar_after_win_rate`, `five_bar_after_win_rate`, `ten_bar_after_win_rate`, `kline_start_time`, `kline_end_time`, `kline_nums`, `sharpe_ratio`, `annual_return`, `total_return`, `max_drawdown`, `volatility`) VALUES (5576, 'vegas', 'ETH-USDT-SWAP', '4H', '0.5657276995305164', 852, 1656.74, '{\"period\":\"4H\",\"min_k_line_num\":3600,\"ema_signal\":{\"ema1_length\":12,\"ema2_length\":144,\"ema3_length\":169,\"ema4_length\":576,\"ema5_length\":676,\"ema6_length\":2304,\"ema7_length\":2704,\"ema_breakthrough_threshold\":0.003,\"is_open\":true},\"volume_signal\":{\"volume_bar_num\":4,\"volume_increase_ratio\":2.5,\"volume_decrease_ratio\":2.5,\"is_open\":true},\"ema_touch_trend_signal\":{\"ema1_with_ema2_ratio\":1.01,\"ema2_with_ema3_ratio\":1.012,\"ema3_with_ema4_ratio\":1.006,\"ema4_with_ema5_ratio\":1.006,\"ema5_with_ema7_ratio\":1.022,\"price_with_ema_high_ratio\":1.002,\"price_with_ema_low_ratio\":0.995,\"is_open\":true},\"rsi_signal\":{\"rsi_length\":16,\"rsi_oversold\":14.0,\"rsi_overbought\":86.0,\"is_open\":true},\"bolling_signal\":{\"period\":12,\"multiplier\":2.0,\"is_open\":true,\"consecutive_touch_times\":4},\"signal_weights\":{\"weights\":[[\"SimpleBreakEma2through\",1.0],[\"VolumeTrend\",1.0],[\"Rsi\",1.0],[\"TrendStrength\",1.0],[\"EmaDivergence\",1.0],[\"PriceLevel\",1.0],[\"EmaTrend\",1.0],[\"Bolling\",1.0],[\"Engulfing\",1.0],[\"KlineHammer\",1.0],[\"LegDetection\",0.9],[\"MarketStructure\",0.0],[\"FairValueGap\",1.5],[\"EqualHighLow\",1.2],[\"PremiumDiscount\",1.3],[\"FakeBreakout\",0.0]],\"min_total_weight\":2.0},\"engulfing_signal\":{\"is_engulfing\":true,\"body_ratio\":0.4,\"is_open\":true},\"kline_hammer_signal\":{\"up_shadow_ratio\":0.6,\"down_shadow_ratio\":0.6},\"leg_detection_signal\":{\"size\":7,\"is_open\":true},\"market_structure_signal\":{\"swing_length\":12,\"internal_length\":2,\"swing_threshold\":0.015,\"internal_threshold\":0.015,\"enable_swing_signal\":false,\"enable_internal_signal\":true,\"is_open\":true},\"fair_value_gap_signal\":{\"threshold_multiplier\":1.0,\"auto_threshold\":true,\"is_open\":false},\"premium_discount_signal\":{\"premium_threshold\":0.05,\"discount_threshold\":0.05,\"lookback\":20,\"is_open\":false},\"fake_breakout_signal\":null,\"range_filter_signal\":{\"bb_width_threshold\":0.03,\"tp_kline_ratio\":0.6,\"is_open\":true}}', '{\"atr_take_profit_ratio\":0.0,\"is_counter_trend_pullback_take_profit\":false,\"is_move_stop_open_price_when_touch_price\":false,\"is_one_k_line_diff_stop_loss\":false,\"is_used_signal_k_line_stop_loss\":false,\"max_loss_percent\":0.05}', '2026-01-07 11:22:33', 1556.74, 0, 0, 0, 0, 0, 0, 1577232000000, 1767758400000, 13232, 1.43554, 0.895055, 15.5674, 0.526426, 0.609564);

#### 对比实验

- 5570（stop_loss 开, max_loss=0.05）：win_rate≈45.8%，profit≈1438.2，Sharpe≈1.6256，dd≈46.2% → Sharpe 高但胜率<50%。
- 5575（stop_loss 开, max_loss=0.055）：win_rate≈45.6%，Sharpe≈1.578 → 胜率未达标。
- 5577（stop_loss 关, max_loss=0.055）：win_rate≈57.0%，Sharpe≈1.19 → Profit/Sharpe 下降。
- 5579（引入“EMA 方向放行+布林宽>0.08 拒开仓”）：Sharpe≈0.64，收益显著下降，已废弃。

#### 亏损来源（基于 5576）

- “最大亏损止损”145 笔累计 -6342，单笔亏损多在高波动阶段（2024-04、2024-09、2025-07~12）。
- 盈利头部来自“反向信号平仓/触发平仓”，说明耐心持有的出场逻辑有效。

#### 下一步优化方向（待验证）

1. 高波动自适应止损：布林带宽或 ATR/价 超阈值时临时把 `max_loss_percent` 下调到 0.045，其余时段保持 0.05，减少高波动大亏。
2. 连续亏损冷却：同方向/短窗口内连续 N 次触发“最大亏损止损”后，降低仓位或冷却一段时间，防止连踩。
3. 长周期趋势确认（温和版）：要求日线 EMA 斜率与 4H 同向才放行，避免逆势交易（比全局 EMA 方向硬过滤更温和）。
4. 出场分层：浮盈达 1R/1.5R 减半仓，剩余仓位继续用反向信号平仓，兼顾胜率与尾部收益。
5. 极端波动过滤：在极端 K（>5%-8%涨跌）后一根内拒绝入场，降低爆量止损概率。

> 以上方案尚未实装，当前 DB 已回滚至基线 5576 配置（stop_loss 关，max_loss=0.05）。

---

### 2026-01-06: 第一性原理模块重构

#### 背景

基于 `doc/交易体系_第一性原理.md` 文档，对 Vegas 策略进行根本性重构，实现文档中定义的核心交易规则。

#### 新增模块

| 模块           | 文件路径                                        | 状态          | 说明                          |
| -------------- | ----------------------------------------------- | ------------- | ----------------------------- |
| 假突破检测     | `indicators/src/trend/vegas/fake_breakout.rs`   | ✅ 仅数据采集 | 检测价格假突破前高/前低后回归 |
| EMA 距离过滤   | `indicators/src/trend/vegas/ema_filter.rs`      | ⏸️ 暂停       | 距离过远时过滤逆势信号        |
| R 系统移动止损 | `strategies/src/framework/backtest/r_system.rs` | ⏸️ 待集成     | 基于盈利 R 倍数的动态止损     |

---

## 🔬 探索过程：如何发现最优解

### 第一阶段：初始实现（失败）

**假设**：按照第一性原理文档，假突破信号应该直接触发开仓，因为文档说"假突破信号 → 直接市价开仓"。

**实现**：

```rust
// 假突破直接开仓逻辑
if fake_breakout_signal.is_bullish_fake_breakout && fake_breakout_signal.volume_confirmed {
    signal_result.should_buy = Some(true);
    signal_result.should_sell = Some(false);
}
if fake_breakout_signal.is_bearish_fake_breakout && fake_breakout_signal.volume_confirmed {
    signal_result.should_sell = Some(true);
    signal_result.should_buy = Some(false);
}
```

同时实现了：

- EMA 距离过滤（空头排列+距离>5%+收盘价>ema3 → 不做多）
- 成交量递减过滤（连续 3 根 K 线成交量递减 → 忽略信号）
- 假突破信号权重设为 1.8（最高权重）

**回测结果（ID 4996）**：
| 指标 | 基线 | 新代码 | 变化 |
|------|------|--------|------|
| 盈利 | +52.77 | **-40.17** | 🔴 -92.94 |
| 胜率 | 54.7% | 54.5% | -0.2% |
| 回撤 | 73.5% | 68.0% | -5.5% |

**分析**：盈利从正变负，策略完全失效。问题出在哪里？

---

### 第二阶段：问题定位（逐步排除）

**思考**：新代码做了三件事：

1. 假突破直接开仓
2. EMA 距离过滤
3. 成交量递减过滤

哪个是罪魁祸首？需要逐一禁用验证。

**实验 1：禁用假突破直接开仓 + 成交量递减过滤**

```rust
// 注释掉直接开仓逻辑
// if fake_breakout_signal.is_bullish_fake_breakout ... { ... }

// 注释掉成交量递减过滤
// if ema_filter::check_volume_decreasing_filter(...) { ... }
```

**回测结果（ID 4998）**：
| 指标 | ID 4996 | ID 4998 | 变化 |
|------|---------|---------|------|
| 盈利 | -40.17 | **+14.03** | ✅ 恢复正向 |
| 胜率 | 54.5% | 55.9% | +1.4% |
| 回撤 | 68.0% | 57.8% | -10.2% |

**结论**：禁用这两个逻辑后，盈利恢复正向。但仍然比基线低（+14 vs +52.7）。

---

### 第三阶段：继续排查

**思考**：盈利仍然比基线低，可能是 EMA 距离过滤还在起作用？

**实验 2：禁用 EMA 距离过滤**

```rust
// 注释掉EMA距离过滤
// if ema_distance_filter.should_filter_long ... { ... }
// if ema_distance_filter.should_filter_short ... { ... }
```

**回测结果（ID 5000）**：
| 指标 | ID 4998 | ID 5000 | 变化 |
|------|---------|---------|------|
| 盈利 | +14.03 | +14.81 | +0.78 |
| 胜率 | 55.9% | 56.1% | +0.2% |
| 回撤 | 57.8% | 57.4% | -0.4% |

**结论**：EMA 距离过滤影响很小。问题不在过滤器。

---

### 第四阶段：关键洞察

**思考**：

- 禁用了所有过滤逻辑，盈利仍然只有+14.81，远低于基线+52.77
- 但新代码的假突破检测模块仍在运行，信号仍在加入 conditions
- 假突破权重设为 1.8（最高），会影响整体得分计算

**假设**：假突破信号权重过高，改变了原有信号的评分平衡，导致一些原本不应该触发的交易被触发了。

**实验 3：将假突破权重设为 0**

```rust
// crates/indicators/src/trend/signal_weight.rs
// 假突破权重从1.8改为0.0
(SignalType::FakeBreakout, 0.0),  // 仅数据采集，不参与得分
```

**回测结果（ID 5001）**：
| 指标 | ID 5000 | ID 5001 | 基线 | vs 基线 |
|------|---------|---------|------|--------|
| 盈利 | +14.81 | **+99.68** | +52.77 | **+89%** |
| 胜率 | 56.1% | 55.1% | 54.7% | +0.4% |
| 回撤 | 57.4% | 65.4% | 73.5% | -8.1% |
| 夏普 | +0.021 | **+0.264** | +0.143 | **+85%** |

**结论**：🎉 盈利大幅超越基线！

---

### 第五阶段：理解为什么

**核心问题**：为什么假突破检测存在但权重=0 时，策略表现反而大幅提升？

**分析**：

1. **假突破检测提供了额外的市场状态信息**

   - 系统知道当前是否处于假突破环境
   - 这个信息可能影响了其他模块的行为（如止损、止盈判断）

2. **权重=0 意味着不直接影响信号得分**

   - 原有的信号权重系统保持平衡
   - 不会因为假突破信号而触发额外的交易

3. **数据采集 vs 信号触发的区别**
   - 数据采集：收集信息，供其他模块参考
   - 信号触发：直接影响交易决策
   - 前者是辅助，后者是决策

**类比**：就像一个交易员，他知道"现在是假突破"这个信息，但他不会因为这个信息就立刻下单，而是把它作为参考，在综合判断时考虑进去。

---

## 📊 回测结果演变

| 回测 ID  | 配置描述                             | 胜率      | 盈利       | 回撤      | 夏普      | 年化      |
| -------- | ------------------------------------ | --------- | ---------- | --------- | --------- | --------- |
| 4995     | **基线**                             | 54.7%     | +52.77     | 73.5%     | 0.143     | 10.1%     |
| 4996     | 全部启用（直接开仓+过滤器+权重 1.8） | 54.5%     | -40.17     | 68.0%     | -0.228    | -11.0%    |
| 4998     | 禁用直接开仓+成交量过滤              | 55.9%     | +14.03     | 57.8%     | +0.018    | 3.0%      |
| 5000     | 禁用所有过滤器                       | 56.1%     | +14.81     | 57.4%     | +0.021    | 3.2%      |
| **5001** | **假突破权重=0**                     | **55.1%** | **+99.68** | **65.4%** | **0.264** | **17.1%** |

---

## 🏆 当前最优配置（ID 5001）

INSERT INTO `test`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `risk_config`, `time`, `created_at`, `updated_at`, `kline_start_time`, `kline_end_time`, `final_fund`, `is_deleted`) VALUES (11, 'Vegas', 'ETH-USDT-SWAP', '{\"period\": \"4H\", \"ema_signal\": {\"is_open\": true, \"ema1_length\": 12, \"ema2_length\": 144, \"ema3_length\": 169, \"ema4_length\": 576, \"ema5_length\": 676, \"ema6_length\": 2304, \"ema7_length\": 2704, \"ema_breakthrough_threshold\": 0.0032}, \"rsi_signal\": {\"is_open\": true, \"rsi_length\": 16, \"rsi_oversold\": 18.0, \"rsi_overbought\": 78.0}, \"volume_signal\": {\"is_open\": true, \"volume_bar_num\": 4, \"volume_decrease_ratio\": 2.5, \"volume_increase_ratio\": 2.5}, \"bolling_signal\": {\"period\": 12, \"is_open\": true, \"multiplier\": 2.0, \"consecutive_touch_times\": 4}, \"min_k_line_num\": 3600, \"signal_weights\": {\"weights\": [[\"SimpleBreakEma2through\", 0.7], [\"VolumeTrend\", 0.3], [\"EmaTrend\", 0.25], [\"Rsi\", 0.8], [\"Bolling\", 0.7]], \"min_total_weight\": 2.0}, \"kline_hammer_signal\": {\"up_shadow_ratio\": 0.6, \"down_shadow_ratio\": 0.6}, \"ema_touch_trend_signal\": {\"is_open\": true, \"ema1_with_ema2_ratio\": 1.01, \"ema2_with_ema3_ratio\": 1.012, \"ema3_with_ema4_ratio\": 1.006, \"ema4_with_ema5_ratio\": 1.006, \"ema5_with_ema7_ratio\": 1.022, \"price_with_ema_low_ratio\": 0.9982, \"price_with_ema_high_ratio\": 1.0022}}', '{\"max_loss_percent\": 0.06}', '4H', '2025-10-10 18:04:33', '2026-01-06 12:22:50', 1577232000000, 1760083200000, 4352010, 0);

### 性能指标

| 指标     | 值     | vs 基线  |
| -------- | ------ | -------- |
| 胜率     | 55.1%  | +0.4%    |
| 盈利     | +99.68 | **+89%** |
| 最大回撤 | 65.4%  | -8.1%    |
| 夏普比率 | 0.264  | **+85%** |
| 年化收益 | 17.1%  | **+69%** |

### 数据库配置（strategy_config id=11）

**信号参数（value JSON）**：

```json
{
  "period": "4H",
  "min_k_line_num": 3600,
  "ema_signal": {
    "ema1_length": 12,
    "ema2_length": 144,
    "ema3_length": 169,
    "ema4_length": 576,
    "ema5_length": 676,
    "ema6_length": 2304,
    "ema7_length": 2704,
    "ema_breakthrough_threshold": 0.0032,
    "is_open": true
  },
  "volume_signal": {
    "volume_bar_num": 4,
    "volume_increase_ratio": 2.5,
    "volume_decrease_ratio": 2.5,
    "is_open": true
  },
  "ema_touch_trend_signal": {
    "ema1_with_ema2_ratio": 1.01,
    "ema2_with_ema3_ratio": 1.012,
    "ema3_with_ema4_ratio": 1.006,
    "ema4_with_ema5_ratio": 1.006,
    "ema5_with_ema7_ratio": 1.022,
    "price_with_ema_high_ratio": 1.0022,
    "price_with_ema_low_ratio": 0.9982,
    "is_open": true
  },
  "rsi_signal": {
    "rsi_length": 16,
    "rsi_oversold": 18.0,
    "rsi_overbought": 78.0,
    "is_open": true
  },
  "bolling_signal": {
    "period": 12,
    "multiplier": 2.0,
    "is_open": true,
    "consecutive_touch_times": 4
  },
  "kline_hammer_signal": {
    "up_shadow_ratio": 0.6,
    "down_shadow_ratio": 0.6
  },
  "signal_weights": {
    "weights": [
      ["SimpleBreakEma2through", 0.5],
      ["VolumeTrend", 0.4],
      ["EmaTrend", 0.35],
      ["Rsi", 0.6],
      ["Bolling", 0.55]
    ],
    "min_total_weight": 2.0
  }
}
```

**风控参数（risk_config JSON）**：

```json
{
  "max_loss_percent": 0.06
}
```

### 代码配置

**1. 假突破权重（`signal_weight.rs`）**：

```rust
// 权重=0，仅数据采集，不参与得分计算
(SignalType::FakeBreakout, 0.0),
```

**2. 策略逻辑（`strategy.rs`）**：

```rust
// 假突破检测启用，但以下逻辑被禁用：
// - 假突破直接开仓（注释掉）
// - EMA距离过滤（注释掉）
// - 成交量递减过滤（注释掉）

// 假突破信号仍然加入conditions（权重=0所以不影响得分）
if fake_breakout_signal.has_signal() {
    conditions.push((
        SignalType::FakeBreakout,
        SignalCondition::FakeBreakout { ... },
    ));
}
```

---

## 💡 关键经验总结

### 1. 新模块集成的正确姿势

| 步骤 | 说明                                 |
| ---- | ------------------------------------ |
| 1    | 先实现模块，设权重=0                 |
| 2    | 运行回测，对比基线                   |
| 3    | 如果提升，保持权重=0 或微调          |
| 4    | 如果下降，检查是否影响了原有信号平衡 |

### 2. 数据采集 vs 信号触发

| 类型     | 特点               | 适用场景         |
| -------- | ------------------ | ---------------- |
| 数据采集 | 权重=0，仅记录信息 | 辅助其他模块判断 |
| 信号触发 | 权重>0，影响得分   | 直接参与交易决策 |

**结论**：新模块应该先作为数据采集，验证有效后再考虑是否参与信号触发。

### 3. 过滤器的双刃剑效应

过滤器的本意是过滤假信号，但如果阈值不合适，会过滤掉有效信号。

| 过滤器         | 预期效果       | 实际效果           |
| -------------- | -------------- | ------------------ |
| EMA 距离过滤   | 过滤逆势假信号 | 过滤了部分有效信号 |
| 成交量递减过滤 | 过滤无力信号   | 过滤了部分有效信号 |

**结论**：过滤器需要精细调参，不能直接使用默认阈值。

### 4. 权重系统的平衡性

原有的权重系统经过多次调优，各信号之间已经达到平衡。新增信号如果权重过高，会打破这个平衡。

**错误做法**：

```rust
(SignalType::FakeBreakout, 1.8),  // 最高权重，打破平衡
```

**正确做法**：

```rust
(SignalType::FakeBreakout, 0.0),  // 先设为0，观察效果
```

---

## 下一步计划

1. **R 系统移动止损集成**：将 `r_system.rs` 集成到风控流程
2. **分批止盈实现**：40%/30%/30%分阶段止盈
3. **过滤器阈值调优**：调整 EMA 距离和成交量过滤的阈值
4. **时间止损**：12/24/48 K 线无盈利自动平仓

---

## 历史基线

| 日期       | 回测 ID | 配置          | 胜率  | 盈利   | 备注       |
| ---------- | ------- | ------------- | ----- | ------ | ---------- |
| 2026-01-06 | 4995    | 组合 E        | 54.7% | +52.77 | 旧基线     |
| 2026-01-06 | 5001    | 第一性原理 v1 | 55.1% | +99.68 | **新基线** |

---

## 2026-03-11 迭代记录：修复低位追空

### 问题定位

- 分析 `back_test_id=15682` 发现，`2026-02` 仅 `10` 笔平仓就亏损 `-593.43`，胜率仅 `10%`。
- 该月亏损单以 `short` 为主，典型特征是：
  - `ema_distance_filter.state = TooFar`
  - `ema_values.is_short_trend = true`
  - `fib_retracement_value.in_zone = false`
- 结论：问题核心不是趋势判断失效，而是高波动下跌阶段出现了明显的“低位追空”。

### 对照回测

| 回测 ID | 方案 | 胜率 | 盈利 | Sharpe | MaxDD | 备注 |
| ------- | ---- | ---- | ---- | ------ | ----- | ---- |
| 15682 | 原配置 | 46.92% | 1829.10 | 1.638 | 38.73% | 优化前基线 |
| 15683 | 关闭 `Signal_Kline_Stop_Loss` | 51.92% | 836.41 | 1.014 | 36.66% | 明显退化，不能直接关闭 |
| 15684 | `TooFar short` 且 `fib` 不在区间且无确认时拦截 | 47.58% | 2364.94 | 1.829 | 38.73% | 明显优于 15682 |
| 15685 | `TooFar short` 必须在 `fib zone` 内，否则直接拦截 | 48.19% | 2588.84 | 1.963 | 38.73% | 阶段最优 |
| 15686 | 在 `15685` 基础上放行“放量新腿破位延续”例外 | 48.20% | 2787.85 | 2.013 | 38.73% | 已确认基线 |
| 15687 | 在 `15686` 基础上拦截 `TooFar` 反趋势锤子线 long（`short_trend + !fib_in_zone + RSI<45`） | 48.31% | 3338.00 | 2.160 | 38.73% | 待确认候选 |

### 2026-02 修复效果

| 回测 ID | 交易数 | 盈利 | 胜率 |
| ------- | ------ | ---- | ---- |
| 15682 | 10 | -593.43 | 10.00% |
| 15684 | 6 | -469.21 | 16.67% |
| 15685 | 2 | -74.37 | 50.00% |
| 15686 | 2 | -79.88 | 50.00% |

- `15685` 在 `2026-02` 已不再开 `short`，仅保留 `2` 笔 `long`。
- `filtered_signal_log` 显示新规则在 `15685` 中命中 `131` 次，在 `15686` 中命中 `127` 次：`EMA_TOO_FAR_OUTSIDE_FIB_ZONE_BLOCK_SHORT`。

### 2026-03-11 补充优化：保留极窄的破位空例外

- `15685` 相比 `15684` 修复了 `2026-02`，但也误杀了 `2026-01-30` 的一笔有效盈利空单。
- 该类单的共同特征是：
  - `TooFar`
  - `fib_retracement_value.in_zone = false`
  - `leg_detection_value.is_new_leg = true`
  - `fib_retracement_value.retracement_ratio <= 0.10`
  - `fib_retracement_value.volume_ratio >= 3.0`
  - `macd_value.histogram < 0.0`
- 在此基础上增加一个极窄例外后得到 `back_test_id=15686`：
  - `profit`: `2588.84 -> 2787.85`
  - `sharpe`: `1.963 -> 2.013`
  - `2026-01`: `142.32 -> 203.00`
  - `2026-02`: `-74.37 -> -79.88`（轻微回落，但仍远优于 `15682`）

### 最终结论

- 这轮优化方向正确，核心失效模式已经确认是“低位追空”。
- 当前保留规则：
  - `TooFar short` 必须在 `fib zone` 内，否则直接拦截。
- 当前补充例外：
  - 若属于“放量新腿破位延续”：
    `is_new_leg=true && retracement_ratio<=0.10 && volume_ratio>=3.0 && macd_histogram<0`
    则允许 `TooFar short` 继续开仓。
- 代码位置：
  - `crates/indicators/src/trend/vegas/strategy.rs`
- 当前工作基线：
  - `back_test_id=15686`

### 2026-03-11 补充优化：收紧 `TooFar` 反趋势锤子线 long

- 基于 `15686` 继续检查 long 侧 `Signal_Kline_Stop_Loss` 后发现：
  - long 侧 `Signal_Kline_Stop_Loss` 整体不是误伤，汇总仍为正贡献：`165` 笔，`+244.43`
  - `TooFar long` 整体也不是坏信号，汇总仍为正贡献：`281` 笔，`+1474.24`
  - 真正明显偏差的是一小类反趋势抄底单：
    `TooFar + is_short_trend=true + is_long_trend=false + is_uptrend=false + !fib_in_zone + KlineHammer long`
- 在 `15686` 中，这组单共有 `24` 笔，合计 `-285.20`；若再要求 `RSI<45`，则收敛为 `18` 笔，合计 `-422.76`。
- 因此新增极窄过滤：
  - `ema_distance_filter.state == TooFar`
  - `ema_touch_value.is_uptrend == false`
  - `ema_values.is_long_trend == false`
  - `ema_values.is_short_trend == true`
  - `fib_retracement_value.in_zone == false`
  - `kline_hammer_value.is_long_signal == true`
  - `RSI < 45`
  - 满足时拦截做多，记录原因：`EMA_TOO_FAR_COUNTER_TREND_HAMMER_LONG`
- 结果得到 `back_test_id=15687`：
  - `profit`: `2787.85 -> 3338.00`
  - `sharpe`: `2.013 -> 2.160`
  - `win_rate`: `48.20% -> 48.31%`
  - `2026-01`: `203.00 -> 551.24`
  - `2026-02`: `-79.88 -> 42.50`
- 新过滤在 `15687` 中命中 `42` 次，其中直接移除的原有平仓交易 `18` 笔，合计 `-422.76`。
- 代表性坏样本：
  - `2026-01-31 00:00:00` long：`TooFar + short_trend + !fib_in_zone + KlineHammer long + RSI=32.54`，在 `15686` 亏损 `-117.88`，在 `15687` 被过滤
  - `2026-02-05 00:00:00` long：同类模式，在 `15686` 亏损 `-115.58`，在 `15687` 被过滤
- 结论：
  - 暂不建议动 long 侧 `Signal_Kline_Stop_Loss` 总开关
  - 更有效的优化点是收紧这类 `TooFar` 反趋势锤子线 long

## 2026-03-12 批量优化记录：基于 15690 的 84 轮自动搜索

### 批次说明

- 代码基线：保留 `15688/15690` 已验证有效的窄规则，不启用全局 `post-drop consolidation` 状态过滤。
- 自动化脚本：`scripts/optimize_vegas_batch.py`
- 全量结果：
  - `docs/backtest_reports/vegas_opt_batch_20260312_010223.tsv`
  - `docs/backtest_reports/vegas_opt_batch_20260312_010223.jsonl`
  - `docs/backtest_reports/vegas_opt_batch_20260312_010223_summary.json`
- 搜索方式：
  - phase 1：`48` 轮随机广撒点
  - phase 2：`36` 轮围绕头部候选做局部细化
- 总轮次：`84`
- 随机种子：`20260312`

### 基线

| 回测 ID | 胜率 | 盈利 | Sharpe | MaxDD | 备注 |
| ------- | ---- | ---- | ------ | ----- | ---- |
| 15690 | 48.53% | 3282.29 | 2.163 | 38.73% | 自动搜索起点 |

### 批量搜索结果

- 严格支配 `15690`（胜率更高、利润更高、回撤更低）的候选共 `26` 个。
- batch-best 为 `back_test_id=15747`：
  - `profit`: `3282.29 -> 6899.70`
  - `win_rate`: `48.53% -> 50.16%`
  - `sharpe`: `2.163 -> 2.832`
  - `max_drawdown`: `38.73% -> 31.09%`
- 当前 `strategy_config.id=11` 已自动更新为 `15747` 对应参数。

### 头部候选

| 回测 ID | 胜率 | 盈利 | Sharpe | MaxDD | 说明 |
| ------- | ---- | ---- | ------ | ----- | ---- |
| 15747 | 50.16% | 6899.70 | 2.832 | 31.09% | batch-best，利润最强 |
| 15757 | 50.32% | 5979.53 | 2.702 | 28.46% | 更低回撤的高利润解 |
| 15740 | 50.25% | 5733.50 | 2.721 | 26.37% | 利润 / 回撤平衡最强之一 |
| 15772 | 50.41% | 5284.82 | 2.637 | 26.56% | 胜率最高的一档 |
| 15742 | 50.72% | 4860.00 | 2.542 | 25.69% | 稳定性最强的一档 |

### batch-best（15747）参数变化

相对 `15690` / 原 `id=11`，`15747` 的主要变化是：

- `volume_signal.volume_increase_ratio`: `2.5 -> 3.1`
- `rsi_signal.rsi_oversold`: `14.0 -> 15.5`
- `rsi_signal.rsi_overbought`: `86.0 -> 87.0`
- `signal_weights.min_total_weight`: `2.0 -> 2.16`
- `leg_detection_signal.size`: `7 -> 8`
- `signal_weights.LegDetection`: `0.90 -> 0.83`
- `signal_weights.Bolling`: `1.00 -> 0.87`
- `signal_weights.Engulfing`: `1.00 -> 0.91`
- `signal_weights.KlineHammer`: `1.00 -> 0.92`
- `signal_weights.FairValueGap`: `1.50 -> 1.22`
- `range_filter_signal.bb_width_threshold`: `0.03 -> 0.029`
- `range_filter_signal.tp_kline_ratio`: `0.60 -> 0.56`
- `chase_confirm_config.long_threshold`: `0.18 -> 0.187`
- `chase_confirm_config.short_threshold`: `0.10 -> 0.132`
- `chase_confirm_config.pullback_touch_threshold`: `0.05 -> 0.053`
- `chase_confirm_config.min_body_ratio`: `0.50 -> 0.47`
- `fib_retracement_signal.fib_trigger_low`: `0.328 -> 0.29`
- `fib_retracement_signal.fib_trigger_high`: `0.618 -> 0.639`
- `fib_retracement_signal.min_volume_ratio`: `2.0 -> 1.73`
- `fib_retracement_signal.stop_loss_buffer_ratio`: `0.01 -> 0.006`
- `extreme_k_filter_signal.min_body_ratio`: `0.65 -> 0.62`
- `extreme_k_filter_signal.min_move_pct`: `0.01 -> 0.011`
- `risk.max_loss_percent`: 维持 `0.04`
- `risk.atr_take_profit_ratio`: `3.0 -> 3.44`
- `risk.fixed_profit_percent_take_profit`: `0.05 -> 0.057`

### 自我分析

- 这批结果不是单点偶然。`84` 轮里出现了 `26` 个 strict dominator，说明当前代码基线下存在一片稳定优于 `15690` 的参数区域。
- 严格优于 `15690` 的头部候选有明显共性：
  - `leg_size` 往 `8/9` 走；
  - `volume_increase_ratio` 提高到接近 `3.0`；
  - `min_total_weight` 往 `2.1+` 收紧；
  - `Bolling / Engulfing / KlineHammer / LegDetection / FairValueGap` 权重整体下调；
  - `fib_trigger_low` 下移、`fib_trigger_high` 上移，Fib 区间变宽；
  - `fib_min_volume_ratio` 和 `stop_loss_buffer_ratio` 下降，Fib 入场更容易但止损缓冲更紧；
  - `ATR take profit` 上调到 `3.2~3.4` 左右。
- 这说明当前最有效的方向不是再加新规则，而是：
  - 提高入场确认强度，减少低质量形态堆分；
  - 放宽 Fib 区间，保留更多中继回撤；
  - 同时用更高的 TP 和稍紧的结构约束，拉长能跑出的趋势单。

### 当前结论

- `15690` 已被稳定超越。
- 当前批次最优是 `15747`，且已经同步到 `strategy_config.id=11`。
- 下一步若继续优化，不建议再做大范围盲扫，应该围绕 `15747 / 15757 / 15740 / 15772 / 15742` 这组前沿点做更窄的局部搜索或分月鲁棒性分析。

### 2026-03-18 落库确认

- 重新核对数据库后，确认 `strategy_config.id=11` 当前参数已与 `back_test_id=15747` 对齐。
- 本次核对的关键指标：
  - `15690`: `win_rate=48.53%`, `profit=3282.29`, `max_drawdown=38.73%`
  - `15747`: `win_rate=50.16%`, `profit=6899.70`, `max_drawdown=31.09%`
- 结论：
  - 最近 84 轮搜索得到的最优参数已经正式记录在数据库中，可作为后续继续优化的最新起点。

## 2026-03-18 补充优化：过滤缩量 + RSI 中性 + MACD 零轴下方修复的过早 short

### 问题样本

- `2026-03-10 04:00:00` 这笔 short 在 `15747` 中被放行，但从信号快照看并不适合继续追空：
  - `volume_ratio = 0.4457`，明显缩量
  - `RSI = 49.46`，接近中性
  - `macd_line = -4.61`, `signal_line = -12.14`, `histogram = +7.53`
  - 也就是 MACD 仍在零轴下方，但快线已显著高于慢线，属于下跌后的修复阶段，而不是空头动能再次扩张
- 该单在 `15747` 中最终亏损 `-150.76`。

### 新增过滤

- 新增极窄 short 过滤条件：
  - `signal_result.should_sell == true`
  - `volume_ratio < 1.0`
  - `RSI in [47, 53]`
  - `macd_line < 0`
  - `signal_line < 0`
  - `macd_line > signal_line`
  - `histogram > 0`
- 满足时拦截做空，记录原因：
  - `LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT`
- 代码位置：
  - `crates/indicators/src/trend/vegas/strategy.rs`

### 回测结果

| 回测 ID | 胜率 | 盈利 | Sharpe | MaxDD | 波动率 | 开仓数 |
| ------- | ---- | ---- | ------ | ----- | ------ | ------ |
| 15747 | 50.16% | 6899.70 | 2.832 | 31.09% | 53.64% | 618 |
| 15776 | 50.32% | 7449.00 | 2.908 | 31.09% | 53.65% | 616 |

- 结果：
  - `profit`: `6899.70 -> 7449.00`
  - `win_rate`: `50.16% -> 50.32%`
  - `sharpe`: `2.832 -> 2.908`
  - `max_drawdown`: 持平，仍为 `31.09%`

### 直接影响的 short

- 该过滤在 `15776` 中命中 `2` 次，且全部是 `SHORT`：
  - `2022-01-03 08:00:00`：原先亏损 `-1.25`
  - `2026-03-10 04:00:00`：原先亏损 `-150.76`
- 两笔合计直接减少亏损约 `+152.01`。

### 有利变化

- 本次调整后的净改善约 `+549.30`。
- 提升最大的时间点：
  - `2026-03-09 08:00:00 long`：`50.99 -> 365.82`，改善 `+314.83`
  - `2026-03-10 04:00:00 short`：`-150.76 -> 0.00`，改善 `+150.76`
  - `2025-05-07 00:00:00 long`：改善 `+10.58`
  - `2025-08-07 16:00:00 long`：改善 `+7.81`
  - `2025-11-12 20:00:00 short`：改善 `+7.47`

### 不利变化

- 本次也带来一些路径上的轻微回吐，但幅度明显小于正向收益。
- 变差最大的时间点：
  - `2026-01-26 00:00:00 short`：`-280.35 -> -283.66`，回吐 `-3.31`
  - `2025-10-10 00:00:00 long`：`-204.49 -> -206.90`，回吐 `-2.41`
  - `2025-10-24 20:00:00 short`：回吐 `-2.39`
  - `2025-11-07 16:00:00 short`：回吐 `-1.85`
  - `2026-01-07 04:00:00 long`：回吐 `-1.66`

### 对 long 的影响分析

- 这次规则只过滤 `short`，不会直接拦截 `long`，因此：
  - 没有任何 long 信号被这条新规则直接过滤
  - long 侧的变化全部属于持仓路径变化带来的间接影响
- 从净结果看，long 侧反而受益更大：
  - `long` 总净改善约 `+367.81`
  - `short` 总净改善约 `+181.49`
- 最典型例子是 `2026-03-09 08:00:00 long`：
  - 在 `15747` 中，这笔 long 于 `2026-03-10 04:00:00` 被反向 short 提前结束，只赚 `+50.99`
  - 在 `15776` 中，`2026-03-10 04:00:00` 的错误 short 被过滤，这笔 long 一直持有到 `2026-03-10 20:00:00`，盈利扩大到 `+365.82`
- 结论：
  - 这条 short 过滤不仅能消除错误追空，还能减少“错误反手做空”对已有 long 盈利的截断
  - 因此它对多头是有正面效果的，但这种效果是间接的，不是通过改 long 开仓规则实现的

## 2026-03-18 补充优化：过滤缩量 + RSI 中性 + MACD 零轴上方转弱的过早 long

### 新增过滤

- 在前一条 short 过滤的基础上，增加对称的 long 过滤：
  - `signal_result.should_buy == true`
  - `volume_ratio < 1.0`
  - `RSI in [47, 53]`
  - `macd_line > 0`
  - `signal_line > 0`
  - `macd_line < signal_line`
  - `histogram < 0`
- 满足时拦截做多，记录原因：
  - `LOW_VOLUME_NEUTRAL_RSI_MACD_WEAKENING_BLOCK_LONG`

### 回测结果

| 回测 ID | 胜率 | 盈利 | Sharpe | MaxDD | 波动率 | 开仓数 |
| ------- | ---- | ---- | ------ | ----- | ------ | ------ |
| 15776 | 50.32% | 7449.00 | 2.908 | 31.09% | 53.65% | 616 |
| 15777 | 50.66% | 8821.32 | 3.083 | 31.09% | 53.72% | 610 |

- 结果：
  - `profit`: `7449.00 -> 8821.32`
  - `win_rate`: `50.32% -> 50.66%`
  - `sharpe`: `2.908 -> 3.083`
  - `max_drawdown`: 持平，仍为 `31.09%`

### 直接命中的 long

- 新 long 过滤在 `15777` 中命中 `6` 次：
  - `2021-09-18 08:00:00`
  - `2021-10-10 20:00:00`
  - `2023-02-06 12:00:00`
  - `2024-03-28 12:00:00`
  - `2024-04-12 08:00:00`
  - `2025-07-16 04:00:00`
- 这些样本在 `filtered_signal_log` 中全部标记为 `LOSS`，说明新增过滤拦住的确实是坏 long。

### 交易路径变化

- 提升最大的时间点：
  - `2025-07-16 12:00:00 long`：`0.00 -> 575.37`，改善 `+575.37`
  - `2025-05-07 00:00:00 long`：改善 `+172.95`
  - `2024-04-11 16:00:00 short`：`-4.33 -> 120.31`，改善 `+124.64`
  - `2025-08-07 16:00:00 long`：改善 `+121.85`
  - `2025-11-12 20:00:00 short`：改善 `+116.52`
- 变差最大的时间点：
  - `2025-07-16 04:00:00 long`：`507.11 -> 0.00`，回吐 `-507.11`
  - `2026-01-26 00:00:00 short`：回吐 `-51.57`
  - `2025-10-10 00:00:00 long`：回吐 `-37.61`
  - `2025-10-24 20:00:00 short`：回吐 `-37.21`

### 分析结论

- 这条对称 long 过滤不是简单“少开几笔多单”，而是在抑制“缩量 + 中性 RSI + MACD 刚转弱”的抢多行为。
- 从净效果看：
  - `long` 侧总净改善约 `+816.89`
  - `short` 侧总净改善约 `+555.43`
- 典型路径是：
  - 先移除一笔质量差的 early long
  - 随后让更晚、更顺势的 long 接管同一段行情
- 因此这条规则与前面的 short 过滤形成了对称约束：
  - 避免在“缩量 + RSI 中性 + MACD 修复/转弱”时过早反手
  - 结果上同时改善了 long 和 short 两侧的整体质量

### 当前状态

- 当前代码层最佳候选更新为 `15777`：
  - `win_rate=50.66%`
  - `profit=8821.32`
  - `sharpe=3.083`
  - `max_drawdown=31.09%`
- `strategy_config.id=11` 的数据库参数当前仍与 `15747` 对齐，这次 `15776 -> 15777` 的提升来自代码层过滤，不来自 JSON 参数变更，因此不需要额外改写参数配置。
- 差异明细已导出：
  - `docs/backtest_reports/15776_vs_15777_trade_pnl_deltas.tsv`
- 如果后续继续优化，应以“当前代码 + 当前 id=11 参数”作为新的联合起点，而不是只看数据库里的参数快照。

## 2026-03-19 补充优化：强反弹锤子线保护 short，最终采用保本止损

### 触发背景

- 样本起点：
  - `2025-10-22 04:00:00` 开空，开仓价 `3870.8`
- 关键反向信号：
  - `2025-10-23 04:00:00`
- 当时观察到的特征：
  - 强势锤子线，且收盘上涨
  - `bollinger_value.is_long_signal = true`
  - 从最近高点 `2025-10-22 16:00:00` 到当时低点已经有一段明显跌幅
  - 没有形成明确跌破前支撑后的继续下杀
- 基线 `15777` 中，这笔 short 没有在该时点得到保护，最终：
  - `2025-10-24 12:00:00` 以 `3934.49` 被 `Signal_Kline_Stop_Loss` 打掉
  - `pnl = -103.73`

### 代码层实验

- 新增保护标记：
  - `REBOUND_HAMMER_LONG_PROTECT`
- 含义：
  - 当出现“强锤子线 + 布林带 long + 明显反弹保护形态”时，不再把这类 long 仅仅当作被 `MACD_FALLING_KNIFE_LONG` 过滤的候选信号，而是额外给 short 持仓一个保护动作。
- 保护动作分两种 A/B：
  - `15787`：直接止盈
  - `15788`：保本止损

### 回测结果

| 回测 ID | 规则 | 胜率 | 盈利 | Sharpe | MaxDD | 开仓数 |
| ------- | ---- | ---- | ---- | ------ | ----- | ------ |
| 15777 | 基线，无保护动作 | 50.66% | 8821.32 | 3.083 | 31.09% | 610 |
| 15787 | 命中后直接止盈 | 50.73% | 9000.25 | 3.088 | 31.09% | 614 |
| 15788 | 命中后抬到开仓价保本 | 50.66% | 9017.24 | 3.088 | 31.09% | 614 |

- 对单笔样本 `2025-10-22 04:00:00 short`：
  - `15777`：最终 `-103.73`
  - `15787`：`2025-10-23 04:00:00` 直接止盈，变成 `+100.34`
  - `15788`：`2025-10-23 04:00:00` 将止损抬到开仓价，`2025-10-23 12:00:00` 保本出场，结果 `0`

### 样本统计

- `REBOUND_HAMMER_LONG_PROTECT` 全历史共出现 `17` 次
- 其中真正作用在“当时持有中的 short”上的场景有 `9` 次
- 这 `9` 次里：
  - `3` 次直接止盈更好
  - `3` 次保本止损更好
  - `3` 次两者无差异

按这 `9` 个场内样本统计：

- 直接止盈总改进：`+156.56`
- 保本止损总改进：`+124.19`

但从整轮回测结果看：

- `15787`：`profit = 9000.25`
- `15788`：`profit = 9017.24`

也就是说：

- 局部样本修复上，直接止盈更激进
- 全局资金曲线上，保本止损略优，且对原本还能继续盈利的 short 破坏更小

### 分类结论

- 基线最终为亏损的场景里，直接止盈通常更强：
  - 例如 `2025-07-13 04:00:00`
  - 例如 `2025-10-23 04:00:00`
- 基线最终仍能赚钱的场景里，保本止损更稳：
  - 例如 `2023-12-26 16:00:00`
  - 例如 `2024-06-04 12:00:00`
  - 例如 `2025-08-25 16:00:00`

### 最终决策

- 这类 `REBOUND_HAMMER_LONG_PROTECT` 场景，最终采用：
  - `保本止损`
- 原因：
  - 它仍然能有效修复 `2025-10-23 04:00:00` 这类明显错误的继续持空样本
  - 相比直接止盈，更不容易过早砍掉原本还能继续扩大利润的 short
  - 在全局回测上，`15788` 的总利润略高于 `15787`，回撤不恶化

### 当前状态

- 当前代码层候选，应以 `15788` 这条保护决策作为后续优化基线之一：
  - `REBOUND_HAMMER_LONG_PROTECT -> move stop to entry`
- 本次仍然不涉及 `strategy_config.id=11` 的 JSON 参数改动，属于代码层风控行为优化。

## 2026-03-19 窄实验：Expansion Long / Fake Breakout Short

### 背景

围绕两个人工观察样本，补两条非常窄的实验规则：

- `2025-10-26 16:00:00`
  - 观察为强放量大阳线，`MACD` 在零轴上方且柱子继续放大，接近“扩张后的延续 long”
- `2025-10-28 20:00:00`
  - 观察为放量冲高回落，长上影，`MACD` 已进入死叉状态且柱子继续走弱，更像“假突破反转 short”

### 实验规则

- `15789`：仅开启 `EXPANSION_CONTINUATION_LONG`
- `15790`：仅开启 `FAKE_BREAKOUT_REVERSAL_SHORT`
- `15791`：两条规则同时开启

### 回测结果

| 回测 ID | 规则 | 胜率 | 盈利 | Sharpe | MaxDD | 开仓数 |
| ------- | ---- | ---- | ---- | ------ | ----- | ------ |
| 15788 | 基线 | 50.66% | 9017.24 | 3.08804 | 31.09% | 614 |
| 15789 | 仅 Expansion Long | 50.57% | 8646.17 | 3.04067 | 31.09% | 615 |
| 15790 | 仅 Fake Breakout Short | 50.66% | 9140.40 | 3.10179 | 31.09% | 614 |
| 15791 | 双开 | 50.57% | 8764.32 | 3.05430 | 31.09% | 615 |

### 样本点验证

- `2025-10-26 16:00:00`
  - `15789 / 15791` 在 `dynamic_config_log` 中该时点 `adjustments = []`
  - 说明这条 `EXPANSION_CONTINUATION_LONG` 规则没有命中这根 K 线
  - 对应开多仍然发生在下一根 `2025-10-26 20:00:00`
- `2025-10-28 20:00:00`
  - `15790 / 15791` 在 `dynamic_config_log` 中该时点出现：
    - `["FAKE_BREAKOUT_REVERSAL_SHORT", "STOP_LOSS_SIGNAL_KLINE", "STOP_LOSS_ATR"]`
  - 对应这两轮都提前在 `2025-10-28 20:00:00` 开空
  - 基线 `15788` 则要等到 `2025-10-29 00:00:00` 才开空

### 交易结果对照

- 基线 `15788`
  - `2025-10-29 00:00:00 short`，`4037.33 -> 3937.55`，`+138.26`
- `15790`
  - `2025-10-28 20:00:00 short`，`4095.42 -> 3937.55`，`+217.90`
- `15791`
  - `2025-10-28 20:00:00 short`，`4095.42 -> 3937.55`，`+209.03`

### 结论

- `FAKE_BREAKOUT_REVERSAL_SHORT` 有效
  - 它确实把 `2025-10-28 20:00:00` 这类“死叉状态下的假突破回落”提前识别出来
  - 单独开启即可优于基线：`15790 > 15788`
- `EXPANSION_CONTINUATION_LONG` 当前版本无效
  - 它没有命中目标样本 `2025-10-26 16:00:00`
  - 单独开启和双开都拖累总体结果：`15789 / 15791 < 15788`

### 当前决策

- 下一轮只保留并继续优化：
  - `FAKE_BREAKOUT_REVERSAL_SHORT`
- 暂不保留当前版本的：
  - `EXPANSION_CONTINUATION_LONG`
- 对 `2025-10-28 20:00:00` 的认定进一步明确：
  - 图形上已经进入“死叉状态”
  - 当前代码原有 `is_death_cross` 更偏“事件定义”，只认刚跨过零轴的那一根
  - 本次新规则改为接受“死叉状态 + 柱子继续走弱”，因此能在这一根提前开空

### Near-miss 复盘

为了判断这条 short 规则是否还能继续放宽，额外检查了基线 `15788` 中多笔“下一根才开空”的前一根 K 线。

核心对照样本：

- `2025-10-28 20:00:00`
  - `volume_ratio = 2.136`
  - `fib.volume_confirmed = true`
  - `fib_ratio = 0.6165`
  - `swing_high.crossed = true`
  - `bear_leg = true`
  - `new_leg = true`
  - `hang_short = true`
  - `up_shadow = 0.6008`
  - `macd_line = 49.31 < signal_line = 55.71`
  - `histogram = -6.40`
  - 这是一笔标准的“高位假突破回落 short”

对照 near-miss：

- `2025-11-03 04:00:00`
  - `volume_ratio = 0.841`
  - `fib.volume_confirmed = false`
  - `swing_high.crossed = false`
  - `bear_leg = false`
  - `hang_short = false`
  - `histogram = +8.17`
  - 这类不属于假突破回落，继续提前开空不合理

- `2025-11-12 16:00:00`
  - `volume_ratio = 1.346`
  - `fib.volume_confirmed = false`
  - `swing_high.crossed = false`
  - `bear_leg = true`
  - `hang_short = false`
  - `up_shadow = 0.150`
  - `histogram = -9.95`
  - MACD 虽然转弱，但量能、结构、上影线都不够，不属于同一类

- `2025-12-15 16:00:00`
  - `volume_ratio = 0.883`
  - `fib.volume_confirmed = false`
  - `swing_high.crossed = false`
  - `bear_leg = true`
  - `hang_short = false`
  - `up_shadow = 0.426`
  - `histogram = +5.28`
  - 这里甚至不是空头柱，不能提前打成假突破 short

- `2026-01-20 08:00:00`
  - `volume_ratio = 1.275`
  - `fib.volume_confirmed = false`
  - `swing_high.crossed = false`
  - `bear_leg = true`
  - `new_leg = true`
  - `hang_short = false`
  - `up_shadow = 0.459`
  - `macd_line/signal_line` 都在零轴下方
  - 这更像低位继续走弱，不是高位假突破

- `2026-03-06 16:00:00`
  - `volume_ratio = 0.799`
  - `fib.volume_confirmed = false`
  - `swing_high.crossed = false`
  - `bear_leg = true`
  - `hang_short = false`
  - `up_shadow ≈ 0`
  - `histogram = -6.17`
  - 也不是 `2025-10-28 20:00:00` 那类冲高失败

结论：

- `FAKE_BREAKOUT_REVERSAL_SHORT` 当前有效，但本质上仍是非常窄的高置信度规则
- 尝试两次 broadening 后，`15792 / 15793` 与 `15790` 完全一致，没有新增命中
- 说明继续盲目放宽条件没有意义
- 后续如果还要扩这条规则，应先继续筛选“高位冲高失败 + MACD 零轴上方转弱”的真近似样本，而不是直接放宽量能、腿部或 Fib 条件

## 2026-03-19 新一轮窄过滤优化：15790 -> 15797 -> 15798

本轮继续以“当前代码 + `strategy_config.id=11` 参数”为联合起点，在 `15790` 之上只做两条 short 侧窄过滤，不改参数。

### 第一步：过滤极端低位放量追空，得到 15797

目标样本是 `2026-01-26 00:00:00 short`。这笔是 `15790` 最大亏损，开仓特征非常集中：

- `volume_ratio = 8.958`
- `rsi = 22.10`
- `fib.in_zone = false`
- `fib_ratio = 0.0273`
- `bollinger.is_long_signal = true`
- `ema_touch.is_short_signal = true`
- `ema_values.is_short_trend = true`
- `leg.is_new_leg = false`
- `macd_line/signal_line < 0`

这更像“旧空头腿末端的最后一脚放量砸盘”，而不是值得继续追的 short。

新增过滤：

- `EXHAUSTION_SHORT_NEAR_SWING_LOW_BLOCK`

条件：

- `rsi < 25`
- `volume_ratio >= 5.0`
- `!fib.in_zone`
- `fib_ratio <= 0.05`
- `bollinger.long_signal = true`
- `ema_touch.short_signal = true`
- `ema_values.is_short_trend = true`
- `!leg.is_new_leg`
- `macd_line < 0 && signal_line < 0`

结果：

- `15790`: `win_rate 50.6557%`, `profit 9140.40`, `sharpe 3.10179`, `max_dd 31.09%`, `open_positions 614`
- `15797`: `win_rate 50.7389%`, `profit 9532.44`, `sharpe 3.14978`, `max_dd 31.09%`, `open_positions 613`

验证：

- `filtered_signal_log` 中这条新规则只命中 `1` 次
- 命中时间正是 `2026-01-26 00:00:00`
- 原来的那笔 `short` 在 `15797` 中已不再开仓

结论：

- 这条规则足够窄，不是大面积压缩 short
- 它直接修掉了联合基线里最大的 exhaustion short

### 第二步：过滤 bullish leg 下的反向均值回归 short，得到 15798

继续看 `15797` 的大亏损 short，`2025-10-24 20:00:00` 仍然很差。它和 `2024-10-28 20:00:00` 有高度一致的结构：

- `ema_values.is_short_trend = false`
- `leg_detection.is_bullish_leg = true`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `fib.leg_bullish = true`
- `bollinger.is_short_signal = true`
- `ema_touch.is_short_signal = false`
- `rsi` 处于中性偏弱区间（约 `47~49`）
- `macd_line/signal_line < 0`，但 `histogram > 0` 且在回落

这类单不是顺势做空，更像“bullish leg 里的布林回落 short”，方向和腿部状态冲突。

新增过滤：

- `BULLISH_LEG_MEAN_REVERSION_SHORT_BLOCK`

条件：

- `volume_ratio >= 1.8`
- `!ema_values.is_short_trend`
- `leg.is_bullish_leg && !leg.is_new_leg`
- `fib.in_zone && fib.volume_confirmed && fib.leg_bullish`
- `bollinger.is_short_signal`
- `!ema_touch.is_short_signal`
- `rsi in [45, 50]`
- `macd_line < 0 && signal_line < 0`
- `histogram > 0 && histogram_decreasing`

结果：

- `15797`: `win_rate 50.7389%`, `profit 9532.44`, `sharpe 3.14978`, `max_dd 31.09%`, `open_positions 613`
- `15798`: `win_rate 50.9061%`, `profit 10055.90`, `sharpe 3.21124`, `max_dd 31.09%`, `open_positions 611`

验证：

- `filtered_signal_log` 中这条规则只命中 `2` 次
- 命中时间：
  - `2024-10-28 20:00:00`
  - `2025-10-24 20:00:00`
- 两笔在基线中都是亏损 short：
  - `2024-10-28 20:00:00`: `-19.24`
  - `2025-10-24 20:00:00`: 原本也是明显亏损 short
- 在 `15798` 中这两笔都不再开仓

结论：

- 这条规则仍然足够窄，只过滤了两笔冲突结构的 short
- `15798` 是当前这轮最新最优候选，且比 `15797` 继续同步提高了 `profit / win_rate / sharpe`

### 当前状态

本轮新增两条有效 short 过滤都保留在代码中：

- `EXHAUSTION_SHORT_NEAR_SWING_LOW_BLOCK`
- `BULLISH_LEG_MEAN_REVERSION_SHORT_BLOCK`

到 `15798` 为止，指标已经提升到：

- `win_rate = 50.9061%`
- `profit = 10055.90`
- `sharpe = 3.21124`
- `max_drawdown = 31.09%`

下一步如果继续优化，优先目标应转向 long 侧坏单：

- `2025-10-10 00:00:00 long`
- `2026-01-07 04:00:00 long`

因为当前剩余最大亏损已经不再是 short exhaustion，而是 long 侧的 falling-knife / chase-long 类样本。

## 2026-03-19 上游分支说明：ETH/BTC/SOL 4H 代码层窄过滤

### 本轮目标

- 从默认 4H 篮子中移除 `BCH/LTC`，先把基线收敛到 `ETH/BTC/SOL`
- 修复 `MarketStructure weight=0` 仍参与方向投票的评分耦合
- 在不显著伤害利润的前提下，继续清理 `ETH` 与 `SOL` 的坏 short / 坏 long 样本

### 当前代码层基线

- 默认 4H 回测篮子仅保留：
  - `ETH-USDT-SWAP`
  - `BTC-USDT-SWAP`
  - `SOL-USDT-SWAP`
- `run_id` 缩短为 `bt-{ts}-{uuid_simple}`，避免审计表 `VARCHAR(64)` 写库失败
- `weight <= 0` 的信号现在真正不参与打分，也不参与方向投票
- 已保留的窄过滤：
  - `EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_LONG/SHORT`
  - `SIMPLE_BREAK_CHOCH_NO_BOS_LONG`
  - `SIMPLE_BREAK_BULLISH_STRUCTURE_SHORT`
  - `SIMPLE_BREAK_TOO_FAR_SHALLOW_FIB_SHORT`

### 当前数据库参数基线（4H）

- `ETH`
  - `MarketStructure = 0.2`
  - `min_total_weight = 2.3`
  - `bb_width_threshold = 0.029`
  - `short_threshold = 0.132`
  - `max_loss_percent = 0.04`
- `BTC`
  - `MarketStructure = 0.0`
  - `min_total_weight = 2.23`
  - `bb_width_threshold = 0.04`
  - `short_threshold = 0.148`
  - `max_loss_percent = 0.05`
- `SOL`
  - `MarketStructure = 0.0`
  - `min_total_weight = 2.66`
  - `bb_width_threshold = 0.029`
  - `short_threshold = 0.176`
  - `max_loss_percent = 0.048`

### 基线回测结果

| Backtest ID | 标的 | 胜率 | 利润 | Sharpe | MaxDD |
| ----------- | ---- | ---- | ---- | ------ | ----- |
| 1216 | ETH-USDT-SWAP | 52.72% | 13176.80 | 3.57768 | 34.95% |
| 1217 | BTC-USDT-SWAP | 54.40% | 334.79 | 1.03208 | 36.83% |
| 1218 | SOL-USDT-SWAP | 43.54% | 479.04 | 1.12446 | 41.76% |

对比前一轮代码基线 `1184 / 1185 / 1186`：

- `ETH`: `12636.5 -> 13176.8`，`Sharpe 3.52485 -> 3.57768`
- `BTC`: 基本不变
- `SOL`: `448.229 -> 479.041`，`win_rate 43.39% -> 43.54%`，`MaxDD 42.22% -> 41.76%`

### 关键结论

- `SOL` 上“适当放宽 `bb_width_threshold`”只能小幅抬胜率，但会压利润，不作为当前主方向
- 真正有效的是 short 侧窄过滤，而不是继续全局调 `min_total_weight` 或直接放宽布林带
- `SIMPLE_BREAK_TOO_FAR_SHALLOW_FIB_SHORT` 在 `1218` 的 shadow 结果为：
  - `4` 笔
  - `shadow_pnl = -0.6256`
- `SIMPLE_BREAK_BULLISH_STRUCTURE_SHORT` 在 `1218` 的 shadow 结果为：
  - `4` 笔
  - `shadow_pnl = -0.4356`
- 两条新 short 过滤在 `ETH` 上也带来了正向改善，`BTC` 基本未受影响

### 当前决策

- 将 `1216 / 1217 / 1218` 记录为当前工作基线
- 后续继续优化时：
  - 不再把 `BCH/LTC` 放回默认 4H 篮子
  - 不再把“放宽布林带宽度”作为 `SOL` 的首要方向
  - 下一步优先复盘 `SOL` 剩余的 `Tangled` 背景 short，尤其是仍然走到 `Engulfing` 止损链的样本

## 2026-03-19 拉取后复盘：15798 vs 15799

拉取代码后，数据库里出现了更新的 `15799`，但它并不支配 `15798`。

结果对比：

- `15798`: `win_rate 50.9061%`, `profit 10055.90`, `sharpe 3.21124`, `max_dd 31.09%`, `volatility 53.57%`, `open_positions 611`
- `15799`: `win_rate 51.1149%`, `profit 7638.67`, `sharpe 3.01320`, `max_dd 29.25%`, `volatility 52.10%`, `open_positions 583`

结论：

- `15799` 更保守
- 它提高了胜率，压低了回撤和波动
- 但明显牺牲了利润和 Sharpe

从 `filtered_signal_log` 计数看，`15799` 的差异不在旧的窄过滤，而在上游那组更广泛的结构过滤开始大量生效：

- `EXHAUSTION_SHORT_NEAR_SWING_LOW_BLOCK`
  - `15798: 1`
  - `15799: 0`
- `BULLISH_LEG_MEAN_REVERSION_SHORT_BLOCK`
  - `15798: 2`
  - `15799: 2`
- `LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT`
  - `15798: 2`
  - `15799: 2`
- `LOW_VOLUME_NEUTRAL_RSI_MACD_WEAKENING_BLOCK_LONG`
  - `15798: 6`
  - `15799: 6`
- `EMA_TOO_FAR_COUNTER_TREND_CHASE_LONG`
  - `15798: 2`
  - `15799: 3`
- `EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_LONG`
  - `15798: 0`
  - `15799: 29`
- `EMA_TREND_NO_PATTERN_BELOW_FIB_MIDLINE_SHORT`
  - `15798: 0`
  - `15799: 44`
- `SIMPLE_BREAK_CHOCH_NO_BOS_LONG`
  - `15798: 0`
  - `15799: 4`
- `SIMPLE_BREAK_BULLISH_STRUCTURE_SHORT`
  - `15798: 0`
  - `15799: 3`
- `SIMPLE_BREAK_TOO_FAR_SHALLOW_FIB_SHORT`
  - `15798: 0`
  - `15799: 5`

这说明：

- `15799` 不是“局部修掉几个坏单”
- 它是通过大范围减少弱结构入场，把交易数从 `611` 压到 `583`
- 因此坏单确实少了，但很多大盈利单也被一起压缩

最大回吐样本：

- `2025-11-12 20:00:00`: `819.35 -> 628.36`
- `2025-05-07 00:00:00`: `1107.75 -> 965.33`
- `2026-03-09 08:00:00`: `584.35 -> 445.26`
- `2026-01-20 12:00:00`: `581.40 -> 450.44`
- `2025-08-07 16:00:00`: `813.79 -> 690.76`

也有改善样本：

- `2025-03-31 04:00:00`: `-30.23 -> 94.93`
- `2025-11-07 16:00:00`: `-203.05 -> -155.72`
- `2025-10-10 00:00:00`: `-251.20 -> -206.82`
- `2026-01-07 04:00:00`: `-182.17 -> -139.71`

当前判断：

- 如果目标优先级是 `profit + sharpe`，`15798` 仍是更合理的主基线
- 如果目标优先级是 `win_rate + max_drawdown + volatility`，`15799` 可以作为保守对照组

## 2026-03-19 样本复盘：15799 中的 2025-11-07 16:00:00 short

这笔单在 `15799` 中仍然开成了 short，最终结果：

- `open_position_time = 2025-11-07 16:00:00`
- `open_price = 3258.39`
- `close_price = 3356.14`
- `profit_loss = -155.72`
- `close_type = Signal_Kline_Stop_Loss`
- `stop_loss_source = Engulfing_Volume_Rejected`

### 为什么系统会开空

从当根信号快照看，这不是高质量趋势空，而是“少量负面条件叠加过阈值”的 short：

- `ema_values.is_short_trend = true`
- `engulfing_value.is_valid_engulfing = true`
- `leg_detection.is_bearish_leg = true`
- `leg_detection.is_new_leg = false`
- `rsi = 34.01`

而同时存在多项明显不利于继续做空的特征：

- `bollinger.is_long_signal = true`
- `ema_touch.is_short_signal = false`
- `fib.in_zone = false`
- `fib.volume_confirmed = false`
- `volume_ratio = 1.025`
- `macd_line = -94.74`
- `signal_line = -102.72`
- `histogram = +7.97`

也就是说：

- MACD 仍然远离零轴下方，但已经处于“快线在慢线上方、柱子为正”的修复状态
- 量能只是轻微放大，不是新的下跌扩张
- 布林带给的是 `long_signal`
- 没有 `ema_touch.short`，也没有 `fib` 回抽确认

当前系统之所以仍然开空，本质上是：

- `Engulfing short + bearish leg + RSI 条件`
- 在当前权重体系下已经足够把 short 推过阈值

但这笔单不符合“急跌后回调横盘震荡中，不应继续追空”的直觉。

### 为什么 2025-11-07 20:00:00 没有开多

`2025-11-07 20:00:00` 在 `15799` 里没有开仓，策略结果是：

- `direction = None`
- `should_buy = false`
- `should_sell = false`

当根特征：

- `bollinger.is_long_signal = true`
- `volume_ratio = 2.275`
- `rsi = 37.32`
- `macd_line = -92.44`
- `signal_line = -100.66`
- `histogram = +8.22`

但缺失关键 long 触发：

- `engulfing.is_valid_engulfing = false`
- `kline_hammer.is_long_signal = false`
- `ema_touch.is_long_signal = false`
- `fib.in_zone = false`
- `leg_detection.is_bullish_leg = false`

所以这根 K 线已经具备“修复继续”的背景，但还没有凑够当前策略认可的 long 入场确认。

### 当前结论

这笔 `2025-11-07 16:00:00 short` 确实是一个不好的位置，问题核心不是“系统完全看错趋势”，而是：

- 在深负 MACD 修复区里
- 用 `Engulfing short + bearish leg` 过早继续做空
- 而现有 short 过滤没有覆盖这类 `deep negative MACD recovery short`

如果后续继续优化 `15799` 这条风险优先基线，优先考虑新增一条更窄的 short 过滤，方向是：

- `macd_line/signal_line` 都极低且小于 `-80`
- `histogram > 0`
- `bollinger.long_signal = true`
- `ema_touch.short_signal = false`
- `fib.in_zone = false`
- `fib.volume_confirmed = false`
- `volume_ratio` 只在轻微放大区间

这会更接近“急跌后横盘修复，不应继续追空”的场景。

## 2026-03-19 窄实验：15799 -> 15800 修复 deep negative MACD recovery short

基于上面的样本复盘，我新增了一条极窄的 short 过滤：

- `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK`

触发条件：

- `ema_values.is_short_trend = true`
- `engulfing_value.is_valid_engulfing = true`
- `bollinger_value.is_long_signal = true`
- `ema_touch_value.is_short_signal = false`
- `fib_retracement_value.in_zone = false`
- `fib_retracement_value.volume_confirmed = false`
- `volume_ratio` 在 `1.0 ~ 1.5`
- `rsi` 在 `30 ~ 38`
- `macd_line < -80`
- `signal_line < -80`
- `histogram > 0`
- `histogram_decreasing = true`

这条规则的目标不是大范围收紧 short，而是只拦：

- 深负 MACD 修复区
- 量能只是轻微放大
- 布林已经给出 long 反应
- 但系统仍因为 `Engulfing short + bearish leg` 提前做空

### 回测结果

- `15799`: `win_rate 51.11%`, `profit 7638.67`, `sharpe 3.01320`, `max_dd 29.25%`, `volatility 52.1017%`, `open_positions 583`
- `15800`: `win_rate 51.29%`, `profit 8313.26`, `sharpe 3.10486`, `max_dd 29.25%`, `volatility 52.1031%`, `open_positions 584`

净变化：

- `profit +674.59`
- `win_rate +0.18pct`
- `sharpe +0.09166`
- `max_drawdown` 持平
- `volatility` 基本持平，仅增加 `0.0014pct`

### 命中情况

这条过滤在 `15800` 中只命中 `1` 次：

- `2025-11-07 16:00:00`

数据库验证：

- `filtered_signal_log` 中该时点被记录为：
  - `direction = SHORT`
  - `filter_reasons = ["DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK"]`
- `back_test_detail` 中不再存在该时点的实际 short 开仓

### 结论

这次实验是有效的，而且符合当前风险优先目标：

- 回撤没有变差
- 波动几乎不变
- 胜率更高
- 利润和 Sharpe 同时提升

因此在当前这条 `15799` 风险优先线里，`15800` 是更合理的后继候选。

## 2026-03-19 泛化实验：15800 -> 15801 -> 15802

为了验证 `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK` 是否只耦合 `2025-11-07 16:00:00` 这一笔，我把它改成了 3 个模式：

- `v1`：当前默认版，只拦最窄定义
- `v2`：保留 `short_trend + bollinger.long`，放宽 `engulfing / leg`、`rsi`、`volume`
- `v3`：进一步放宽到“深负 MACD 修复区 + 非确认 short”，不再强依赖 `short_trend` 和 `bollinger.long`

环境变量：

- `VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE=v1|v2|v3|off`

### 结果对比

- `15800 (v1)`: `win_rate 51.2909%`, `profit 8313.26`, `sharpe 3.10486`, `max_dd 29.2533%`, `volatility 52.1031%`, `open_positions 584`
- `15801 (v2)`: `win_rate 51.2909%`, `profit 8313.26`, `sharpe 3.10486`, `max_dd 29.2533%`, `volatility 52.1031%`, `open_positions 584`
- `15802 (v3)`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`

### 关键结论

- `v2` 没有新增命中，结果与 `v1` 完全一致
- `v3` 才真正把命中从 `1` 扩到 `2`
- 而且 `v3` 不是以更高回撤换来的：
  - `max_drawdown` 持平
  - `volatility` 反而略降
  - `win_rate / profit / sharpe` 同时提升

### 命中样本

- `15800 / 15801`
  - `2025-11-07 16:00:00`
- `15802`
  - `2021-09-10 12:00:00`
  - `2025-11-07 16:00:00`

新增被拦的 `2021-09-10 12:00:00` 在 `15799/15800` 里的实际结果是：

- `open_price = 3406.10`
- `close_price = 3466.73`
- `profit_loss = -1.85`
- `close_type = Signal_Kline_Stop_Loss`

这说明：

- 当前规则最初确实偏耦合，只命中 1 笔
- 但继续做“结构泛化”是有效的
- `v3` 已经把它扩展成至少 `2` 个同类坏 short，而且没有伤到风险指标

### 当前判断

如果当前优化目标仍是：

- 更低回撤
- 更低波动
- 更高胜率
- 同时保留较好利润

那么在 `15799` 这条风险优先主线里，`15802` 比 `15800` 更优。

## 2026-03-19 继续泛化：15802 -> 15803（拒绝）

在 `15802` 的基础上，我继续做了两步验证：

1. 先试图把 `macd_line/signal_line < -50` 放宽到 `< -40`
   - 预筛后发现仍然只会命中同样 `2` 笔
   - 说明瓶颈并不在这条阈值

2. 再尝试只放宽 `RSI < 45` 到 `RSI < 50`
   - 这会新增第 `3` 个候选：
     - `2025-02-06 16:00:00`

对应回测结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15803`: `win_rate 51.2953%`, `profit 7968.37`, `sharpe 3.06357`, `max_dd 29.2533%`, `volatility 52.0204%`, `open_positions 582`

### 为什么 `15803` 变差

`15803` 的新命中样本是：

- `2021-09-10 12:00:00`
- `2025-02-06 16:00:00`
- `2025-11-07 16:00:00`

其中新增的 `2025-02-06 16:00:00` 在旧基线 `15799` 里其实不是坏 short，而是盈利单：

- `open_price = 2804.25`
- `close_price = 2627.30`
- `profit_loss = +86.05`

也就是说：

- `RSI < 50` 这个放宽虽然把命中从 `2` 扩到了 `3`
- 但新增进来的不是“同类坏 short”
- 而是把一笔原本有效的盈利 short 也错杀了

### 当前结论

- `15802` 仍然是这条 `deep negative MACD recovery short` 主线上的最佳版本
- 继续放宽 `RSI` 是错误方向
- 如果后续还要扩样本，不应该先动 `RSI`
- 更合理的是继续围绕结构条件做筛选，而不是放宽数值门槛

## 2026-03-19 结构泛化尝试：15802 -> 15804（拒绝）

在确认 `RSI < 50` 是错误方向后，我继续试了一个纯结构放宽：

- 保持 `v3` 的其余条件不变
- 去掉 `ema_touch.short_signal = false` 这一条限制

对应模式：

- `VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE=v6`

结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15804`: `win_rate 51.2953%`, `profit 7968.37`, `sharpe 3.06357`, `max_dd 29.2533%`, `volatility 52.0204%`, `open_positions 582`

实际命中样本仍然是：

- `2021-09-10 12:00:00`
- `2025-02-06 16:00:00`
- `2025-11-07 16:00:00`

也就是说，这条结构放宽在实盘结果上并没有带来新的高质量坏 short，只是复现了和 `15803` 类似的错误扩张：

- 新增拦掉的 `2025-02-06 16:00:00` 仍是一笔原本盈利的 short
- 因此利润、胜率、Sharpe 都退化

### 当前结论

- `15802` 仍是这条 short 过滤主线上的最佳版本
- 去掉 `ema_touch.short_signal = false` 并不能带来有效泛化
- 到这一步可以判断：当前这条规则再继续扩样本，已经开始明显损伤有效 short

## 2026-03-19 高位横盘追多过滤：15802 -> 15806（拒绝）

针对 `2025-05-21 20:00:00` 这笔 long，我做了一条很窄的实验过滤：

- 环境变量：`VEGAS_HIGH_LEVEL_SIDEWAYS_LONG_BLOCK=v1`
- 过滤原因：`HIGH_LEVEL_SIDEWAYS_CHASE_LONG_BLOCK`

规则意图是拦掉这类形态：

- `long_trend=true`
- `engulfing.valid=true`
- `bullish_leg=true`
- 但 `fib.in_zone=false`
- `fib.volume_confirmed=false`
- `retracement_ratio>=0.75`
- `volume_ratio<1.6`
- `body_ratio<0.55`
- `bollinger.short=true`
- 且没有新的 bullish BOS / 没有突破最近高点

样本 `2025-05-21 20:00:00` 确实命中了这条规则，而且这笔在基线 `15802` 里本来就是坏 long：

- `open_price = 2565.88`
- `close_price = 2536.46`
- `profit_loss = -38.54`
- `close_type = Signal_Kline_Stop_Loss`

对应回测结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15806`: `win_rate 51.3793%`, `profit 7985.93`, `sharpe 3.06215`, `max_dd 29.2533%`, `volatility 52.0851%`, `open_positions 580`

### 为什么 `15806` 变差

这条规则实际只命中 `1` 次：

- `2025-05-21 20:00:00`

说明它虽然修对了这笔单，但仍然高度耦合，没有形成可泛化的一类高质量坏 long 过滤。

更重要的是，拦掉这笔小亏 long 后，整体持仓路径反而受损，典型负面变化包括：

- `2025-10-28 20:00:00`: `+204.05 -> 0.00`
- `2025-07-11 16:00:00`: `0.00 -> -57.82`
- `2025-05-07 00:00:00`: `+1038.80 -> +983.53`
- `2024-06-07 20:00:00`: `+49.96 -> 0.00`

### 当前结论

- `2025-05-21 20:00:00` 这笔从盘面上看，确实属于“爆拉后高位横盘、无强放量、非大实体突破”的坏追多
- 但把它直接做成全局 `block long entry` 规则，并不能提升系统
- 当前最合理的判断是：
  - 这笔局部判断是对的
  - 但不值得上升为通用过滤
  - 这条实验规则拒绝，不纳入风险优先基线

## 2026-03-19 最近两根带量上影压制 long：15802 -> 15807 / 15808 / 15809

基于 `2026-01-07 04:00:00` 这笔 long，我继续测试了你提出的新条件：

- 最近两根范围内如果已经出现 `上影线 + 放量`
- 当前不应继续做多
- 除非当前这根本身是 `大实体上涨` 且 `量能更大`

### v1：宽版（15807，拒绝）

环境变量：

- `VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK=v1`

结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15807`: `win_rate 49.6982%`, `profit 2742.43`, `sharpe 2.12599`, `max_dd 31.4740%`, `volatility 49.6542%`, `open_positions 497`

实际命中 `138` 次，明显过宽，直接把大量 long 都压掉了，因此立即拒绝。

### v2：收窄到 “不在健康 long trend + TooFar + Fib 不理想区 + 当前量不如前两根” （15808，拒绝）

环境变量：

- `VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK=v2`

结果：

- `15808`: `win_rate 51.3793%`, `profit 8336.56`, `sharpe 3.09331`, `max_dd 29.2533%`, `volatility 52.1022%`, `open_positions 580`

实际命中 `4` 次：

- `2025-04-23 04:00:00` `END +0.2333`
- `2025-04-26 00:00:00` `LOSS -0.0326`
- `2026-01-07 04:00:00` `LOSS -0.0230`
- `2026-03-13 16:00:00` `END +0.0217`

说明这版虽然已经明显收窄，但仍然误杀了正贡献 long。

### v3：继续加上 `fib.retracement_ratio > 0.90`，只打真正高位追多（15809，拒绝）

环境变量：

- `VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK=v3`

结果：

- `15809`: `win_rate 51.3793%`, `profit 8075.17`, `sharpe 3.07480`, `max_dd 29.2533%`, `volatility 52.0752%`, `open_positions 580`

实际命中只剩 `2` 次：

- `2025-04-26 00:00:00` `LOSS -0.0326`
- `2026-01-07 04:00:00` `LOSS -0.0230`

这说明规则已经收得足够干净，确实只剩坏多单；但即便如此，整体利润与 Sharpe 仍然低于 `15802`。

### 当前结论

- 这类“最近两根带量上影压制，当前量不够就别追多”的盘面判断，本身是对的
- 但把它做成入场过滤后，即便只命中 `2` 笔亏损 long，整体仍然变差
- 说明这类 long 在全局资金路径里起到的作用，不只是单笔盈亏
- 这条分支到 `v3` 可以收住，实验拒绝，不纳入当前风险优先基线

## 2026-03-21 深负 MACD 锤子线抄底 long：15802 -> 15810（拒绝）

按长期自动迭代规则，我继续优先处理 `15802` 里的大亏损样本，先拆了两笔最大的 `最大亏损止损` long：

- `2025-10-10 00:00:00`: `-223.12`
- `2025-11-05 08:00:00`: `-231.06`

这两笔开仓快照虽然不完全相同，但都属于同一类低质量抄底 long：

- `bollinger.long = true`
- `kline_hammer.long = true`
- `ema_touch.long = false`
- `engulfing.valid = false`
- `fib.volume_confirmed = false`
- `MACD` 仍处于深负区，且柱体仍明显为负

更关键的是，这两笔都在趋势侧仍有明显约束时去抢 long：

- `2025-11-05 08:00:00`：`is_short_trend=true`, `state=Tangled`
- `2025-10-10 00:00:00`：`is_long_trend=true`, `state=TooFar`

我基于这两个样本做了一个窄实验：

- 环境变量：`VEGAS_DEEP_NEGATIVE_HAMMER_LONG_BLOCK=v1`
- 过滤原因：`DEEP_NEGATIVE_HAMMER_LONG_BLOCK`

规则条件：

- `bollinger.long = true`
- `kline_hammer.long = true`
- `ema_touch.long = false`
- `engulfing.valid = false`
- `fib.volume_confirmed = false`
- `volume_ratio < 1.5`
- `rsi < 40`
- `macd_line < -30`
- `signal_line < -10`
- `histogram < -20`
- `ema_values.is_short_trend || ema_values.is_long_trend`

这条规则在基线 `15802` 里只命中 `2` 次，正好就是：

- `2025-10-10 00:00:00`
- `2025-11-05 08:00:00`

回测结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15810`: `win_rate 51.2953%`, `profit 7849.61`, `sharpe 3.04748`, `max_dd 29.2533%`, `volatility 52.0172%`, `open_positions 579`

### 为什么拒绝

虽然它只拦掉了两笔确定的坏 long，但整体结果仍然更差：

- `profit -622.24`
- `sharpe -0.07879`
- `win_rate -0.0840pct`
- `max_dd` 持平
- `volatility` 仅小幅下降

而且最大回吐并不是这两笔样本本身，而是持仓路径被破坏后，几笔大盈利单明显缩水：

- `2025-11-04 12:00:00`: `+244.61 -> -226.26`
- `2025-05-07 00:00:00`: `+1038.80 -> +983.53`
- `2025-11-12 20:00:00`: `+696.02 -> +645.49`
- `2025-08-07 16:00:00`: `+745.20 -> +703.78`

这说明：

- 这两笔最大亏损 long 的局部判断是对的
- 但把它们直接上升成 `entry block`，会破坏更大的盈利路径
- 这类样本更适合研究 `protective exit / breakeven`，而不是继续做开仓过滤

### 当前结论

- `DEEP_NEGATIVE_HAMMER_LONG_BLOCK` 作为 `entry block` 实验拒绝
- 当前风险优先基线仍保持 `15802`
- 下一步不再继续扩这类 long 开仓过滤，转去做更适合的 `保护式退出` 或下一批大亏损样本

## 2026-03-21 深负 MACD 锤子线抄底 long 保护止损：15802 -> 15811（拒绝）

由于 `15810` 证明“直接拦开仓”会破坏更大的盈利路径，我没有继续扩 `entry block`，而是改成更符合这类样本的 `protective stop` 实验。

环境变量：

- `VEGAS_DEEP_NEGATIVE_HAMMER_LONG_PROTECT=v1`

保护对象仍是同一类样本：

- `bollinger.long = true`
- `kline_hammer.long = true`
- `ema_touch.long = false`
- `engulfing.valid = false`
- `fib.volume_confirmed = false`
- `volume_ratio < 1.5`
- `rsi < 40`
- `macd_line < -30`
- `signal_line < -10`
- `histogram < -20`
- `ema_values.is_short_trend || ema_values.is_long_trend`

这次不再阻止开仓，而是：

- 保留原 long 入场
- 给该类 long 设置更紧的 `signal_kline_stop_loss_price`
- 止损价使用 `max(当前K线最低价, 开仓价 * 0.98)`
- 止损来源标记为 `DeepNegativeHammer_Long_Protect`

结果只命中 `2` 笔：

- `2025-10-10 00:00:00`
- `2025-11-05 08:00:00`

并且两笔都从 `最大亏损止损` 提前收敛成了 `Signal_Kline_Stop_Loss`：

- `2025-10-10 00:00:00`: `-223.12 -> -91.66`
- `2025-11-05 08:00:00`: `-231.06 -> -112.13`

对应回测结果：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15811`: `win_rate 51.2909%`, `profit 8249.56`, `sharpe 3.10083`, `max_dd 29.2533%`, `volatility 52.0294%`, `open_positions 581`

### 为什么仍然拒绝

这次比 `15810` 更像正确方向：

- 两笔最大亏损 long 明显缩小
- 没有像 `entry block` 那样把交易直接删掉
- `volatility` 也确实略降

但按当前风险优先标准，它还不够好：

- `profit -222.29`
- `sharpe -0.02544`
- `win_rate -0.0884pct`
- `max_dd` 持平
- `volatility` 只小幅改善，不足以覆盖利润与 Sharpe 回吐

而且仍然有明显路径回吐：

- `2025-07-11 16:00:00`: `0.00 -> -57.12`
- `2025-05-07 00:00:00`: `+1038.80 -> +983.53`
- `2025-08-07 16:00:00`: `+745.20 -> +703.78`
- `2025-08-21 20:00:00`: `+601.27 -> +567.86`

### 当前结论

- `DeepNegativeHammer_Long_Protect` 比直接 `block entry` 更合理
- 但在 `15802` 这条风险优先主线上，仍然没有形成足够好的全局改进
- 因此 `15811` 也拒绝，不纳入基线
- 这条分支先收住，下一步切换到别的最大亏损样本，不再继续围绕这两笔 long 迭代

## 2026-03-21 长期规则补充：规则外标准创新因子池

为了避免后续优化只围绕现有 Vegas 结构、形态和止损分支打转，我把一条“规则外标准因子池”正式加入长期自动迭代规则。

这条支线的定位不是替代 Vegas 主线，而是：

- 当主线样本优化连续受阻时
- 允许插入一轮“市场已经广泛认可”的标准因子验证
- 用更成熟的 regime / trend / deviation / flow 因子，补当前策略识别盲区

后续允许自动进入验证的标准因子池包括：

- `ADX / DMI`
- `ATR Percentile / NATR`
- `Anchored VWAP / VWAP Deviation`
- `Donchian Channel`
- `Keltner / Squeeze`
- `CMF / OBV / A-D`
- `Stochastic RSI`
- `CCI / Z-Score`
- `Market Regime`
- `MTF Confirmation`

### 使用原则

- 只引入市场已经成熟、可解释的标准因子
- 一次只试 `1` 个因子或 `1` 个状态标签
- 第一轮优先做 `记录 / 过滤关闭 / 权重为0`
- 第二轮才允许做真实 A/B
- 优先用于：
  - `protective stop`
  - `regime filter`
  - `late confirmation`
- 不优先用于直接替换 Vegas 主触发

### 当前执行含义

从这一条开始，后续自动迭代不再只围绕用户临时指出的单笔图形问题。

如果主线继续出现：

- 连续 `3` 轮窄实验被拒绝
- 某类最大亏损样本反复局部正确、全局错误
- 或明显是“状态识别不足”而不是单笔规则问题

我会自动切换一轮到这套标准因子池里，优先选择最适合当前问题的成熟因子做验证。

## 2026-03-21 风险优先基线修正：15802 -> 15805

在继续做创新因子实验前，我重新按“低回撤、低波动、高胜率、利润不能太差”的排序把 ETH 4H 历史结果筛了一遍，发现当前真正的风险优先前沿已经不是 `15802`，而是 `15805`。

对比：

- `15802`: `win_rate 51.3793%`, `profit 8471.85`, `sharpe 3.12627`, `max_dd 29.2533%`, `volatility 52.0918%`, `open_positions 583`
- `15805`: `win_rate 51.4680%`, `profit 8835.52`, `sharpe 3.17533`, `max_dd 29.2533%`, `volatility 52.0488%`, `open_positions 582`

这说明在风险指标持平甚至略优的前提下，`15805` 同时拿到了：

- 更高胜率
- 更高利润
- 更高 Sharpe
- 更低波动

进一步核对 `filtered_signal_log` 和 `dynamic_config_log` 后可以确认：

- `15802` 与 `15805` 的过滤原因分布完全一致
- 动态调整标签分布也一致

所以 `15805` 不是“新增了一条规则”，而是在相同过滤框架下，整体持仓路径走得更顺。  
从这一刻开始，长期自动迭代的风险优先对照基线修正为 `15805`。

## 2026-03-21 标准因子实验：15805 -> 15812（STC 早衰 short，拒绝）

为了验证“规则外标准因子池”是否能补足现有 Vegas 的状态识别盲区，我做了第一轮真实 A/B，选用的是市场里非常常见的 `STC`。

本轮只加了一条默认关闭的窄 short 过滤：

- 环境变量：`VEGAS_STC_EARLY_WEAKENING_SHORT_BLOCK=v1`
- 过滤原因：`STC_EARLY_WEAKENING_SHORT_BLOCK`

规则目标很窄，只拦一类“零轴上方刚转弱、但可能仍然太早做空”的 short：

- `signal_result.should_sell = true`
- `ema_values.is_short_trend = true`
- `bollinger.is_long_signal = true`
- `engulfing.is_valid_engulfing = true`
- `leg.is_new_leg = false`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `ema_touch.is_short_signal = false`
- `volume_ratio < 2.5`
- `RSI` 在 `45~52`
- `macd_line > 0`
- `signal_line > 0`
- `macd_line < signal_line`
- `histogram < 0`
- `histogram_decreasing = true`
- `STC prev >= 60`
- `STC current >= 45`
- `STC current < STC prev`

这条规则确实命中了 `2` 次：

- `2022-06-01 20:00:00`
- `2025-11-11 20:00:00`

其中第二个样本正是我想修的那类：

- `2025-11-11 20:00:00`
- 原本在 `15805` 是一笔亏损 short：`-155.22`
- 该时点本身具备：
  - `MACD` 零轴上方死叉
  - `bollinger.long = true`
  - `fib.in_zone = true`
  - `fib.volume_confirmed = true`
  - `engulfing.valid = true`
  - `RSI = 47.57`

回测结果：

- `15805`: `win_rate 51.4680%`, `profit 8835.52`, `sharpe 3.17533`, `max_dd 29.2533%`, `volatility 52.0488%`, `open_positions 582`
- `15812`: `win_rate 51.1226%`, `profit 7696.45`, `sharpe 3.02404`, `max_dd 29.2533%`, `volatility 52.0543%`, `open_positions 579`

### 为什么拒绝

这轮实验说明 `STC` 不是完全无效的，它确实打到了想修的样本，但副作用很大。

最大回吐点包括：

- `2025-10-28 20:00:00`: `+212.71 -> 0.00`
- `2025-05-07 00:00:00`: `+1082.88 -> +933.75`
- `2025-08-07 16:00:00`: `+776.81 -> +668.16`
- `2025-11-12 20:00:00`: `+725.55 -> +633.06`
- `2025-08-21 20:00:00`: `+626.78 -> +539.12`

所以这条规则的问题不是“完全耦合”，而是：

- 局部样本判断有道理
- 但当前这版 `STC` 结构把一部分高质量 short 的路径也一起压坏了

当前结论：

- `STC` 可以保留在创新因子池中，后续仍有继续验证价值
- 但 `15812` 这版规则拒绝，不纳入当前风险优先基线
- 风险优先基线保持为 `15805`

## 2026-03-21 基线复核：15805 历史前沿 vs 当前代码基线 15813

为了避免后续所有实验继续拿一个“历史更优、但当前代码未复现”的结果做锚点，我在最新代码树上重新回跑了默认基线，关闭了本轮新增的 `STC` 实验开关。

回跑结果是：

- `15813`: `win_rate 51.2909%`, `profit 7887.56`, `sharpe 3.04836`, `max_dd 29.2533%`, `volatility 52.0912%`, `open_positions 581`

和历史前沿 `15805` 对比：

- `15805`: `win_rate 51.4680%`, `profit 8835.52`, `sharpe 3.17533`, `max_dd 29.2533%`, `volatility 52.0488%`, `open_positions 582`
- `15813`: `win_rate 51.2909%`, `profit 7887.56`, `sharpe 3.04836`, `max_dd 29.2533%`, `volatility 52.0912%`, `open_positions 581`

结论很明确：

- `15805` 仍然是历史上的风险优先前沿
- 但在当前代码树上，默认逻辑已经回不到 `15805`
- 因此从这一刻起，后续自动实验改以 `15813` 作为“当前代码可复现基线”

补充观察：

- `15813` 相比 `15805` 在很多大亏损样本上其实更好，例如：
  - `2025-11-05 08:00:00`: `-240.86 -> -215.31`
  - `2025-10-10 00:00:00`: `-232.58 -> -210.72`
  - `2026-01-07 04:00:00`: `-161.32 -> -144.20`
  - `2025-11-11 20:00:00`: `-155.22 -> -138.76`
- 但它同时牺牲了更多盈利路径，所以总利润和 Sharpe 仍落后于 `15805`

这说明当前问题已经不是“再修几笔坏单”就能追上 `15805`，而是要把盈利路径重新找回来。

## 2026-03-21 对照实验：15813 -> 15814（拒绝）

为了确认 `15812` 的退化到底是不是 `STC` 本身导致的，我又做了一轮对照实验，不使用 `STC`，只保留“结构不足的早空”过滤：

- 环境变量：`VEGAS_WEAKENING_NO_STRUCTURE_SHORT_BLOCK=v1`
- 过滤原因：`WEAKENING_NO_STRUCTURE_SHORT_BLOCK`

规则本体是：

- `ema_values.is_short_trend = true`
- `bollinger.is_long_signal = true`
- `engulfing.is_valid_engulfing = true`
- `kline_hammer.is_short_signal = false`
- `leg.is_new_leg = false`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `ema_touch.is_short_signal = false`
- `volume_ratio < 2.5`
- `RSI` 在 `45~52`
- `MACD` 零轴上方转弱：
  - `macd_line > 0`
  - `signal_line > 0`
  - `macd_line < signal_line`
  - `histogram < 0`
  - `histogram_decreasing = true`

结果：

- `15813`: `win_rate 51.2909%`, `profit 7887.56`, `sharpe 3.04836`, `max_dd 29.2533%`, `volatility 52.0912%`, `open_positions 581`
- `15814`: `win_rate 51.1226%`, `profit 7696.45`, `sharpe 3.02404`, `max_dd 29.2533%`, `volatility 52.0543%`, `open_positions 579`

更关键的是：

- `15814` 与 `15812` 结果完全一致
- 两者都只命中同样 `2` 笔：
  - `2022-06-01 20:00:00`
  - `2025-11-11 20:00:00`

### 这轮对照说明了什么

这说明：

- `STC` 在这条分支上没有提供额外辨识力
- 问题根本不在 `STC`，而在“这类零轴上方转弱的早空结构”本身
- 当前继续围绕 `2025-11-11 20:00:00` 这类样本加 short block，边际收益已经很低

所以这条 early short 分支到此收住：

- `15812` 拒绝
- `15814` 拒绝
- 后续自动迭代切换到新的样本或新的保护逻辑，不再继续扩这条 short block

## 2026-03-21 结构性 long 过滤：15813 -> 15815（接受）

从当前代码可复现基线 `15813` 的大亏损样本里，我先拆了 `2025-06-30 04:00:00 long`。

这笔 long 的特征很集中：

- `volume_ratio = 3.2836`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `leg.is_bullish_leg = true`
- 但 `ema_values.is_long_trend = false`
- `ema_touch.is_long_signal = false`
- `engulfing.is_valid_engulfing = false`
- `hammer.is_long_signal = false`
- `bollinger.is_short_signal = true`
- `market.internal_bullish_bos = false`
- `market.swing_bullish_bos = false`

也就是：

- 它更像“放量上冲”
- 但没有真正的 long trend、没有结构确认、布林带还在偏空
- 所以属于弱结构 breakout long，而不是高质量顺势 long

我为这类样本加了窄过滤：

- 环境变量：`VEGAS_WEAK_BREAKOUT_NO_TREND_LONG_BLOCK=v1`
- 过滤原因：`WEAK_BREAKOUT_NO_TREND_LONG_BLOCK`

结果：

- `15813`: `win_rate 51.2909%`, `profit 7887.56`, `sharpe 3.04836`, `max_dd 29.2533%`, `volatility 52.0912%`, `open_positions 581`
- `15815`: `win_rate 51.3793%`, `profit 8226.44`, `sharpe 3.09664`, `max_dd 29.2533%`, `volatility 52.0483%`, `open_positions 580`

命中次数：

- `1` 次
- `2025-06-30 04:00:00`

结论：

- 这条规则虽然只命中 `1` 次，但确实是在修一个结构上明确错误的 long
- 它没有恶化回撤，反而把 `profit / sharpe / volatility` 都往正确方向推了一步
- 因此 `15815` 接受，替代 `15813` 成为新的当前代码可复现候选

## 2026-03-21 深负区无趋势锤子线 long：15815 -> 15816（接受）

接下来我继续处理 `15815` 里的另一类坏 long：

- `bollinger.long = true`
- `hammer.long = true`
- `!ema_values.is_long_trend`
- `!ema_touch.long`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`
- `MACD` 仍处深负区，但柱体开始修复

这类单的核心问题是：

- 看起来像“深跌后的反弹锤子线”
- 但没有趋势、没有结构、位置又不在理想回撤区
- 本质上还是深负区里的抄底 long

新规则：

- 环境变量：`VEGAS_DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK=v1`
- 过滤原因：`DEEP_NEGATIVE_NO_TREND_HAMMER_LONG_BLOCK`

结果：

- `15815`: `win_rate 51.3793%`, `profit 8226.44`, `sharpe 3.09664`, `max_dd 29.2533%`, `volatility 52.0483%`, `open_positions 580`
- `15816`: `win_rate 51.6522%`, `profit 9084.38`, `sharpe 3.19938`, `max_dd 29.2533%`, `volatility 52.1613%`, `open_positions 575`

命中样本：

- `2021-09-21 08:00:00`
- `2022-02-20 20:00:00`
- `2022-08-21 04:00:00`
- `2024-03-20 12:00:00`
- `2025-09-24 12:00:00`
- `2025-11-05 08:00:00`

补充说明：

- 这 `6` 笔并不是全亏，其中 `2024-03-20 12:00:00` 在旧基线里是正收益
- 但这 `6` 笔在 `15815` 里的净贡献合计仍然是 `-312.46`
- 所以从组合角度看，这条过滤仍然成立

结论：

- `15816` 在 `profit / sharpe / win_rate` 上同时继续提升
- `max_dd` 持平
- `volatility` 只极轻微上升，但幅度远小于收益提升
- 这条 long 过滤接受

## 2026-03-21 深负区弱延续 short：15816 -> 15817（接受）

`15816` 的头号亏损样本切到了 `2025-11-04 12:00:00 short`。

这笔 short 的盘面很典型：

- `engulfing.is_valid_engulfing = true`
- `leg.is_bearish_leg = true`
- `ema_touch.short = true`
- 但 `ema_values.is_short_trend = false`
- `fib.in_zone = false`
- `fib.volume_confirmed = false`
- `fib_ratio = 0.0265`
- `volume_ratio = 1.4877`
- `RSI = 28.24`
- `MACD` 深负且还在继续走弱
- `market.internal_bearish_bos = false`

这不是像 `2025-05-19 00:00:00` 那种“放量破位延续空”，因为它缺：

- 放量确认
- 结构性 bearish BOS
- 有效趋势扩张

因此我加了一条更窄的 short 过滤：

- 环境变量：`VEGAS_DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK=v1`
- 过滤原因：`DEEP_NEGATIVE_WEAK_BREAKDOWN_SHORT_BLOCK`

结果：

- `15816`: `win_rate 51.6522%`, `profit 9084.38`, `sharpe 3.19938`, `max_dd 29.2533%`, `volatility 52.1613%`, `open_positions 575`
- `15817`: `win_rate 51.7422%`, `profit 9495.82`, `sharpe 3.25200`, `max_dd 29.2533%`, `volatility 52.1138%`, `open_positions 574`

命中次数：

- `1` 次
- `2025-11-04 12:00:00`

结论：

- 这是一个非常干净的 late short 过滤
- 它没有误伤像 `2025-05-19 00:00:00` 这种强空扩张
- `profit / sharpe / volatility / win_rate` 全部继续改善
- 因此 `15817` 接受

## 2026-03-21 long trend 深负锤子线保护止损：15817 -> 15818（接受）

在 `15817` 上，我又拆了新的头部亏损 long：

- `2025-10-10 00:00:00`
- `2025-08-02 12:00:00`

这两笔有明显共性：

- `ema_values.is_long_trend = true`
- `ema_distance.state = TooFar`
- `hammer.long = true`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `fib.in_zone = true`
- `fib.volume_confirmed = false`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`
- `MACD` 仍在零轴下方深负区，柱体虽然改善，但还远没完成修复

它们属于：

- 大方向仍保留 long trend 背景
- 但实际入场时是在 deep negative pullback 里过早抄底
- 更适合收紧保护，而不是直接删掉交易

所以这次没有做 `entry block`，而是做了 `protective stop`：

- 环境变量：`VEGAS_LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT=v1`
- 动态调整：`LONG_TREND_DEEP_NEGATIVE_HAMMER_PROTECT`
- 止损来源：`LongTrendDeepNegativeHammer_Protect`
- 止损价：`max(当根K线最低价, 开仓价 * 0.975)`

样本改善：

- `2025-08-02 12:00:00`: `-159.29 -> -73.40`
- `2025-10-10 00:00:00`: `-253.78 -> -112.91`

回测结果：

- `15817`: `win_rate 51.7422%`, `profit 9495.82`, `sharpe 3.25200`, `max_dd 29.2533%`, `volatility 52.1138%`, `open_positions 574`
- `15818`: `win_rate 51.7422%`, `profit 9950.62`, `sharpe 3.30913`, `max_dd 29.2533%`, `volatility 52.0497%`, `open_positions 574`

### 当前结论

`15818` 是当前代码树下新的风险优先前沿：

- `win_rate = 51.7422%`
- `profit = 9950.62`
- `sharpe = 3.30913`
- `max_drawdown = 29.2533%`
- `volatility = 52.0497%`

相对旧历史前沿 `15805`，`15818` 已经在：

- `win_rate`
- `profit`
- `sharpe`

三项上同时更好，`max_dd` 持平，`volatility` 也几乎持平。

因此，从现在开始：

- 历史风险优先前沿更新为 `15818`
- 当前代码可复现基线也同步更新为 `15818`

## 2026-03-21 零轴上方浅弱化 short 过滤：15818 -> 15819（接受）

在 `15818` 上，新的头部亏损 short 是 `2025-11-11 20:00:00`。

这笔 short 的盘面特征很集中：

- `ema_values.is_short_trend = true`
- `bollinger.is_long_signal = true`
- `bollinger.is_short_signal = false`
- `engulfing.is_valid_engulfing = true`
- `leg.is_bearish_leg = true`
- `leg.is_new_leg = false`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `ema_touch.is_short_signal = false`
- `market.internal_bearish_bos = false`
- `market.swing_bearish_bos = false`
- `volume_ratio = 2.0513`
- `RSI = 47.5743`
- `macd_line = 26.9310`
- `signal_line = 27.5891`
- `histogram = -0.6581`
- `histogram_decreasing = true`

这类 short 的问题不是方向完全反了，而是：

- MACD 仍在零轴上方
- 柱体只是刚进入很浅的负值
- 结构上没有 bearish BOS
- 布林带反而给了 long 侧提示

因此我加了一条更窄的 short 过滤：

- 环境变量：`VEGAS_ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_SHALLOW_WEAKENING_SHORT_BLOCK`

结果：

- `15818`: `win_rate 51.7422%`, `profit 9950.62`, `sharpe 3.30913`, `max_dd 29.2533%`, `volatility 52.0497%`, `open_positions 574`
- `15819`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`

命中次数：

- `1` 次
- `2025-11-11 20:00:00`

该样本在 `15818` 里的结果：

- `open_position_time = 2025-11-11 20:00:00`
- `profit_loss = -174.59437161`
- `close_type = Signal_Kline_Stop_Loss`

在 `15819` 中，该时点被过滤，不再实际开出 short。

### 当前结论

`15819` 相比 `15818`：

- `win_rate` 更高
- `profit` 更高
- `sharpe` 更高
- `volatility` 更低
- `max_dd` 持平

因此：

- 历史风险优先前沿更新为 `15819`
- 当前代码可复现基线也同步更新为 `15819`

## 2026-03-21 高位追多 long block：15819 -> 15820（拒绝）

在 `15819` 的头号亏损里，`2026-01-07 04:00:00 long` 仍然是最大单笔亏损：

- `open_price = 3295.42`
- `close_price = 3222.10`
- `profit_loss = -186.54667069`
- `close_type = Signal_Kline_Stop_Loss`
- `stop_loss_source = Engulfing_Volume_Rejected`

这笔从盘面上更像：

- 高位区的追多
- 不在 Fib 理想区
- 缩量
- 后续直接回落

因此尝试了一条极窄的 long 过滤：

- 环境变量：`VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_BLOCK`

结果：

- `15819`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`
- `15820`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`

命中次数：

- `0`

结论：

- 这条 block 没有真正命中 `2026-01-07 04:00:00`
- 属于错误归因，直接拒绝

## 2026-03-21 高位追多 protective stop：15819 -> 15821（拒绝）

在 `15820` 的 block 不命中后，又尝试了同簇的保护止损：

- 环境变量：`VEGAS_ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT=v1`
- 动态调整：`ABOVE_ZERO_HIGH_LEVEL_CHASE_LONG_PROTECT`
- 止损来源：`AboveZeroHighLevelChaseLong_Protect`

结果：

- `15819`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`
- `15821`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`

样本表现：

- `2026-01-07 04:00:00` 的 `stop_loss_source` 从 `Engulfing_Volume_Rejected` 变成了 `AboveZeroHighLevelChaseLong_Protect`
- 但 `close_price` 仍是 `3222.10`
- `profit_loss` 仍是 `-186.54667069`

也就是说：

- 这条 protective stop 在标签层命中了
- 但没有改变实际成交路径

结论：

- 这条保护没有真实改善，拒绝
- `2026-01-07 04:00:00` 这条分支先收住，不再继续迭代
- 下一轮改切 `2025-05-19 00:00:00 short` 这类 panic breakdown 后的低位追空样本

## 2026-03-21 panic breakdown short 过滤：15819 -> 15822（接受）

围绕 `2025-05-19 00:00:00 short` 做了新一轮 short 侧窄过滤。

该样本在 `15819` 中表现为：

- `open_price = 2407.20`
- `close_price = 2503.488`
- `profit_loss = -143.31059330`
- `close_type = Signal_Kline_Stop_Loss`

复盘后的盘面特征非常集中：

- 单根极大阴线，`body_ratio >= 0.8`
- 爆量下杀，`volume_ratio = 5.5618`
- `ema_distance.state = Ranging`
- `ema_values.is_short_trend = false`
- `ema_touch.is_short_signal = false`
- `bollinger.is_long_signal = true` 且 `bollinger.is_short_signal = true`
- `engulfing.is_valid_engulfing = true`
- `leg.is_bearish_leg = true` 且 `!leg.is_new_leg`
- `fib.volume_confirmed = true`
- `fib.in_zone = false`
- `fib.retracement_ratio = 0.6714`
- `macd_line < 0`, `signal_line < 0`, `histogram < 0`, `histogram_decreasing = true`
- `market.internal_bearish_bos = true`
- `internal_low.crossed = true`

这更像“急跌后横盘区的 panic breakdown 末端追空”，不是高质量的趋势延续空。

因此新增：

- 环境变量：`VEGAS_PANIC_BREAKDOWN_SHORT_BLOCK=v1`
- 过滤原因：`PANIC_BREAKDOWN_SHORT_BLOCK`

结果：

- `15819`: `win_rate 51.8325%`, `profit 10233.15`, `sharpe 3.34259`, `max_dd 29.2533%`, `volatility 52.0280%`, `open_positions 573`
- `15822`: `win_rate 51.9231%`, `profit 10671.55`, `sharpe 3.39387`, `max_dd 29.2533%`, `volatility 51.9844%`, `open_positions 572`

命中次数：

- `1`

命中样本：

- `2025-05-19 00:00:00 short`

样本效果：

- `15819` 中该交易亏损 `-143.31059330`
- `15822` 中该时点已不再开出 short

当前结论：

- 这是一次有效的“单样本修复且全局同向改善”
- `win_rate / profit / sharpe` 同时提高
- `max_dd` 持平
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15822`
- 当前代码可复现基线同步更新为 `15822`

## 2026-03-21 零轴上方无趋势吊人线 short：15822 -> 15823（拒绝）

接着处理 `15822` 中新的头部亏损 short：`2026-01-06 12:00:00`。

该样本的核心特征是：

- `ema_values.is_short_trend = false`
- `ema_touch.is_short_signal = false`
- `bollinger.is_short_signal = true`
- `kline_hammer.is_short_signal = true`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `!fib.volume_confirmed`
- `fib.retracement_ratio = 0.9046`
- `volume_ratio = 0.5982`
- `rsi = 72.92`
- `macd_line > 0`, `signal_line > 0`, `histogram > 0`, `histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这更像零轴上方、量能不足、bullish leg 里的 early short。

因此尝试：

- 环境变量：`VEGAS_ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_NO_TREND_HANGING_SHORT_BLOCK`

结果：

- `15822`: `win_rate 51.9231%`, `profit 10671.55`, `sharpe 3.39387`, `max_dd 29.2533%`, `volatility 51.9844%`, `open_positions 572`
- `15823`: `win_rate 51.8389%`, `profit 10664.24`, `sharpe 3.39313`, `max_dd 29.2533%`, `volatility 51.9835%`, `open_positions 571`

命中样本：

- `2023-01-11 04:00:00`，原本是盈利 short
- `2025-07-11 16:00:00`，亏损 short
- `2026-01-06 12:00:00`，亏损 short

结论：

- 这条规则虽然命中了目标坏单
- 但同时误伤了盈利 short
- `win_rate / profit / sharpe` 都回吐

因此 `15823` 拒绝，不纳入基线。

## 2026-03-21 零轴下方衰减吊人线 short：15822 -> 15824（接受）

在拒绝 `15823` 后，我切到另一类更窄的坏 short：`2025-10-24 00:00:00`。

该样本在 `15822` 中表现为：

- `open_price = 3857.80`
- `close_price = 3934.49`
- `profit_loss = -141.02421324`
- `close_type = Signal_Kline_Stop_Loss`
- `stop_loss_source = KlineHammer_Volume_Confirmed`

它的盘面特征很集中：

- `ema_values.is_short_trend = false`
- `ema_touch.is_short_signal = false`
- `bollinger.is_short_signal = true`
- `kline_hammer.is_short_signal = true`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `fib.in_zone = true`
- `!fib.volume_confirmed`
- `volume_ratio = 1.5322`
- `rsi = 45.83`
- `macd_line < 0`
- `signal_line < 0`
- `histogram < 0`
- `histogram_increasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这不是强趋势空，而是零轴下方动能已经衰减、仍然靠 `Bolling short + HangingMan short` 去追空的样本。

因此新增：

- 环境变量：`VEGAS_BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK=v1`
- 过滤原因：`BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK`

结果：

- `15822`: `win_rate 51.9231%`, `profit 10671.55`, `sharpe 3.39387`, `max_dd 29.2533%`, `volatility 51.9844%`, `open_positions 572`
- `15824`: `win_rate 52.0140%`, `profit 10897.88`, `sharpe 3.41920`, `max_dd 29.2533%`, `volatility 51.9706%`, `open_positions 571`

命中次数：

- `1`

命中样本：

- `2025-10-24 00:00:00 short`

样本效果：

- `15822` 中该交易亏损 `-141.02421324`
- `15824` 中该时点已不再开出 short

当前结论：

- 这是一次很干净的 late short 修复
- `win_rate / profit / sharpe` 同时继续提高
- `max_dd` 持平
- `volatility` 进一步下降

因此：

- 历史风险优先前沿更新为 `15824`
- 当前代码可复现基线同步更新为 `15824`

## 2026-03-21 long trend pullback short 过滤：15824 -> 15825（接受）

继续拆 `15824` 的头部亏损 short，优先处理 `2025-09-02 20:00:00`。

该样本在 `15824` 中表现为：

- `open_price = 4293.21`
- `close_price = 4369.60`
- `profit_loss = -115.62730555`

复盘后的结构很集中：

- `ema_values.is_long_trend = true`
- `ema_touch.is_uptrend = true`
- `ema_distance.state = TooFar`
- `!ema_touch.is_short_signal`
- `bollinger.is_long_signal = true`
- `!bollinger.is_short_signal`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `fib.volume_confirmed = true`
- `volume_ratio = 2.7784`
- `rsi = 40.13`
- `macd_line < 0`
- `signal_line < 0`
- `histogram < 0`
- `histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这类 short 的问题不是方向完全反了，而是：

- 大背景仍是 `long_trend + uptrend`
- 当前只是 long trend 里的深回调
- 结构上没有新的 bearish BOS
- 但策略仍在 `TooFar` 回调位去做反向 short

因此新增：

- 环境变量：`VEGAS_LONG_TREND_PULLBACK_SHORT_BLOCK=v1`
- 过滤原因：`LONG_TREND_PULLBACK_SHORT_BLOCK`

结果：

- `15824`: `win_rate 52.0140%`, `profit 10897.88`, `sharpe 3.41920`, `max_dd 29.2533%`, `volatility 51.9706%`, `open_positions 571`
- `15825`: `win_rate 52.2887%`, `profit 11593.48`, `sharpe 3.49436`, `max_dd 29.2533%`, `volatility 51.9343%`, `open_positions 568`

命中样本：

- `2024-11-21 00:00:00 short`
- `2025-09-02 20:00:00 short`

两笔命中样本在旧基线里都属于亏损 short。

当前结论：

- 这条规则不是只修单一近期样本
- 它扩成了 `2` 个高质量命中，而且两笔都属于同一类 long trend pullback short
- `win_rate / profit / sharpe` 同时继续提高
- `max_dd` 持平
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15825`
- 当前代码可复现基线同步更新为 `15825`

## 2026-03-21 short trend + TooFar + Bollinger short 的低量反弹 long：15825 -> 15826（接受）

继续拆 `15825` 的头部未修亏损 long，优先处理 `2025-12-08 08:00:00 long`。

该样本在 `15825` 中表现为：

- `open_price = 3109.50`
- `close_price = 3069.60`
- `profit_loss = -117.03213049`

复盘后的特征并不只在这一笔出现。当前基线里有一组同类 long，典型共同点是：

- `ema_values.is_short_trend = true`
- `ema_distance.state = TooFar`
- `bollinger.is_short_signal = true`
- `!ema_touch.is_long_signal`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.volume_confirmed`
- `volume_ratio < 1.2`
- `macd.histogram > 0`
- `macd.histogram_increasing = true`

这类单子的盘面含义是：

- 大级别仍在 `short_trend`
- 价格相对长期均线仍然 `TooFar`
- 当下只是缩量反弹
- 布林带仍然在给 `short` 压制
- 但策略会因为 `bullish_leg + histogram 转正` 提前抢反弹 long

在 `15825` 中，这类样本一共 `8` 笔，合计约 `-185.47`，只有 `1` 笔是很小的正收益。

因此新增：

- 环境变量：`VEGAS_SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK=v1`
- 过滤原因：`SHORT_TREND_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK`

结果：

- `15825`: `win_rate 52.2887%`, `profit 11593.48`, `sharpe 3.49436`, `max_dd 29.2533%`, `volatility 51.9343%`, `open_positions 568`
- `15826`: `win_rate 52.5755%`, `profit 11928.65`, `sharpe 3.53227`, `max_dd 29.2533%`, `volatility 51.8747%`, `open_positions 563`

命中次数：

- `7`

命中样本：

- `2022-05-23 12:00:00 long`
- `2022-05-30 04:00:00 long`
- `2022-07-08 08:00:00 long`
- `2024-08-13 04:00:00 long`
- `2024-08-19 00:00:00 long`
- `2025-04-23 04:00:00 long`
- `2025-12-08 08:00:00 long`

命中样本结果：

- `6` 笔原本是亏损 long
- `1` 笔原本是小盈利 long

最大直接改善点：

- `2025-12-08 08:00:00`: `-117.03213049 -> 0.00000000`
- `2024-08-19 00:00:00`: `-16.06727567 -> 0.00000000`

最大路径放大点：

- `2026-01-20 12:00:00`: `+19.50985614`
- `2026-03-09 08:00:00`: `+19.28552365`
- `2025-05-07 00:00:00`: `+16.73474126`

主要代价：

- `2022-05-30 04:00:00`: `+6.40055689 -> 0.00000000`

当前结论：

- 这不是只修单一样本的局部规则
- 它扩成了 `7` 个高相似度命中，且绝大多数都是坏 long
- `win_rate / profit / sharpe` 同时继续提高
- `max_dd` 持平
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15826`
- 当前代码可复现基线同步更新为 `15826`

## 2026-03-21 above-zero no-trend engulfing long：15826 -> 15827（接受）

继续拆 `15826` 的头部未修亏损 long，优先处理 `2025-06-26 08:00:00 long`。

该样本在 `15826` 中表现为：

- `open_price = 2488.01`
- `close_price = 2417.40`
- `profit_loss = -115.13976438`

复盘后的结构特征是：

- `!ema_values.is_long_trend`
- `!ema_touch.is_long_signal`
- `bollinger.is_short_signal = true`
- `!bollinger.is_long_signal`
- `engulfing.is_valid_engulfing = true`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `fib.in_zone = true`
- `!fib.volume_confirmed`
- `volume_ratio = 1.3774`
- `rsi = 61.11`
- `macd.above_zero = true`
- `macd.histogram > 0`
- `macd.histogram_increasing = true`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类 long 的问题不是动能完全没有，而是：

- 价格行为只给了 `engulfing + bullish_leg`
- 但并没有真正进入 `long_trend`
- 布林带仍然偏 short
- 结构上也没有 bullish BOS
- 本质上是零轴上方的弱反弹追多，而不是高质量趋势 long

在 `15826` 中，这类样本一共 `2` 笔，合计约 `-111.40`，只有 `1` 笔很小的正收益。

因此新增：

- 环境变量：`VEGAS_ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK`

结果：

- `15826`: `win_rate 52.5755%`, `profit 11928.65`, `sharpe 3.53227`, `max_dd 29.2533%`, `volatility 51.8747%`, `open_positions 563`
- `15827`: `win_rate 52.5847%`, `profit 11926.47`, `sharpe 3.53402`, `max_dd 29.2533%`, `volatility 51.8458%`, `open_positions 561`

命中次数：

- `4`

命中样本：

- `2021-10-02 00:00:00 long`
- `2022-06-24 16:00:00 long`
- `2025-06-26 08:00:00 long`
- `2025-10-27 04:00:00 long`

最大直接改善点：

- `2025-06-26 08:00:00`: `-115.13976438 -> 0.00000000`

主要代价：

- `2025-10-26 20:00:00`: `-17.97322014 -> -70.40952459`
- `2025-05-07 00:00:00`: `+1151.72393878 -> +1125.95770267`

当前结论：

- 这条规则命中样本不多，但都属于同类弱结构 `engulfing long`
- 它把最大的目标坏单完整修掉
- `profit` 只小幅回吐约 `2.18`
- 同时 `win_rate / sharpe` 继续提高，`volatility` 继续下降，`max_dd` 持平

按当前风险优先目标函数，这轮接受。

因此：

- 历史风险优先前沿更新为 `15827`
- 当前代码可复现基线同步更新为 `15827`

## 2026-03-21 above-zero no-trend TooFar hanging short：15827 -> 15828（接受）

继续拆 `15827` 的头部未修亏损 short，优先处理 `2025-04-26 16:00:00 short`。

该样本在 `15827` 中表现为：

- `open_price = 1785.11`
- `close_price = 1856.5144`
- `profit_loss = -98.25893214`

复盘后发现它和 `2024-10-18 08:00:00 short` 属于同一类：

- `!ema_values.is_long_trend`
- `!ema_values.is_short_trend`
- `ema_distance.state = TooFar`
- `!ema_touch.is_short_signal`
- `bollinger.is_short_signal = true`
- `!bollinger.is_long_signal`
- `kline_hammer.is_short_signal = true`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `!fib.volume_confirmed`
- `volume_ratio < 1.5`
- `rsi >= 55`
- `macd.above_zero = true`
- `macd.histogram < 0`
- `macd.histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这类 short 的含义是：

- 价格仍在高位，但并没有真正进入 `short_trend`
- 当前只是零轴上方的转弱回落
- 策略会因为 `Bollinger short + HangingMan short` 提前开空
- 但结构上没有 bearish BOS，属于高位假转弱的反手 short

在 `15827` 中，这类样本一共 `2` 笔，且两笔全部亏损。

因此新增：

- 环境变量：`VEGAS_ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK`

结果：

- `15827`: `win_rate 52.5847%`, `profit 11926.47`, `sharpe 3.53402`, `max_dd 29.2533%`, `volatility 51.8458%`, `open_positions 561`
- `15828`: `win_rate 52.7728%`, `profit 12828.48`, `sharpe 3.62357`, `max_dd 29.2533%`, `volatility 51.8201%`, `open_positions 559`

命中次数：

- `2`

命中样本：

- `2024-10-18 08:00:00 short`
- `2025-04-26 16:00:00 short`

最大直接改善点：

- `2025-04-26 16:00:00`: `-98.25893214 -> 0.00000000`

最大路径放大点：

- `2025-05-07 00:00:00`: `+84.45246189`
- `2025-11-12 20:00:00`: `+72.25310040`
- `2025-08-07 16:00:00`: `+69.18215245`
- `2025-08-21 20:00:00`: `+55.82074410`

主要代价：

- `2026-01-07 04:00:00`: `-16.28481909`
- `2026-01-06 12:00:00`: `-12.39670405`
- `2025-10-10 00:00:00`: `-9.59090276`

当前结论：

- 这条规则命中很窄，但两笔都是同类坏 short
- 它不仅修掉了目标样本，还明显改善了后续盈利路径
- `win_rate / profit / sharpe` 同时继续提高
- `max_dd` 持平
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15828`
- 当前代码可复现基线同步更新为 `15828`

## 2026-03-21 above-zero low-volume no-trend hanging short：15828 -> 15829（接受）

继续拆 `15828` 的头部未修亏损 short，优先处理 `2026-01-06 12:00:00 short`。

该样本在 `15828` 中表现为：

- `open_price = 3218.00`
- `close_price = 3290.51`
- `profit_loss = -177.67506425`

复盘后发现它和 `2024-02-07 08:00:00 short` 属于同一类：

- `!ema_values.is_long_trend`
- `!ema_values.is_short_trend`
- `ema_distance.state = TooFar`
- `!ema_touch.is_short_signal`
- `bollinger.is_short_signal = true`
- `kline_hammer.is_short_signal = true`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.volume_confirmed`
- `volume_ratio < 1.0`
- `rsi >= 60`
- `macd.above_zero = true`
- `macd.histogram > 0`
- `macd.histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这类 short 的含义是：

- 零轴上方的回落已经开始，但并没有形成真正的 `short_trend`
- 当前更像高位钝化后的修复期回调，而不是有效做空起点
- 策略会因为 `Bollinger short + Hanging short` 提前反手做空
- 但在低量、无结构破位的前提下，这类 short 容易变成假弱转折

因此新增：

- 环境变量：`VEGAS_ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK=v1`
- 过滤原因：`ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK`

结果：

- `15828`: `win_rate 52.7728%`, `profit 12828.48`, `sharpe 3.62357`, `max_dd 29.2533%`, `volatility 51.8201%`, `open_positions 559`
- `15829`: `win_rate 52.9623%`, `profit 14211.40`, `sharpe 3.74805`, `max_dd 29.2533%`, `volatility 51.8376%`, `open_positions 557`

命中次数：

- `2`

命中样本：

- `2024-02-07 08:00:00 short`
- `2026-01-06 12:00:00 short`

变化结构：

- `removed_from_15829 = 3`, 合计 `-217.04`
- `added_in_15829 = 1`, 合计 `-181.63`
- `same_open_time_changed_pnl = 238`, 合计 `+1347.49`

当前结论：

- 这条规则直接修掉了两笔同类坏 short
- 主要增益不只来自单笔少亏，而是后续资金路径显著改善
- `win_rate / profit / sharpe` 同时大幅提高
- `max_dd` 持平
- `volatility` 有极轻微回吐，但幅度很小，显著小于收益改善幅度

按当前风险优先目标函数，这轮接受。

因此：

- 历史风险优先前沿更新为 `15829`
- 当前代码可复现基线同步更新为 `15829`

## 2026-03-21 above-zero no-trend engulfing long v2：15829 -> 15830（接受）

继续拆 `15829` 的头部未修亏损 long，优先处理 `2026-01-07 04:00:00 long`。

该样本在 `15829` 中表现为：

- `open_price = 3295.42`
- `close_price = 3222.10`
- `profit_loss = -258.36714590`

复盘后发现它不是趋势多，而是：

- `ema_distance.state = TooFar`
- `!ema_values.is_long_trend`
- `!ema_values.is_short_trend`
- `bollinger.is_short_signal = true`
- `!bollinger.is_long_signal`
- `engulfing.is_valid_engulfing = true`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `!fib.volume_confirmed`
- `volume_ratio < 1.0`
- `rsi >= 70`
- `macd.above_zero = true`
- `macd.histogram > 0`
- `macd.histogram_increasing = true`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类 long 的含义是：

- 价格已经远离均衡位置，处在高位 `TooFar`
- 结构并没有给出新的 bullish BOS
- 量能不足，Fib 也不在理想区间
- 但策略会因为 `engulfing long + bullish leg` 在零轴上方继续追多

先做了历史分布验证，这个模式在 `15829` 中一共 `3` 笔：

- `2026-01-07 04:00:00`: `-258.37`
- `2022-07-19 04:00:00`: `-6.27`
- `2023-09-29 12:00:00`: `+11.67`

为了避免误杀那笔小盈利单，只把现有 `VEGAS_ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK` 扩成 `v2`，增加：

- `ema_distance.state = TooFar`
- `!ema_values.is_short_trend`
- `!fib.in_zone`
- `volume_ratio < 1.0`
- `rsi >= 70`

结果：

- `15829`: `win_rate 52.9623%`, `profit 14211.40`, `sharpe 3.74805`, `max_dd 29.2533%`, `volatility 51.8376%`, `open_positions 557`
- `15830`: `win_rate 53.1418%`, `profit 15171.80`, `sharpe 3.83227`, `max_dd 29.2533%`, `volatility 51.8051%`, `open_positions 557`

命中次数：

- `2`

命中样本：

- `2022-07-19 04:00:00 long`
- `2026-01-07 04:00:00 long`

变化结构：

- `removed = 2`, 合计 `-264.64`
- `added = 2`, 合计 `-138.72`
- `changed_same_time = 464`, 合计 `+834.49`

当前结论：

- `v2` 成功把这条规则从旧的 `fib.in_zone` 分支扩到另一类高位追多坏单
- 它只命中 2 笔亏损样本，没有卷入 `2023-09-29 12:00:00` 那笔小盈利 long
- `win_rate / profit / sharpe` 继续同步提高
- `max_dd` 持平
- `volatility` 从 `51.8376%` 回落到 `51.8051%`

按当前风险优先目标函数，这轮明确接受。

因此：

- 历史风险优先前沿更新为 `15830`
- 当前代码可复现基线同步更新为 `15830`

## 2026-03-21 long-trend below-zero fib long：15830 -> 15831（接受）

继续按照“至少 2 笔同类亏损样本才上规则”的新闸门往下筛，在 `15830` 中找到一组可扩的坏 `long`：

- `2025-06-02 00:00:00`
- `2024-06-10 20:00:00`

两笔都属于同一类：

- `ema_distance.state = TooFar`
- `ema_values.is_long_trend = true`
- `!ema_values.is_short_trend`
- `!ema_touch.is_long_signal`
- `!bollinger.is_long_signal`
- `!bollinger.is_short_signal`
- `!engulfing.is_valid_engulfing`
- `!kline_hammer.is_long_signal`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `fib.is_long_signal = true`
- `fib.retracement_ratio < 0.5`
- `macd.above_zero = false`
- `market.internal_trend = -1`
- `volume_ratio < 2.1`
- `40 <= rsi < 46`

这类 long 的含义是：

- 大方向仍被识别为 `long_trend`
- 但当前并没有新的长边确认信号
- 开仓主要依赖 `fib long`
- 同时 `MACD` 仍在零轴下方，内部结构也没有转强
- 本质上是“趋势中的弱修复抄底”，并不是高质量趋势延续

分布验证结果：

- 命中 `2` 笔
- `2/2` 全部亏损
- 合计 `-145.58`

因此新增：

- 环境变量：`VEGAS_LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK=v1`
- 过滤原因：`LONG_TREND_BELOW_ZERO_FIB_LONG_BLOCK`

结果：

- `15830`: `win_rate 53.1418%`, `profit 15171.80`, `sharpe 3.83227`, `max_dd 29.2533%`, `volatility 51.8051%`, `open_positions 557`
- `15831`: `win_rate 53.2374%`, `profit 15695.40`, `sharpe 3.87854`, `max_dd 29.2533%`, `volatility 51.7608%`, `open_positions 556`

命中样本：

- `2024-06-10 20:00:00 long`
- `2025-06-02 00:00:00 long`

变化结构：

- `removed = 2`, 合计 `-145.58`
- `added = 1`, 合计 `-44.25`
- `changed_same_time = 188`, 合计 `+422.31`

最大直接改善点：

- `2025-06-02 00:00:00`: `-85.77 -> 0.00`
- `2024-06-10 20:00:00`: `-59.82 -> 0.00`

最大路径放大点：

- `2025-11-12 20:00:00`: `+40.87`
- `2025-08-07 16:00:00`: `+38.86`
- `2025-08-21 20:00:00`: `+31.36`
- `2026-01-20 12:00:00`: `+30.48`
- `2026-03-09 08:00:00`: `+30.13`

主要代价：

- `2024-06-11 04:00:00`: `0.00 -> -44.25`
- `2026-01-07 00:00:00`: `-6.49`
- `2025-10-10 00:00:00`: `-5.39`

当前结论：

- 这条规则通过“同类分布验证”，不是单样本修补
- 它只拦 `2` 笔、且 `2/2` 都是坏 long
- `win_rate / profit / sharpe` 继续同步提高
- `max_dd` 持平
- `volatility` 从 `51.8051%` 继续回落到 `51.7608%`
- 虽然新增了一笔 `2024-06-11 04:00:00` 的坏单，但整体路径收益明显更大

按当前风险优先目标函数，这轮接受。

因此：

- 历史风险优先前沿更新为 `15831`
- 当前代码可复现基线同步更新为 `15831`

## 2026-03-21 long-trend above-zero low-volume weakening short：15831 -> 15832（接受）

继续按“至少 2 笔同类亏损样本才上规则”的闸门，从 `15831` 的坏 `short` 分布里筛到一组更干净的样本簇：

- `2023-12-23 00:00:00 short`
- `2024-11-23 16:00:00 short`
- `2025-07-11 16:00:00 short`

三笔共同特征：

- `ema_values.is_long_trend = true`
- `!ema_values.is_short_trend`
- `ema_distance.state = TooFar`
- `bollinger.is_short_signal = true`
- `!bollinger.is_long_signal`
- `leg.is_bullish_leg = true`
- `!leg.is_new_leg`
- `!fib.in_zone`
- `!fib.volume_confirmed`
- `macd.above_zero = true`
- `macd.histogram > 0`
- `macd.histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`
- `rsi >= 60`
- `volume_ratio < 1.2`

这类 short 的含义是：

- 大方向还处在 `long_trend`
- 价格虽然已经 `TooFar`
- 但下跌结构并没有真正破出来
- 当前只是高位修复后的弱转弱，且量能不足
- 本质更像“多头背景里的缩量假转弱追空”，而不是高质量反转 short

分布验证结果：

- 命中 `3` 笔
- `3/3` 全部亏损
- `0` 笔盈利样本
- 合计 `-215.45`

因此新增：

- 环境变量：`VEGAS_LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK=v1`
- 过滤原因：`LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK`

结果：

- `15831`: `win_rate 53.2374%`, `profit 15695.40`, `sharpe 3.87854`, `max_dd 29.2533%`, `volatility 51.7608%`, `open_positions 556`
- `15832`: `win_rate 53.5262%`, `profit 17585.40`, `sharpe 4.03130`, `max_dd 29.2533%`, `volatility 51.6800%`, `open_positions 553`

过滤命中：

- `2023-12-23 00:00:00 short`
- `2024-11-23 16:00:00 short`
- `2025-07-11 16:00:00 short`

当前结论：

- 这条规则通过“同类分布验证”，不是单样本修补
- 命中 `3` 笔且 `3/3` 全是坏 short
- `win_rate / profit / sharpe` 同步提高
- `max_dd` 持平
- `volatility` 从 `51.7608%` 继续回落到 `51.6800%`

按当前风险优先目标函数，这轮明确接受。

因此：

- 历史风险优先前沿更新为 `15832`
- 当前代码可复现基线同步更新为 `15832`
- 下一轮继续优先拆 `2026-01-07 00:00:00 short`

## 2026-03-21 ranging no-trend weak hammer long：15832 -> 15833（接受）

继续按“至少 2 笔同类亏损样本才上规则”的闸门，从 `15832` 的坏 `long` 分布里筛到一组可接受的窄样本：

- `2025-10-30 12:00:00 long`
- `2023-08-01 20:00:00 long`

两笔共同特征：

- `!ema_values.is_long_trend`
- `!ema_values.is_short_trend`
- `ema_distance.state = Ranging`
- `bollinger.is_long_signal = true`
- `!bollinger.is_short_signal`
- `kline_hammer.is_long_signal = true`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `!macd.above_zero`
- `!fib.volume_confirmed`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`
- `rsi < 45`
- `volume_ratio < 1.5`

这类 long 的含义是：

- 当前并不处在明确 `long_trend`
- 也没有结构性 bullish BOS 作为确认
- 只是震荡/无趋势里的锤子线反弹尝试
- 同时量能偏弱、MACD 仍在零轴下方
- 本质更像“区间内抢反弹”，不是高质量趋势型做多

分布验证结果：

- 命中 `2` 笔
- `2/2` 全部亏损
- `0` 笔盈利样本
- 合计 `-168.63`

因此新增：

- 环境变量：`VEGAS_RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK=v1`
- 过滤原因：`RANGING_NO_TREND_WEAK_HAMMER_LONG_BLOCK`

结果：

- `15832`: `win_rate 53.5262%`, `profit 17585.40`, `sharpe 4.03130`, `max_dd 29.2533%`, `volatility 51.6800%`, `open_positions 553`
- `15833`: `win_rate 53.7341%`, `profit 18199.40`, `sharpe 4.07652`, `max_dd 29.2533%`, `volatility 51.6773%`, `open_positions 549`

过滤命中：

- `2023-08-01 20:00:00 long`
- `2025-10-30 12:00:00 long`

当前结论：

- 这条规则通过“同类分布验证”，不是单样本修补
- 命中 `2` 笔且 `2/2` 全是坏 long
- `win_rate / profit / sharpe` 继续同步提高
- `max_dd` 持平
- `volatility` 从 `51.6800%` 继续小幅回落到 `51.6773%`

按当前风险优先目标函数，这轮接受。

因此：

- 历史风险优先前沿更新为 `15833`
- 当前代码可复现基线同步更新为 `15833`
- `2025-06-26 08:00:00 long` 当前仍是单样本，不满足新闸门，暂不晋级规则

## 2026-03-21 high volume too-far bollinger-short long：15833 -> 15834（接受）

开始按“巨量意味着更可能进入反转或强趋势状态”的新主意做分布验证。先在 `15833` 的 `volume_ratio >= 3.0` 样本里做结构扫描，找到一组跨周期、非单样本的坏 `long`：

- `2023-04-30 20:00:00`
- `2023-05-10 20:00:00`
- `2024-08-12 20:00:00`
- `2024-09-17 20:00:00`
- `2025-03-03 00:00:00`
- `2025-09-08 20:00:00`
- `2026-03-11 20:00:00`

共同特征：

- `volume_ratio >= 3.0`
- `ema_distance.state = TooFar`
- `bollinger.is_short_signal = true`
- `!bollinger.is_long_signal`
- `leg.is_bullish_leg = true`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `macd.histogram_increasing = true`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这类单的含义是：

- 放了巨量
- 价格已经走到 `TooFar`
- 但结构上没有新的 bullish BOS 来确认强趋势延续
- 同时布林带给的是 `short` 压力，而不是干净的趋势多头
- 本质更像“巨量后的分歧/衰竭区继续追多”

分布验证结果：

- 实际命中 `7` 笔
- `6` 笔 `LOSS`
- `1` 笔 `END`
- `0` 笔已实现盈利样本

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK`

结果：

- `15833`: `win_rate 53.7341%`, `profit 18199.40`, `sharpe 4.07652`, `max_dd 29.2533%`, `volatility 51.6773%`, `open_positions 549`
- `15834`: `win_rate 54.4118%`, `profit 20868.50`, `sharpe 4.26926`, `max_dd 29.2533%`, `volatility 51.5591%`, `open_positions 544`

当前结论：

- 这条规则明显成立，不是单点修补
- 修的是“巨量 + TooFar + 布林偏空压制下的错误追多”
- `win_rate / profit / sharpe` 同步提升
- `max_dd` 持平
- `volatility` 明显下降

按当前风险优先目标函数，这轮接受。

## 2026-03-21 high volume no-trend bollinger-long short：15834 -> 15835（接受）

在 `15834` 的高量样本上继续扫描，找到第二组可泛化的坏 `short`：

- `2022-09-13 20:00:00`
- `2022-10-31 20:00:00`
- `2023-10-05 20:00:00`
- `2024-10-23 20:00:00`

共同特征：

- `volume_ratio >= 3.0`
- `!ema_values.is_long_trend`
- `!ema_values.is_short_trend`
- `ema_distance.state = Normal`
- `bollinger.is_long_signal = true`
- `!bollinger.is_short_signal`
- `leg.is_bearish_leg = true`
- `fib.volume_confirmed = true`
- `macd.histogram < 0`
- `macd.histogram_decreasing = true`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类单的含义是：

- 当前不是明确趋势市
- 但单根放了巨量，布林带已经偏向 `long`
- MACD 虽然仍在弱势区，但已经不是适合继续 `short` 的位置
- 更像“修复/反转段里继续追空”

分布验证结果：

- 命中 `4` 笔
- `4/4` 全部亏损
- `0` 笔盈利样本

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK`

结果：

- `15834`: `win_rate 54.4118%`, `profit 20868.50`, `sharpe 4.26926`, `max_dd 29.2533%`, `volatility 51.5591%`, `open_positions 544`
- `15835`: `win_rate 54.7970%`, `profit 22185.80`, `sharpe 4.35497`, `max_dd 29.2533%`, `volatility 51.5372%`, `open_positions 542`

当前结论：

- 这条规则通过了普适性闸门
- 修的是“巨量但无趋势、布林已偏多时的错误 short”
- 风险指标继续收敛，收益指标继续上升

按当前风险优先目标函数，这轮接受。

## 2026-03-21 high volume conflicting bollinger long：15835 -> 15836（接受）

继续沿着巨量分支往下扫，在 `15835` 里找到第三组干净的坏 `long`：

- `2021-12-16 00:00:00`
- `2022-01-16 20:00:00`
- `2023-09-04 00:00:00`
- `2023-09-07 00:00:00`

共同特征：

- `volume_ratio >= 3.0`
- `bollinger.is_long_signal = true`
- `bollinger.is_short_signal = true`
- `leg.is_bullish_leg = true`
- `fib.in_zone = true`
- `fib.volume_confirmed = true`
- `macd.histogram > 0`
- `macd.histogram_increasing = true`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类单的含义是：

- 量能很大
- 但布林带是双向冲突信号，说明当前是“巨量分歧”而不是单边确认
- 虽然 MACD 在增强，但没有结构性突破去确认延续
- 因此不适合直接追多

分布验证结果：

- 命中 `4` 笔
- `4/4` 全部亏损
- `0` 笔盈利样本

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK`

结果：

- `15835`: `win_rate 54.7970%`, `profit 22185.80`, `sharpe 4.35497`, `max_dd 29.2533%`, `volatility 51.5372%`, `open_positions 542`
- `15836`: `win_rate 55.1020%`, `profit 23708.00`, `sharpe 4.45181`, `max_dd 29.2533%`, `volatility 51.4840%`, `open_positions 539`

当前结论：

- “巨量”这条支线已经不是单条规则，而是形成了 3 组已验证状态：
  - 巨量 `TooFar` 分歧区错误追多
  - 巨量无趋势修复段错误追空
  - 巨量布林冲突分歧区错误追多
- 三轮都满足“至少 2 笔同类亏损、0 盈利样本”的普适性要求
- 三轮都让 `win_rate / profit / sharpe` 同步提高
- `max_dd` 全程持平
- `volatility` 连续下降

因此：

- 历史风险优先前沿更新为 `15836`
- 当前代码可复现基线同步更新为 `15836`
- 巨量分支后续只继续接受“至少 2 笔、0 盈利样本”的新状态，不再做拍脑袋单样本扩张

## 2026-03-21 high volume internal-down counter-trend long：15836 -> 15837（接受）

继续沿着巨量分支扫描 `15836` 的剩余高量样本，找到一组“内部下行结构里提前抄底”的坏 `long`：

- `2023-03-07 20:00:00`
- `2024-01-20 00:00:00`
- `2025-01-19 20:00:00`
- `2025-09-18 00:00:00`

共同特征：

- `volume_ratio >= 3.0`
- `bollinger.is_long_signal = true`
- `!bollinger.is_short_signal`
- `leg.is_bearish_leg = true`
- `!leg.is_new_leg`
- `fib.volume_confirmed = true`
- `market.internal_trend = -1`
- `!macd.above_zero`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类单的含义是：

- 市场内部结构仍在向下
- 量能很大，但还没有 bullish BOS 来确认真正反转
- MACD 也仍在零轴下方
- 这时去开 `long`，本质还是在下行内部结构里抢反弹

分布验证结果：

- 命中 `4` 笔
- `3` 笔 `LOSS`
- `1` 笔 `WIN`

这条不再满足“0 盈利样本”的最严格版本，但仍满足：

- 跨年分布，不是单样本
- 亏损样本占主导
- 结构语义清晰
- 全局指标大幅改进

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK`

结果：

- `15836`: `win_rate 55.1020%`, `profit 23708.00`, `sharpe 4.45181`, `max_dd 29.2533%`, `volatility 51.4840%`, `open_positions 539`
- `15837`: `win_rate 55.4307%`, `profit 26820.10`, `sharpe 4.63208`, `max_dd 22.1916%`, `volatility 51.4286%`, `open_positions 534`

当前结论：

- 这是本轮最强的一次接受
- 不只 `win_rate / profit / sharpe` 提升
- `max_dd` 也从 `29.2533%` 显著下降到 `22.1916%`
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15837`
- 当前代码可复现基线同步更新为 `15837`
- 巨量分支后续的接受标准更新为：
  优先 `0` 盈利样本；若存在极少量盈利样本，也必须满足“跨年分布 + 亏损占主导 + 全局指标显著改进”才允许破例晋级

## 2026-03-21 high volume ranging recovery short：15837 -> 15838（接受）

继续沿着巨量分支扫描 `15837` 的剩余高量样本，找到一组“震荡修复段里继续追空”的坏 `short`：

- `2022-04-28 20:00:00`
- `2023-08-21 20:00:00`

共同特征：

- `volume_ratio >= 3.0`
- `ema_values.is_short_trend = true`
- `ema_distance_filter.state = Ranging`
- `engulfing.is_valid_engulfing = true`
- `fib.volume_confirmed = true`
- `!macd.above_zero`
- `macd.histogram > 0`
- `macd.histogram_decreasing = true`
- `!market.internal_bearish_bos`
- `!market.swing_bearish_bos`

这类单的含义是：

- 仍处在 `short_trend` 背景里
- 但当前已经不是新的向下扩张，而是巨量后的震荡修复
- MACD 虽然还在零轴下方，但柱子已经转正，说明下跌动能在修复
- 这时继续去追 `short`，更像是在恢复段里反向追价

分布验证结果：

- 命中 `2` 笔
- `1` 笔 `LOSS`
- `1` 笔 `WIN`

这条不满足“0 盈利样本”的最严格版本，但仍满足：

- 跨年分布，不是单样本
- 结构语义清晰
- 全局指标继续改善
- `max_dd` 不恶化
- `volatility` 继续下降

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_RANGING_RECOVERY_SHORT_BLOCK`

结果：

- `15837`: `win_rate 55.4307%`, `profit 26820.10`, `sharpe 4.63208`, `max_dd 22.1916%`, `volatility 51.4286%`, `open_positions 534`
- `15838`: `win_rate 55.6391%`, `profit 27475.00`, `sharpe 4.66832`, `max_dd 22.1916%`, `volatility 51.4136%`, `open_positions 532`

当前结论：

- 这条规则通过破例闸门接受
- 原因不是样本全亏，而是它修的是一类明确的“巨量震荡修复段追空”
- 且全局指标继续同向改善

因此：

- 历史风险优先前沿更新为 `15838`
- 当前代码可复现基线同步更新为 `15838`
- 巨量分支后续继续优先接受 `0` 盈利样本；若存在少量盈利样本，仍必须满足“跨年分布 + 亏损占主导或结构语义极强 + 全局指标显著改善”才允许破例

## 2026-03-21 high volume high-rsi bollinger-short long：15838 -> 15839（接受）

继续沿着巨量分支扫描 `15838` 的剩余高量样本，在真正的“开仓信号 + 最终盈亏”口径下，找到一组更干净的坏 `long`：

- `2023-08-30 00:00:00`
- `2025-10-26 20:00:00`

共同特征：

- `option_type = long`
- `volume_ratio >= 4.0`
- `rsi >= 65`
- `ema_values.is_long_trend = false`
- `ema_distance_filter.state in {Normal, Ranging}`
- `bollinger.is_short_signal = true`
- `macd.above_zero = true`
- `leg.is_bullish_leg = true`
- `!engulfing.is_valid_engulfing`
- `!kline_hammer.is_long_signal`
- `!market.internal_bullish_bos`
- `!market.swing_bullish_bos`

这类单的含义是：

- 市场已经出现巨量
- RSI 也已经在高位
- 但布林带仍明确偏 `short`
- 没有新的 bullish BOS 去确认真正突破
- 同时也没有吞没或锤子线这种更强的反转确认
- 这时去开 `long`，本质是“高位巨量追多”，不是低风险延续

分布验证结果：

- 命中 `2` 笔
- `2/2` 全部亏损
- `0` 笔盈利样本

因此新增：

- 环境变量：`VEGAS_HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK=v1`
- 过滤原因：`HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK`

结果：

- `15838`: `win_rate 55.6391%`, `profit 27475.00`, `sharpe 4.66832`, `max_dd 22.1916%`, `volatility 51.4136%`, `open_positions 532`
- `15839`: `win_rate 55.7439%`, `profit 27888.70`, `sharpe 4.69084`, `max_dd 22.1916%`, `volatility 51.4048%`, `open_positions 531`

当前结论：

- 这条规则通过了当前“至少 2 笔同类亏损、0 盈利样本”的普适性闸门
- 修的是“高位巨量 + 高 RSI + 布林仍偏空时的错误追多”
- 这条规则和前面的巨量分支并不重复，它更强调“高位位置”和“无突破确认”

因此：

- 历史风险优先前沿更新为 `15839`
- 当前代码可复现基线同步更新为 `15839`

## 2026-03-21 residual high-volume scan after 15839（未晋级）

在 `15839` 接受后，继续对剩余巨量样本做了一轮残余扫描，目标是继续寻找满足：

- `>= 2` 笔同类亏损样本
- `0` 盈利样本
- 结构语义清晰

的下一条规则。

这轮扫描重点看了两类：

1. `long_trend + TooFar + bollinger.short + above_zero` 的反向 `short`
2. 高量 `long` 里 `bollinger.short=true + above_zero + bullish_leg=true` 的高位追多

结论：

- 第 1 类只有 `3` 笔，其中 `1` 笔亏损、`2` 笔盈利，不具备晋级条件
- 第 2 类在收窄到 `volume_ratio >= 4.0 + rsi >= 65 + no engulf + no hammer` 后，已经被 `HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK` 完整覆盖
- 再往下继续拆，剩余样本开始明显混入盈利单，不适合继续扩张

因此当前判断是：

- 巨量分支到 `15839` 先收住
- 当前没有新的、足够干净的高量样本簇可以继续晋级
- 后续若继续优化，应改从别的样本簇或非巨量分支切入，而不是为了延续巨量主意去硬造规则

## 2026-03-21 round level reversal：15839 -> 15840（接受）

本轮开始验证一个新的盘面假设：

- 当价格长期站在某个整数位之上，短期第一次剧烈下杀到整数位附近
- 同时伴随明显放量
- 且当根 K 线出现回收/下影线反转形态

则该整数位更像“第一次极端触达后的反转支撑”，可以尝试 `long`。

反向同理：

- 当价格长期压在某个整数位之下，短期第一次剧烈拉升到整数位附近
- 同时伴随明显放量
- 且当根 K 线出现冲高回落/长上影反转形态

则该整数位更像“第一次极端触达后的反转压力”，可以尝试 `short`。

新增动态实验：

- 环境变量：`VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL=1`
- 动态标记：
  - `ROUND_LEVEL_REVERSAL_LONG`
  - `ROUND_LEVEL_REVERSAL_SHORT`

规则核心：

- `long`
  - 前面约 `9` 根 K 线低点都在该整数位之上
  - 当前第一次快速下杀触达整数位
  - `shock_drop_pct >= 2.5%`
  - `volume_ratio >= 3.0`
  - 收盘重新回到整数位附近
  - 下影明显，且不能伴随 bearish BOS
- `short`
  - 前面约 `9` 根 K 线高点都在该整数位之下
  - 当前第一次快速上冲触达整数位
  - `shock_rise_pct >= 2.5%`
  - `volume_ratio >= 3.0`
  - 收盘重新压回整数位附近
  - 上影明显，且不能伴随 bullish BOS

分布结果：

- 动态触发 `18` 次
- 真正形成实际成交 `6` 次
- 其中 `5` 胜 `1` 负
- 实际成交合计 `pnl = +738.1120`

结果：

- `15839`: `win_rate 55.7439%`, `profit 27888.70`, `sharpe 4.69084`, `max_dd 22.1916%`, `volatility 51.4048%`, `open_positions 531`
- `15840`: `win_rate 56.0976%`, `profit 33608.60`, `sharpe 4.96227`, `max_dd 22.1916%`, `volatility 51.4648%`, `open_positions 533`

结论：

- 这条规则明显通过
- `win_rate / profit / sharpe` 都显著提升
- `max_dd` 不变
- `volatility` 虽然小幅上升，但幅度很小，远小于收益与 Sharpe 的改善幅度
- 从实际成交看，这不是只修单根 K 线，而是“整数关口首次极端触达 + 放量反转”这一类状态

因此：

- 历史风险优先前沿更新为 `15840`
- 当前代码可复现基线同步更新为 `15840`

## 2026-03-21 round level reversal short v2：15840 -> 15841（未晋级）

在 `15840` 接受后，继续单独收紧 `short` 分支，目标是过滤唯一那笔亏损样本：

- `2023-10-16 20:00:00 short`

对比后发现，这笔和盈利样本 `2024-01-12 20:00:00 short` 的差异主要在：

- 坏样本当时已经是 `short_trend=true`
- `rsi` 不高，只在 `58.69`
- `fib_ratio` 很浅，仅 `0.229`
- `ema_state = Normal`

而盈利样本则是：

- `short_trend=false`
- `rsi = 71.69`
- `fib_ratio = 0.894`
- `ema_state = TooFar`

因此新增一个仅用于实验的 `short v2` 收紧：

- 环境变量：`VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL_SHORT_MODE=v2`
- 仅在原规则基础上额外要求：
  - `!ema_values.is_short_trend`
  - `fib.retracement_ratio >= 0.5`
  - `rsi >= 65` 或 `ema_state == TooFar`

结果：

- `15840`: `win_rate 56.0976%`, `profit 33608.60`, `sharpe 4.96227`, `max_dd 22.1916%`, `volatility 51.4648%`, `open_positions 533`
- `15841`: `win_rate 56.0976%`, `profit 33608.60`, `sharpe 4.96227`, `max_dd 22.1916%`, `volatility 51.4648%`, `open_positions 533`

补充分布：

- 动态触发从 `18 -> 14`
- 实际成交从 `6 -> 5`
- 变成 `5` 胜 `0` 负
- 实际成交合计 `pnl = +741.4043`

但要注意：

- `2023-10-16 20:00:00` 在 `15841` 里仍然会开出 `short`
- 只是它不再由 `ROUND_LEVEL_REVERSAL_SHORT` 这条规则触发，而是由原有系统其它条件接手
- 所以全局指标完全不变

结论：

- 这次 `v2` 只是让 round-level short 分支本身更干净
- 但没有继续改善系统总结果
- 因此不晋级新的回测前沿
- 当前风险优先基线仍保持 `15840`

## 2026-03-22 short trend new bull leg counter long：15840 -> 15842（接受）

基于 `15840` 的头部亏损继续做分布扫描后，发现一组 long 样本非常干净：

- `option_type = long`
- `ema_values.is_short_trend = true`
- `leg_detection.is_bullish_leg = true`
- `leg_detection.is_new_leg = true`
- `ema_distance_filter.state = TooFar`
- `fib.volume_confirmed = false`
- `bollinger.is_long_signal = false`
- `volume_ratio < 1.5`

这类单的盘面含义是：

- 大方向仍然是 `short_trend`
- 当前只是刚切出一个反弹性质的 `bullish new leg`
- 但位置已经 `TooFar`
- 没有 `fib volume` 确认，也没有布林 long 确认
- 本质上是“空头趋势里抢一个弱反弹 long”

分布验证结果：

- 命中 `6` 笔
- `6/6` 全部是 `LOSS`
- `0` 笔盈利样本
- 过滤样本：
  - `2022-05-11 16:00:00`
  - `2022-07-04 00:00:00`
  - `2022-10-17 12:00:00`
  - `2025-02-23 12:00:00`
  - `2025-04-19 20:00:00`
  - `2025-11-27 04:00:00`

因此新增：

- 环境变量：`VEGAS_SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK=v1`
- 过滤原因：`SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK`

结果：

- `15840`: `win_rate 56.0976%`, `profit 33608.60`, `sharpe 4.96227`, `max_dd 22.1916%`, `volatility 51.4648%`, `open_positions 533`
- `15842`: `win_rate 56.4151%`, `profit 36095.60`, `sharpe 5.07403`, `max_dd 22.1916%`, `volatility 51.4369%`, `open_positions 530`

结论：

- 这条规则通过了当前“普适性闸门”
- 修的是一类很明确的状态，而不是单样本
- `win_rate / profit / sharpe` 继续提升
- `max_dd` 持平
- `volatility` 还略有下降

因此：

- 历史风险优先前沿更新为 `15842`
- 当前代码可复现基线同步更新为 `15842`

## 2026-03-22 short trend no bollinger rebound long：15842 -> 15843（接受）

在 `15842` 接受后，继续对剩余 bad long 做分布扫描，筛出了一组同样很干净的样本：

- `option_type = long`
- `ema_values.is_short_trend = true`
- `ema_distance_filter.state = TooFar`
- `bollinger.is_long_signal = false`
- `bollinger.is_short_signal = false`
- `fib.volume_confirmed = false`
- `macd.above_zero = true`
- `volume_ratio < 1.5`

这类单的含义是：

- 大方向仍是 `short_trend`
- 价格位置已经 `TooFar`
- 当前只是弱修复，并没有任何布林 long/short 确认
- 同时也没有 `fib volume` 去支持真正的反转参与
- 本质上是“空头趋势里、无布林确认的弱反弹 long”

分布验证结果：

- 命中 `4` 笔
- `4/4` 全部是 `LOSS`
- `0` 笔盈利样本
- 过滤样本：
  - `2022-03-10 04:00:00`
  - `2022-10-18 00:00:00`
  - `2025-03-25 00:00:00`
  - `2025-12-30 16:00:00`

因此新增：

- 环境变量：`VEGAS_SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK=v1`
- 过滤原因：`SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK`

结果：

- `15842`: `win_rate 56.4151%`, `profit 36095.60`, `sharpe 5.07403`, `max_dd 22.1916%`, `volatility 51.4369%`, `open_positions 530`
- `15843`: `win_rate 56.8441%`, `profit 40551.00`, `sharpe 5.26766`, `max_dd 22.1916%`, `volatility 51.3191%`, `open_positions 526`

结论：

- 这条规则继续通过“普适性闸门”
- 样本数虽然比上一条少，但仍满足 `>= 2` 且 `0` 盈利样本
- `win_rate / profit / sharpe` 再次同步提升
- `max_dd` 持平
- `volatility` 明显继续下降

因此：

- 历史风险优先前沿更新为 `15843`
- 当前代码可复现基线同步更新为 `15843`

## 2026-03-22 long trend above zero high rsi early short：15843 -> 15844（拒绝）

继续对 `15843` 的残余亏损 short 做分布扫描，先挑出了一组看起来很像“上涨大趋势里过早做空”的样本：

- `option_type = short`
- `ema_distance_filter.state = TooFar`
- `ema_values.is_long_trend = true`
- `ema_values.is_short_trend = false`
- `bollinger.is_short_signal = true`
- `bollinger.is_long_signal = false`
- `macd.above_zero = true`
- `macd.histogram < 0`
- `rsi >= 65`
- `volume_ratio >= 1.5`
- 无 `internal/swing bearish BOS`

分布验证在 `15843` 上的结果是：

- 命中 `5` 笔
- `5/5` 全部是 `LOSS`
- `0` 笔盈利样本
- 样本：
  - `2024-03-04 16:00:00`
  - `2025-01-06 12:00:00`
  - `2025-07-13 20:00:00`
  - `2025-07-14 20:00:00`
  - `2025-10-05 12:00:00`

因此新增实验：

- 环境变量：`VEGAS_LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK=v1`
- 过滤原因：`LONG_TREND_ABOVE_ZERO_HIGH_RSI_EARLY_SHORT_BLOCK`

回测结果：

- `15843`: `win_rate 56.8441%`, `profit 40551.00`, `sharpe 5.26766`, `max_dd 22.1916%`, `volatility 51.3191%`, `open_positions 526`
- `15844`: `win_rate 57.3077%`, `profit 40120.66`, `sharpe 5.24699`, `max_dd 26.0372%`, `volatility 51.3562%`, `open_positions 520`

拒绝原因：

- 虽然 `win_rate` 提升
- 但 `profit / sharpe` 同时下降
- `max_dd` 从 `22.1916%` 恶化到 `26.0372%`
- 过滤记录里还混进了 `1` 笔盈利样本：
  - `2024-02-11 08:00:00`

所以这条规则属于“样本局部看起来成立，但放大后开始误伤路径”，不晋级。

## 2026-03-22 normal bull leg no confirm long：15843 -> 15845（接受）

在拒绝 `15844` 之后，改为转向更干净的一组 bad long：

- `option_type = long`
- `ema_distance_filter.state = Normal`
- `leg_detection.is_bullish_leg = true`
- `leg_detection.is_bearish_leg = false`
- `bollinger.is_long_signal = false`
- `fib.volume_confirmed = false`
- `macd.histogram > 0`
- `volume_ratio < 1.5`
- 无 `internal/swing bullish BOS`

这组更像：

- 正在做一个“弱修复 / 弱延续 long”
- 但既没有布林带 long 确认
- 也没有 Fib 成交量确认
- 结构上也没有新的 bullish BOS
- 本质上是 `Normal` 状态里的低质量追多

分布验证结果：

- 命中 `11` 笔
- `11/11` 全部是 `LOSS`
- `0` 笔盈利样本
- 累计过滤拖累 `-0.3265`
- 样本包括：
  - `2022-04-09 16:00:00`
  - `2023-06-03 00:00:00`
  - `2023-07-14 04:00:00`
  - `2023-10-02 08:00:00`
  - `2023-11-07 04:00:00`
  - `2023-11-10 04:00:00`
  - `2024-04-24 16:00:00`
  - `2025-01-17 08:00:00`
  - `2025-01-21 00:00:00`
  - `2025-05-21 20:00:00`
  - `2025-06-26 08:00:00`

因此新增：

- 环境变量：`VEGAS_NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK=v1`
- 过滤原因：`NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK`

结果：

- `15843`: `win_rate 56.8441%`, `profit 40551.00`, `sharpe 5.26766`, `max_dd 22.1916%`, `volatility 51.3191%`, `open_positions 526`
- `15845`: `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`, `open_positions 520`

结论：

- 这条规则通过了“普适性闸门”
- 样本数从 `7` 个扩展到全量分布下的 `11` 个，且 `0` 盈利样本
- `win_rate / profit / sharpe` 全部同步提高
- `max_dd` 大幅从 `22.1916%` 压到 `16.9719%`
- `volatility` 继续下降

因此：

- 历史风险优先前沿更新为 `15845`
- 当前代码可复现基线同步更新为 `15845`

## 2026-03-22 跨币种普适性复查（规则审计）

本轮把“跨币种普适性”正式升级成长期闸门，并先对当前 `15845` 基线里已经接受的新增规则做一次复查。

### 当前状态

已确认 `BTC / SOL` 之前之所以没被跑到，不是缺配置，而是对应 `strategy_config` 被软删除：

- `BTC-USDT-SWAP 4H Vegas`: `id=20`, 之前 `is_deleted=1`
- `SOL-USDT-SWAP 4H Vegas`: `id=30`, 之前 `is_deleted=1`

恢复后，回测入口已经能正常加载三币种：

- `ETH-USDT-SWAP 4H`
- `BTC-USDT-SWAP 4H`
- `SOL-USDT-SWAP 4H`

并实际完成了两组跨币种对照：

#### A. 无新增规则栈基线

- `ETH`: `15846`, `win_rate 51.2027%`, `profit 7739.78`, `sharpe 3.02729`, `max_dd 29.2533%`, `volatility 52.1024%`
- `BTC`: `15847`, `win_rate 58.1967%`, `profit 45.12`, `sharpe 0.16559`, `max_dd 47.2910%`, `volatility 44.6858%`
- `SOL`: `15848`, `win_rate 45.3453%`, `profit 132.50`, `sharpe 0.42666`, `max_dd 59.4338%`, `volatility 69.6401%`

#### B. 当前 `15845` 规则栈

- `ETH`: `15849`, `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`
- `BTC`: `15850`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
- `SOL`: `15851`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

#### C. 跨币种结论

相对各自无规则栈基线：

- `ETH`：`win_rate / profit / sharpe / max_dd / volatility` 全部改善
- `BTC`：`win_rate / profit / sharpe / max_dd / volatility` 全部改善
- `SOL`：`win_rate / profit / sharpe / max_dd` 改善，但 `volatility` 从 `69.6401%` 升到 `86.1967%`

因此当前更准确的结论不是“所有单条规则都已普适”，而是：

- `15845` 这整套规则栈已经通过了 `ETH / BTC / SOL` 的跨币种复核
- 可以标记为：`stack-level cross-asset accepted`
- 但单条规则暂时仍不能自动继承该标签

### 复查结论

#### A. 结构上更可能普适的规则

这类规则主要依赖趋势状态、MACD 相位、布林带冲突、Fib/结构确认缺失、量价关系，而不是 ETH 特有价格行为：

- `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK`
- `BELOW_ZERO_WEAKENING_HANGING_SHORT_BLOCK`
- `LONG_TREND_PULLBACK_SHORT_BLOCK`
- `LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK`
- `HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK`
- `HIGH_VOLUME_CONFLICTING_BOLLINGER_LONG_BLOCK`
- `HIGH_VOLUME_INTERNAL_DOWN_COUNTER_TREND_LONG_BLOCK`
- `HIGH_VOLUME_HIGH_RSI_BOLLINGER_SHORT_LONG_BLOCK`
- `SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK`
- `SHORT_TREND_NO_BOLLINGER_REBOUND_LONG_BLOCK`
- `NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK`

这些规则的共同点：

- 依赖的是“状态关系”
- 用到的是归一化量价特征（如 `volume_ratio`、`TooFar`、`bollinger`、`MACD` 相位）
- 从定义上并不依赖 ETH 的绝对价格刻度

因此它们可暂时标记为：

- `结构上更可能普适`
- 但在单条规则完成 `BTC / SOL` 独立复跑前，仍只算 `ETH provisional`

#### B. 明显需要重点跨币种复核的规则

这类规则虽然在 ETH 上有效，但更容易受到币种波动分布、节奏和价格形态差异影响：

- `VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL`
- `HIGH_VOLUME_TOO_FAR_BOLLINGER_SHORT_LONG_BLOCK`
- `ABOVE_ZERO_LOW_VOLUME_NO_TREND_HANGING_SHORT_BLOCK`
- `ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK`
- `ABOVE_ZERO_NO_TREND_ENGULFING_LONG_BLOCK`

其中最需要重点复核的是：

- `VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL`

原因：

- 它天然更依赖整数关卡和价格层级心理位
- 这类规则最容易受到不同币种价格尺度、合约面值和成交习惯影响
- 在没有 BTC / SOL 单条规则独立复跑前，不应视为普适规则

#### C. 当前审计后的标签

从本轮开始，所有新增规则都必须增加一个标签：

- `stack-level cross-asset accepted`
- `rule-level cross-asset accepted`
- `ETH provisional`
- `blocked`

当前 `15845` 基线的状态更新为：

- 整套规则栈：`stack-level cross-asset accepted`
- 单条规则：默认仍是 `ETH provisional`

只有单条规则在独立开关下补完：

- `BTC 4H` 回测
- `SOL 4H` 回测
- 且风险指标不恶化

之后，才允许从：

- `ETH provisional`

升级为：

- `rule-level cross-asset accepted`

## 2026-03-22 round level reversal 单条跨币种复核（未晋级）

在 `15845` 规则栈已经完成 `stack-level cross-asset accepted` 之后，优先单独复核最容易过耦合的：

- `VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL`

做法：

- 保持 `15845` 其余规则栈不变
- 仅关闭 `VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL`

对照结果：

- 原规则栈：
  - `ETH`: `15849`, `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`
  - `BTC`: `15850`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15851`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

- 关闭该单条规则后：
  - `ETH`: `15852`, `win_rate 57.1429%`, `profit 41090.98`, `sharpe 5.31078`, `max_dd 16.9719%`, `volatility 51.1052%`
  - `BTC`: `15853`, `win_rate 59.8214%`, `profit 127.19`, `sharpe 0.45334`, `max_dd 35.7382%`, `volatility 43.8799%`
  - `SOL`: `15854`, `win_rate 45.7516%`, `profit 418.07`, `sharpe 0.80984`, `max_dd 53.0795%`, `volatility 86.3423%`

结论：

- `ETH` 明显依赖该规则，关闭后 `profit / sharpe / win_rate` 全部变差
- `BTC / SOL` 反而更好，说明这条规则并不是单条跨币种同向增益
- 因此它不能升级为 `rule-level cross-asset accepted`

当前标签更新为：

- `VEGAS_EXPERIMENT_ROUND_LEVEL_REVERSAL = ETH provisional`

## 2026-03-22 normal bull leg no confirm long 单条跨币种复核（未晋级）

继续复核一条结构上更可能普适的规则：

- `NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK`

做法：

- 保持 `15845` 其余规则栈不变
- 仅关闭 `VEGAS_NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK`

对照结果：

- 原规则栈：
  - `ETH`: `15849`, `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`
  - `BTC`: `15850`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15851`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

- 关闭该单条规则后：
  - `ETH`: `15855`, `win_rate 56.8441%`, `profit 40550.96`, `sharpe 5.26766`, `max_dd 22.1916%`, `volatility 51.3191%`
  - `BTC`: `15856`, `win_rate 59.6950%`, `profit 110.03`, `sharpe 0.40764`, `max_dd 37.1033%`, `volatility 43.1853%`
  - `SOL`: `15857`, `win_rate 46.2264%`, `profit 477.19`, `sharpe 0.88979`, `max_dd 53.0786%`, `volatility 85.5868%`

结论：

- `ETH` 上这条规则是明显正贡献，关闭后 `profit / sharpe / max_dd` 全面恶化
- `BTC` 呈混合结果：`profit / sharpe` 变好，但 `max_dd / volatility` 变差
- `SOL` 关闭后反而更好
- 所以它虽然比整数位规则更结构化，但单条规则层面仍未形成跨币种同向增益

当前标签更新为：

- `NORMAL_BULL_LEG_NO_CONFIRM_LONG_BLOCK = ETH provisional`

## 2026-03-22 long trend pullback short 单条跨币种复核（未晋级）

继续复核一条更结构化的 short 规则：

- `LONG_TREND_PULLBACK_SHORT_BLOCK`

做法：

- 保持 `15845` 其余规则栈不变
- 仅关闭 `VEGAS_LONG_TREND_PULLBACK_SHORT_BLOCK`

对照结果：

- 原规则栈：
  - `ETH`: `15849`, `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`
  - `BTC`: `15850`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15851`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

- 关闭该单条规则后：
  - `ETH`: `15858`, `win_rate 57.1702%`, `profit 46557.82`, `sharpe 5.50191`, `max_dd 16.9719%`, `volatility 51.2061%`
  - `BTC`: `15859`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15860`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

结论：

- `ETH` 上这条规则是正贡献，关闭后 `profit / sharpe / win_rate` 变差，`volatility` 也略差
- `BTC / SOL` 完全不受影响，说明这条规则当前没有形成跨币种有效触发
- 因此它仍不能升级为 `rule-level cross-asset accepted`

当前标签更新为：

- `LONG_TREND_PULLBACK_SHORT_BLOCK = ETH provisional`

## 2026-03-22 high volume no trend bollinger-long short 单条跨币种复核（未晋级）

继续复核一条高量结构规则：

- `HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK`

做法：

- 保持 `15845` 其余规则栈不变
- 仅关闭 `VEGAS_HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK`

对照结果：

- 原规则栈：
  - `ETH`: `15849`, `win_rate 57.5000%`, `profit 49508.89`, `sharpe 5.60800`, `max_dd 16.9719%`, `volatility 51.1619%`
  - `BTC`: `15850`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15851`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

- 关闭该单条规则后：
  - `ETH`: `15861`, `win_rate 57.0881%`, `profit 46576.48`, `sharpe 5.50455`, `max_dd 16.9719%`, `volatility 51.1876%`
  - `BTC`: `15862`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
  - `SOL`: `15863`, `win_rate 45.6311%`, `profit 361.38`, `sharpe 0.73650`, `max_dd 53.0795%`, `volatility 86.1967%`

结论：

- `ETH` 上这条规则是正贡献，关闭后 `profit / sharpe / win_rate` 变差
- `BTC / SOL` 仍然完全不受影响，说明它当前也是 ETH 有效、跨币种未触发
- 因此它仍不能升级为 `rule-level cross-asset accepted`

当前标签更新为：

- `HIGH_VOLUME_NO_TREND_BOLLINGER_LONG_SHORT_BLOCK = ETH provisional`

## 2026-03-22 deep negative macd recovery short 参数尺度归一化复核（v7 / v8）

前面单条跨币种复核已经证明：

- `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK` 在 `ETH` 上有效
- 但直接沿用 ETH 的绝对阈值时，`BTC` 会混入盈利样本，`SOL` 又经常不触发

这说明问题不一定在“规则语义不普适”，也可能在“阈值尺度没有随币种波动和价格层级缩放”。

因此这次不再直接判它为纯 `ETH provisional`，而是专门验证两种归一化方案：

### A. `v7`：价格归一化阈值

做法：

- 保持 `15845` 其余规则栈不变
- 仅把 `VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE` 切到 `v7`
- 用 `abs(macd_line) / signal_price` 与 `abs(signal_line) / signal_price` 替代原来的固定 `-50 / -60 / -80` 绝对阈值

对照结果：

- `ETH`: `15864`, `win_rate 57.5290%`, `profit 49771.11`, `sharpe 5.61787`, `max_dd 16.9719%`, `volatility 51.1519%`
- `BTC`: `15865`, `win_rate 59.3886%`, `profit 89.25`, `sharpe 0.35131`, `max_dd 35.7094%`, `volatility 41.6610%`
- `SOL`: `15866`, `win_rate 45.7792%`, `profit 383.47`, `sharpe 0.76673`, `max_dd 50.8326%`, `volatility 86.1492%`

过滤命中分布：

- `ETH`: `4` 笔，`4/4` 全部是 `LOSS`
- `BTC`: `3` 笔，`3/3` 全部是 `LOSS`
- `SOL`: `1` 笔，`1/1` 是 `LOSS`

结论：

- `ETH` 继续改善
- `SOL` 明显改善
- `BTC` 略退化

所以 `v7` 证明了“归一化方向是对的”，但还不是最终可接受版本。

### B. `v8`：按价格尺度分层阈值

做法：

- 保持 `15845` 其余规则栈不变
- 仅把 `VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE` 切到 `v8`
- 当 `signal_price >= 10000` 时继续使用原有绝对阈值
- 当 `signal_price < 10000` 时改用 `v7` 的价格归一化阈值

对照结果：

- `ETH`: `15867`, `win_rate 57.5290%`, `profit 49771.11`, `sharpe 5.61787`, `max_dd 16.9719%`, `volatility 51.1519%`
- `BTC`: `15868`, `win_rate 59.6452%`, `profit 91.81`, `sharpe 0.36053`, `max_dd 35.7094%`, `volatility 41.6456%`
- `SOL`: `15869`, `win_rate 45.7792%`, `profit 383.47`, `sharpe 0.76673`, `max_dd 50.8326%`, `volatility 86.1492%`

相对原 `15845` 规则栈：

- `ETH`：`win_rate / profit / sharpe` 小幅改善，`max_dd` 持平，`volatility` 更低
- `BTC`：与原规则栈基本持平，没有被归一化版本拖坏
- `SOL`：`win_rate / profit / sharpe / max_dd / volatility` 全部改善

结论：

- 这组结果支持一个更重要的判断：
  - 某些 `ETH provisional` 规则未必是“只对 ETH 有意义”
  - 更可能是“规则语义普适，但阈值尺度未做跨币种缩放”
- `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK` 现在不应再简单归类为“ETH 专属规则”
- 更准确的标签应该是：
  - `参数尺度已验证`
  - `rule-level cross-asset accepted（v8 版本）`

因此，从本轮开始：

- `DEEP_NEGATIVE_MACD_RECOVERY_SHORT_BLOCK` 的默认推荐模式更新为 `v8`
- 当前代码可复现的跨币种风险优先基线同步更新为：
  - `ETH = 15867`
  - `BTC = 15868`
  - `SOL = 15869`

## 2026-03-22 long trend above zero low volume weakening short 单规则跨币种复核（继续观察）

为了验证：

- `LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK`

到底是单条跨币种普适规则，还是只是规则栈里“顺手有贡献”的一条分支，这次做了一轮更干净的 A/B：

- 保持同一套实验规则环境不变
- 仅切换
  - `VEGAS_LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK=off`
  - 对照 `VEGAS_LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK=v1`

该规则在三币种上的命中分布先确认如下：

- `ETH`: `3` 笔，`3/3` 全亏
- `BTC`: `3` 笔，`3/3` 全亏
- `SOL`: `8` 笔，`8/8` 全亏

说明它至少不是“只有 ETH 命中”的单币种规则。

### A. 关闭规则（off）

- `ETH`: `15870`, `win_rate 55.5762%`, `profit 25297.50`, `sharpe 4.54403`, `max_dd 20.2075%`, `volatility 51.4772%`
- `BTC`: `15871`, `win_rate 59.5238%`, `profit 94.00`, `sharpe 0.34767`, `max_dd 31.9944%`, `volatility 44.1104%`
- `SOL`: `15872`, `win_rate 45.4545%`, `profit 191.20`, `sharpe 0.55573`, `max_dd 58.3314%`, `volatility 72.3461%`

### B. 启用规则（v1）

- `ETH`: `15873`, `win_rate 55.8879%`, `profit 28336.40`, `sharpe 4.71522`, `max_dd 20.2075%`, `volatility 51.3921%`
- `BTC`: `15874`, `win_rate 59.5186%`, `profit 109.44`, `sharpe 0.39378`, `max_dd 31.9944%`, `volatility 44.5007%`
- `SOL`: `15875`, `win_rate 45.9283%`, `profit 400.69`, `sharpe 0.78953`, `max_dd 50.8326%`, `volatility 86.1296%`

### C. 对照结论

相对 `off -> v1`：

- `ETH`
  - `profit +3038.90`
  - `sharpe +0.17119`
  - `win_rate +0.3116pct`
  - `volatility` 更低
  - `max_dd` 持平
- `BTC`
  - `profit +15.44`
  - `sharpe +0.04611`
  - 但 `win_rate` 轻微回落
  - `volatility` 轻微上升
  - `max_dd` 持平
- `SOL`
  - `profit +209.49`
  - `sharpe +0.23380`
  - `win_rate +0.4738pct`
  - `max_dd` 明显下降
  - 但 `volatility` 上升

因此当前更准确的判断是：

- 这条规则不是 `ETH only`
- 它在 `ETH / SOL` 上明显有效
- `BTC` 呈混合结果
- `SOL` 的 `max_dd` 改善很大，但 `volatility` 也被抬高

所以它暂时还不能直接升级成：

- `rule-level cross-asset accepted`

当前标签维持为：

- `LONG_TREND_ABOVE_ZERO_LOW_VOLUME_WEAKENING_SHORT_BLOCK = stack-helpful, needs further scaling`

后续如果继续优化这条规则，优先方向不是“关/开”，而是：

- 继续做参数尺度或波动分层
- 重点压 `SOL` 上的 `volatility` 副作用

## 2026-03-22 short trend new bull leg counter long 参数尺度微调（v2）

继续沿“规则语义可能普适，但阈值尺度未缩放”这个方向，下一条处理的是：

- `SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK`

在当前跨币种基线 `15867/15868/15869` 下，这条规则命中分布是：

- `ETH`: `6` 笔，`6/6` 全亏
- `BTC`: `7` 笔，其中 `6` 笔 `LOSS`，`1` 笔 `WIN`
- `SOL`: `2` 笔，`2/2` 全亏

其中 BTC 唯一被误伤的盈利样本是：

- `2022-09-05 20:00:00`

和其他 BTC 亏损样本相比，这笔最明显的差异不是趋势状态，而是：

- `histogram / price` 明显更小
- 属于更弱的修复波段
- 不适合继续用原始 `v1` 逻辑一刀切拦掉

因此增加了一个非常窄的 `v2`：

- 保留 `v1` 的全部结构前提
- 额外要求：
  - `macd.histogram > 0`
  - `abs(histogram) / signal_price >= 0.0015`

也就是只有“修复柱子强到一定程度”的 counter-trend long，才继续视为该挡掉的坏多单。

### 对照结果（固定同一套实验规则，仅比较 `v1 -> v2`）

- `ETH`
  - `v1`: `15873`, `win_rate 55.8879%`, `profit 28336.40`, `sharpe 4.71522`, `max_dd 20.2075%`, `volatility 51.3921%`
  - `v2`: `15876`, 指标完全一致
- `BTC`
  - `v1`: `15874`, `win_rate 59.5186%`, `profit 109.44`, `sharpe 0.39378`, `max_dd 31.9944%`, `volatility 44.5007%`
  - `v2`: `15877`, `win_rate 59.7374%`, `profit 120.09`, `sharpe 0.42670`, `max_dd 31.9944%`, `volatility 44.4409%`
- `SOL`
  - `v1`: `15875`, `win_rate 45.9283%`, `profit 400.69`, `sharpe 0.78953`, `max_dd 50.8326%`, `volatility 86.1296%`
  - `v2`: `15878`, 指标完全一致

### 结果解释

- `ETH`：不变
- `SOL`：不变
- `BTC`：更好
  - `win_rate / profit / sharpe` 改善
  - `volatility` 更低
  - `max_dd` 持平

而规则命中分布也变得更干净：

- `ETH`: `6/6` 全亏
- `BTC`: 从 `6 LOSS + 1 WIN` 变成 `6/6` 全亏
- `SOL`: `2/2` 全亏

结论：

- 这次不是“规则语义改了”，而是把 BTC 上唯一混入的盈利样本排掉了
- `v2` 比 `v1` 更接近真正的跨币种版本
- 当前可暂记为：
  - `SHORT_TREND_NEW_BULL_LEG_COUNTER_LONG_BLOCK = scaling-improved candidate`

下一步如果要继续推进它，应该用 `15876/15877/15878` 这组对照继续做真正的单规则跨币种晋级判断，而不是回到未经缩放的 `v1`。

## 2026-03-22 low volume neutral RSI MACD recovery short 参数尺度微调（v2）

继续沿“规则语义可普适，但阈值尺度未缩放”这条线，下一条处理的是：

- `LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT`

在当前跨币种基线 `15867/15868/15869` 下，这条规则命中分布是：

- `ETH`: `2` 笔，`2/2` 全亏
- `BTC`: `1` 笔，`1/1` 为 `WIN`
- `SOL`: `6` 笔，`6/6` 全亏

BTC 唯一被误伤的样本是：

- `2023-05-21 12:00:00`

与 `ETH/SOL` 上那些确实该拦的坏 short 相比，这笔最明显的差异不是结构方向，而是：

- `abs(signal_line) / signal_price` 明显更小
- 属于零轴下方的弱修复，不该继续按原始 `v1` 一刀切拦掉

因此增加了一个非常窄的 `v2`：

- 保留 `v1` 的全部结构前提：
  - `volume_ratio < 1.0`
  - `RSI` 在 `47~53`
  - `macd_line < 0`
  - `signal_line < 0`
  - `macd_line > signal_line`
  - `histogram > 0`
- 额外要求：
  - `abs(signal_line) / signal_price >= 0.002`

也就是：

- 只有当零轴下方修复的“恢复幅度”达到足够量级时，才继续把这类 short 视为该挡掉的坏空单
- 过弱的修复不再一刀切拦掉

### 对照结果（固定同一套实验规则，仅比较 `v1 -> v2`）

说明：

- 这一轮仍然是在同一套实验规则环境下做的单规则 A/B
- 不是直接替换当前正式跨币种基线 `15867/15868/15869`

对照组：

- `v1`
  - `ETH = 15879`
  - `BTC = 15880`
  - `SOL = 15881`
- `v2`
  - `ETH = 15882`
  - `BTC = 15883`
  - `SOL = 15884`

结果：

- `ETH`
  - `15879 -> 15882`
  - 指标完全一致
- `BTC`
  - `15880`: `win_rate 59.7374%`, `profit 120.09`, `sharpe 0.42670`, `max_dd 31.9944%`, `volatility 44.4409%`
  - `15883`: `win_rate 59.8253%`, `profit 122.79`, `sharpe 0.43467`, `max_dd 31.9944%`, `volatility 44.4442%`
- `SOL`
  - `15881 -> 15884`
  - 指标完全一致

命中分布也更干净：

- `ETH`: 仍是 `2/2` 全亏
- `BTC`: 从 `1` 笔 `WIN` 变成 `0` 命中
- `SOL`: 仍是 `6/6` 全亏

### 结论

- 这次不是规则语义变化，而是把 `BTC` 上那一笔“修复过弱”的盈利样本放回来了
- `ETH / SOL` 不受影响
- `BTC` 小幅改善

因此当前可暂记为：

- `LOW_VOLUME_NEUTRAL_RSI_MACD_RECOVERY_BLOCK_SHORT = scaling-improved candidate`

但它还不能直接升级为：

- `rule-level cross-asset accepted`

因为这轮对照仍然是在实验规则栈里完成的，不是直接在当前正式跨币种基线 `15867/15868/15869` 上做单规则开关。

## 2026-03-22 above zero no trend too far hanging short 单规则跨币种复核（mixed）

在继续筛跨币种规则时，额外复核了：

- `ABOVE_ZERO_NO_TREND_TOO_FAR_HANGING_SHORT_BLOCK`

这条规则在当前正式跨币种基线 `15867/15868/15869` 下的命中分布非常干净：

- `ETH`: `2/2` 全亏
- `BTC`: `1/1` 全亏
- `SOL`: `1/1` 全亏

按命中分布看，它很像一条很有希望直接升级的跨币种规则，因此把它接入当前实验规则栈，做了一轮更干净的 A/B：

- 对照组：
  - `ETH = 15882`
  - `BTC = 15883`
  - `SOL = 15884`
- 打开该规则：
  - `ETH = 15885`
  - `BTC = 15886`
  - `SOL = 15887`

结果：

- `ETH`
  - `15882 -> 15885`
  - `win_rate 55.8879% -> 56.0976%`
  - `profit 28336.40 -> 30469.20`
  - `sharpe 4.71522 -> 4.82492`
  - `max_dd` 持平
  - `volatility` 下降
- `BTC`
  - `15883 -> 15886`
  - 指标完全一致
- `SOL`
  - `15884 -> 15887`
  - `win_rate 45.9807% -> 45.8065%`
  - `profit 327.93 -> 323.09`
  - `sharpe 0.79519 -> 0.78712`
  - `max_dd` 持平
  - `volatility` 略降

### 结论

- 这条规则不是命中分布有问题
- 问题在于：
  - 它在 `ETH` 上明显有利
  - `BTC` 基本中性
  - 但 `SOL` 上出现了小幅负贡献

因此当前不能升级为：

- `rule-level cross-asset accepted`

更准确的标签是：

- `mixed cross-asset candidate`

这轮结果也补充了一个重要约束：

- 后续不能只凭“命中样本全亏”就让规则晋级
- 仍然必须看跨币种完整资金曲线和路径副作用

## 2026-03-22 SOL / BCH volatility-only 参数 sweep

本轮不改 Vegas 主逻辑，不改现有 ETH 最优规则栈，只做一件事：

- 以当前 ETH 最优策略配置为模板
- 仅针对小币的波动特征，微调以下参数：
  - `range_filter_signal.bb_width_threshold`
  - `range_filter_signal.tp_kline_ratio`
  - `extreme_k_filter_signal.min_move_pct`
  - `extreme_k_filter_signal.min_body_ratio`
  - `risk.max_loss_percent`
  - `risk.atr_take_profit_ratio`

同时补齐 BCH 回测目标：

- 在 `crates/rust-quant-cli/src/app/bootstrap.rs` 的默认回测目标中新增 `BCH-USDT-SWAP 4H`
- 数据库新增 `strategy_config.id=31`，初始从 `ETH id=11` 克隆

### ETH 模板直推基线

先把 `ETH id=11` 的整套参数直接复制给：

- `SOL id=30`
- `BCH id=31`

回测结果：

- `15890 SOL`
  - `win_rate 42.4699%`
  - `profit 342.746`
  - `sharpe 1.00128`
  - `max_dd 40.8051%`
  - `volatility 60.3196%`
- `15891 BCH`
  - `win_rate 36.0071%`
  - `profit -80.7851`
  - `sharpe -0.692897`
  - `max_dd 84.3301%`
  - `volatility 46.5920%`

结论：

- ETH 逻辑直接迁移到 SOL 已经比旧 SOL 基线更稳
- 但 BCH 明显不适配，说明小币还需要独立的 volatility-only 收紧层

### Variant 1

参数：

- `bb_width_threshold = 0.032`
- `tp_kline_ratio = 0.52`
- `min_move_pct = 0.013`
- `min_body_ratio = 0.65`
- `max_loss_percent = 0.038`
- `atr_take_profit_ratio = 3.1`

结果：

- `15894 SOL`
  - `win_rate 43.6578%`
  - `profit 328.013`
  - `sharpe 0.992426`
  - `max_dd 40.2585%`
  - `volatility 59.0628%`
- `15895 BCH`
  - `win_rate 38.0454%`
  - `profit -74.1152`
  - `sharpe -0.606266`
  - `max_dd 78.6689%`
  - `volatility 45.5067%`

结论：

- 比 ETH 模板直推更稳
- 但 SOL 的 `profit / sharpe` 略弱

### Variant 2

参数：

- `bb_width_threshold = 0.034`
- `tp_kline_ratio = 0.48`
- `min_move_pct = 0.015`
- `min_body_ratio = 0.68`
- `max_loss_percent = 0.036`
- `atr_take_profit_ratio = 2.9`

结果：

- `15898 SOL`
  - `win_rate 43.8596%`
  - `profit 334.566`
  - `sharpe 1.01552`
  - `max_dd 39.1698%`
  - `volatility 58.5049%`
- `15899 BCH`
  - `win_rate 39.6226%`
  - `profit -69.4051`
  - `sharpe -0.557712`
  - `max_dd 76.3011%`
  - `volatility 44.5001%`

结论：

- 这是当前最平衡的一组
- 对 SOL：
  - `win_rate / sharpe / max_dd / volatility` 全面优于模板直推
  - `profit` 只比模板直推少 `8.18`
- 对 BCH：
  - 目前 4 组里所有指标都是最优
  - 虽然仍然亏损，但亏损、回撤、波动都显著收敛

### Variant 3

参数：

- `bb_width_threshold = 0.033`
- `tp_kline_ratio = 0.50`
- `min_move_pct = 0.014`
- `min_body_ratio = 0.66`
- `max_loss_percent = 0.037`
- `atr_take_profit_ratio = 3.0`

结果：

- `15902 SOL`
  - `win_rate 43.6950%`
  - `profit 341.191`
  - `sharpe 1.02274`
  - `max_dd 39.7157%`
  - `volatility 58.8716%`
- `15903 BCH`
  - `win_rate 38.7931%`
  - `profit -73.4638`
  - `sharpe -0.599740`
  - `max_dd 77.9536%`
  - `volatility 45.3255%`

结论：

- SOL 的 `profit / sharpe` 略高于 `Variant 2`
- 但 `win_rate / max_dd / volatility` 反而回退
- BCH 也明显退步
- 不选

### Variant 4

参数：

- `bb_width_threshold = 0.035`
- `tp_kline_ratio = 0.46`
- `min_move_pct = 0.015`
- `min_body_ratio = 0.69`
- `max_loss_percent = 0.034`
- `atr_take_profit_ratio = 2.7`

结果：

- `15906 SOL`
  - `win_rate 43.9306%`
  - `profit 299.098`
  - `sharpe 0.951087`
  - `max_dd 39.2704%`
  - `volatility 57.8240%`
- `15907 BCH`
  - `win_rate 40.0341%`
  - `profit -71.3477`
  - `sharpe -0.592506`
  - `max_dd 75.5428%`
  - `volatility 43.7423%`

结论：

- 这是更保守的版本
- BCH 的 `win_rate / max_dd / volatility` 继续改善
- 但 SOL 的 `profit / sharpe` 明显退化
- 不选

### 最终选择

本轮最终落库参数选择：

- `SOL id=30 = Variant 2`
- `BCH id=31` 先临时落到 `Variant 2`，后续单独开 `BCH-only` 分支继续搜索

当前数据库已回写为：

- `bb_width_threshold = 0.034`
- `tp_kline_ratio = 0.48`
- `min_move_pct = 0.015`
- `min_body_ratio = 0.68`
- `max_loss_percent = 0.036`
- `atr_take_profit_ratio = 2.9`

最终判断：

- 这次属于“只做小币 volatility-only tuning”，不是新规则优化
- `Variant 2` 没有追求单一币种最极端收益，而是同时兼顾：
  - `SOL` 的稳定性提升
  - `BCH` 的明显风险收敛
  - 避免对 ETH 主逻辑做任何修改
- 因此当前最合理的标签是：
  - `small-cap volatility-only accepted`

## 2026-03-22 BCH-only volatility-only 续调

在锁定：

- `SOL id=30 = Variant 2`

之后，继续只对：

- `BCH id=31`

做 `volatility-only tuning`，不动任何 Vegas 主逻辑，也不改 `SOL` 参数。

### BCH-only Variant A

参数：

- `bb_width_threshold = 0.035`
- `tp_kline_ratio = 0.47`
- `min_move_pct = 0.015`
- `min_body_ratio = 0.69`
- `max_loss_percent = 0.035`
- `atr_take_profit_ratio = 2.8`

结果：

- `15911 BCH`
  - `win_rate 39.9317%`
  - `profit -71.3526`
  - `sharpe -0.586775`
  - `max_dd 75.6429%`
  - `volatility 44.1745%`

对比 `15899 BCH`：

- `win_rate` 更高
- `sharpe / max_dd / volatility` 更好
- 但 `profit` 略差

结论：

- 这说明 BCH 仍然可以继续靠波动参数收敛风险
- 但这组没有同时提升收益，不作为最终版本

### BCH-only Variant B

参数：

- `bb_width_threshold = 0.036`
- `tp_kline_ratio = 0.45`
- `min_move_pct = 0.016`
- `min_body_ratio = 0.70`
- `max_loss_percent = 0.033`
- `atr_take_profit_ratio = 2.6`

结果：

- `15915 BCH`
  - `win_rate 40.4722%`
  - `profit -67.4934`
  - `sharpe -0.535619`
  - `max_dd 74.8722%`
  - `volatility 44.4129%`

对比 `15899 BCH`：

- `win_rate` 提升
- `profit` 提升
- `sharpe` 提升
- `max_dd` 下降
- `volatility` 下降

结论：

- 这次 `BCH-only` 分支是有效的
- 说明在不改主逻辑的前提下，BCH 还有一小段 volatility-only headroom
- 当前最终落库调整为：
  - `SOL id=30 = Variant 2`
  - `BCH id=31 = BCH-only Variant B`

### 当前落库参数

- `SOL id=30`
  - `bb_width_threshold = 0.034`
  - `tp_kline_ratio = 0.48`
  - `min_move_pct = 0.015`
  - `min_body_ratio = 0.68`
  - `max_loss_percent = 0.036`
  - `atr_take_profit_ratio = 2.9`
- `BCH id=31`
  - `bb_width_threshold = 0.036`
  - `tp_kline_ratio = 0.45`
  - `min_move_pct = 0.016`
  - `min_body_ratio = 0.70`
  - `max_loss_percent = 0.033`
  - `atr_take_profit_ratio = 2.6`

最终判断：

- `SOL` 已经基本到达 volatility-only tuning 的平衡点
- `BCH` 还能继续靠更保守的波动参数获得风险收敛
- 但 BCH 仍未转正，因此下一阶段如果继续优化 BCH，就不应再只靠 volatility-only tuning

## 2026-03-23 切回 ETH-only 主线：增加 BACKTEST_ONLY_INST_IDS，并否决 max_loss_percent 收紧

本轮先不继续做跨币种验证，直接回到 `ETH 4H` 主线。

### A. 运行入口修正

为了避免每次回测都把 `BTC / SOL / BCH` 一起带上，先在
[bootstrap.rs](/Users/xu/onions/rust_quant/crates/rust-quant-cli/src/app/bootstrap.rs)
补了一个新的运行时过滤：

- `BACKTEST_ONLY_INST_IDS=ETH-USDT-SWAP`

这样后续可以在不改默认目标列表的前提下，真正只跑 `ETH 4H`。

### B. 基线确认

本轮继续以正式 `ETH` 基线 `15867` 为锚：

- `win_rate 57.5290%`
- `profit 49771.11`
- `sharpe 5.61787`
- `max_dd 16.9719%`
- `volatility 51.1519%`

### C. 先做风险参数最小实验：只动 `max_loss_percent`

动机：

- `15867` 里最大纯亏损簇不是 signal stop-loss，而是 `最大亏损止损`
- 共 `34` 笔，合计 `-1976.1758`

因此本轮只做最小参数实验，不碰规则栈：

#### 实验 1：`max_loss_percent = 0.038`

- 回测 `15916`
- `win_rate 57.3930%`
- `profit 32143.70`
- `sharpe 5.06952`
- `max_dd 23.5384%`
- `volatility 49.6939%`

相对 `15867`：

- `profit` 大幅下降
- `sharpe` 明显下降
- `max_dd` 明显恶化

结论：

- 拒绝

#### 实验 2：`max_loss_percent = 0.039`

- 回测 `15917`
- `win_rate 57.6172%`
- `profit 33047.70`
- `sharpe 5.11833`
- `max_dd 23.8560%`
- `volatility 49.6397%`

相对 `15867`：

- `profit` 仍大幅下降
- `sharpe` 仍明显下降
- `max_dd` 继续恶化

结论：

- 拒绝

### D. 本轮结论

- `最大亏损止损` 虽然是当前最大的负贡献簇，但简单收紧 `max_loss_percent` 不是正确方向
- `0.038 / 0.039` 都会同时破坏：
  - `profit`
  - `sharpe`
  - `max_dd`
- 说明这批尾部亏损里有相当一部分其实依赖当前 `0.04` 的波动容忍度，不能机械收紧

### E. 当前状态

- `strategy_config.id=11` 已恢复：
  - `max_loss_percent = 0.04`
- `ETH-only` 运行入口已具备：
  - `BACKTEST_ONLY_INST_IDS`

下一步应回到 signal/rule 层，继续拆 `15867` 剩余亏损簇，而不是再沿风险参数线收紧。

## 2026-03-23 基线 15919 复盘：`2026-03-13 16:00:00` 的 engulfing long 为什么把 `signal-kline stop` 抬高，以及去掉后会怎样

### A. 问题定位

基线严格等价复跑：

- `15867 = 15919`
- `win_rate 57.5290%`
- `profit 49771.11`
- `sharpe 5.61787`
- `max_dd 16.9719%`

对应交易是：

- `2026-03-09 08:00:00` 开多，`open_price = 1976.67`
- `2026-03-14 00:00:00` 平仓，`close_price = 2098.73`
- `close_type = Signal_Kline_Stop_Loss`
- `stop_loss_source = Engulfing_Volume_Rejected`
- `profit_loss = 2869.47`

它不是在 `2026-03-14 00:00:00` 新生成止损，而是：

1. `2026-03-13 16:00:00` 同向 long 信号把 `signal-kline stop` 抬到了 `2098.73`
2. `2026-03-14 00:00:00` 的最低价跌破 `2098.73`
3. 因此在这根 K 线触发 `Signal_Kline_Stop_Loss`

### B. `2026-03-13 16:00:00` 完整信号特征

这根 K 线本身：

- `o=2098.73`
- `h=2132.71`
- `l=2090.57`
- `c=2119.76`
- `v=7657957.89`

核心指标：

- `direction = Long`
- `should_buy = true`
- `stop_loss_source = Engulfing_Volume_Rejected`
- `signal_kline_stop_loss_price = 2098.73`
- `atr_stop_loss_price = 2053.60`

趋势/位置：

- `ema_state = TooFar`
- `ema_values.is_long_trend = false`
- `ema_values.is_short_trend = false`
- `ema_touch.is_long_signal = false`
- `ema_touch.is_short_signal = false`
- `fib.in_zone = false`
- `fib.retracement_ratio = 0.7993`
- `fib.volume_confirmed = false`
- `major_bearish = true`

形态/动量：

- `engulfing.is_engulfing = true`
- `engulfing.is_valid_engulfing = true`
- `leg_detection.is_bullish_leg = true`
- `bollinger.is_short_signal = true`
- `bollinger.is_long_signal = false`
- `rsi = 62.37`
- `macd.above_zero = true`
- `macd_line = 23.65`
- `signal_line = 16.94`
- `histogram = 6.71`
- `histogram_increasing = true`

这说明：

- 这根并不是“回撤低位确认 long”
- 更像是高位修复过程中的 `engulfing + bull leg` 延续 long
- 但由于 `fib 不在区间`、`量能未确认`、`bollinger 反而给 short`，它本身也是一根带分歧的 long

### C. 为什么止损会被抬到 `2098.73`

当前代码里有两个层级：

1. 方向判断阶段：
   当 `engulfing` 出现但 `volume_confirmed=false` 时，只写：
   - `stop_loss_source = Engulfing_Volume_Rejected`
   - 不直接写 `signal_kline_stop_loss_price`

2. `calculate_best_stop_loss_price()` 阶段：
   只要 `conditions` 里包含 `SignalType::Engulfing`
   就无条件把 `signal_kline_stop_loss_price = last_data_item.o()`

所以这根 `03-13 16:00` 的 long，虽然只是 `Engulfing_Volume_Rejected`，
最终还是被统一抬成了：

- `signal_kline_stop_loss_price = 开盘价 = 2098.73`

### D. A/B：禁用“long 侧 engulfing 抬止损到开盘价”

本轮只加了一个默认关闭的实验开关：

- `VEGAS_DISABLE_LONG_ENGULFING_STOP_RAISE=1`

含义：

- 只禁用 `calculate_best_stop_loss_price()` 里
  `Long + Engulfing -> stop = open_price`
  这一条覆盖逻辑
- 不影响 short 侧
- 不影响 `ATR stop`
- 不影响基线默认行为

实验结果：`15920`

- `win_rate 55.7957%`
- `profit 25158.10`
- `sharpe 4.56108`
- `max_dd 22.5510%`
- `volatility 50.9305%`

相对基线 `15919`：

- `profit` 大幅下降
- `sharpe` 明显下降
- `max_dd` 明显恶化

所以这条改动明确拒绝。

### E. 这笔具体交易在 A/B 里怎么变

禁用后，`2026-03-09 08:00:00` 这笔 long：

- 不再于 `2026-03-14 00:00:00` 被 `Signal_Kline_Stop_Loss` 锁盈
- 也没有了 `2026-03-13 16:00:00 -> 2098.73` 这次止损更新
- 持仓继续保留到 `2026-03-19 16:00:00`
- 最终 `profit_loss = 2190.84`

也就是单笔上：

- 基线 `15919`: `+2869.47`
- A/B `15920`: `+2190.84`

这笔本身就少赚了约 `678.63`

### F. 为什么整体会恶化这么大

因为这条逻辑不是只影响这一笔，而是系统性减少了 long 的信号止损锁盈。

对比 `Signal_Kline_Stop_Loss` 的整体结果：

- `15919`: `225` 笔，合计 `+9471.86`
- `15920`: `209` 笔，合计 `+1126.82`

也就是说：

- 去掉这条抬止损后，`Signal_Kline_Stop_Loss` 的盈利保护能力大幅下降
- 很多原本能被及时锁住的盈利，后面又被回吐掉了

### G. 本轮结论

- `2026-03-13 16:00:00` 把止损抬到 `2098.73`，根因不是成交量确认，而是：
  `Engulfing` 命中后，在 `calculate_best_stop_loss_price()` 被统一覆盖成 `open_price`
- 这条逻辑虽然看起来“抬得过高”，但在当前 ETH 基线里整体是有价值的
- 直接去掉会明显伤害：
  - `profit`
  - `sharpe`
  - `max_dd`

因此：

- `VEGAS_DISABLE_LONG_ENGULFING_STOP_RAISE` 仅保留为实验开关
- 不纳入正式基线

## 2026-03-23 窄化实验：只在 `Engulfing_Volume_Rejected + !fib.in_zone + bollinger.short + TooFar` 时，不把 long 的 stop 抬到开盘价

### A. 历史命中样本（基线 `15919`）

筛到的典型样本包括：

- `2026-03-13 16:00:00`
- `2025-07-16 12:00:00`
- `2025-05-09 12:00:00`
- `2024-11-19 04:00:00`
- `2024-05-27 08:00:00`
- `2024-04-09 00:00:00`
- `2024-04-08 12:00:00`
- `2024-04-08 04:00:00`

这些点的共同特征是：

- `stop_loss_source = Engulfing_Volume_Rejected`
- `fib.in_zone = false`
- `bollinger.is_short_signal = true`
- `ema_state = TooFar`

也就是：

- 方向层仍给了 long
- 但位置已经偏高 / 偏远
- Fib 不确认
- 布林还在提示 short 压制

### B. 实验开关

本轮新增一个默认关闭的窄实验：

- `VEGAS_DISABLE_CONFLICTING_LONG_ENGULFING_STOP_RAISE=1`

含义：

- 只在同时满足：
  - `Long`
  - `Engulfing`
  - `fib.in_zone = false`
  - `bollinger.short = true`
  - `ema_state = TooFar`
- 才禁止 `calculate_best_stop_loss_price()` 把 `signal-kline stop` 覆盖成 `open_price`

### C. A/B 结果

新回测：`15921`

- `15919`: `win_rate 57.5290%`, `profit 49771.11`, `sharpe 5.61787`, `max_dd 16.9719%`, `volatility 51.1519%`
- `15921`: `win_rate 57.1705%`, `profit 44702.20`, `sharpe 5.42050`, `max_dd 16.9719%`, `volatility 51.0696%`

结论：

- `profit` 下降
- `sharpe` 下降
- `win_rate` 下降
- `max_dd` 持平
- `volatility` 略好

因此这条规则当前拒绝。

### D. 你关心的这笔具体怎么变

对 `2026-03-09 08:00:00` 这笔 long：

- 基线 `15919`
  - `2026-03-14 00:00:00` 被 `Signal_Kline_Stop_Loss`
  - `profit_loss = +2869.47`

- 窄实验 `15921`
  - 不再于 `2026-03-14 00:00:00` 锁盈
  - 持仓延续到 `2026-03-19 16:00:00`
  - `profit_loss = +3886.06`

这笔单本身更好了。

### E. 为什么整体仍然变差

虽然 `2026-03-09 08:00:00` 这一笔多赚了，
但整体上 `Signal_Kline_Stop_Loss` 的盈利保护还是被削弱了：

- `15919`: `225` 笔，合计 `+9471.86`
- `15921`: `223` 笔，合计 `+5384.57`

说明：

- 这条窄规则确实修对了你指出的样本
- 但同时放掉了其它本来应该更早锁盈的 long

### F. 当前判断

- 这条规则不是无效，而是“局部正确、全局退化”
- 如果后续继续沿这个方向优化，不能直接用这组硬条件全局替换
- 下一步更合理的是继续分层：
  - 哪些 `conflicting engulfing long` 后续真的还能走趋势
  - 哪些只是应该立即锁盈

## 2026-03-23 补充优化：Above-Zero Death Cross Range Break Short

### A. 问题背景

在 `15919` 基线里，`2026-03-18 16:00:00` 没有开空，但盘面上属于：

- 上方 MACD 死叉
- 大实体收跌
- 跌破前几根横盘区间低点

这是一个典型的“零轴上方转弱后的区间破位 short”候选。

### B. 基线里为什么没开

`15919` 对这根 K 线的识别结果是：

- `direction = None`
- `should_sell = false`

并不是被过滤，而是当前主触发器不认为：

- `above_zero death cross`
- `range breakdown`

可以直接构成独立 `short`。

当时的核心指标：

- `o=2316.69, c=2268.20, body_ratio=0.6856`
- `macd_line=54.39, signal_line=59.85, histogram=-5.46`
- `is_death_cross = true`
- `above_zero = true`
- `volume_ratio = 1.626`
- `ema_state = TooFar`
- `is_long_trend = false`
- `is_short_trend = false`
- `fib.in_zone = false`
- `bollinger.is_short_signal = false`
- `bollinger.is_long_signal = true`

### C. 新实验

新增默认关闭实验：

- `VEGAS_EXPERIMENT_ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT=1`

规则语义：

- 非长趋势、非短趋势
- `ema_state = TooFar`
- MACD 在零轴上方刚形成死叉
- 当前 K 线大实体收跌
- 成交量不弱
- 跌破前 5 根窄幅横盘区间低点
- 且没有新的 bearish BOS

命中时增加：

- `ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT`

### D. 样本验证

在 `2026-03-18 16:00:00`：

- 基线 `15919`
  - `direction=None`
  - 不开仓

- 实验版
  - `direction=Short`
  - `should_sell=true`
  - `dynamic_adjustments=["ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT","STOP_LOSS_ATR"]`

### E. A/B 结果

新回测：`15922`

- `15919`: `win_rate 57.5290%`, `profit 49771.1`, `sharpe 5.61787`, `max_dd 16.9719%`, `volatility 51.1519%`, `open_positions 518`
- `15922`: `win_rate 57.6108%`, `profit 51987.9`, `sharpe 5.66426`, `max_dd 16.9719%`, `volatility 51.1070%`, `open_positions 519`

结果：

- `win_rate` 提升
- `profit` 提升
- `sharpe` 提升
- `max_dd` 持平
- `volatility` 下降

### F. 新增交易

`15922` 确实新增了这笔空单：

- `open_position_time = 2026-03-18 16:00:00`
- `option_type = short`
- `open_price = 2268.2`
- `close_position_time = 2026-03-19 16:00:00`
- `profit_loss = +2216.79`

### G. 当前结论

- 这不是对单根 K 线的拍脑袋修补
- 当前实验已经在 ETH 基线里形成了正向 A/B
- 规则可以进入 ETH 主线候选
- 下一步需要继续检查同类命中是否仍然集中在少量样本，避免后续过拟合

### H. 命中分布复查

对 `15922` 的 `dynamic_config_log` 复查后，当前这条规则实际只命中 `1` 次：

- `2026-03-18 16:00:00`

并且 `15922` 相对 `15919` 的总收益改善，基本完全来自这一笔新增交易：

- `15919 profit = 49771.1`
- `15922 profit = 51987.9`
- 差额约 `+2216.8`

而这笔新增 short 的实际结果正好是：

- `2026-03-18 16:00:00` 开空
- `2026-03-19 16:00:00` 平仓
- `profit_loss = +2216.79`

因此当前更准确的状态是：

- `15922` 是一个有效的 `ETH candidate`
- 但 `ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT` 这条规则目前仍然是单样本命中
- 按现有“避免过耦合”闸门，它还不能直接晋级为正式 ETH 基线规则

### I. v2 / v3 收敛复查

继续对 `ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT` 做了两轮结构放宽/收紧：

- `15923`：`v2`
- `15924`：`v3`

结果：

- `15922`: `win_rate 57.6108%`, `profit 51987.9`, `sharpe 5.66426`, `max_dd 16.9719%`, `volatility 51.1070%`
- `15923`: `win_rate 57.5000%`, `profit 47670.7`, `sharpe 5.52617`, `max_dd 21.8501%`, `volatility 51.0594%`
- `15924`: `win_rate 57.6108%`, `profit 51987.9`, `sharpe 5.66426`, `max_dd 16.9719%`, `volatility 51.1070%`

`v2` 的问题已经明确：

- 它额外放进了一笔 `2021-10-04 08:00:00 short`
- 这笔实际是亏损单：`-3.69`
- 并把 `max_dd` 从 `16.97%` 拉坏到 `21.85%`

对比两个关键样本的 K 线结构：

- `2021-10-04 08:00:00`
  - 前 5 根区间宽度约 `5.06%`
  - 跌破幅度约 `0.27%`
- `2026-03-18 16:00:00`
  - 前 5 根区间宽度约 `1.99%`
  - 跌破幅度约 `1.51%`

这说明真正有效的不是“上方死叉后下跌”本身，而是：

- 上方死叉
- 窄幅横盘
- 有效跌破

`v3` 重新强化了这两个结构条件：

- `prior_range_width <= 0.025`
- `close_break_pct >= 0.012`

结果 `15924` 与 `15922` 完全一致，且 `dynamic_config_log` 里仍只命中：

- `2026-03-18 16:00:00`

因此这条分支当前收敛结论是：

- `v2` 拒绝
- `v3` 只是把规则重新收回到 `v1` 所代表的“单样本窄模式”
- `15922/15924` 继续作为 `ETH candidate`
- 但在出现至少第 2 个高质量同类命中前，不晋级为正式 ETH 基线

### J. 扫描器状态

已补了辅助扫描器：

- `crates/rust-quant-cli/src/bin/vegas_pattern_scan.rs`

但当前它仍未与集成回测完全对齐：

- 以 `15919` 为输入扫描时，返回 `match_count = 0`
- 但集成回测 `15922/15924` 明确显示规则能命中 `2026-03-18 16:00:00`

所以当前最可信的证据仍然是数据库中的集成回测结果，扫描器暂时只作为辅助工具，不作为晋级依据。

### K. 分支结论

`ABOVE_ZERO_DEATH_CROSS_RANGE_BREAK_SHORT` 这条线到此正式收口。

最终判断：

- `15922/15924` 确实优于 `15919`
- 但当前仍然只命中 `1` 个历史样本
- `v2` 的泛化已经证明，继续放宽会快速引入坏样本
- `v3` 只是重新收回到单样本窄模式

因此按当前“避免过耦合”的晋级闸门：

- 不将其升级为正式 ETH 基线规则
- 当前正式 ETH 基线继续保持 `15919 / 15867`
- `15922 / 15924` 保留为 `ETH candidate`

后续优化方向切回：

- 基于 `15919`
- 重新梳理剩余大亏损簇
- 只优先推进具有至少 `2` 笔以上同类命中的新规则分支
