# Market Momentum Opposite Move v14 排名动量去价格幅度重复门禁评估清单

## 1. 策略身份与单一假设

v14 仍是同一个策略的版本迭代，不创建独立研究策略身份：

- 策略键：`market_momentum_opposite_move_reversal`
- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_volume_rank_no_price_floor_v14`

v13 已用严格时序的滚动 24 小时近似 quote 成交额排名跃升定义 market 动量，但又
沿用旧伪事件的“当前 15m K 线实体至少 0.8%”门禁，导致 22 币一年开发窗只有 34 个
原始事件和 0 笔成交。v14 只删除这一个重复门禁：`min_price_change_pct=None`；排名
阈值仍为 `delta_rank >= 3`，极端实体上限仍为 8%。

当前 K 线仍必须非十字星，并由既有方向/延续等待逻辑处理。反向净幅度或长时间分支、
历史极值量能优势、反转确认、均线收回、止损、Volume-ATR 止盈和成本全部保持 v13，
不扫描其他参数。

## 2. 时序、Universe 与停止规则

排名构造、完整 universe 快照和有效事件聚类完全沿用 v13。开发 universe 仍为编码前
已有 15m 表的 22 个当前存活 USDT 永续，属于已见数据并有幸存者偏差；开发结果固定
`promotion_eligible=false`。

查看结果前冻结停止规则：

- 少于 20 个有效事件：样本不足，停止；
- 扣费后固定初始 R 的 EV `<=0R`、PF `<=1`、半数及以上成交成员亏损，或后半段
  EV `<=0R`：淘汰；
- 通过最低开发门槛后才回填 v12 已冻结且尚未查看的 unseen v4，不按结果删币；
- 正式晋级仍须满足净 EV `>=0.6R`、净 PF `>=2.2`、统一组合最大回撤 `<=15%`、
  Recovery `>=4`、Sharpe `>=1.5`，以及主项目要求的全部稳健性和实盘约束。

## 3. 执行结果

- 22 币完整排名开发窗产生 63 个原始事件；58 个未通过历史反向结构或基础量能门禁，
  5 个进入执行确认，但没有形成可成交入场。
- 实际成交与有效事件仍均为 0，低于预登记的 20 个有效事件下限，判定样本不足。
- v14 不进入 unseen v4，不写入 `back_test_log` / `back_test_detail`，也不进入
  Paper/Live。

删除价格幅度下限只增加了未通过结构门禁的事件，没有增加有效 setup。当前剩余的尺度
偏差是：生产扫描器在全市场排名中持久化 `delta >= 3`，而 22 币内部上升 3 名已占
横截面 13.6%。若继续，下一版本只能预登记“22 币小 universe 最小可分辨跃升 1 名”
这一项修正，不得同时改变其他规则。
