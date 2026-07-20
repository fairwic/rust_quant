# Agent 进度：PA Quant Tree

## 2026-07-20：交易系统目标架构文档按最新方案更新

- 目标目录改为 `apps` 加 `crates/{domains,quant,contracts,adapters,platform}`，默认一个 Contracts crate、一个按 owner module 隔离的 Postgres Adapter crate；不预建空 crate。
- 将含义过宽的 Operations 改为 Reconciliation；`signal-worker` 只产生 StrategySignal，用户账户级 Portfolio 和事前 Risk 等 Web ExecutionRequest 带回稳定账户上下文后由 `execution-worker` 装配，持续 Risk 初期可由 account-worker 装配。
- 新增业务代码与数据访问规范，固定 Command、Query、Event Consumer 三条调用链，并明确 SQL/Row/事务只在 Postgres Adapter，Use Case 只定义业务原子性。
- 明确 Web `execution_tasks` 的目标语义是 ExecutionRequest 商业交接；OrderIntent、Order、Fill、Protection 和 Reconciliation 的唯一事实源迁到 Core，Web 只保留结果投影。
- 将 Strategy Manifest 拆为 Definition、StrategyArtifact、Release、RuntimeSnapshot；ResearchEvidence 改由独立 Research Domain 拥有，Release 只引用已完成证据。
- 补齐部分成交保护、撤单/成交竞态、Unknown 与最大未保护窗口；新增 AI 放置声明、Golden Template、架构门禁 ratchet 和 ADR-0007。
- 以 ADR-0009 取代 ADR-0008：`quant/backtest` 只保留确定性时钟、事件调度、回放、成交与成本模型；Experiment、Run、Checkpoint、SimulationLedger 和证据发布全部归 Research。
- 将模拟分为 ResearchBar、PaperEvent、RecoveryHarness：前者验证策略表现，中者验证订单事件，后者验证 lease/outbox/Unknown/对账恢复，禁止拿普通回测冒充生产恢复验证。
- 重写 `vegas-backtest-migration.md`，把当前 HTTP -> Runner -> Executor -> Pipeline -> Service -> SQL 主链映射成 Research 控制流和逐 decision-time 事件循环。
- 明确 Vegas `StrategyEvaluationStateKey = EvaluationScopeId + StrategyRuntimeSnapshotId + MarketStreamPartition`；记录当前 7000/4000/300 窗口、无 config/version 缓存键和仅 Universal 进行缺口检查等 parity 风险。
- 多币种回放必须先收集同一 `decision_time` 的全部候选，再统一进入 Portfolio 排序、净额、容量和 Risk；随机 symbol 输入顺序不得改变结果。
- Research 使用独立 SimulationLedger，不写生产 AccountProjection；Evidence 对象先上传，再由 Research owner 数据库事务一次发布 manifest、指标引用、幂等记录和 `Completed`，只承诺原子可见。
- 将历史 `position_leverage=0.58` 解释为 Portfolio `allocation_ratio`，与真实交易所 leverage 分开；Strategy 只输出候选失效价，最终仓位、止损与审批归 Portfolio/Risk。
- 本轮只修改架构文档并新增项目级架构 Skill，没有创建业务 crate、实现 `xtask`、迁移数据库、修改运行链路或触发任何交易动作。

下一步：先只读冻结依赖、Contract、表 owner、运行入口、Vegas 逐事件基线与随机 symbol 顺序基线；再建立最小 Research owner 和只拦新增违规的 `arch-check`。随后先迁移 Vegas Evaluator 与 ResearchBar 单资产切片，再扩展多资产 barrier 和 PaperEvent，不直接把完整生产 OMS 搬进普通回测。

## 2026-07-15

- 完成 BTC、ETH、SOL、BCH 四市场 365 天 15m/1h 已确认 K 线补齐与连续性核对。
- 完成 15m 独立训练：趋势与区间策略均为负期望，无合格 Challenger。
- 完成研究 CLI 周期隔离，`--timeframe 1h` 使用独立策略 key、manifest 和数据指纹。
- 完成 1h 独立训练：趋势 879 笔、平均 -0.096R；区间 193 笔、平均 -0.384R，无合格 Challenger。
- 保持密封 OOS 未打开、资金费率未接入、`promotion_eligible=false`，未修改 Vegas 或生产执行路径。

下一步：修正逻辑回归的成本后期望目标与阈值选择，接入历史资金费率，并在新训练协议预注册后再执行下一轮训练。

## 2026-07-15：训练协议 v2

- M2 逻辑回归改为训练折内按成本后平均 R 选择 keep 阈值，并设置最小有效样本量。
- PA analytics focused tests 为 17/17 通过。
- 独立重跑 15m/1h：15m趋势选择M2但验证均值仍为 -0.034R，其余分支选择M0，全部禁止晋级。
- 核对 `quant_core.funding_rates`：四市场均为0行；现有91天TSV不足以覆盖全年。

下一步：先补齐全年资金费率事实，再实现逐持仓资金费率累计和完整成本重跑。

## 2026-07-15：资金费率代理 v3

- 新增分源 funding backfill CLI；Binance 因451不可用后，只保留 Hyperliquid 单一来源。
- Hyperliquid 15m窗口每市场8,760点、1h窗口8,759点，小时桶无缺口。
- 历史结算器按持仓小时累计绝对资金费率，新增保守代理成本测试。
- v3 训练仍全部负期望，最优 `pa_trend_15m` M2 验证均值为 -0.040R。
- 未打开密封OOS，未修改Vegas或生产执行路径。

下一步：进入阶段12，先预注册趋势入场质量特征与候选结构假设。

## 2026-07-15：趋势质量 v4

- 预注册并实现 Feature Registry v2 的4个趋势质量特征。
- 修复模型特征维度与梯度维度不一致，并新增回归测试。
- 15m趋势 M2 walk-forward 平均转为 +0.090R，但标准误0.117R。
- 全训练两倍成本为 -0.079R，BTC/ETH为负，未达到60%胜率和跨市场稳定门槛。
- v4不进入Shadow/Paper，密封OOS继续关闭。

下一步：阶段13设计跟随确认候选的新策略版本合同，保留v1/v4全部证据。

## 2026-07-15：趋势跟随确认 v5

- 新增独立跟随确认策略键和 `t -> t+1 -> t+2` 确定性候选，不覆盖原趋势策略。
- 15m有效样本255笔，合并平均 -0.414R、两倍成本 -0.672R，四市场全部为负。
- 15m共享组合回撤18.01%，M0仍是入选模型；1h仅97笔，被样本门禁拒绝。
- v4 baseline在新增能力后仍保持字节级可复现，旧证据未被污染。
- v5归档为失败实验，不进入Shadow/Paper，不打开密封OOS。

下一步：若继续结构迭代，先预注册“不追突破、等待浅回踩”的独立v6候选；不得在当前训练窗口微调v5确认阈值。

## 2026-07-15：评估框架纠偏 v6

- 暂停浅回踩候选，新增显式 `selected-oof-v6` 评估协议。
- 记录每个验证候选的 OOF 决策，并只对入选家族 OOF 保留路径计算两倍成本、组合和 block bootstrap。
- 15m趋势 M2 OOF基础+0.090R，但两倍成本-0.005R、bootstrap下界-0.238R，仅BTC/SOL为正。
- legacy v4 JSON保持字节级一致，密封OOS和生产路径未变化。

下一步：执行 v1/v5 同setup A/B/C配对反事实，然后建立统一Holm实验账本。

## 2026-07-15：工程基线与唯一 PA A/B/C 诊断 v7

- 暂停新策略开发；从默认研究策略注册表移除 SMC、Keltner，仍保留显式研究入口和历史证据。
- 修复 Range Breakout Drop 过期测试初始化与真实数据 smoke API，`rust-quant-strategies` 全目标构建基线恢复通过。
- 建立统一实验账本、完整源码身份指纹和 Holm 校正门禁；历史没有原始 p 值的实验保持空白，不反推统计量。
- 在预注册文档冻结后，仅执行一次 `pa-diagnostic-v7-abc-counterfactual`：A 2,350 笔，B/C 严格配对 255 笔。
- B 相对 A 的描述性选择差为 +0.197R，但 C 相对 B 的可执行延迟差为 -0.337R；C 均值 -0.414R、胜率 33.33%、PF 0.500、两倍成本 -0.672R，四市场全部为负。
- 共享市场 7 天块 bootstrap 下界 -0.604R，去除绝对收益最大的 5% 后均值 -0.451R，组合最大回撤 18.01%，Holm 校正后的两个 p 值均为 1.0。
- 预注册硬门禁中仅样本量、市场覆盖和 B 的诊断标识通过，其他 10 项失败；执行固定决策 `archive_pa_standalone`。
- PA 不进入 Shadow/Paper/Live，不打开密封 OOS，不继续开发 PA 独立候选或 PA Meta-filter；未修改 Vegas 与生产执行行为。

下一步：把研究与工程资源集中到已有生产证据的 Market Velocity/Vegas，先盘点其真实运行版本、信号/执行/风控证据和实验账本缺口，再决定唯一的增量验证问题。

## 2026-07-15：Market Velocity / Vegas 生产证据审计

- 只读核对生产 Core `d502eca668a84c4e1b8efb38fdb131234c3c5ba6` 与 Web `54f535973d83edffbbd7585dbfc0a85d059d764f`；列出的运行角色均 running、restart=0。
- 确认 radar 约 10 秒、scanner 60 秒、backfill 300 秒、主 handoff 60 秒、short handoff 300 秒、symbol sync 3600 秒、execution worker 约 5 秒轮询。
- 排名快照与 episode 约 15 秒新鲜，OKX symbol filters 小于 1 小时，ETH 4H 当前；但 BTC/ETH/SOL 15m 分别滞后约 10/10/25 天，backfill 126 symbols 与 scanner 269 symbols 覆盖不一致。
- 近 7 天主 Market Velocity 29,211 条、Breakdown Short 409 条 handoff 全 blocked，没有 pass/dispatch；当前 Core 启动后没有 execution task，worker 持续 handled=0。
- 主 handoff 固定绑定已过期 combo 4；当前有效 combo 7 的 Binance signed snapshot 已过期。Short 的 runtime underscore slug、产品/API combo hyphen slug、DRAFT 产品和 active production pointer 不一致。
- 生产 Core/Web 代码具备 signed readiness、symbol filters、数量/价格量化、强制止损和保护单确认合同；问题是运行身份和证据未同时满足，不是缺少门禁代码。
- 历史 Vegas task 68 确认成交 0.02 ETH long 且当时保护单成功，但 Web position leg 自 7 月 6 日仍为 active/confirmed，没有今天的 signed position/open-order 对账；execution-result delivery 仍为 pending。
- 已把真实阻塞原因写入统一实验账本和机器可读审计快照；全程 mutation count=0，未创建任务、下单、撤单、平仓、写回、重启或部署。

下一步：只执行预注册 `prod-vegas-open-leg-readonly-reconciliation-20260715`，一次性核对历史 active 仓位、open orders 和 recent fills；禁止任何生产 mutation。完成前不启动 Market Velocity/Vegas 新机制或参数实验。

## 2026-07-16：Vegas 历史 active leg 只读对账执行门禁失败

- 复核生产 Core revision 仍为 `d502eca668a84c4e1b8efb38fdb131234c3c5ba6`，相关运行容器配置了 `EXECUTION_EVENT_SECRET` 与 `RUST_QUAN_WEB_BASE_URL`；只核对变量名，未读取或输出密钥值。
- 部署版本 reconciliation runtime 必须用内部密钥通过 Web owner API 精确解析 credential；同一密钥非空时又会无条件写回 exchange account snapshot。
- `RECONCILIATION_SNAPSHOT_REPORT=false` 不能关闭账户快照写回，现有入口无法满足预注册的 `mutation count=0`。
- 按失败即停止规则，未启动探针、未发起 signed exchange 请求、未发起 Web 写请求，也未使用临时代理或直连脚本绕过 Core；本次 mutation count=0。
- task 68 当前仓位、open orders、recent fills 与保护单状态仍未知；不能从 7 月 6 日历史确认推导当前安全。

下一步边界：不重跑该预注册验证，不启动新策略或新参数问题。若要继续补齐 Vegas 当前仓位安全证据，必须先单独批准一个工程增量：让凭证解析鉴权与账户快照写回授权解耦、默认关闭全部写回，完成测试并经 CI/CD 发布后，再制定新的预注册验证；本记录不授权该代码修改、部署或生产写回。

## 2026-07-16：Market Velocity 旧库迁移、回放基线与严格复跑

- 从旧 `quant_core_postgres` 数据卷只读迁移到独立数据库 `quant_core_mv_replay_20260716`，未覆盖当前 `quant_core`；目标库 1,737 MB，包含 5,287,535 条 rank events、4,521 条 episodes，以及 168/109/171 张 15m/1h/4h K 线表。
- 定位并修复 `raw_state` 任意毫秒事件无法命中 15m K 线起点、导致权益报告把真实交易静默显示为 0 的工程基线问题；新增回归测试，相关 21 个测试、格式检查和 CLI 构建通过。
- 精确复跑生产 Long preset：46,801 个候选、784 个信号通过、0 个执行通过；714 个未等到 FVG 50% 回补，70 个没有近期有效 FVG，判定为 0 入场硬失败。
- 精确复跑 Breakdown Short v6：43 笔，48h 完整胜率 58.14%、resolved 胜率 60.98%；框架权益胜率 62.79%、最大单 symbol 回撤 8.11%、隔离资金合计利润 37.16U。
- Short 未达到至少 50 笔；去 Top3 后胜率降到 57.14%，Top5 贡献 83.08% 利润；当前框架没有显式滑点，因此严格门禁失败，不推进 Live。
- 全程未创建执行任务、未下单、未撤单、未平仓、未部署，也未改动生产环境。

下一步：保持 Long 非晋级、Short Paper/研究态；先补显式滑点合同，然后只执行固定 v6、仅追加 7 月 6 日之后数据、累计至少 100 笔的前向稳健性验证。
