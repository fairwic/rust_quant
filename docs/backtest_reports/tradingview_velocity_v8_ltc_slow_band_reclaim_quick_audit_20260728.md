# TradingView Velocity V8 LTC 慢均线带收复空单门禁快速审计

## 结论

- LTC `2026-07-26 05:30 Asia/Shanghai` 不是刚站上内部 EMA696，而是信号棒
  本身放量重新收复图上 EMA596，并同时站上
  `max(EMA596, EMA696)` 的慢均线带上沿。
- V8 只阻断普通“RSI 超买 + 长上影”空单；RSI 顶背离、二次扫高、假突破、
  EMA 空头和努力无结果等独立结构空单不变。
- TradingView 同一 LTC 图表快速对照中，目标亏损单被移除，净利润与 PF
  改善，最大回撤不变。
- 按用户要求，本轮停止数据库复审与正式 60/60 回放。当前结果只能证明目标
  LTC 图表和 Rust 固定规则一致，V8 仍是 Research-only，不能解释为跨币种
  晋级或生产可用。

## 1. 四层认知框架

### 已知的已知

- 信号棒 OHLC 为 `46.46 / 46.80 / 46.46 / 46.58`；
- `vol_ccy=23049`、过滤量比 `9.490995x`、RSI14 `72.0490`、ATR14
  `0.109806`、上影占振幅 `64.706%`；
- EMA596 为 `46.477084`，EMA696 为 `46.425698`，收盘同时位于两者上方；
- 下一根开盘 `46.57`，原空单最大有利波动约 `0.61R`，随后在 `46.80`
  止损。

### 已知的未知

- 该门禁在其他币种与月份中删除的普通超买空单是否仍为负期望；
- 五根保护窗口在慢均线下降或剧烈波动状态下是否会误删有效衰竭空单。

### 未知的已知

- 主图紫线实际是 EMA596，Pine 只在内部计算 EMA696；
- 字面 EMA696 已连续站上六根，若只按“EMA696 最近五根上穿”实现，反而
  无法命中目标；
- 现有多头切换保护检查 EMA12 对 EMA144/596 的交叉，不检查价格收复整个
  慢均线带，所以原规则漏掉该单。

### 未知的未知

- 15m OHLC 无法还原同棒内先突破还是先形成上影；
- 单一 LTC 已见失败样本可能存在个例偏差，不能替代冻结后的 forward shadow。

## 2. V8 规则

```text
slow_band_upper = max(EMA596, EMA696)

strong_bullish_volume_impulse =
    volume_event
    && filtered_volume_ratio >= 6
    && close > open
    && bullish_body >= 1 ATR

fresh_slow_band_reclaim =
    strong_bullish_volume_impulse 完成收盘上穿 slow_band_upper
    && 当前至后4根每根完成收盘仍高于当根 slow_band_upper

只阻断：
    普通 rsi_overbought_pattern
    && long_upper_shadow
```

任一完成棒收盘回到慢均线带上沿或以下，保护立即解除。窗口边界为收复棒
`age=0` 到 `age=4`，`age=5` 放行。

## 3. TradingView 快速对照

同一 `OKX:LTCUSDT.P`、15m 图表、相同脚本参数与当前已加载历史：

| 指标 | V5 `a36f0e19` | V8 `252225ec` | 变化 |
|---|---:|---:|---:|
| 交易数 | 18 | 17 | -1 |
| 盈利 / 亏损 | 7 / 11 | 7 / 10 | 删除 1 笔亏损 |
| 净利润 | -0.06 USDT | +0.17 USDT | +0.23 USDT |
| Gross loss | 3.37 USDT | 3.14 USDT | -0.23 USDT |
| Profit Factor | 0.9822 | 1.0541 | +0.0719 |
| 胜率 | 38.89% | 41.18% | +2.29 个百分点 |
| 最大回撤 | 1.45 USDT | 1.45 USDT | 不变 |

TradingView Pine 编译为 0 错误，V8 已重新加载到 LTC 图表，原目标开空箭头
不再出现。

## 4. Rust 同源验证

- V8 fixed LTC：V5 生成 `rsi_overbought_pattern`，V8 以
  `V8_RSI_OVERBOUGHT_UPPER_WICK_FRESH_SLOW_EMA_BAND_RECLAIM_5`
  阻断；
- `age=0/4` 阻断、`age=5` 放行；
- 等于慢均线、仅影线刺穿、跌回后重启和看跌吞没-only 均有固定测试；
- V8 focused `5/5`、完整 parity `91/91`、strict CLI `4/4` 通过；
- Pine UTF-16 FNV-1a 为 `252225ec`，快照 SHA256 为
  `467c746a2b44956dbe3c3b493b1099825170c56a2864d5b740aa1dfee56829ff`。

## 5. 状态

```text
research_only_active_on_tradingview_quick_ltc_improvement
formal_cross_symbol_evaluation_deferred_by_user
not_promoted_to_paper_or_live
```

没有下单、撤单、平仓或任何交易所 mutation。
