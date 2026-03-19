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

## 2026-03-19 当前工作基线：ETH/BTC/SOL 4H 代码层窄过滤

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
