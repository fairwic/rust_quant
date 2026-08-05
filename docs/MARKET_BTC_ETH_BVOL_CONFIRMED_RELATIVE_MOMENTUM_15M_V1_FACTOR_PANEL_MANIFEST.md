# BTC—ETH BVOL 确认相对动量 15m V1 因子面板清单

## 1. 身份与唯一假设

- `factor_key=market_btc_eth_bvol_confirmed_relative_momentum`
- `version=15m_v1_20260721`
- `rule_version=price24h_bvol24h_opposite_rank_6h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在下载完整档案和计算任何 forward outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

前述 BOS/FVG/CHoCH、funding/OI/taker、订单簿、基差、premium、横截面动量/carry 和
positioning 因子均没有稳定净优势。本轮只验证一个新的期权市场假设：若 BTC/ETH 过去
24 小时的价格强弱与隐含波动率重定价方向相反，即较弱资产的 BVOL 扩张更多，则期权市场
对弱者的风险定价可能确认其下一段相对弱势。

## 2. 冻结数据、时序和方向

- 数据窗口固定为 `[2023-06-01 00:00:00 UTC, 2024-11-01 00:00:00 UTC)`；
- 只使用当前仍 live 的 `BTCUSDT`、`ETHUSDT` USD-M 永续，不涉及退市币；
- 期权因子使用 Binance 官方 daily `BTCBVOLUSDT/ETHBVOLUSDT BVOLIndex` 与 SHA-256；
- 价格和执行使用 Binance 官方 regular `BTCUSDT/ETHUSDT 15m` 月包与 SHA-256；
- 不使用 1m；BVOL 原始 1 秒只作为信号时点已发布的外生期权指标，所有价格入场、持有、
  outcome 和未来策略执行周期均为 15m；
- 每个 UTC `00:00/06:00/12:00/18:00` 决策 `T`，只读取 `T-1s` 与 `T-24h-1s`
  的 BVOL；官方毫秒偏移最多允许归一到同一秒内，缺点即阻塞，不使用未来最近值；
- 价格强弱使用 `T-15m` 已完成收盘相对 `T-24h-15m` 已完成收盘的对数收益；
- `price_diff=BTC_return-ETH_return`，`bvol_diff=BTC_BVOL_change-ETH_BVOL_change`；
- 仅当 `price_diff*bvol_diff<0` 时为因子组：做多价格较强、BVOL 扩张较少的一腿，做空
  价格较弱、BVOL 扩张较多的一腿；
- `price_diff*bvol_diff>0` 为价格动量方向相同但期权不确认的对照组；任一差为零时跳过；
- `T` 的 15m 开盘入场，主 outcome 为 `T+6h` 开盘，辅助 outcome 为 `T+24h` 开盘；
  窗口内任意 15m 缺口阻塞；收益为等名义 `long_return-short_return`；
- 不使用 BVOL 水平阈值、z-score、分位数、ATR、EMA、MACD、BOS/FVG/CHoCH、funding、
  OI、taker、订单簿或币种筛选。

Discovery 固定为 2023-06～2024-01 八个月，Validation 固定为 2024-02～2024-10
九个月。不得根据结果调换方向、日期、6h 间隔或 24h 窗口。

## 3. 覆盖、晋级和停止门槛

先做不含 outcome 的覆盖审计：BTC、ETH 在整个窗口各月 15m 完整，BVOL 有效日占请求日
至少 95%，并形成至少 1,800 个可计算决策点；任一失败则停止，不计算收益。

覆盖通过后只运行一次冻结 outcome，并同时满足：

- 因子组不少于 1,000，Discovery/Validation 各不少于 450；
- 17 个完整月中每月因子组频率位于 `50～120`，至少 12/17 月 6h 毛价差为正；
- 总体、Discovery、Validation 的 6h 毛价差都大于四腿标准成本 `32bps`，标准成本后
  平均净价差都至少 `+20bps`，6h 命中率都至少 55%；
- 三个窗口相对各自对照组的 6h 增量都至少 `+25bps`；
- 24h 只作方向持续性诊断，不得在 6h 失败后改用 24h 晋级；
- 双倍交易成本 `64bps` 后总体仍为正。

任一核心门槛失败则 V1 永久淘汰，不反向、不扫描 BVOL 变化周期、价格周期、决策频率、
持有期或阈值。只有因子面板全部通过，才允许另立策略版本，加入实际 funding、固定初始
止损/R、统一资金、容量和保护单，并进一步满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤
`<=10%`、Recovery `>=4`、Sharpe `>=1.5`。本面板不授权 Paper、Live、部署或交易 mutation。

## 4. 一次性结果与决策

首次覆盖运行发现实现把同一秒内的官方毫秒偏移错误限制为 100ms，而第 2 节冻结合同允许
归一到其所属整秒。代表文件仅有一行位于该秒 `829ms`，没有跨秒或未来借值；在任何 outcome
被读取前修正为整秒归一，并新增 `1ms/999ms/下一整秒` 回归测试。

修正后 regular 15m 月包 `38/38` 有效，共 `111,360` 根；BVOL 日包请求 `1,040`，有效
`987`、官方缺失 `50`、存在但缺少四个决策前点 `3`，文件覆盖率 `94.90%`，低于预注册
`95%`。`2,076` 个决策点中可计算 `1,943`，其中因子组 `870`、对照组 `1,073`；虽然总可
计算点超过 `1,800`，因子组也低于后续要求的 `1,000`。

结论：`coverage_gate_passed=false`，V1 按预注册规则停止。程序没有读取任何 `T+6h/T+24h`
outcome，全部收益字段保持空值；不把频率改为 4h、不缩短窗口、不删除缺失日，也不另立方向
相反或阈值版本。本面板未进入 Paper/Live、部署或交易 mutation。
