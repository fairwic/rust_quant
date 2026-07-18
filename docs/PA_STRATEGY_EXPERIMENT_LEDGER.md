# 策略实验账本

## 账本规则

- 新实验必须在读取结果前固定实验 ID、假设、协议、市场、周期、数据指纹、Git HEAD、目标源码指纹和 dirty 状态。
- 同一实验 ID 只能消费一次数据。缺失身份、重复 ID、非法 p 值或未预注册的可晋级实验由代码账本拒绝。
- 同一批次的原始单侧 p 值统一执行 Holm-Bonferroni 校正；历史报告没有可靠 p 值时保持空白，不反推、不补造。
- `research_only=true` 永远不等于 Shadow、Paper 或 Live。归档只冻结研究方向，不删除历史代码和证据。
- 本文件中的旧实验是对已有报告的事实补录。旧报告没有保存 Git HEAD/目标源码指纹的条目明确标记为历史身份欠缺，因此不能作为晋级证据。

## PA Quant Tree 历史实验

| 实验 ID | 协议 / 范围 | 预注册 | 数据 / 源码身份 | 原始 p / Holm p | 已发生结论 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| `pa-v1-baseline` | v1 原始 PA trend/range，15m 与 1h，多市场训练 | 部分早期探索 | 报告保存部分数据指纹；未保存完整 Git HEAD 与目标源码指纹 | 空 / 空 | 原始候选基础成本后整体为负；不打开 OOS，不创建 Challenger | `archived_research` |
| `pa-v2-expected-r-threshold` | `pa-training-v2-expected-r-threshold` | 是 | 15m、1h 数据指纹见 v2 报告；目标源码身份未完整保存 | 空 / 空 | 修复 M2 固定阈值后，15m trend 验证均值仍为 -0.034R；不能进入 Shadow/Paper | `archived_research` |
| `pa-v3-funding-proxy` | `pa-training-v3-hyperliquid-funding-proxy` | 是 | 联合数据证据见 v3 报告；目标源码身份未完整保存 | 空 / 空 | 资金费率代理未改变失败方向；代理来源继续阻止 Promote | `archived_research` |
| `pa-v4-trend-quality` | `pa-training-v4-trend-quality-features` | 是 | 预注册和训练报告可追溯；旧输出只记录笼统 revision，缺少 scoped source fingerprint | 空 / 空 | 15m trend M2 出现弱正均值，但两倍成本、胜率、跨市场和置信下界失败 | `diagnostic_lead_only` |
| `pa-v5-followthrough` | `pa-training-v5-trend-followthrough`，15m/1h | 是 | 联合数据指纹和冻结 baseline JSON 哈希见 v5 报告；缺 scoped source fingerprint | 空 / 空 | 15m 平均 -0.414R、胜率 33.33%、PF 0.500、回撤 18.01%；1h 不足 100 笔 | `archived_research` |
| `pa-v6-selected-oof` | `pa-evaluation-v6-selected-oof-*` | 是 | 预注册、legacy JSON 哈希与报告可追溯；缺 scoped source fingerprint | 空 / 空 | v4 弱线索降级：15m trend OOF 两倍成本 -0.005R、bootstrap 下界 -0.238；停止新独立 PA 候选 | `diagnostic_lead_only` |
| `pa-v7-abc-counterfactual-once` | `pa-diagnostic-v7-abc-counterfactual`，15m A/B/C 同 setup 配对 | 是，且结果读取前冻结 | Git HEAD `d502eca668a84c4e1b8efb38fdb131234c3c5ba6`；scoped source `sha256:ae87ec802d43947551387a548f6c540a0e2b94b1183b50221e305a00dba8adfe`；dirty=true；完整数据身份见结果证据 | `1.0 / 1.0`、`1.0 / 1.0` | B-A +0.197R；C-B -0.337R；C 均值 -0.414R，10 项硬门禁失败，固定归档 PA 独立策略 | `archived_research` |

PA 历史证据来源：

- [PA 15m 初始训练报告](PA_QUANT_TREE_15M_TRAINING_REPORT.md)
- [PA 1h 365 天训练报告](PA_QUANT_TREE_1H_365D_TRAINING_REPORT.md)
- [PA v2 期望阈值报告](PA_QUANT_TREE_EXPECTED_R_THRESHOLD_V2_REPORT.md)
- [PA v3 Funding 代理报告](PA_QUANT_TREE_FUNDING_PROXY_V3_REPORT.md)
- [PA v4 趋势质量报告](PA_QUANT_TREE_TREND_QUALITY_V4_REPORT.md)
- [PA v5 Followthrough 报告](PA_QUANT_TREE_FOLLOWTHROUGH_V5_REPORT.md)
- [PA v6 评估纠偏报告](PA_QUANT_TREE_EVALUATION_V6_REPORT.md)
- [PA v7 A/B/C 预注册](PA_QUANT_TREE_ABC_V7_PREREGISTRATION.md)
- [PA v7 A/B/C 结果报告](PA_QUANT_TREE_ABC_V7_REPORT.md)
- [PA v7 决策级聚合证据](evidence/pa_quant_tree_abc_v7_result.json)

## 其他近期独立策略研究

| 实验 ID | 范围 | 数据 / 源码身份 | 原始 p / Holm p | 已发生结论 | 状态 |
| --- | --- | --- | --- | --- | --- |
| `keltner-scalper-research-20260708` | BTC/ETH 1m，并扩展 5m/15m；反转、确认、趋势、波动、出场等对照 | 报告记录市场、K 线数量和费用；未保存 scoped source fingerprint | 空 / 空 | 1m 最佳高频候选仍为负，5m 合并负收益，15m 低频且 ETH 不稳；默认入口冻结 | `archived_research` |
| `smc-v1-research-20260708` | BTC/ETH/SOL 5m/15m 结构、回踩、FVG、sweep 与 Market Velocity 组合观察 | 报告记录 case 和样本口径；未保存 scoped source fingerprint | 空 / 空 | 局部低频候选可达门槛，但跨市场高频候选显著亏损；不作为默认过滤层或 paper preset | `archived_research` |
| `range-breakout-drop-20260709` | BTC 4H 多窗口与多轮参数尝试 | 报告记录 2,000/5,000 K 线切片；未保存 scoped source fingerprint | 空 / 空 | 短切片正收益未能跨窗口稳定，长窗口胜率/回撤不达标；停止继续调参 | `archived_research` |
| `mv-long-prod-preset-replay-20260716` | 精确复跑生产 Long preset `research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1`；固定 20 市场样本 | 独立回放库 `quant_core_mv_replay_20260716`：`market_rank_events=5,287,535`，范围 `2026-05-27T03:29:52Z`—`2026-07-06T10:16:18Z`，314 symbols；运行时源码 dirty，未作为发布身份 | 空 / 空 | 46,801 个原始候选中 784 个通过信号门禁，但执行通过为 0；714 个未在 24 根 15m 内回补 FVG 50%，70 个没有近期有效 FVG。该 preset 在本样本上是机制性 0 入场，不得推进 Live | `blocked_zero_executable_entries` |
| `mv-breakdown-short-v6-replay-20260716` | 精确复跑 `research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6`，`raw_state`，参数不调优 | 同一独立回放库；122 个 candle pairs、454 个候选；修复 raw event 任意毫秒时间无法进入框架权益回放的基线缺陷后，以 21 个 focused tests 固定合同 | 空 / 空 | 43 笔；48h 完整胜率 58.14%、resolved 胜率 60.98%；框架胜率 62.79%、最大单 symbol 回撤 8.11%、28 symbols 每个 100U 的隔离资金合计利润 37.16U。样本少于预注册 50 笔；去 Top3 后胜率 57.14%，Top5 贡献 83.08% 利润，且尚无显式滑点，严格门禁失败 | `paper_only_strict_gate_failed` |

其他策略证据来源：

- [Keltner 迭代日志](BTC_ETH_STRATEGY_FAMILY_ITERATION_LOG.md)
- [SMC 迭代日志](SMART_MONEY_CONCEPTS_ITERATION_LOG.md)
- [策略暂停评估](../crates/strategies/STRATEGY_ITERATION_PAUSE_EVALUATION.md)

## 生产证据审计

生产证据审计不计算策略收益，也不参与模型晋级。它只回答版本、数据、readiness、执行和风控证据是否能在同一时间截面闭环。

| 审计 ID | 范围 | 身份 / 时间截面 | 真实运行证据 | 真实阻塞原因 | 状态 |
| --- | --- | --- | --- | --- | --- |
| `prod-mv-vegas-chain-audit-20260715` | Market Velocity / Breakdown Short / Vegas，从行情到 Web 回写 | Core revision `d502eca668a84c4e1b8efb38fdb131234c3c5ba6`；Web revision `54f535973d83edffbbd7585dbfc0a85d059d764f`；生产只读审计窗口 `2026-07-15T13:18:48Z`—`13:36:18Z` | Core/Web 容器运行且 restart=0；rank 快照约 15 秒新鲜；OKX symbol filters 小于 1 小时；ETH 4H 最新已确认 K 线为 `12:00Z`；execution worker 每约 5 秒轮询，lease limit=1；历史 Vegas task 68 已成交且当时保护单确认 | 主 MV 近 7 天 `29,211` 条 handoff 全 blocked；short `409` 条全 blocked；主 handoff 固定绑定已过期 combo 4，当前有效 combo 7 的 signed preflight 已过期；short 的运行 slug、产品 slug 和订阅 slug 不一致，DRAFT 产品却有 production pointer；BTC/ETH/SOL 15m K 线分别滞后约 10/10/25 天；历史 Vegas 仓位腿仍为 active/confirmed，但 7 月 6 日后没有当前 signed position/open-order reconciliation，execution-result delivery 仍为 pending | `blocked_by_runtime_contract_and_evidence_drift` |

完整证据见 [Market Velocity / Vegas 生产证据审计](MARKET_VELOCITY_VEGAS_PRODUCTION_AUDIT_20260715.md) 和 [机器可读审计快照](evidence/market_velocity_vegas_production_audit_20260715.json)。

## 当前验证状态与唯一下一增量问题

| 实验 ID | 协议 | 假设 | 固定通过条件 | 固定失败条件 | 状态 |
| --- | --- | --- | --- | --- | --- |
| `prod-vegas-open-leg-readonly-reconciliation-20260715` | 对历史 Vegas task 68 / combo 2 / ETH-USDT-SWAP 做一次 signed read-only position + open orders + recent fills 对账；禁止创建任务、下单、撤单、平仓、写回或重启 | Web 中仍标记为 active 的 0.02 ETH long，要么在交易所仍持有且有覆盖剩余仓位的有效止损，要么已由交易所成交事实关闭；不存在“Web 显示 active/confirmed，但交易所仓位或保护单已漂移”的第三种状态 | 单一身份命中；快照生成时间不超过 5 分钟；若仓位存在，方向/数量一致且有效保护单覆盖剩余数量；若仓位为零，必须有对应 close fill / protective-order 终态证据；输出只读差异报告且 mutation count=0 | 凭证或身份歧义、快照过期、仓位数量不一致、仓位存在但无有效保护、仓位为零但无 close fill、保护单孤立、请求失败或任何 mutation 尝试，任一即失败并停止 | `failed_before_exchange_probe_mutation_safe_path_unavailable` |
| `mv-breakdown-short-v6-forward-robustness` | 固定 v6 全部策略参数，不读取结果调参；只追加 `2026-07-06T10:16:18Z` 之后生产可见的 raw_state/K 线，先补显式滑点合同，再累计到至少 100 笔有效交易 | 当前正收益是否能在新增样本、完整超时口径和去集中度后继续成立，而不是由少量头部 symbol 或 resolved-only 口径造成 | 有效交易不少于 100；完整胜率不少于 60%；最大回撤小于 15%；费用与预注册滑点后净收益为正；去 Top3 后完整胜率仍不少于 60% 且净收益为正；前后时间半区均不为负 | 任一通过条件失败，或读取结果后修改参数、滑点、样本边界、超时定义；失败即保留 Paper/研究态，不进入 Live | `preregistration_required_not_started` |

2026-07-16 执行前门禁发现：生产 Core revision `d502eca668a84c4e1b8efb38fdb131234c3c5ba6` 的 reconciliation runtime 必须使用内部密钥解析精确 credential，但同一密钥存在时又无条件向 Web `POST /api/commerce/internal/exchange-account-snapshots`。`RECONCILIATION_SNAPSHOT_REPORT=false` 只关闭差异报告，不能关闭账户快照写回。因此现有入口无法同时满足“精确凭证解析”和 `mutation count=0`，验证在任何交易所 signed 请求之前失败并停止；本次验证 signed exchange request count=0、Web write request count=0、mutation count=0，未获得新的仓位或保护单事实。机器证据见 [Vegas open leg 只读对账执行门禁](evidence/vegas_open_leg_readonly_reconciliation_20260716.json)。

该失败不授权临时代理、直连脚本、代码修复、部署或生产写回。历史 active 仓位仍未完成当前 signed reconciliation；在另行批准并发布可显式关闭全部写回的 Core 入口前，不再运行本验证，也不选择新的策略机制或参数问题。

`pa-v7-abc-counterfactual-once` 已且仅执行一次。固定门禁失败后结论为 `archive_pa_standalone`；不得重复运行、改门槛、打开密封 OOS、进入 Paper，或借 PA Meta-filter 继续该独立策略路线。
