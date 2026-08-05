# Vegas v2 Paper/ReadOnly 迭代记录（2026-07-19）

ETH 专用 v1/v2 对照请以 [Vegas ETH 4H v1/v2 同口径对照报告](./VEGAS_ETH_4H_V1_V2_SAME_SCOPE_REPORT_20260719.md) 为准；本文的 v27 数据只代表跨市场组合研究，不能替代 ETH v2 的晋级证据。

## 结论

本轮在 v27 固定信号上把风险再缩放到 70%，将保守盘中最大回撤压到 `9.48%`，且聚合净 EV、Profit Factor、Recovery 与 Sharpe 同时过线；但前向样本外仍无新交易，walk-forward 仍存在弱窗口。因此结论是：

- 允许 `eth_4h_id102_live_v2` 进入 Web 的 `paper_observing` / ReadOnly 观察通道。
- 禁止创建或恢复 `production_default` 指针。
- 版本化 v2 信号如果没有与 `production_default` 完全匹配，只进入信号 inbox 并记录 blocker，不生成 execution task。

## 策略身份边界

- `v27 + risk_scale=0.70` 是跨市场组合回放的研究观察版本，只用于证明统一风险缩放后组合回撤可以低于 10%；它没有原地覆盖任何现有 Vegas 配置，也没有进入默认实盘。
- `eth_4h_id102_live_v2` 是 ETH 4H 信号与 Web 发布合同版本。本轮只为它创建 Paper/ReadOnly manifest 和观察指针，不把跨市场 v27 的参数静默写入 ETH v2。
- 两条证据链必须分别晋级；跨市场回放通过聚合门槛，不代表 ETH v2 已具备生产默认消费资格。
- 本轮没有授权、触发或恢复真实下单。

## 初始止损与 R 合同

- 开仓时冻结 `initial_stop_price`，后续移动止损不得回写初始风险。
- 交易记录持久化 `initial_risk_amount`。
- 完整平仓后持久化成本后 `net_profit_r`。
- 同一根 K 线同时触发信号止损和最大亏损止损时，采用更紧的保护价，避免回测选择更差的退出价。
- 历史明细缺少新字段时，组合回放仅对已有 `stop_loss_update_history` 保留显式兼容；新明细使用直接字段。

## 成本压力与风险压缩结果

证据范围：`back_test_log.id=10280..10379`，v27 的 100 份 4H 配置，初始权益 100 U，最大并发 12。

固定压力条件：

- 原回测手续费保留。
- 额外单边滑点：5 bps。
- 每跨越一个 8 小时结算点：扣除 1 bps 资金费压力。
- v27 原始统一风险约 0.75%/笔。
- 二次风险缩放：`risk_scale=0.70`，等效约 0.525%/笔；不改入场、出场和候选排序。

组合结果：

| 指标 | 结果 | 门槛 | 状态 |
|---|---:|---:|---|
| 保守盘中最大回撤 | 9.48% | ≤10% | 通过 |
| Recovery Factor | 4.53 | ≥4 | 通过 |
| 日频 Sharpe（sqrt365） | 1.64 | ≥1.5 | 通过 |
| 净 EV | 0.693R | ≥0.6R | 通过 |
| Profit Factor | 2.235 | ≥2.2 | 通过 |
| 胜率 | 46.75% | 组合口径参考 | 观察 |
| 接纳交易数 | 154 | 需继续累计 OOS | 观察 |

风险缩放解决了聚合回撤问题，同时保留 v27 的净 EV 与 PF；当前晋级 blocker 已转为真实前向 OOS、历史币池完整性和滚动窗口稳定性，而不是聚合指标。

## 样本外与 walk-forward

- 严格前向 OOS 起点冻结为 `2026-07-16T12:00:00Z`。
- 当前回测结果在该时点之后没有新入场交易，因此 `out_of_sample=null`，不能宣称 OOS 通过。
- walk-forward 使用 12 个月训练隔离期、3 个月滚动测试期，模式为 `fixed_parameters_rolling_oos_no_refit`。
- 七个滚动测试窗中，第 2 个窗口为负收益且第 3 个窗口质量很弱；第 4 个窗口 PF 仍低于总门槛，说明稳定性尚不足，不得只汇总盈利窗口。
- Paper/ReadOnly 期间不得修改规则后继续沿用同一 OOS 标识；任何规则或风险合同变化都必须创建新版本并重置证据窗口。

## 发布与执行门禁

- Core 信号 payload 必须携带 `strategy_version` 和 `entry_rule_version`。
- Web v2 manifest 和商品 `core_strategy_version` 同步为 `eth_4h_id102_live_v2`。
- Web 只创建 `paper_observing` 指针，manifest 明确 `default_execution_consumption=false`、`promotion_eligible=false`。
- Web 生成执行任务时只认版本完全匹配的 `production_default`；Paper 指针不能越权。
- 历史无版本信号暂时保留兼容，是因为生产存量确实存在无版本 payload；新信号不得继续缺失版本。

## 订阅与仓位对账

- 会员来源 combo 到期后，在信号候选筛选前持久化为 `expired + signal_only`，并暂停通知，避免“数据库 active、执行查询已排除”的状态漂移。
- `exchange_position_flat` 新合同携带 `exchange + api_credential_id`。
- Web 收到只读 flat 证据后，只关闭该账户范围的持仓腿、清零预留数量并释放活动 reservation。
- 历史报告缺少账户范围时，只清理旧 position snapshot，不修改持仓腿账本，避免误清其他账户。

## 后续晋级条件

只有以下条件全部满足，才允许人工评审下一阶段；仍不等于自动恢复实盘：

1. 前向 OOS 至少 6 个完整自然月、50 笔组合交易、30 个有效市场事件。
2. 成本压力后净 EV ≥0.6R、PF ≥2.2、最大回撤 ≤10%、Recovery ≥4、日频 Sharpe ≥1.5。
3. walk-forward 各窗口不存在无法解释的结构性失效，并通过月份、方向、市场状态和头部贡献移除审计。
4. 订阅、凭证、signed read-only preflight、symbol filters、仓位与保护单对账全部 ready。
5. 通过显式 promote 创建生产指针；不得通过默认值、迁移副作用或旧 `live` 指针隐式恢复消费。
