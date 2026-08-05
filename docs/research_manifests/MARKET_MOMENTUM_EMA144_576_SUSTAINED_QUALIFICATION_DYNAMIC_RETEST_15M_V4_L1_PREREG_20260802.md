# 15 分钟动量 EMA144/576 持续资格刷新动态回踩 V4：L1 预注册

## 当前等级与唯一修正

- 当前等级：`L1 快速研究`；本清单先于 V4 扫描及任何收益读取冻结。
- V1/V2/V3 分别只匹配 1/3、2/3、2/3 用户目标，均保留 `rejected_definition_mismatch` 报告。
- V3 的 576 根资格年龄从连续状态首次达到第 144 根时开始；对于之后仍连续位于同侧数百根的 EMA 状态，这会在均线真正离开该侧之前提前过期，不符合“之前在下面够久”的持续状态语义。
- V4 唯一变量：连续状态达到 144 根后，只要 `EMA144<EMA576` 仍成立，就用每根新完成 K 刷新 long qualification；short 镜像。离开该侧后才开始累计最多 576 根资格年龄。
- V3 的双向独立资格、576 根上限、价格转换、0.75 ATR 离开、0.75 ATR 重扩张、前一根 EMA144/ATR14 的 0.30 ATR 首次触碰与去重全部保持不变。

## 冻结身份

- 候选键：`market_momentum_ema144_576_sustained_qualification_dynamic_retest_15m_v4`
- 规则版本：`l1_age144_refresh_while_sustained_recent576_transition_reexpand075atr_first_touch030atr_v4`
- 资格刷新不是结果过滤：它只改变信号时点可见的 qualification timestamp，不读取触碰后的任何价格。

## 其余规则、数据与门禁

- 其余因果规则完全继承 V3 预注册。
- 三个目标窗口保持 NMR `1782835200000..=1782878400000` long、BTC `1782943200000..=1782964800000` long、BTC `1783828800000..=1783850400000` long。
- OKX USDT 永续 15m、`confirm='1'`、Top60 seed、720 根预热、评价窗口 `2025-07-01 00:00 UTC` 至 `2026-07-19 14:15 UTC` 均不变。
- L1 门禁保持：三目标全命中、候选至少 10、聚类事件至少 5、至少 4 个币种、3 个月、双向各至少 2、BTC/NMR 输入完整。
- L1 不读取未来 K、成交、MFE、MAE、退出、R、胜负或 PnL；通过后最多进入 `coverage_pass_ready_for_l2_prereg`，不接入运行时。
