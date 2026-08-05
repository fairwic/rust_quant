# 15 分钟动量 EMA144/576 双向独立资格动态回踩 V5：L1 预注册

## 当前等级与唯一变量

- 当前等级：`L1 快速研究`；本清单在 V5 扫描与任何结果回放之前冻结。
- V1/V2/V3/V4 的目标命中为 1/3、2/3、2/3、2/3，均保留独立失败报告。
- V4 已正确把 long qualification 刷新到持续 `EMA144<EMA576` 状态的末端，但后续一次 short price transition 会覆盖全局唯一的 active direction；第三张 BTC 图因此仍无法使用尚未过期的 long qualification。
- V5 唯一变量：把“全局只允许一个 active direction”改为“long/short transition latch 独立保存并各自按资格有效期过期”；另一个方向成立不删除当前方向的历史资格。
- 当前已完成 K 的价格位置负责单向选择：只有 `close>EMA576` 才能武装 long EMA144 回踩，只有 `close<EMA576` 才能武装 short EMA144 反抽，因此同一根收盘不会同时武装双向。

## 冻结身份与规则

- 候选键：`market_momentum_ema144_576_dual_active_dynamic_retest_15m_v5`
- 规则版本：`l1_age144_refresh_recent576_dual_active_price_side_reexpand075atr_first_touch030atr_v5`
- 资格：V4 的 144 根、持续刷新、离开后 576 根有效期不变。
- direction active：对应资格未过期时，连续两根完成 EMA576 方向突破并在 24 根内离开 0.75 ATR 后锁存；镜像方向成立不清除此锁存，只有本方向资格过期才清除。
- 回踩武装：long active 且已完成 K 同时满足 `close>EMA576`、`close-EMA144>=0.75ATR14`；short 完全镜像。
- 触发：下一根或更后的首次 `low<=上一根EMA144+0.30×上一根ATR14`；short 镜像。触发后解除本方向武装，必须重新扩张才能再次触发。
- 触碰 K 收盘守线与刺穿深度仅记录，不参与 L1 筛选。

## 数据、目标与门禁

- 三张目标、Top60 seed、720 根预热、评价窗口和 V4 完全相同。
- NMR `1782835200000..=1782878400000` long。
- BTC `1782943200000..=1782964800000` long。
- BTC `1783828800000..=1783850400000` long。
- L1 门禁仍为三目标全命中、候选至少 10、聚类事件至少 5、至少 4 个币种、3 个月、双向各至少 2、BTC/NMR 输入完整。
- 不读取未来 K、成交、MFE、MAE、退出、R、胜负或 PnL；通过后最多为 `coverage_pass_ready_for_l2_prereg`，不接入 Paper/Live/生产配置。
