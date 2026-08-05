# 15m 放量锚点 RSI 背离反转 V1 评估清单

## 最小假设

两个相邻的方向性异常放量锚点之间，价格继续创新低/新高而 RSI14 不再恶化，可能代表卖压/买压衰竭。

本策略只检验“异常量锚点 + RSI 背离 + 价格拒绝/确认”。不读取历史 96 根净移动、MACD、EMA、平台、
布林带、BOS、FVG、CHoCH 或排名动量事件。

## 冻结身份

- 策略键：`market_volume_anchor_rsi_divergence_reversal_15m_v1`
- 产品 slug：`market-volume-anchor-rsi-divergence-reversal-15m-v1`
- 规则版本：`kline15m_filtered_vol2p5_anchor_rsi_wick_or_touch_fixed1r_v1`
- 研究预设：`research_market_volume_anchor_rsi_divergence_reversal_both_15m_v1`
- 状态：`research_unvalidated`
- 边界：Research-only，不注册 Paper/Live，不覆盖 V9～V13。

## 冻结入场

1. 当前已完成异常放量 K 为 `p`；在 `p` 前最多 48 根中选择最近的同方向异常放量 K 为 `q`，
   先固定最近锚点，再验证背离，禁止向前挑选有利样本。
2. `q/p` 分别按各自时点满足：
   - RSI14 做多侧 `<=30`，做空侧 `>=70`；
   - 因果过滤量比 `>=2.5`，过滤均量算法与动量衰竭家族相同；
   - `vol_ccy` 不低于各自此前连续 672 根的 nearest-rank P90。
3. 做多：`p.low < q.low` 且 `p.RSI14 >= q.RSI14`；做空完全镜像。RSI 相等允许。
4. `p` 的成交方式：
   - 方向性长下/上影线：紧邻下一根开盘成交；
   - 其他形态：只在紧邻下一根盘中严格越过 `p.high/p.low` 时成交；
   - 紧邻下一根未触发即过期。
5. 方向性长影线定义与动量衰竭家族完全相同。

## 冻结风险、成本与样本

- 初始止损：实际成交价反方向 `1.5 * ATR14[p]`。
- 止盈：实际成交价顺方向 `1.5 * ATR14[p]`，毛目标固定 `+1R`。
- 最长持仓 48 小时；单笔账户风险 1%；单边手续费 5 bps；单边滑点 3 bps。
- 不启用量比目标档位、趋势目标、移动止损或二次放量保护。
- 首轮使用 `sample_limit=60`、`sample_seed=top60_v36_direct_kline_20260721`、
  `1751328000000..=1784470500000`。
- current-live Top60 仅作受污染的诊断样本，不得视为样本外。

## 查看结果前冻结的报告

1. setup、成交、过期、影线开盘与下一根触价数量。
2. 整体、多空、顶/底背离分别报告交易数、币种、有效事件、胜率、净 EV、净 PF、净 R、Sharpe、回撤。
3. 报告入场后 1/2/4/8/16/32 根 K 的 MFE/MAE 与 `+1R/-1R` 先后次序。
4. 盈利和失败背离按 q/p 间隔、价格创新幅度、RSI 改善、两个锚点量比、周成交额分位进行分组，
   但不在看完结果后修改本版本阈值。
5. 改变 MACD、EMA、历史 96 根净移动不得改变信号。

未达到职业级联合门槛时保持 Research-only；任何阈值变化必须新增规则版本并重新冻结样本。
