# 15 分钟动量 EMA144/576 永续历史资格回踩挂单 V6：L1 预注册

## 当前等级与最终语义校正

- 当前等级：`L1 快速研究`；本清单先于 V6 扫描与任何结果读取冻结。
- V1～V5 均因目标定义不完整停止，报告保留；未执行任何收益筛选。
- 用户的最终硬语义是“只要之前 EMA144 在 EMA576 下方够久，就可以触发；反之镜像”。该语义没有资格过期时间，也没有要求价格在触及 EMA144 前必须持续收在 EMA576 同侧。
- V6 唯一生命周期变量：历史 qualification 与已经完成的方向 transition 不再按 576 根过期；回踩武装后不因中途收回 EMA576 另一侧而取消，只在触碰后消费，或由另一个方向后来完成重扩张并武装时替换。
- 144 根资格、两收盘突破、24 根内 0.75 ATR 离开、EMA144 方向重扩张 0.75 ATR、上一根 EMA144/ATR14 的 0.30 ATR 首次触碰全部保持。

## 冻结身份

- 候选键：`market_momentum_ema144_576_persistent_qualification_order_retest_15m_v6`
- 规则版本：`l1_age144_persistent_dual_transition_latest_arm_reexpand075atr_first_touch030atr_v6`
- long 与 short qualification 独立，一旦连续 144 根成立即保持；数据不连续或必要指标失效时状态失败关闭并重新建立。
- long 与 short effective transition 独立保持；另一个方向成立不删除历史 transition。
- 已完成 K 位于 EMA576 上方且相对 EMA144 扩张 0.75 ATR 时武装 long；下方镜像武装 short。新武装会替换另一方向尚未触发的旧武装，保证同一时刻只有一个待触发方向。
- 武装后使用每根上一根已完成 EMA144/ATR14 更新因果锚点，首次触碰 0.30 ATR 区形成候选；中途穿越 EMA576 不撤单。
- 触碰 K 是否守住 EMA144 只记录，不参与 L1 门禁。

## 数据、目标与门禁

- 三目标保持：NMR `1782835200000..=1782878400000` long；BTC `1782943200000..=1782964800000` long；BTC `1783828800000..=1783850400000` long。
- OKX USDT 永续 15m、`confirm='1'`、Top60 seed、720 根预热、评价窗口均与前序版本一致。
- L1 门禁保持三目标全命中、候选至少 10、聚类事件至少 5、至少 4 币种、3 个月、双向各至少 2、BTC/NMR 输入完整。
- L1 不读取未来 K、成交、MFE、MAE、退出、R、胜负或 PnL；通过后最多为 `coverage_pass_ready_for_l2_prereg`，不接入运行时。
