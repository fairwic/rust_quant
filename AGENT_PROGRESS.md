# Agent 进度：PA Quant Tree

## 2026-07-28：架构迁移文档与技能防腐收敛

- 已接受 ADR-0013：产品没有 Core 自营账户或系统 `ExecutionRequest`；Web 是唯一的用户执行请求 creator。平台固定 API Key 仅是 Market 公共只读数据访问能力，不能成为用户凭证、私有流、Risk、Execution 或 mutation 权限。
- 已补齐 Web → Core 的 Claim/Renew/Release/Outcome Contract、Gateway 短期凭证 capability、跨仓库 Contract owner/source/Envelope 规则，以及 `ExecutionPlan`（持久 aggregate）与 child `OrderPlan` 的唯一关系。
- 已统一运行与研究规范：公共/私有配额、Phase 1/2/3 状态、degraded 禁止新增风险、角色 readiness、状态存储矩阵和 ResearchBar Continuous Risk 闭环。
- 已将 Migration Manifest 收敛为父计划加单 Owner 子 Manifest；技术状态、研究晋级和 Cutover 分离，历史 Research 记录不再伪装为未来迁移验证。
- 已同步 `rust-quant-architecture` Skill 与 AI 护栏；本轮未修改业务代码、数据库、部署、生产配置或交易所账户。
- 后续文档内冲突审计已完成并落实：新增可机读 Program registry、明确 Claim command/receipt 方向和 B0 Evidence、将 `ExecutionPlanningValue` 与 live OMS aggregate 分离，并建立 ActivationEligibility、MarketDecisionReadiness、PortfolioEvaluationBatch、AccountRecoveryClosed/SafetyObligation 与外部仓位授权边界。
- 本轮仅验证文档差异、关键术语、无冲突标记和 Migration TOML 解析；没有把这些目标规范宣称为已落地代码、已部署运行态或生产可用性。

## 2026-07-21：Core 默认生产拓扑收敛为六角色

- 保持模块化单体，不拆微服务；同一 Core 仓库和 runtime image 新增 `control-api / market-worker / signal-worker / account-worker / execution-worker / reconciliation-worker` 六个固定入口，生产 Compose 默认只启动这六个长期角色。
- Market 收敛 symbol sync、radar、K 线 scanner 和最多 2 天在线修复；移除 Web execution secret、关闭 Market signal dispatch，并将 paper observation、全市场只读观察、schema 和大范围历史 backfill 移出默认长期拓扑。
- Signal 共享 Vegas/Universal 4H 行情连接，但按策略类型隔离 symbol scope，并把启动时允许的 config ID 传到 WebSocket handler，避免共享 socket 后跨配置执行；Market Velocity handoff 按 `strategy_key@preset` 加载两份不可变数据库配置，缺失或 slug 错配时 fail-closed。
- Execution、Account、Reconciliation 改为代码入口固定的互斥 lane。当前 Account 仍是 confirmation bridge，含成交后保护同步 mutation；Reconciliation 仍是 report replay bridge，均已在规范中明确为迁移债务，没有冒充最终领域隔离。
- runtime image、Compose、发布、回滚和只读验收合同已同步。首次从旧容器切换必须提供一次性确认 token，并保存 legacy 服务镜像拓扑；单次/scheduler live-handoff 都纳入清退/回滚，六角色 previous image 不完整时 rollback 恢复旧拓扑。CI 会在进程稳定检查后强制执行只读生产验收；依赖级 readiness 仍是后续债务。
- 本地验证通过：六个 binary 编译；CLI app 层 714/714、WebSocket config scope 5/5、Execution 环境/lane 10/10、既有生产安全合同 7/7 与新增六角色合同 5/5 通过；脚本语法、Compose 六默认角色集合、格式和差异检查通过。部署尚未执行。
- 发布脚本完成零行为去重：`promote_stable.sh` / `rollback.sh` 从 475/474 行缩到各 5 行，公共入口 79 行，远端安全与切换实现只保留一份；六角色改由仓库内 `runtime-services.txt` 固定，CI 不再读取可漂移的 `DEPLOY_SERVICES` Secret。迁移期 cutover/legacy restore 尚保留，待生产验收及回滚窗口结束后删除。

下一步：先在无 mutation 环境核对两份 `strategy_configs` 与六角色 startup/readiness，再做 shadow/canary。随后把保护 mutation 收回 Execution owner、建立真正 AccountProjection/Reconciliation，并将 process-only health 升级为依赖与数据新鲜度 readiness；未完成这些证据前不宣称目标架构已完全落地。

## 2026-07-21：交易持久化与外部 Mutation 协议收敛

- 将 ADR-0006 设为订单、撤单和保护单外部 mutation 顺序的唯一权威，清除“先持久化 OrderIntent/Outbox、后生成 ExecutionPlan”的反向顺序。
- 固定 Risk/Execution owner 边界：Risk 以 request、target/snapshot hash、policy version/generation 形成 `risk_evaluation_id` 并提交不可变决策；一个批准决策只绑定一个 parent OrderIntent/plan hash，Execution 不建立跨 owner 数据库事务。
- Execution 先固定 OrderIntent、ExecutionPlan、ProtectionPlan，再以单一事务取得持久 AccountOpeningSlot，并提交 `SubmitPending + 完整计划 + Idempotency + OrderSubmissionRequestedV1 Outbox`；提交只授予投递资格，不授权调用方直接访问交易所。
- Dispatcher 在提交时最终门禁后，通过 aggregate version/空 send claim/current account-order fence 条件更新记录 attempt 并签发短期 MutationPermit；只有 Fenced Gateway 在真正网络 I/O 边界原子消费 current permit 后才能触达 raw SDK，revoked/stale/expired permit 明确为 DefinitelyNotSent。
- 删除语义重复的 Submitted，保留 Acknowledged 作为交易所明确接受证据；取消/恢复 revoke 与 Gateway consume 竞争同一 permit CAS。Submit/Cancel/Protect 统一绑定 `mutation_event_id/mutation_generation/expected_aggregate_version`；任何已确认 delivery 但仍需重试的路径都原子 rollover 到新 generation 的 Outbox/RetrySchedule，旧 delivery 只能 ack/no-op。Unknown outcome 禁止直接重投同 kind mutation；只有 DefinitivelyAbsent/RecoveryAuthorized 恢复事务可保持原 mutation identity 并按原 kind 创建新授权。
- 最终门禁失败区分 Expired、Blocked 和可恢复 blocker；后者持久化 next_eligible_at/唤醒条件，禁止 nack 热循环或 ack 后丢任务。
- 启动恢复改为先订阅并缓冲 User Stream，再合并 signed snapshot/query watermark、补 gap 和对账；闭合前 NotReady 且 Dispatcher 禁用。
- Golden Slice 明确拆分 Dry-run、PaperEvent、RecoveryHarness：前两者不写生产 Order/Outbox，只有 disposable RecoveryHarness 验证生产形状的持久化与恢复协议，生产库、真实凭证和 live Adapter 物理不可达。
- 未建立 Risk Reservation ADR 前，同账户独立开仓由持久 AccountOpeningSlot/唯一约束串行；slot 等 permit/attempt、Account watermark、最终 Fill 和保护闭合后释放。风险降低旁路必须可证明 reduce-only，并先冻结风险增加 claim。
- 本轮只修改架构与跟踪文档，没有修改业务代码、数据库、运行配置或触发任何交易动作。

下一步：实现前先补 Execution owner 的 schema/Port/RecoveryHarness 设计切片和原子事务测试；如需同账户并发独立开仓，先单独决策 Risk Reservation owner、状态机、释放和崩溃恢复，不能在本协议上隐式扩展。

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

## 2026-07-19：Market Momentum Opposite Move v11/v12 同身份停止评估

- v11/v12 始终沿用策略键 `market_momentum_opposite_move_reversal`、产品 slug
  `market-momentum-opposite-move-reversal` 和数据库类型 `market_velocity_kline_15m`；版本号只用于冻结假设与证据，没有创建独立策略身份。
- v11 补齐双向真实交易方向：下跌放量大阴线先等待多头反转确认，上涨放量大阳线先等待空头反转确认；止损、固定初始 R、权益回放和 Paper 审计均使用确认后的显式方向。新池 112 笔原始交易 EV `-0.0265R`、PF `0.9643`，聚类后 86 个有效事件 EV `-0.1162R`、PF `0.8470`，停止。
- v12 只预登记一次“BTC 前 96 根已完成 15m K 线绝对净变动小于 2% 时做空”假设。开发池 176 笔扣费后交易 EV `0.0586R`、PF `1.0802`；聚类后 108 个有效事件 EV `-0.0893R`、PF `0.8800`，且后半段为负、移除 AAVE 后总利润转负，停止。
- v12 预留的 11 币新 Universe v4 保持未回填、未查看，避免用失败后的样本继续调参。v11/v12 在 `back_test_log` / `back_test_detail` 均为 0 条，不进入 Paper/Live。
- 格式、编译和方向/BTC 时序测试通过；Market Velocity 模块为 340 项通过，仅剩本轮开始前已有的 profit-protection 断言失败，本轮未改无关逻辑。

下一步：冻结已查看窗口，不再扫描 BTC 阈值、季度、币种或小样本触发分支。只有在预登记新的结构性因果假设并取得新增前向/OOS 数据后，才继续同一策略身份的下一版本；晋级仍须完整满足主项目联合门槛。

## 2026-07-19：Vegas live-only 币池补齐与 v28/v29 扩展回归

- 当前币池只保留 OKX `live`、`instCategory=1` 的加密币 USDT 永续；退市币直接跳过，股票、贵金属等非币类合约也跳过。本地快照 262 个 live 加密币中有 246 个上市满 150 天，246/246 均有 4H 表。
- 只补齐仍 live 的 MAGIC、S：分别恢复到 7,579 / 3,264 根 4H K 线；验证成功后删除原 82 行不完整备份。此前误建的 DMAIL 退市币表及退市补数入口均已删除。
- v28 对 246 币统一重算 5 档波动参数，月度成交额 Top100 回放为 166 笔、+82.69%、PF 1.877、EV 0.537R、Sharpe 1.270、Recovery 4.149、保守回撤 14.05%，退化并淘汰。
- v29 对原 100 币冻结 v27 全局档位，只给 146 个扩展币使用统一分档；与 v27 重叠币的单币执行结果差异为 0。月度 Top100 回放为 166 笔、+96.16%、PF 1.960、EV 0.590R、Sharpe 1.408、Recovery 3.471、回撤 13.85%，仍未优于 v27。
- v29 放开到全部 live 合格币后达到 249 笔、+119.02%，但 PF 1.700、EV 0.445R、Sharpe 1.379、回撤 16.75%，证明盲目扩大币池只增加低质量交易。5%/8%/10%/12% 全局做空 EMA 偏离邻域也没有同时提高次数和总盈利，整组淘汰。
- 当前继续保留 disabled v27：154 笔、100U → 210.325U、PF 2.254、EV 0.693R、Sharpe 1.647、Recovery 4.214、保守回撤 13.12%。v28/v29 共 492 份配置全部 disabled，没有提交、部署、生产切换或真实交易。

下一步：不再查看区间内继续加参数；只按冻结 v27 累积至少 6 个完整月、50 笔组合交易、30 个有效事件的前向 OOS，再决定是否进入 Paper/ReadOnly。

## 2026-07-19：Vegas 跨币种 v27 低耦合风险/容量候选

- 保留 v26 全部信号和退出语义，不增加币种阈值；创建 `xasset_4h_top100_v27_risk075_cap12_20260719` 100 份 disabled 配置，唯一策略内变化是统一风险 0.75%，组合容量固定为 12。
- 真实 100 币回测写入 `back_test_log.id=10280..10379`：154 笔全部被组合接纳，100U → 210.325U，胜率 46.75%，净 PF 2.254，净 EV 0.693R，日频 Sharpe 1.647，Recovery 4.214，保守盯市回撤 13.12%。
- 0.70% / 0.75% / 0.80% 风险邻域在基础压力成本下均通过五项数值门槛；额外成本提高到单边 10 bps + 每 8 小时 2 bps 后 PF 2.071、Sharpe 1.496、Recovery 3.830，费用压力门槛失败。
- 154 笔成交聚类为 121 个有效事件，事件月频 4.05；移除最大单笔、前三大单笔、最大贡献币后仍分别盈利 102.17U、86.96U、97.12U。BTC bull/bear/neutral 均为正贡献，但 neutral 仅 5 笔。
- 固定参数 12 个月训练隔离、3 个月滚动测试显示 2025 年中存在负收益/低 PF 小样本窗口；当前改按用户确认的 live-only 研究边界补齐“仍 live 且上市满 150 天”的 4H 数据，退市币直接排除并显式接受幸存者偏差。
- 新增 `docs/VEGAS_CROSS_ASSET_V27_EVALUATION_MANIFEST.md`；候选不晋级、不部署、不触发真实交易。focused 回放测试 36/36 通过。

下一步：先补齐 live-only 合格币种的 4H 数据并按月重建 Top100；同时从冻结点累计 v27 前向 OOS，不在已查看窗口继续修改信号或风险。

## 2026-07-19：Market Momentum Opposite Move v4 成交量优势过滤

- 保持原策略键 `market_momentum_opposite_move_reversal`，新增 v4 preset/rule；v1-v3 参数和历史证据不变，未新增独立策略身份。
- 当前信号与前 2 根 K 线组成反转量能簇；与过去 96 根内、左右各 3 根确认的历史低点/高点量能簇比较。当前量弱于历史最强极值量时阻断，所有计算截止于信号 K 线。
- 精确回放确认：BTC 2026-06-04 21:45 的错误开仓被阻断；BTC/ETH 10:30 首轮强放量反转保留；ETH 2026-06-15 05:30 长时间趋势反转保留。
- 冻结并执行 `0.8/1.0/1.2` 邻域和 2025 Q3、2025 Q4、2026 春夏三个窗口。主参数保存为 `back_test_log` 10278/10279/10277，共 57 笔开仓、55 个 30 分钟有效事件、114 条明细。
- 主参数明细毛平均 R 为 `0.1277 / 0.1055 / 0.4196`，毛 PF 为 `1.3659 / 1.3249 / 无亏损但仅 4 笔`；计入零成交月份后月频 0~13 笔。Q3/Q4 移除头部 3 笔盈利后总 R 转负，未达到净 0.6R、净 PF 2.2、频率和集中度要求。
- 当前回放费用字段为空，且缺少可用的滑点、资金费率、统一资金、Sharpe、Recovery、容量和跨市场相关性诊断；`promotion_eligible=false`，未切换 Paper/Live，未部署或触发真实下单。
- 验证：v4 focused tests 6/6 通过，目标事件数据库回放通过，目标文件格式检查与 2,000 行硬门禁通过。全库 485 项中 483 项通过；另有 1H/4H 默认目标断言和 profit-protection 旧路径各 1 项失败，均不在 v4 成交量门禁路径。

下一步：若继续研究，先补统一资金、显式费用/滑点/资金费率、组合容量与历史 universe 版本化评估能力，再用新增样本做样本外验证；不要在当前三窗口继续调成交量比例。

## 2026-07-18：实盘执行链逻辑与性能修复（待发布）

- 修正普通 execution worker 的平仓任务类型，并在生产编排中补齐订单确认 worker 与结果回放 worker；未执行提交、部署或生产交易动作。
- Vegas Web 分发模式不再维护 Core 本地持仓、不再执行 legacy 本地平仓对账；WebSocket K 线回调移除重复任务派发。
- Market Velocity 非指定事件只读取 TTL 内的最新候选，K 线加载前先做过期判断；持续调度周期必须不大于 TTL，生产默认由 60/300 秒收敛为 5 秒/10 秒 TTL。
- Market Velocity 长期运行复用 Web 客户端、数据库连接池与 OKX HTTP 客户端，避免每轮重建连接。
- execution worker 空轮询 checkpoint 降为最多每 30 秒一次；rank snapshot 从逐行 UPSERT 改为每批最多 5,000 行的批量 UPSERT。
- 验证通过：四个相关 crate 的 `cargo check`；Market Velocity 51 项、execution worker 150 项、生产部署契约 7 项、WebSocket 1 项测试。另有既有契约测试因仓库缺少 `docs/STARTUP_GUIDE.md` 导致 1 项失败，其余 13 项通过。

下一步：代码评审后由 CI/CD 构建发布；发布后只读核对三个 execution worker 角色、5 秒 handoff 调度、checkpoint 写入速率、候选新鲜度和任务状态，不直接在生产服务器构建。

## 2026-07-19：Vegas 跨币种 v26 合规报告第一阶段

- 修复共享账户回放把单笔风险写死为 2.5% 的审计错误，改为逐份读取 `max_loss_percent * position_leverage`；v26 的 146 笔接纳交易均为约 2%，最大同时配置风险预算约 10%。
- `1R` 改为入场时信号保护价风险与最大亏损止损风险的更紧者；146/146 笔均有完整初始风险证据，平均初始风险 1.990%，最大 2.000%。
- 新增 UTC 日频 `sqrt(365)` Sharpe、Recovery Factor、保守盯市最大绝对回撤；修正保守回撤不能小于纯收盘回撤的峰值口径。
- 同口径成本压力回放：100U → 401.6947U，146 笔，胜率 45.21%，净 PF 2.049，净 EV 0.557R，日频 Sharpe 1.545，Recovery 4.018，保守盯市回撤 28.77%，约 4.89 笔/月。
- 当前仅 Recovery 与 Sharpe 勉强过线；EV、PF、回撤和频率目标未达标。2024 年仅 +0.774U，跨年份稳定性仍弱。
- 冻结 `docs/VEGAS_CROSS_ASSET_V26_EVALUATION_MANIFEST.md`：已查看区间不再作为 OOS，下一阶段先实现历史 Top100 成员重建和预声明事件聚类，再累计 2026-07-16 之后的前向证据。
- 验证：`cargo test -p rust-quant-cli --bin vegas_cross_asset_portfolio_replay` 30/30 通过；回放行情路径 146/146 完整，缺失 4H K 线、开平仓价格越界均为 0。未提交、未部署、未改生产配置、未触发真实交易。

下一步：实现只读历史币池成员构建与有效市场事件聚类报告；在该证据完成前，不修改 v26 信号参数，也不创建 Live promote。

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

## 2026-07-19：Vegas 初始 R、成本压力、时间验证与 Paper/ReadOnly 门禁

- 回测交易合同新增并持久化 `initial_stop_price`、`initial_risk_amount`、`net_profit_r`；初始止损在开仓时冻结，移动止损不回写 1R。
- Vegas 组合回放新增严格 `--oos-start`、固定参数 12m/3m walk-forward、单边滑点和 8 小时资金费压力报告。
- v27 `back_test_log.id=10280..10379` 在 5 bps 单边滑点、1 bps/8h 资金费、容量 12、风险缩放 0.70 下：154 笔、PF 2.235、EV 0.693R、保守回撤 9.48%、Recovery 4.53、日频 Sharpe 1.64。
- 聚合门槛已通过，但 2026-07-16T12:00:00Z 后 OOS 暂无交易；七个滚动窗存在一个亏损窗和多个低质量窗，保持不可 Live 晋级。
- Core 新信号携带 `strategy_version/entry_rule_version`；Web 仅准备 v2 `paper_observing` manifest，版本化信号没有匹配 `production_default` 时写 blocker 且不生成任务。
- Web 信号入口会持久化会员过期 combo；flat 对账按 exchange + api credential 精确关闭持仓腿并释放 reservation。相关 Core/Web 单元测试和本地 PostgreSQL disposable fixture 通过。
- 未执行真实下单、撤单、平仓、生产迁移、部署或 Live promote。

下一步：只累计版本化 Paper/ReadOnly 信号、前向 OOS 和 signed read-only 对账证据；在历史币池、滚动稳定性和发布门禁全部完成前，不创建 production_default。

## 2026-07-19：ETH Vegas 4H v1/v2 真正同口径复跑

- 从生产只读提取 `eth_4h_id102_live_v1/v2` 配置，以 disabled 状态导入本机；未修改生产、未部署、未触发真实交易。
- 固定同一份 14,385 根 ETH 4H 已确认 K 线、同一当前工作树引擎、100U 初始权益和 `position_leverage=0.58`，生成本机回测 `10872/10874`；v2 原配置回测为 `10873`。
- 原回测成本下，v1/v2 分别为 461/447 笔、51.84%/53.02% 胜率、+690.81%/+914.13% 总收益、1.879/2.152 PF、0.283R/0.325R EV；同仓位最大回撤均为 15.52%。
- 加入单边 5 bps 滑点和 1 bps/8h 资金费压力后，v1/v2 总收益为 +436.33%/+593.83%，PF 1.667/1.890，EV 0.205R/0.246R，日频 Sharpe 1.518/1.734，Recovery 4.124/5.447，保守盘中回撤 20.15%/20.79%。
- 446 笔共同入场中 v2 多贡献约 125.87U；v2 避开 15 笔 v1 净亏损约 101.90U，仅新增 1 笔亏损约 4.44U。v2 改善同时来自过滤器和 ATR 止盈 3.44。
- 2025 年以来历史留出段 v2 的收益、PF、EV、回撤、Recovery、Sharpe 均优于 v1；但 v2 于 2026-07-18 才冻结，冻结后已平仓交易为 0，严格前向 OOS 仍为 null。
- ETH 专用策略不再要求历史币池覆盖；改看 ETH 时间状态、Long/Short、成本压力、滚动窗口和冻结后前向事实。
- v2 风险缩放 0.40 后，成本压力总收益 +126.29%、保守盘中回撤 9.06%、Sharpe 1.728、Recovery 8.26；这证明 10% 应作为资金配置门槛，而不是继续扭曲信号规则。
- v2 仍未达到胜率 55%、PF 2.2、EV 0.6R 和滚动稳定性要求；同时存在 `position_leverage`、`entry_rule_version`、固定止盈字段合同缺口，维持 Paper/ReadOnly，不恢复 production_default。
- 新增 `docs/VEGAS_ETH_4H_V1_V2_SAME_SCOPE_REPORT_20260719.md`，记录配置/K线/源码指纹、回测 ID、同口径结果、时间验证、风险缩放与放行边界。

下一步：冻结 ETH v2，先修复版本与风险字段合同；按 ETH 单策略累计至少 20 笔前向 Paper 交易做早期复核，50 笔后再评审是否进入小资金 Live。

## 2026-07-20：Vegas Universal 4H 多空差异与 XRP 流动性扫单迭代

- 复核 `back_test_detail.id=1957503` 属于 XRP：`2026-07-06 20:00:00` 空头信号棒下影占比约 70.56%，并同时出现 Hammer、布林多头支持和 MACD 回升，确认存在方向冲突；该笔最终盈利约 2.539R，但未用未来结果合理化入场。
- `2026-07-04 20:00:00` 是放量强阳突破，未在当根收盘猜顶；新增严格因果的两 K 线流动性扫单确认候选，在 `2026-07-05 00:00:00` 收阴扫高确认后做空，XRP 单例约 4.124R。
- v27 原始信号漏斗为多头 188、空头 661，接受率分别 17.02% 与 18.46%；多空差距主要来自样本期大级别/腿方向生成，非过滤器单独造成。高波动延迟空头的不对称配置会进一步放大差距。
- 完成 v28 对称扫单、v29 仅空头扫单、v30 对称高波动延迟多头 100 币回测；月频最高仅 5.92，v28/v29/v30 的 PF 分别为 2.092/2.187/1.822，均未同时达到联合门槛。
- 所有新开关默认关闭，研究配置保持 disabled；不覆盖 v27、不部署、不触发真实下单。v27 仍为冻结基线，下一轮应另立多头策略假设并使用 point-in-time 币种池与新 forward OOS。

## 2026-07-20：Vegas Bull Rank 4H v1 独立多头频率候选

- 在查看结果前冻结独立研究身份 `vegas_bull_rank_4h_research / bull_rank_4h_v1_20260720`，只恢复被多头弱布林过滤拦截的候选，再以已完成 4H 横截面成交额排名、正向价格冲击、等待窗口和 RSI 门禁筛选。
- 修复 Universal 组合审计误把策略类型写死为 `vegas` 的问题；当前 live-only `246/246` K 线覆盖通过，但历史上市快照晚于回测起点且排除退市币，严格 point-in-time 门禁仍失败。
- 新建 `100` 条 disabled v31 配置并完成本地回测 `11481..11580`；宽口径多头使交易数从 `154` 增至 `210`，但 PF/EV/Sharpe 分别降至 `1.722/0.434R/1.344`，多头净贡献约 `-0.043U`。
- 修复组合 passthrough 只查候选日志导致基线路径被提前开仓改写的问题：显式合并完整 v27 与新增候选、基线事实优先，并阻止同币种重叠持仓；相关 `44` 个 CLI 单元测试通过。
- 冻结 Rank 规则只放行 `1` 笔新增多头，该笔为 `-0.8116U/-1.0821R`；合并结果 `155` 笔、约 `5.19` 笔/月、PF `2.225`、EV `0.682R`、Sharpe `1.629`、保守回撤 `13.12%`。
- 按预注册停止规则拒绝 Bull Rank v1，不搜索第二组阈值、不打开多窗口/OOS；v27 保持冻结研究基线，v31 全部 disabled，无部署和实盘 mutation。

## 2026-07-20：Vegas Universal 4H 亏损逆向分析与盈利保护反事实

- 冻结 v27 `10976..11075`，确认 154 笔组合候选全部被接受；约 5.1 笔/月、4.05 个有效事件/月，频率缺口来自 setup 密度，不是币种池、容量或 readiness。
- 多头 32 笔、胜率 65.63%、EV 0.466R；空头 122 笔、胜率 41.80%、EV 0.836R。空头靠平均约 3.468R 的趋势赢家贡献右尾，不能以提高胜率为单一优化目标。
- 空头 71 笔亏损中 20 笔曾在已完成 K 线收盘达到至少 +1R，确认盈利回吐存在；多头亏损几乎没有同类路径，因此不应对多空套用相同保护。
- v32 长下影线拦截在发现段为负、验证段强正；v33 `1.5R→BE` 与 v34 `2R→+1R` 均显著降低空头 EV/PF，按门禁全部拒绝，不继续扫描阈值。
- XRP `2026-07-06 20:00` 的 bullish rejection 冲突成立，但该笔最终 +2.539R，说明不能静态禁空；`2026-07-04 20:00` 当根是收在高位的放量强阳，v27 根本未生成空头，不能用后续跌幅反推当时猜顶。
- 找到回测/实时退出合同缺口：实时模块存在 1.5R 保本行为，而冻结 v27 回测没有默认复现；完成版本化 parity test 前，不把历史回测当作实盘退出结果。
- 三个研究版本和 300 条配置全部 disabled；未覆盖 v27、未部署、未触发真实下单。详细结论见 `docs/VEGAS_UNIVERSAL_4H_LOSS_REVERSE_ANALYSIS_20260720.md`。

下一步：如继续，只允许先冻结“Fib 空头与 bullish rejection 冲突后，延迟 1—2 根已完成 4H K 线确认”的独立候选；频率提升另立非重叠 setup，不从亏损样本直接放宽 v27。

## 2026-07-20：Vegas Universal 4H v35—v42 亏损归因与频率—质量前沿

- v35 证明“新腿且没有延迟量能确认”是稳定亏损特征，但删单后频率降至约 4.88/月；按频率门禁拒绝。
- v36 以两根已完成 K 线识别低 ATR 扫高反转：XRP 从 `2026-07-06 20:00` 长下影后的 Fib 空头，改为 `2026-07-05 00:00` 转弱确认空，结果从约 `+2.539R` 改善到 `+4.124R`；另新增 LTC `+2.278R`。样本只有 2 个且双倍成本 Recovery 失败，不晋级。
- v38 动量回测把成交频率提高到约 8.97/月，但 PF/EV/Sharpe/回撤全面失败；v39 压缩突破也只有频率与 Recovery 过线，多头 discovery 10/10 全亏。
- v40 只保留压缩突破空头，并用此前 5 根整理区下沿作为结构失效止损：244 笔、PF `2.096`、EV `0.599R`，仍不达标。
- v41 合并 v40 与冻结 v36：245 笔、约 `8.20/月`、178 个有效事件（`5.96/月`），收益 `+167.22%`、EV `0.612R`、PF `2.120`、Sharpe `1.648`、Recovery `5.028`、盘中回撤 `14.79%`；PF 和滚动稳定性失败，未运行双倍成本。
- 新增突破空头中，`relative_volume<2.5x && EMA=Normal` 的 7 笔在 discovery/validation EV 分别为 `-0.461R/-0.572R`。v42 真实重放精确移除这 7 笔，EV 提到 `0.641R`、PF 提到 `2.163`，但频率降至约 `7.97/月`，walk-forward test-2 仍负；按预注册门禁拒绝。
- 代码新增开关均默认关闭，v35—v42 本地研究配置全部 disabled；没有覆盖 v27、没有部署、没有触发 Paper/Live 或任何交易 mutation。
- 定向测试已通过：压缩突破与结构止损/弱扩张门禁 5 个测试，策略配置转换 1 个测试。完整验证结果和既有工作树阻塞另见本轮最终报告。

下一步：停止在已查看的 7 笔弱扩张子集上继续筛 RSI、影线、月份或微调量能阈值。若继续，只允许冻结新的非重叠 setup，并等待 forward shadow；优先扩充“扫上方流动性后固定一根确认”的独立事件样本。

## 2026-07-20：Vegas Universal 4H v43 1ATR 失效止损反证

- 在查看结果前冻结 v43，只修改压缩突破空头止损：取原 5 根整理区下沿与 `入场收盘+1ATR` 的更宽者；风险引擎仍保留 3.5% 最大亏损上限。
- 本地回测 `12782..12881` 的 245 个入场与 v41 完全一致；标准成本下约 `8.20/月`、178 个有效事件，EV `0.584R`、PF `2.032`、Sharpe `1.600`、Recovery `4.952`、盘中回撤 `14.79%`。
- discovery/validation EV 为 `0.471R/0.790R`；walk-forward test-2 为 `-0.555R`，比 v41 更差。放宽止损提高少量胜率，却降低按冻结初始风险计量的收益质量。
- 按预注册门禁拒绝 v43，不运行双倍成本、不扫描 ATR 倍数。100 个配置保持 disabled；没有覆盖旧版本、部署或触发任何交易 mutation。

下一步：只验证入场确认，不再从止损宽度找答案；弱量突破若继续研究，必须作为固定一根已完成 K 线确认的新版本预注册，且不能读取未来结果挑选确认条件。

## 2026-07-20：Vegas Universal 4H v46—v50 FVG/BOS/MACD 迭代

- v46 失败突破收回空头将频率提高到约 `8.67/月`，但新增组从 discovery `1.382R / PF 3.863` 反转为 validation `-0.501R / PF 0.403`，拒绝。
- v47 全局关闭震荡动态止盈后改变持仓周期并阻塞后续入场，入场集合从 238 变为 235，违反消融身份；未把未闭合仓位解码失败解释为收益改善。
- v48 启用因果 bearish FVG、active bearish BOS 与 MACD 转弱，312 笔组合虽达频率，但 PF/EV/回撤为 `1.565/0.391R/21.85%`；新增 79 笔本身 `-0.298R/PF 0.636`。
- v49 反向验证 bearish FVG 完整收复 + MACD 改善多头，新增 2368 笔胜率 `71.11%`，但平均盈亏 `0.299R/-1.057R`，组合 PF `0.761`、回撤 `75.93%`，判定为普通反弹过度交易。
- v50 增加当根 fresh internal bullish CHoCH 后，245 笔、约 `8.14/月`、PF `2.087`、EV `0.599R`、Sharpe `1.629`、Recovery `5.032`、回撤 `13.87%`；新增 9 笔本身仍为 `-0.085R/PF 0.818`，2.5R 反事实毛 EV 仅 `0.167R`。
- v46—v50 共 500 条本地研究配置全部 disabled；未覆盖 v27/v42、未部署、未触发 Paper/Live 或任何真实交易。FVG/BOS/MACD 保留为审计特征，不再在当前已查看区间扫描阈值或拼接入场。

下一步：新的迭代必须先提出与 v46—v50 失败样本非重叠的市场机理并冻结 forward OOS；当前可复现前沿仍是 v41 的频率和 v42 的质量，两者都没有通过全部职业级门禁。

## 2026-07-20：Vegas Universal 4H v44 弱量固定一根确认

- v44 在弱量压缩突破触发棒不入场，只允许下一根仍收在整理区下方且 MACD 继续走弱时入场；强量突破与 v41 其他规则不变。
- 本地回测 `12884..12983`：241 笔、约 `8.07/月`、176 个有效事件；EV `0.578R`、PF `2.031`、Sharpe `1.578`、Recovery `4.882`、盘中回撤 `13.92%`。
- 30 笔延迟确认空头仅 5 胜，EV `-0.364R`、PF `0.591`；15 笔严格四小时配对中，即时 EV `-0.157R`，延迟后降到 `-0.556R`，validation 也从正转负。
- 按标准门禁拒绝 v44，不运行双倍成本、不修改确认条件。100 个配置继续 disabled，没有部署或交易 mutation。

下一步：不再等待更晚入场；只验证信号棒本身的价格位移是否能在量能不足时提供即时冲击证据，使用自然的 `1.5ATR` 尺度并继承 v42 弱扩张门禁。

## 2026-07-20：Vegas Universal 4H v45 价格位移替代量能

- v45 继承 v42，只允许当根振幅达到 `1.5ATR` 时替代缺失的 `1.5x` 量能；弱量且 EMA Normal 仍被拦截。
- 本地回测 `12984..13083`：241 笔、约 `8.07/月`，EV `0.612R`、PF `2.114`、Sharpe `1.625`、Recovery `4.986`、盘中回撤 `14.79%`。
- 新规则只新增 CRO/AGLD 两笔，discovery/validation 各一笔且都约 `-1.1R`；walk-forward test-2 仍为负。
- PF 与新增 setup 质量失败，按门禁拒绝，不运行双倍成本。100 个配置保持 disabled，无部署和交易 mutation。

下一步：转向与压缩突破非重叠的“放量收上压力后，下一根收回区间”的失败突破反转；固定一根确认，不在冲击棒猜顶，也不要求确认棒再次精确扫高。

## 2026-07-19：ETH Vegas 4H v3 受控实盘切换完成

- 新增 `eth_4h_id102_live_v3`，继承 v2 信号规则并补齐显式版本、初始止损/R 和仓位缩放合同；v1/v2 保留为可审计回滚与 Paper 对照，不覆盖历史结果。
- Core CI/CD 最终通过并发布 `312af50e0c1f5aa2394d80522b0b580a791c242b`；生产 ETH 4H worker 仅加载 v3，WebSocket 健康，等待未来已确认 K 线。
- signed read-only preflight 证明当前账户无非零仓位、无活动挂单，且未触发交易所 mutation；临时验证进程已清理，运行容器保持单进程消费。
- 用户对 20.79% 压力回撤作出人工风险确认；v3 的自动晋级标记仍为 false，严格前向 OOS 仍为待观察项。
- 实盘边界：只消费未来新鲜 v3 信号，必须带保护单；禁止重放 7 月 18 日旧信号或人工强制下单。

## 2026-07-21：Vegas Universal 4H v51b/v52c MACD + 结构隔离复跑

- 发现公共 `market_structure_value` 会被多个旧 Vegas 规则旁路读取；新 setup 仅为取 CHoCH/BOS 开启公共结构，也可能改变基线入场。
- 为 MACD 背离反转与趋势复位分别增加专用运行时结构实例/快照；旧规则不可读，快照 `serde(skip)` 不进入公共 payload。
- v52b 曾因专用结构 payload 超出旧 `signal_value` 长度而未落库，但内部接口仍返回 HTTP 200；该运行作废。v52c `13887..13986` 已核对 `572` 条明细完整持久化。
- v51b `13987..14086`：247 笔、约 `8.27/月`、180 个有效事件，EV/PF `0.632R/2.096`、Sharpe `1.639`、Recovery `6.236`、回撤 `6.09%`。新增 9 笔中发现段依赖两笔大赢家，验证段 3 笔空头全部止损，拒绝。
- v52c `13887..13986`：286 笔、约 `9.56/月`、209 个有效事件，EV/PF `0.493R/1.781`、Sharpe `1.448`、Recovery `5.695`、回撤 `6.09%`。新增 48 笔全为空头，合计 `-8.208R`；全部同时出现布林多头回归提示，长下影组 PF 仅 `0.323`，定性为下跌末端追空，拒绝。
- XRP 当前基线只在 `2026-07-05 00:00` 开空，`2026-07-08 20:00` 约 `+4.124R` 平仓；没有在 `2026-07-06 20:00` 长下影处重复追空。
- 相关定向测试、CLI 构建和 200 条 disabled 配置核对完成；未覆盖 v42，未部署，未触发 Paper/Live 或任何真实交易 mutation。

下一步：v51b 只允许冻结参数 forward shadow，v52c 归档；不在当前已查看区间扫 RSI、影线、MACD 幅度或 BOS 长度。

## 2026-07-21：Vegas Universal 4H v58—v60 结构补频与 MACD 位置门禁

- v58 把失败跌破多头延迟为严格四棒更高低点突破；`15088..15187` 与 v54 的 249 笔开平仓事实完全一致，新增 0 笔，按零机会门禁归档。
- v59 新增“放量收盘扫高、下一棒收回实体中点、第三棒跌破确认低点”空头；`15188..15287` 相对 v54 新增 4 笔、移除 0 笔。TRB/LTC 盈利，FIL/APE 止损；频率约 `8.38/月`，EV/PF `0.638R/2.180`，PF 失败。
- 亏损逆向发现 v59 两笔盈利的 MACD 主线仍在零轴上方，两笔止损已在零轴下方。明确标记该发现为事后假设，新增 v60 独立版本，只约束 v59 新分支，不修改 v54 旧交易。
- v60 `15288..15387` 为 251 笔、约 `8.31/月`、186 个有效事件；标准成本 EV/PF `0.652R/2.225`、Sharpe `1.751`、Recovery `6.619`、盘中回撤 `6.09%`，聚合数值首次全部越线。
- 稳健性仍失败：样本内 PF/EV `1.847/0.539R`，12/3 月 walk-forward test-2 `-0.397R/PF 0.675`，双倍成本 `0.582R/PF 2.041`；v60 两笔增量约 91.2% 的净 R 来自 TRB。只能保留 disabled forward shadow，不能晋级 Paper/Live。
- 全部 v58/v59/v60 配置保持 `enabled=false`；没有覆盖旧版本、部署、实盘下单、撤单、平仓或交易所 mutation。

下一步：等待 v60 冻结规则产生未见 forward shadow。当前历史不再用于构造 v61；若获得新样本，首先验证 MACD 零轴位置门禁是否仍能区分扫高反转与成熟下跌追空，并继续按统一成本、有效事件与走步窗口复核。

## 2026-07-21：Vegas Universal 4H v53b—v57 首回踩与多头反转复核

- v53b 以严格相邻三棒定义“扫流动性、确认收回、首次回测守住”，相对 v42 保持 238 笔共同交易、新增 11 笔、移除 0 笔；标准成本 PF `2.151`，未通过。
- v54 只对首次回测设置基于最终有效止损的 2R 上限；AVNT 从约 `-1.057R` 改为 `+1.945R`，组合达到 249 笔、约 `8.24/月`、EV/PF `0.635R/2.188`、Sharpe `1.694`、Recovery `6.419`、回撤 `6.09%`。PF、样本内 PF `1.797` 和 walk-forward test-2 PF `0.675` 仍失败。
- v55 把首次回测退出完全替换为固定 R 后，原先在近目标盈利的多笔交易转成止损，2R 组合 PF 降至 `2.148`；确认该 setup 是短促回归，不是持续趋势。
- v56 开启普通两棒扫低多头，新增 9 笔的 discovery EV/PF `-0.529R/0.195`；组合 EV/PF 降为 `0.596R/2.115`。MACD 柱虽全部改善，但不能区分赢家与亏损。
- v57 改为收盘真实跌破支撑后下一棒收回结构，新增 43 笔在 discovery/validation EV 分别为 `-0.260R/-0.397R`；组合 EV/PF 降为 `0.496R/1.912`。41 笔 MACD 改善组与 4 笔金叉组仍为负期望。
- v53b—v57 共 500 条本地配置全部 `enabled=false`；未覆盖 v42、未部署、未进入 Paper/Live、未触发真实下单或任何交易 mutation。

下一步：冻结 v54 作为当前最接近联合门槛的研究候选，只累计未见 forward shadow。当前历史已经覆盖 FVG、BOS/CHoCH、MACD、扫单、首次回测和两个多头反转分支，不再通过指标拼接拟合 `0.012` 的 PF 缺口。

## 2026-07-21：15m BOS/CHoCH/FVG 因果审计与正交反转证伪

- 修正结构特征为固定右侧确认、交替 pivot、完整 `H-L-H-L` 配对；区分 bullish BOS
  延续与 bullish CHoCH 反转，并给 FVG 增加 ATR 尺度、位移实体和填补状态。逐前缀
  一致性及 SMC 21 个测试通过，没有使用未来 K 线。
- v36 在 2024-07～2025-06、版本化 current-live-only 月度 Top60 的 84 笔扩窗中，
  净 EV `-0.0194R`、PF `0.9719`；早期 57 笔正收益属于时间选择与头部集中，不恢复。
- 独立 CHoCH 后 FVG 中点回补 V1 共 1,081 笔；零成本 EV/PF
  `-0.0726R/0.8994`，标准成本 `-0.1654R/0.7905`，两个半年都为负，12 月仅 1 月
  为正，按预注册规则淘汰。
- 从 OKX 官方逐日全永续归档严格对齐 137,423 条历史 funding；不同结算频率导致覆盖
  不足的横截面整点阻塞，funding 至少延迟一根完成 15m 后才可见。
- funding 最低五分位 + 24h 下跌 + 放量扫低收复 V1 共 856 笔，约 71.3 笔/月；频率
  达标但零成本 EV/PF `-0.0972R/0.8688`，标准成本 `-0.2242R/0.7317`，两个半年
  均亏，淘汰。
- 当前本地 OI、taker 与多空比仅覆盖 2026 年约两个月且只有 BTC/ETH，不能回填
  2024-07～2025-06 的 120 币历史。停止扫描 BOS/FVG/funding/影线参数；下一候选必须
  先获得信号时点可见的去杠杆或主动卖盘衰竭证据。
- 全程只读研究，无实盘下单、撤单、平仓、部署、提交或生产 mutation。

## 2026-07-21：Vegas Universal 4H v61—v64 亏损逆向与机会指标复核

- V61 `15388..15487` 对压缩突破空头使用既有 `2.5x` 冲击量门禁：标准 EV/PF `0.722R/2.387`、回撤 `5.26%`，但 224 笔仅约 `7.42/月`，频率失败。
- V62 `15488..15587` 只增加确认后第二根的严格首次回测；仅新增 ICP 一笔 `+0.447R`，225 笔约 `7.45/月`，机会覆盖不足。
- V63 `15620..15719` 改用既有 `2.0x` 普通放量边界，相对 V60 精确移除 8 笔、保留 243 笔；标准约 `8.04/月`、EV/PF `0.674R/2.271`、Sharpe `1.759`、Recovery `6.653`、回撤 `6.09%`。
- V63 稳健性仍失败：样本内 EV/PF `0.525R/1.838`，walk-forward test-2 `-0.330R/PF 0.733`，双倍成本 `0.604R/PF 2.084`。`2.0—2.5x` 的 19 笔自身 PF 仅 `1.125/1.362`，只是补频率。
- V64 `15720..15819` 新增严格对称的“下方扫单、收回、局部 bullish BOS、MACD 仍在零轴下方”多头；共同 243 笔不变，只新增 17 笔。新增组 EV/PF `0.123R/1.331`，仅 7 个触发时点，MACD 柱 17/17 改善仍有 6 笔止损。
- V64 标准组合 260 笔、约 `8.61/月`、EV/PF `0.635R/2.222`，但样本内降到 `0.485R/1.794`，双倍成本 `0.568R/2.039`，滚动负窗口仍在，按门禁拒绝。
- 下影线、ATR、EMA、RSI 与通用 BOS/CHoCH 在压缩家族继续跨期翻转；不追加 RSI/EMA/MACD 幅度事后补丁。V61—V64 共 400 个配置均 `enabled=false`，未部署、未进入 Paper/Live、未触发任何交易 mutation。

下一步：冻结 V63 仅作可复现聚合前沿、V61 仅作质量对照、V64 归档。当前历史停止继续拼接 FVG/BOS/MACD/影线阈值；只累计冻结后的未见 forward shadow，或先取得信号时点可见且有完整历史覆盖的卖压衰竭/去杠杆结束证据。

## 2026-07-21：V63 冻结后 V65 Forward Shadow 首轮

- 在新增行情读取前预登记 V65，保持 V63 策略键、100 币、4H、信号规则、风险与组合参数不变，只扩展回放上界；100 条配置均 `enabled=false`。
- 99 币完成 OKX 官方增量回填；LRC 返回官方 `51001`。修复 dry-run 仍会写回删除状态的副作用，并增加回归测试，后续只读运行不再修改交易对元数据。
- 修复连续性检查遗漏“已经结束但仍 `confirm=0`”K 线的问题；ETH 对应旧 K 线已用 OKX 官方值补正，避免回测因静默缺口失败或漂移。
- V65 `15820..15919` 相对 V63 精确差分为共同 `243`、新增 `1`、移除 `0`、共同交易变化 `0`。新增 NEAR 多头完整动态止盈 `+0.206329R`，但发生在冻结前的 sealed local gap；冻结后首根完成 4H K 线没有新增交易。
- 标准组合 244 笔、约 `8.04/月`、179 个有效事件，EV/PF `0.671558R/2.272746`、Sharpe `1.756919`、Recovery `6.662530`、回撤 `6.085569%`。样本内 PF `1.837874`、walk-forward test-2 PF `0.732601` 仍失败。
- 双倍成本 EV/PF `0.602545R/2.085475`，PF 仍失败。结果定为 `forward_shadow_observation_insufficient`；未新增指标门禁、未进入 Paper/Live、未部署或触发交易 mutation。

下一步：继续累计冻结后的完成 4H bar；只有出现完整 strict-forward 交易后才更新盈亏证据。当前不把 NEAR 历史缺口盈利用于晋级，也不从零笔前向样本继续拟合 MACD/FVG/BOS。

## 2026-07-21：V66 熊市压缩跌破失败回收多头

- 组合相关簇诊断否定“重复相关仓位是主要亏损源”：28 个多币相关事件合计贡献约 `+35.83U`，直接限簇会切掉主要盈利。
- 动态配置审计确认最差 walk-forward test-2 的 12 笔中，8 笔属于压缩跌破空头且仅 2 胜；全样本中该家族在 BTC 多头状态为正，在 BTC 空头状态样本内外均为负，27 笔中 23 笔止损。
- 在查看反向路径前预登记独立 V66：熊市压缩跌破后等待完成 4H 收盘站回冻结止损，再以确认棒低点止损做 2R 多头；不读取原空头盈亏生成候选。
- V66 漏斗为 75 个压缩种子、27 个熊市种子、18 笔完整回收多头。标准成本 4 胜，EV/PF `-0.604013R/0.349731`；双倍成本 `-0.689878R/0.309138`。
- 样本内 EV/PF `-0.898510R/0.051791`，样本外 `-0.416605R/0.545663`，2024—2026 均亏，按预登记门禁拒绝。
- 新增独立只读研究入口及 5 个因果/成本测试；没有策略配置、Paper/Live、部署或交易 mutation。

下一步：保留“BTC 市场状态影响压缩空头质量”的诊断，但不把空头止损直接翻译为多头信号。若继续使用 MACD，只允许作为信号时点前的 BTC 市场级动量状态提出独立假设，不再修改 V66 的 12/12 根窗口或 2R 几何。

## 2026-07-21：V67 BTC 正 MACD + 本币实体中点收回

- 用严格早于原信号的 BTC 4H MACD `12/26/9` 拆分 75 笔压缩跌破空头：正柱组样本内 7 笔全亏，样本外 21 笔合计仍亏，市场级方向冲突具有诊断意义。
- 在读取反向路径前预登记 V67：正 MACD 种子只观察两根，本币阳线收回原信号实体中点后做多，信号/确认共同低点止损、固定 2R、最多持有 12 根。
- 28 个正柱种子仅 5 个确认，5 笔成本后全部亏损；标准 EV/PF `-0.721507R/0`，双倍成本 EV `-0.760347R`。
- 样本内 4 笔全亏，样本外唯一一笔成本后也亏；交易数量与全部质量门禁失败，按预登记淘汰，不替换 V63 空头。
- V66/V67 共用只读研究入口，当前 7 个因果、同根保守路径和成本测试通过；没有新增运行配置、Paper/Live、部署或交易 mutation。

下一步：MACD 只保留为风险诊断，不再构造反向入场。策略目标仍未达到；继续等待 V65 严格 forward 样本，或先提出与“失败跌破反向”不同的新市场机制。

## 2026-07-21：15m 去杠杆、回踩接受与反弹失败独立窗口复核

- 修复 Binance metrics `00:05~次日00:00`、重复午夜、空指标日和 top-trader 诊断字段的真实数据合同；OI 与 taker 缺失仍阻塞，不补 0 或未来值。
- OI+taker 立即反转 V2 在 2023-10～2024-04 为 653 笔，标准成本 EV/PF `-0.083R/0.895`；回踩接受 V3 在 2025-07～2026-05 为 673 笔，`-0.146R/0.820`，两者 Discovery/Validation 均未通过。
- 为更早未见窗口补齐 OKX 早期 `vol_quote` 合约名义重建、99.9% 排名覆盖、partial 15m 缺口阻塞和旧表数值紧凑存储；未使用 1m 信号。
- 反弹失败顺势空头 V4 在 2022-07～2023-06 为 313 笔、约 26.1 笔/月，标准成本 EV/PF `-0.099R/0.874`；Discovery 为负、Validation 仅微正，移除 Top3 后 `-51.44R`。
- V2/V3/V4 全部 research-only 淘汰；未部署、未切换生产、未触发下单、撤单、平仓或交易所 mutation。

下一步：停止从已见窗口扫描 BOS/FVG、96/192 根、影线、等待期和退出参数。只有新增 forward OOS，或先冻结与 OHLC 结构、funding、OI/taker 翻转都正交的新市场机制，才允许继续实验。

## 2026-07-21：15m 残差、杠杆资金流与订单簿正交因子复核

- BTC 残差动量单腿 668 笔，标准成本 EV/PF `-0.1524R/0.8134`；残差极端单腿回归 362 笔，
  标准成本 `-0.0259R/0.9622`，两者均说明“定义为相对收益、执行为裸币种腿”没有稳定优势。
- 杠杆资金流双向 V1 在修复覆盖统计语义后得到 490 笔，标准成本 `-0.0365R/0.9470`；其中
  Short 70 笔虽为正，但独立旧窗口的下行延续空头只有 275 笔、标准成本 `0.0143R/1.0209`，
  回撤 `23.59R`，按门禁淘汰。
- 静态 1% 订单簿深度方向确认总体 4h 前瞻为负；对侧深度衰减动态因子 911 个确认观测的
  4h 均值 `+0.3374%`，但 Discovery 仅 `+0.1409%`、命中约 50%、Long 为负，未达到预注册因子门禁。
- 修复 Binance 官方 `bookDepth.percentage` 在 2026 年由整数文本变为 `-1.00/1.00` 的解析兼容；
  仅修正协议格式，不改变因子规则，最终 3,262/3,264 个候选日文件有效、0 个内容解析失败。
- 上述策略/因子均为 research-only；没有部署、Paper/Live、下单、撤单、平仓或生产 mutation。

下一步：停止继续叠加 BOS/FVG/CHoCH、OI/taker 或订单簿过滤器。已预登记独立
`market_beta_hedged_residual_mean_reversion`：以冻结 Beta 同时成交币种腿和 BTC 反向对冲腿，
只在双腿共同 15m 收盘判断价差、下一共同开盘退出，并计入四次成交成本；只运行一次独立窗口。

## 2026-07-21：BTC Beta 对冲双腿残差回归 V1

- 冻结 2024-07～2025-06 current-live-only Top60 独立残差窗口；每 4h 只选一个 24h 残差极端且
  6h 已转向的候选，同时成交币种腿和固定 Beta 的反向 BTC 腿。
- 修正双腿现实语义：只在共同 15m 收盘判断价差止损/目标，下一共同开盘退出并承受跳空；成本按
  两腿进出四次成交和两腿不利资金计入，不使用两条 K 线各自 high/low 拼造同步路径。
- 6 项因果/成本测试与独立二进制构建通过；2,190 个决策时点无覆盖阻塞，最终 530 组交易、
  月均 `44.17` 组、3/12 个正收益月。
- 费前 EV/PF 已为 `-0.0769R/0.8568`；标准成本后为 `-0.2327R/0.6272`，最大回撤
  `132.43R`；Long/Short Residual 两边都为负，移除前三盈利币后净 `-132.31R`。
- 按预登记早停永久淘汰，不调整节奏、score、Beta、风险或退出，不计算统一资金组合权益；没有
  Paper/Live、部署、下单、撤单、平仓或生产 mutation。

下一步：残差家族停止。只允许先建立不依赖 BOS/FVG/CHoCH、裸价格残差或订单簿方向确认的
跨交易所永续基差/溢价因子面板；在冻结候选和规则后证明边际价值，未通过则不生成交易策略。

## 2026-07-21：OKX—Binance 7 日基差 z-score 因子面板

- 使用 2022-07～2023-06 current-live-only Top60，严格映射当前 Binance live crypto perpetual；
  79/81 个合约映射，请求 1,106 个官方 15m 月包，961 个有效、145 个历史未上市 404、0 个无效，
  共解析 2,782,058 根并验证官方 SHA-256。
- 首次下载因单个 HTTP 200 响应体中断而在 outcome 前失败；只补充响应体错误有界重试，7 项
  格式/因果/映射测试和独立二进制构建通过，未修改任何因子规则。
- 1,856 个 extreme 样本的 1h/4h/24h 平均毛配对收益为
  `0.0600% / 0.0724% / 0.0862%`，4h 命中率 `80.39%`；Discovery/Validation 和正/负 z
  均为正，证明微小基差确实高概率收敛。
- 但 Discovery/Validation 的 4h 毛收益仅 `8.04bps / 6.46bps`，相对对照增量只有
  `1.13bps / 2.44bps`，远低于两腿进出约 `32bps` 成本；`factor_gate_passed=false`。
- z-score 机制永久淘汰，不因高命中率生成策略，不扫描 z-score、lookback、节奏或持有期；没有
  Paper/Live、部署或交易 mutation。

下一步：只允许在独立窗口测试由成本合同直接推导的“相对 7 日均值首次跨越 50bps 绝对偏离”事件；
它必须同时满足经济幅度、50～120 笔/月机会密度和前后半年稳定性，否则整个跨交易所基差分支停止。

## 2026-07-21：OKX—Binance 50bps 可执行基差首次越界

- 在独立 2024-07～2025-06 窗口逐 15m 计算当前/上一 7 日均值偏离；115/119 个当前 Binance
  live 合约映射，1,420/1,610 个官方月包有效、190 个历史未上市 404、0 个无效，共 4,094,598 根。
- 9 项因果、首次越界、映射和格式测试与独立二进制构建通过；35,040 个决策时点无覆盖阻塞，
  2,040,341 个完整同步因子。
- Top1 后只有 501 个 50bps 可执行事件，月均 `41.75`；按 4h 聚类只剩 107 个有效市场事件，
  均未达到预登记频率和独立样本门槛。
- 1h/4h/24h 毛收益为 `0.1056% / 0.1809% / 0.2449%`，4h 命中 `65.67%`，但仍低于
  四次成交约 `0.32%` 成本；正负方向的 4h 均值也都低于成本。
- Discovery/Validation 可执行样本为 `21/480`，4h 均值从 `0.5121%` 降到 `0.1664%`，
  说明机会严重依赖市场阶段；`factor_gate_passed=false`。
- 跨交易所基差分支永久停止，不扫描 50bps、32bps、lookback、节奏或持有期，不进入交易策略、
  Paper/Live、部署或 mutation。

下一步：改为验证 Binance premium index 的“下跌时显著折价、随后 1h 修复”是否能作为更及时的
去杠杆衰竭因子；先做因子面板，未通过则不生成新策略。

## 2026-07-21：Binance premium index 折价修复因子面板

- 使用 2022-07～2023-06 current-live-only Top60；79/81 个当前 Binance live 合约映射，
  795/1,106 个 premium 月包严格有效、146 个历史未上市 404、165 个内容完整性无效，解析
  2,298,347 根；无效文件仅阻塞，不补值。
- 11 项官方格式、校验和、映射、连续性、因果与选择测试及独立二进制构建通过；2,190 个决策中
  20 个价格覆盖阻塞，2,537 个前二候选 premium 完整。
- 仅 237 个“至少 5bps 折价且 1h 修复”确认，月均 `19.75`、4h 聚类 199 个有效事件；
  1h/4h 毛收益 `0.0711% / 0.0938%`，4h 命中 `49.37%`，低于成本和全部因子门槛。
- Discovery 4h `+0.1706%`，Validation 变为 `-0.0722%`；样本、方向和跨期稳定性失败，
  `factor_gate_passed=false`。
- premium 分支永久淘汰，不扫描 5bps、1h、候选数、价格窗口或持有期；没有生成交易策略、
  Paper/Live、部署或 mutation。

下一步：冻结市场中性的横截面动量价差；每 8h 等名义做多 24h 最强币、做空最弱币，固定下一
共同 15m 开盘与 8h/24h outcome，先验证毛价差是否稳定高于四次成交成本和目标频率。

## 2026-07-21：Vegas Universal 4H V68 成本效率与 V69 普通放量首回踩

- V68 在结果读取前冻结入场预计双倍往返成本 `<=0.20R`，只使用入场价、数量和初始风险；251 笔中拦截 39 笔，38 笔为压缩突破空头。
- V68 标准组合接受 212 笔、约 `7.02/月`，EV/PF `0.735R/2.421`；双倍成本 `0.671R/2.230`。质量改善但频率、IS Sharpe/PF、OOS Recovery 和弱 walk-forward 未通过。
- 被过滤组在 IS 仍贡献约 `+12.85R`，到 OOS 才转为约 `-1.04R`；成本/R 是时间漂移风险特征，不是可继续扫描阈值的稳定删单规则。
- V69 为既有首次回踩家族增加独立 `2.0x` 普通放量阈值，旧版本默认行为保持不变；100 条配置全部 disabled，除该字段外无非预期配置漂移。
- V69 `15921..16020` 相对 V60 共同 251 笔、新增 3 笔、移除 0 笔、共同结果变化 0；DOGE/ETHFI/CFX 三笔新增多头全部盈利，合计约 `+1.156R`。
- V69 叠加成本门禁后接受 215 笔、约 `7.12/月`；标准 EV/PF `0.730R/2.432`，双倍成本 `0.666R/2.239`，但频率与分段稳健性仍失败。
- 相关策略、配置转换和组合回放聚焦测试已通过；没有覆盖旧版本、进入 Paper/Live、部署或触发任何交易 mutation。

下一步：不扫描成本上限、首回踩量能、MACD/FVG/BOS/影线或币种条件。若继续历史研究，只允许先冻结一个此前未回放、能独立补足组合机会密度且具有完整因果序列的新机制。

## 2026-07-21：V70 EMA144/169 隧道顺势回踩确认

- 在读取结果前冻结三棒“干净趋势侧 -> 进入并收复 EMA144/169 隧道 -> 突破确认”规则，使用独立策略键和版本，旧 V69 信号优先。
- 新增策略枚举、默认关闭配置、三棒因果判断、EMA 递推回滚和专属标签；聚焦测试及 CLI 构建通过，100 条配置全部 disabled。
- 正式回测 `16021..16120` 相对 V69 共同 254 笔、新增 83 笔、移除 0 笔、共同结果变化 0；频率机会充足但质量失败。
- 新增多头 69 笔、合计 `-16.143R`、PF `0.453`；新增空头 14 笔、合计 `-0.663R`、PF `0.943`。多头 IS/OOS 都为负，空头 IS PF 只有 `0.290`。
- 多头虽约 59% 胜率，但赢家平均仅约 `+0.326R`，最大损失组平均约 `-1.053R`；空头依赖极少数大盈利。预计执行成本约 `0.13R`，不是主要根因。
- 按预登记增量门禁早停，不再做事后过滤或参数扫描；未进入 Paper/Live、未部署、未触发交易 mutation。

下一步：将 EMA 隧道保留为背景诊断，不再作为新增入场家族。下一候选只能采用新的、信号时点可完整重建的市场机制，并先冻结规则再回放。

## 2026-07-21：V71 固定成交量价值区突破回踩

- 在读取结果前冻结 48 根历史、24 分箱、70% 价值区、`2.0x` 放量突破与下一棒回踩接受规则；不使用 MACD/FVG/BOS/EMA 作为触发条件。
- 首轮 `16121..16220` 因新增信号影响旧持仓退出而作废；改为独立家族回放，再由组合层执行 V69 旧交易优先，未改任何市场阈值。
- 有效 `16221..16320` 共 97 笔：总 EV/PF `-0.299R/0.428`，IS `-0.250R/0.537`，OOS `-0.383R/0.225`。
- 多头 62 笔、44 胜，但赢家平均仅 `+0.215R`、止损约 `-1.05R`；空头 35 笔仅 5 胜。12 根路径中多头仅 26 笔先于止损达到 2R，延长持有也达不到目标期望。
- 按增量门禁早停，不进入组合晋级或参数扫描；100 条配置保持 disabled，未进入 Paper/Live、未部署、未触发交易 mutation。

下一步：价值区回踩不再迭代。若继续 4H 历史研究，只能测试更早、无需等待回踩确认的独立价值区突破语义，并在结果前冻结止损与单一退出合同；否则等待新的 forward OOS。

## 2026-07-21：V72 固定价值区即时放量突破

- 使用新策略键冻结“前 48 根价值区 + 当前 `2.0x` 放量从区内收盘突破”，只把 V71 入场提前一棒，保留边界止损与动态退出。
- 8 项因果测试、配置转换、策略枚举和 CLI 构建通过；100 条配置全部 disabled。
- `16321..16420` 共 702 笔，整体 EV/PF `-0.002R/0.997`，OOS `-0.169R/0.793`，按增量门禁拒绝。
- 多头 190 笔在 IS/OOS 均大幅为负；空头 512 笔在 IS/OOS 均为正，整体 `+0.219R/PF 1.272`，但仍远低于目标。
- 多头表现为短促挤压后回落；空头低胜率但少数赢家平均约 `+4.66R`，形成显著长尾。未进入组合、Paper/Live、部署或交易 mutation。

下一步：V72 本身不做方向或阈值补丁。若继续，只把“空头价值区跌破 + 标准 ADX/DMI 趋势强度”登记为新的 V73 版本，承认方向选择已被查看并要求独立验证。

## 2026-07-21：V73 空头价值区跌破 + ADX/DMI

- 在任何 ADX 分桶前冻结 `DMI/ADX 14/14`、`ADX>=25`、`-DI>+DI`，只允许 V72 空头，方向选择显式标记为后验假设。
- 新增 Wilder DMI/ADX 因果实现及 10 项相关测试，配置转换和 CLI 构建通过；100 条配置全部 disabled。
- `16421..16520` 共 200 笔，EV/PF `-0.077R/0.911`；IS `-0.214R/0.754`，OOS `+0.381R/1.448`。
- ADX 只改善后期样本、破坏早期样本，属于滞后和时间漂移；按门禁拒绝，不扫描周期或阈值。
- 未进入组合、Paper/Live、部署或交易 mutation。

下一步：不再给价值区跌破叠加 MACD/ADX/EMA/FVG/BOS。仅剩的可验证问题是 V72 空头长尾是否能用固定有效风险 `2R` 退出降低收益集中度；必须先预登记单一目标再回放。

## 2026-07-21：V74 原始价值区跌破空头固定 2R

- 在路径分析前冻结最终有效初始风险 `2R`，替换动态止盈；入口、价值区、量能、止损与 V72 一致，不继承 ADX。
- 新增通用版本化 R 目标解析与回归测试，配置转换和 CLI 构建通过；100 条配置全部 disabled。
- `16521..16620` 共 527 笔，EV/PF `+0.091R/1.139`；IS 为正，OOS `-0.105R/0.856`。
- Top3 占总净 R 约 12.3%，集中度下降但入场失败率仍过高，按门禁拒绝，不扫描其他目标。
- 未进入 V69 组合、Paper/Live、部署或交易 mutation。

下一步：停止价值区趋势延续分支的指标与退出补丁。若继续使用成交量分布，只允许新立“上方放量突破后重新收回价值区”的失败拍卖均值回归空头，目标冻结为历史 POC。

## 2026-07-21：V75 上方价值区失败拍卖回归 POC

- 在实现前冻结突破前 48 根、24 分箱、70% 价值区；要求 `2.0x` 放量上破 VAH 后下一棒阴线收回 `POC—VAH`，共同高点外 `0.6%` 止损，冻结 POC 为唯一目标。
- 新增独立策略键、默认关闭配置、严格两棒因果判断和 POC-only 持仓合同；5 个指标测试、持仓测试、配置转换与 CLI 构建通过。
- `16621..16720` 共 80 笔，EV/PF `+0.106R/1.229`；IS `-0.065R/0.872`，OOS `+0.390R/2.013`，按门禁拒绝。
- Top3 占总净 R `160.49%`；POC 距离 `<0.5R` 虽跨期微正但 EV 仅约 `0.079R`，远目标组只在 OOS 被少数赢家拉高。MACD、RSI、EMA、确认量和实体尺度没有稳定救援分组。
- 100 条配置全部 disabled；未进入 V69 组合、Paper/Live、部署或任何交易 mutation。

下一步：结束成交量价值区家族，不对 80 笔事后扫描 POC 距离或收回深度。若继续补频率，只能预登记一个独立、非均值回归且不复用 FVG/BOS/MACD 投票的新机会家族。

## 2026-07-21：V76 Donchian 20 根放量通道突破

- 在结果前冻结前 20 根通道、当前 `2.0x` 放量收盘突破、2ATR 保护位和最终有效风险固定 2R，不叠加任何旧指标门禁。
- 新增独立策略键、默认关闭配置、因果通道判断和版本化固定目标；6 个指标测试、持仓测试、配置转换与 CLI 构建通过。
- `16721..16820` 共 1,405 笔，EV/PF `+0.104R/1.163`；IS `+0.169R/1.273`，OOS `-0.067R/0.904`。
- 多头 OOS `-0.264R/PF 0.660`，空头 OOS `-0.033R/PF 0.952`；两个方向都不能独立保留。Top3 仅占净 R `4.02%`，问题是广泛时间漂移而非头部集中。
- 100 条配置全部 disabled；未进入 V69 组合、Paper/Live、部署或任何交易 mutation。

下一步：停止从当前历史继续增加 OHLC 派生指标家族。MACD、FVG/BOS、EMA、成交量价值区和 Donchian 都已分别证伪；当前只能保留 V69/V65 冻结规则累计真正未见 forward shadow，或先取得新的、完整 point-in-time 外生历史数据再立候选。

## 2026-07-21：V77 Donchian 突破后一棒接受

- 在查看 V76 后续路径前冻结紧邻一棒确认：种子沿用 Donchian20 + `2.0x` 量能，下一根须同向收在冻结通道外，通道内侧 `0.6%` 止损和固定 2R。
- 新增独立策略键、默认关闭配置、严格相邻时序与过期测试；7 个指标测试、持仓目标测试、配置转换和 CLI 构建通过。
- `16821..16920` 共 676 笔，EV/PF `+0.073R/1.112`；IS `+0.095R/1.150`，OOS `+0.004R/1.006`，按质量门禁拒绝。
- 多头 OOS `-0.371R/PF 0.539`；空头跨期为正但总 EV/PF 只有 `+0.144R/1.234`。ETH 贡献约占总净 R 46.8%，不能单独保留。
- 100 条配置全部 disabled；未进入 V69 组合、Paper/Live、部署或任何交易 mutation。

下一步：不扫描第二根确认、仅空头、通道周期、量能、边界或目标。当前 OHLC 派生机会家族已无新的独立证据；继续目标只能依靠冻结 forward 样本或新增完整 point-in-time 外生数据。

## 2026-07-21：V77 MACD 亏损分层审计

- 使用已持久化的信号时点 `MACD 12/26/9` 做零轴、柱体方向和归一化幅度诊断，没有修改或重跑策略。
- V77 多头 OOS 80 笔全部处于柱体同向增强状态；其中 DIF 同向 73 笔仍为 `EV -0.345R / PF 0.566`，MACD 顺势确认不能修复多头。
- 空头标准同向组总 PF 仅 `1.155`；表面较强的 DIF 逆向过渡组 31 笔中 30 笔位于 IS，OOS 只有 1 笔，不具备晋级覆盖。
- 亏损单 `|DIF|/price` 在多空两侧均高于盈利单，支持“动量越强越可能是成熟趋势末端”的解释；该特征为后验发现，不设置阈值、不生成 V78。
- 配置仍全部 disabled；未进入 Paper/Live、未部署、未触发任何交易 mutation。

下一步：继续累计冻结后 forward 样本；在新样本形成前，不再用当前历史扫描 MACD 周期、交叉、幅度或方向组合。

## 2026-07-21：BTC 日级 OI 扩张空头门禁验证

- 核对当前本地外部数据事实：通用快照并非全币池；只使用可通过 OKX 公共只读入口刷新并落入 BTC 分片表的日级上下文，不把旧研究清单的 93 币缓存当作当前数据。
- 5/6 月发现组中，V69/V77 的 BTC OI 扩张空头分别为 `EV -0.397R/-0.431R`，先预登记唯一规则“OI 扩张禁止新空头”，未叠加资金费、主动流或多空比。
- 刷新 BTC OI 到 2026-07-18 后按 24 小时延迟连接：7 月 V69 被阻塞组为 `EV +1.534R / PF 3.903`，V77 为 `EV +0.428R / PF 1.794`，时间关系反转。
- 该规则会错误拦截 XRP `2026-07-05 00:00 UTC` 的约 `+4.124R` 空头，按门禁拒绝，不生成 V78、不扫描 OI 幅度或第二因子。
- 研究配置保持 disabled；未进入 Paper/Live、未部署、未触发任何交易 mutation。

下一步：BTC 日级 OI 仅保留为上下文审计。当前样本不再组合 funding/OI/taker/ratio；继续目标仍依赖冻结后的新交易，或一份当前真实存在、完整 point-in-time 的全币池因子档案。

## 2026-07-21：横截面动量与 Funding Carry 因子面板

- 横截面动量每 8h 等名义做多 24h 最强、做空最弱，共 `1,092` 组、`546` 个固定起点 8h 事件；修复了相邻时点链式吞并全年为 1 个事件的研究工具缺陷，6 项回归测试通过。
- 动量总体 8h/24h 毛价差 `-0.1931%/-0.0714%`；Discovery 24h `-1.1425%`，Validation `+1.0234%`，方向翻转，精确反向也必然跨期失败。
- Funding carry V1 使用 Binance 官方 funding/regular 15m 月包与 SHA-256；Top60 的 80% 要求结构上不可实现，单时点 8h funding 共同覆盖最多 41，零 outcome，不原地降门槛。
- 独立 V2 只把母集冻结为至少 30 个共同可交易成员；`931` 个可排序时点中只有 12 个费率差达到 32bps，完成 11 组，约 `0.92` 组/月。
- V2 下一 funding PnL 平均 `+0.4373%`，但价格相对 PnL `-0.5925%`，零成本/标准成本后为 `-0.1552%/-0.4752%`，命中率 `27.27%`；FLOKI 参与 `45.45%`，永久淘汰。
- 相关测试 17 项通过，两个二进制均通过检查；所有研究路径只读，未进入组合、Paper/Live、部署或任何交易 mutation。

下一步：不从已查看 outcome 反向选择动量月份、funding 方向或降低 32bps 阈值。继续职业级目标必须先取得新的完整 point-in-time 外生因子档案，或等待冻结策略形成真正 forward 样本。

## 2026-07-21：Top Trader 定位因子面板

- 使用 Binance 官方 daily metrics 与 regular 15m 数据，固定每 8 小时读取精确 `T-5m` 已发布点；外生定位指标为原生 5m，入场、持有和收益评估全部保持 15m，没有使用 1m。
- 修复官方 `create_time` 存在 1～6 秒发布抖动却被旧解析器误判为断档的问题；只允许最多 60 秒归一到名义 5m 槽，超过 60 秒拒绝，并新增相邻、超时、跨日重复/冲突回归测试。首次 814 组不完整结果作废。
- 修复后 1,095 个决策点形成 1,089 组完整价差：头部交易者金额/账户确信度因子总体 24h 毛价差 `-0.2837%`，Discovery/Validation 为 `-0.6963%/+0.1358%`，只有 5/12 月为正；精确反向总体仅 `+28.37bps`，低于 32bps 成本且验证期转负。
- 最后一项头部持仓/全市场人群分歧因子总体 8h/24h 毛价差 `-0.0757%/-0.4572%`，对照 24h `+0.0251%`；Discovery/Validation 24h 为 `-0.7200%/-0.1900%`，只有 4/12 月为正。
- 分歧因子精确反向总体约 `+45.72bps`，但 Validation 仅约 `+19.00bps`，低于标准成本；按结果前冻结的停止规则，不再排列 positioning、taker、OI 字段、方向、rank、频率或持有期。
- 相关 26 项测试和运行入口检查通过；所有路径只读，没有进入组合、Paper/Live、部署或任何交易 mutation。

下一步：当前已查看历史上的 OHLC 结构、成交量、funding/OI/taker、订单簿、跨交易所基差、premium、横截面动量/carry 与 positioning 家族均无稳定净优势。继续职业级目标只能依赖新的完整 point-in-time 外生数据，或等待冻结版本积累真正未见 forward 样本。

## 2026-07-21：V69 组件去耦审计

- 将只读组合回放拆为机会来源、预计成本质量政策、组合容量三段；旧成本门禁接受交易 ID/顺序保持一致，币种输入反转后整份报告一致。
- V69 原始 254 笔拆为 Legacy 153、压缩突破空头 83、即时扫流动性空头 4、首次回踩 14；39 笔成本拒绝中 38 笔来自压缩突破。
- 首次回踩 14 笔虽全部盈利，但标准/双倍成本 EV 仅 `0.475R/0.435R`；V60 到 V69 的可审计成交口径为 11 -> 14，三个新增多头没有使该家族独立达标。
- 成本过滤后的压缩突破为 45 笔，标准/双倍成本 EV `0.765R/0.704R`，但全部为空头，最弱 walk-forward 窗口仅 `EV 0.593R / PF 2.068`，仍不具备稳健性。
- 多头原始/通过为 43/42 笔，EV `0.443R`；空头为 211/173 笔，EV `0.799R`。多空差距来自机会家族结构，不是容量选择；本轮 215 笔质量通过交易全部被组合接纳。
- 聚焦测试 51 项通过，标准与双倍成本总指标精确复现。包级检查被无关的 Market Velocity 未完成文件阻塞，本次未修改该用户代码。
- V69 保持 `rejected_not_promotable`；没有创建新版本、配置、Paper/Live、部署或交易 mutation。

下一步：新机会家族必须先独立通过总样本、IS/OOS、walk-forward 与成本压力，再允许进入组合；不得继续借用 V69 聚合指标掩盖组件失败。

## 2026-07-21：Market Momentum v38 收盘价 MACD 动量衰减门禁

- 从 v36 分叉独立 research-only 规则，只新增收盘价 MACD(12,26,9)“前一根负柱、当前柱缩短”门禁；未扫描周期、金叉、零轴或幅度。
- 新增因果 MACD 序列、研究参数门禁、审计字段与未来 K 线不影响当前判断测试；主回测文件拆分后为 1885 行，未超过 2000 行硬上限。
- 动态开发 Top60 因当前 live 币池漂移得到 0 个候选，判为不可与旧 v36 比较；冻结扩窗 manifest 的 v36 基线精确复现 84 笔、EV/PF `-0.0194R/0.9719`。
- v38 只剩 17 笔，EV/PF `0.1870R/1.2927`、Sharpe `0.4431`、最大回撤 `3.1576%`；前半段仍负，12 个月仅 2 个月为正，移除 Top3 盈利币后总收益 `-16.9635U`。
- MACD 有方向性改善但以删掉 `79.76%` 交易为代价，样本、频率、EV、PF、Sharpe、月份和集中度门禁全部失败；v38 淘汰，不进入 Paper/Live，不扫描 MACD 参数。

下一步：不再围绕 v36 已见扩窗对 MACD、BOS、FVG、影线或 192/96 做补丁；继续职业级目标需要新的 point-in-time 外生数据或真正冻结后的 forward 样本。

## 2026-07-21：Market Momentum v36 开发池最优结果恢复落库

- 从 2026-07-20 原始运行产物恢复 v36 开发池 57 笔逐笔 `framework_equity_trade`，校验为 41 个币、30 胜 27 负、净 `43.139277884994R`、利润 `129.20929799U`。
- 新增本地 `quant_core.back_test_log` 记录 `17022`，并写入 114 条 `back_test_detail` 生命周期明细（57 开仓、57 平仓）；初始止损、初始风险金额和出场净 R 均完整。
- 记录显式标记为 `restored_original_research_run_artifact`、`ResearchBar`、`promotion_eligible=false`；原动态 Top60 成员清单未冻结，因此不伪装为 2026-07-21 重跑结果。
- 汇总复核为 EV `0.7568294366R`、PF `2.7544110627`、胜率 `52.6316%`、Sharpe `3.3213550335`、逐币隔离最大回撤 `9.17683513%`；仍因后半段、Q4 与频率失败保持淘汰。

下一步：`17022` 仅作为开发池历史研究快照查询；扩窗基线继续使用 `16921`，二者不得混称或用于 Paper/Live promote。

## 2026-07-21：Vegas Universal 4H V79 压缩突破空头独立回放

- 预登记后新增默认关闭的 `CompressedRangeBreakoutConfig.standalone` 和独立策略键，只解除 Legacy、扫流动性及其他研究家族的入口优先级；V79 相对 V69 的 100 条策略参数和风险配置没有其他漂移。
- 本地创建 100 条 disabled 配置并完成 `back_test_log.id=16922..17021`：86 笔原始交易全部带 `COMPRESSED_RANGE_BREAKOUT_SHORT`，其他机会家族标签为 0，隔离合同通过。
- 相对 V69 压缩组件共同 83 笔且结果零漂移，新增 3 个压缩标签；但相对 V69 完整机会集只有 CRO `2024-06-11 08:00` 是新增交易且净 `-1.0567R`，XRP/PI 同时点已有 Legacy 交易。去耦没有发现新的频率来源。
- 成本门禁接受 48 笔、约 `1.67笔/月`。标准成本 EV/PF 为 `0.726886R/2.351645`，Sharpe `0.719482`、Recovery `1.456673`；双倍成本 EV/PF 为 `0.665186R/2.178731`，Sharpe `0.667708`、Recovery `1.343422`。
- 最后固定 walk-forward 窗口标准成本为 `EV -0.383113R / PF 0.560804`，双倍成本为 `EV -0.454186R / PF 0.515006`；移除前三个盈利事件簇后标准组合为负，且熊市/中性状态均亏损。
- XRP `2026-07-04 20:00` 与 `2026-07-06 20:00` 在 V79 均无开仓；没有用已查看的压力位、长下影或 MACD 事后补规则。
- V79 状态定为 `rejected_frequency_robustness_and_double_cost_pf`；100 条配置继续 disabled，未进入 Paper/Live、未部署、未触发真实交易 mutation。

下一步：停止在已见历史中继续扫描压缩宽度、跌破幅度、量能、MACD、FVG/BOS、影线、止损或成本阈值。V69 只保留为不可 promote 的历史质量前沿；新证据必须来自冻结后真正未见的 forward 样本或新的完整 point-in-time 外生数据。

## 2026-07-21：Vegas ETH 15m 随机参数性能优化与首轮迭代

- 原随机代码实际遍历笛卡尔网格：进度声称 `777,600` 组，生成器实际只有 `194,400` 组；RSI 组合索引被消费两次，后续风险维度错位，且 `k_line_hammer_shadow_ratios` 未进入真实空间。默认 40,000 根 K 线对应约 77.76 亿次 candle-step，不具备安全启动条件。
- 改为固定 seed 的常量内存无重复采样，本轮数量、batch、并发、单组耗时阈值均可审计；CPU replay 移入 blocking 线程，增加 `BACKTEST_ONLY_TARGETS=ETH-USDT-SWAP@15m` 精确入口，未修改任何 live worker 或生产策略指针。
- 第一轮 `17028..17091` 使用 2025-05-28 17:00 至 2026-07-19 08:45 UTC 的 40,000 根已确认 15m K 线。64 组全部亏损，平均 `-90.8103`、最佳 `-79.6453`，平均约 1,248 笔；4 核运行约 42 秒，峰值 RSS 约 48 MiB。
- 以 disabled 本地诊断配置 `b7e15015-0001-4e7a-8000-202607210001` 重放第一轮最佳组 `17092`：966 笔、净 PF `0.7193`、净期望 `-0.082449/笔`；按 legacy 双边费率重建的手续费约 `71.1148`，费前仍约亏 `8.5305`。
- 第二轮只对 ETH 15m 使用成本感知空间：1%/2% 止损、1.8R/2.2R/2.6R/3.0R 止盈及第一轮相对较优的入场维度。`17093..17156` 仍为 64/64 亏损，平均 `-79.4833`、最佳 `17107=-67.0149`。
- `17107` 共 904 笔，胜率 `54.42%`，净 PF `0.7651`、Sharpe `-2.1294`、最大回撤 `71.50%`；估算手续费 `85.0035`，费前利润约 `+17.9886`，费前 PF 也仅 `1.0729`。频率足够但优势不足，不能因高胜率晋级。
- 第二轮发现专用 `ENABLE_RANDOM_TEST_VEGAS` 未被底层识别，误写 133,262 条本地成交明细，峰值 RSS 升至约 219 MiB。已统一通用/Vegas/NWE 三种随机开关；修复后 `17157..17164` 为 8 条汇总、0 条成交明细，峰值 RSS 回落到约 43 MiB，单组墙钟约 `0.732s`。
- 聚焦回归、CLI 构建、格式和 diff 检查通过；现有 warning 未扩展。所有配置均为 research/disabled，本次未部署、未切换生产、未触发 Paper/Live 或真实交易 mutation。

下一步：停止扩大当前 OHLC 参数网格。若继续 ETH 15m，先预注册一个能减少反向信号平仓与高频费用侵蚀的独立机制，并在新的固定 seed / 时间窗口上做小样本筛选；没有净 PF 明显改善前不进入 walk-forward、跨币种或 promote。

## 2026-07-21：15m 趋势耗竭一次性状态去重 V1

- 新增独立 `market-trend-exhaustion-one-shot-reversal` 研究规则；历史背景、极端量、成本与退出参数保持冻结，只改变同一趋势背景的 setup 消费方式。
- 数据链路直接扫描本地已完成 15m K 线，并通过当前 `exchange_symbols` live、线性、OKX USDT 永续过滤；没有读取 `market_rank_events`、episode、1m，也没有纳入已标记退市币。
- 状态扫描覆盖 216 个候选表：167,461 次 armed、167,364 次中性重置；66,902 个有效极端量 setup 只发出 20,644 个，最终同币持仓锁成交 18,360 笔，约 1,465.31 笔/月。
- 胜率提高到 `35.6427%`，但成本后 EV `-0.072563R`、PF `0.888247`、Sharpe `-7.194071`、最大单币隔离回撤 `82.4487%`；前后半段及 Q1～Q4 均未形成正 EV。
- V1 按门禁标记为 `rejected_frequency_and_edge`。后验确认单根中性 K 线重置会在 192 净变化与 96 回归 OR 阈值附近频繁抖动；该结论不用于修改 V1，只能进入下一独立预登记假设。
- 聚焦状态机、参数门禁、当前 live 币池 SQL 与滚动回归因果一致性测试通过；未落库回测明细、未部署、未进入 Paper/Live、未执行任何真实交易 mutation。

下一步：若继续提升胜率，先冻结“连续多根中性 + 趋势失效确认”的重置语义，并验证状态 episode 数是否真正下降；继续严格只使用信号时点已完成 15m K 线。

## 2026-07-21：15m 趋势耗竭稳定重置 V2

- V2 在结果读取前冻结连续 8 根中性 15m K 线的重置确认；只修改状态生命周期，不改变 192/96 趋势背景、量比、振幅、止损、Volume-ATR 止盈、成本、币池或样本窗口。
- 默认 1 根确认精确复现 V1：167,461 次 armed、167,364 次重置、20,644 个状态后 setup、18,360 笔交易及全部核心绩效指标零漂移。
- 8 根稳定重置将 armed 降至 97,037、重置降至 96,930、状态后 setup 降至 17,566，最终交易降至 16,288 笔，约 1,299.94 笔/月；防抖生效但频率仍严重超标。
- V2 胜率 `36.2782%`，成本后 EV `-0.070694R`、PF `0.889728`、Sharpe `-6.688597`、最大单币隔离回撤 `82.3868%`；相对 V1 只改善 `0.001869R` EV 和 0.64 个百分点胜率。
- 前后半段及 Q1～Q4 全部为负，Q2 最差为 `EV -0.140128R / PF 0.797544`；移除前三盈利币后总利润进一步降至 `-3,487.77U`。
- V2 按冻结门禁标记为 `rejected_stable_reset_without_edge`。没有扫描 4/8/12 长度，没有追加 MACD/FVG/BOS/影线，未落库、未部署、未进入 Paper/Live 或任何真实交易 mutation。

下一步：停止继续调重置长度。若建立 V3，必须先冻结多空方向诊断，再仅测试极端量后生产可见短窗口内的价格拒绝/回收确认；不得利用窗口后的 K 线决定是否入场。

## 2026-07-21：Core WebSocket 与 AWS CPU 热路径优化

- OKX 策略 WebSocket 新增显式 ticker 开关，默认关闭；生产 ETH 4H 与 Universal 4H worker 都固定为 candle-only，保留 K 线业务流和原有 watchdog。
- 未收盘 K 线不再写 Redis 或数据库队列；收盘确认会在成交量不变时仍正确推进，重复确认不会重复落盘。
- 正常健康检查不再每 10 秒序列化并输出全部目标，只写紧凑 debug；异常仍保留总数和最多 20 个过期目标预览。
- Market Velocity REST 全市场扫描默认由 10 秒降至 60 秒，生产合同固定该值，预期请求频率降为原来的六分之一。
- `cargo test -p rust-quant-market` 通过 22 项；`cargo check -p rust-quant-cli` 通过；生产部署合同通过 7 项；现有 warning 未由本轮扩大。
- `c92d33f` 已经 CI/CD 发布：生产 CloudWatch CPU 从部署前约 `42%～50%` 降至连续三个窗口约 `11.4% / 12.1% / 12.0%`，对应 `CPUSurplusCreditsCharged` 连续为 `0`。
- `70f3281` 已经 CI/CD 发布：雷达同一分表只在首次访问时 ensure，K 线按最多 1000 根批量 UPSERT，内容未变化时不更新；本地 23 项单元测试和真实 PostgreSQL 集成测试通过。
- 生产 revision 已核对为 `70f3281`，容器重启数为 0；上线后 4 个连续雷达周期均正常产生事件，25 次 DDL 全部集中在冷启动第一轮，持久化错误为 0。
- 本轮未触发任何 Paper/Live 或交易 mutation。

下一步：继续观察 24 小时 CPU credits、CPU 利用率、确认 K 线落库延迟和 Data Transfer；当前 CPU 已低于 t3.medium 基线，不在缺少新增业务需求的情况下引入全市场 1m 聚合服务。

## 2026-07-21：15m 趋势极端量外部证据纠偏

- 对照 TradingView 官方 Volume、图形确认、Relative Volume/RVAT 与 Volume Delta 定义，确认“极端总成交量必然反转”没有平台依据：放量既可能延续，也可能耗竭，反转还需要价格确认；方向性量能需要更低层成交数据聚合。
- 本地 V2 诊断显示 `94.22%` 的极端量 setup 实体仍沿历史趋势；滚动量比从 `2～3x` 提高到 `5x+` 时，反向 EV 从 `-0.033015R` 恶化到 `-0.127569R`，振幅越大也越差。
- 实际实现 setup-open reclaim V3 后，新窗口 `5,068` 笔、EV/PF `-0.160582R/0.764387`；独立旧窗口 `3,962` 笔、`-0.143475R/0.801138`。旧的未来路径分组是诊断，不是可执行优势。
- 独立极端量延续 V1 在新窗口 `15,173` 笔、EV/PF `-0.090580R/0.864218`，旧窗口 `11,433` 笔、`+0.055022R/1.084146`；方向改为延续只在旧市场阶段微弱有效。
- 按 TradingView Regular RVAT 思路实现 UTC 同时点过去 10 日 RVAT10，缺任一时点即失败关闭。V2 新窗口 `11,092` 笔、EV/PF `-0.093252R/0.861429`，旧窗口 `8,257` 笔、`+0.090063R/1.137263`；最新窗口多空、前后半段和四分位全部为负。
- 所有新增入口只读取 `confirm=1` 的本地已完成 15m K 线及当前 live 币池；未读取 `market_rank_events`、episode、1m，未落库、未进入 Paper/Live、未触发交易 mutation。

下一步：停止继续扫描 MACD、FVG/BOS、RVAT 天数/阈值或退出参数。若用户允许扩大数据合同，再预登记“逐笔主动买卖量聚合成已完成 15m volume delta/taker imbalance”的独立策略；若只允许现有 15m OHLCV，则本策略家族到此停止。

## 2026-07-21：15m Taker Delta 背离反转 V1

- 本地 `quant_core` 每币 15m 表只有 OHLCV 与 confirm，没有 taker buy/sell 或逐笔 side；OKX 历史逐笔端点又只覆盖近三个月，因此没有用普通成交量或 K 线颜色伪造方向量。
- 新增只读研究入口，使用 Binance USD-M 官方原生 15m 月包的 quote volume 与 taker buy quote volume；ZIP 继续执行官方 checksum、UTC 月份、单 CSV、15m 连续性和 current-live crypto perpetual 映射校验。
- 策略冻结为：192 根净涨跌 8% 或 96 根 R² 0.60 趋势；OKX 量比 2、振幅 1.4、实体 20%；下跌阴线但 taker buy share 至少 60% 做多，上涨阳线但 share 至多 40% 做空；下一根开盘、3% 止损、1.8R～3R Volume-ATR 目标、48h 上限、单边 8bps 成本。
- 开发窗口 current-live top60 月成员并集 155 个、Binance 映射 148 个；4,893,615 行有效 15m flow 形成 189 笔交易、177 个事件，成本后 EV/PF `+0.077393R/1.136688`，双倍成本 EV `+0.024072R`。Q2/Q3 为负，移除 BONK/GALA/MEME 后净收益 `-0.188560R`。
- 重新生成 crypto-only 旧窗口币池后，月成员并集 119 个、Binance 映射 115 个；4,094,598 行有效 flow 形成 129 笔、124 个事件。零成本 EV/PF 已为 `-0.058993R/0.904515`，成本后为 `-0.112376R/0.827757`；多空及前后半段均负。
- 两窗合计月均约 13.25 笔，远低于 50～120 笔目标；按四舍五入汇总 EV 约 `+0.000411R`、PF 约 `1.0007`，等同无优势。V1 标记 `rejected_temporal_instability_concentration_and_low_frequency`。
- 定向单元测试 8 项与 Release 构建通过；没有写回测业务表、没有回测 ID、没有进入 Paper/Live 或真实交易 mutation。

下一步：不要把单棒方向量直接升级为新门禁，也不要扫描 60%/40% 邻域。若继续，先做多棒累计 delta 分桶因子面板，固定 1h/4h outcome 检查其相对价格与总量是否有跨窗口增量信息。

## 2026-07-21：15m 多棒累计 Taker Delta 增量因子 V1

- 在结果前冻结四根 Binance 原生 15m taker quote Delta 合成 1h flow；每 4h 取一次决策点，价格和 outcome 继续使用本地已完成 OKX 15m，不读取 1m、事件归档或退市币。
- 为隔离价格和总量贡献，每个时点、每个价格方向按可见价格幅度中位数与过去 20 小时相对总量中位数形成 2×2 分层，再统计背离组减同向组的 time-level 1h/4h 差。
- 开发窗口 126,103 个完整 observation；下跌后反转多 4h 配对增量 `+0.035890%`，上涨后反转空为 `-0.054704%`。多头仅 3.589 bps，空头方向错误。
- 独立旧窗口 126,034 个完整 observation；多头/空头 4h 配对增量为 `+0.004369%/+0.055970%`。旧窗空头的小幅正差在开发窗翻负，多头后半年也翻负。
- 四个背离象限自身的 4h 平均方向收益都没有达到 16 bps；两个旧窗象限均为负。V1 标记 `rejected_no_economic_increment_and_temporal_direction_flip`。
- 5 项定向单测和二进制构建通过；研究入口只读，没有业务表写入、回测 ID、Paper/Live 或真实交易 mutation。

下一步：累计 Delta 不进入 15m 动量反转开仓门禁，也不基于已见结果改成只做多/只做空。停止扫描累计长度、强度阈值、持有期、MACD 或 BOS/FVG；下一次迭代需要真正新的 point-in-time 外生信息或冻结后的 forward 样本。

## 2026-07-21：全市场收盘 K 线亚秒监听实现

- 新增独立 `all_market_candle_volume_monitor`：从 OKX business WebSocket 分片订阅所有 active perpetual 的 1m K 线，盘中 `confirm=0` 在 DTO 构造前丢弃。
- 单一 Tokio 聚合任务以 Decimal 同步派生 5m/15m/4H，计算当前成交量相对前 20 根平均成交量的倍数；默认阈值 2.0，事件明确标记 `trading_signal=false`。
- 正常热路径无数据库、REST 和持久化等待；启动预热逐币种交接，断线缺口按币种异步修复，其他币种继续处理。
- 每分钟输出本机 P50/P95/P99、最大延迟、队列深度与 `<1s` SLO 是否满足；交易所何时发出 `confirm=1` 另以 `exchange_close_arrival_ms` 记录。
- 新服务已加入 runtime 镜像、生产 Compose、默认发布/回滚服务清单和只读 revision 验收。
- 当前工作区验证：Market 10 项、CLI 4 项、部署合同 7 项全部通过，专用二进制检查与部署脚本语法通过；现有 warning 未由本轮扩大。
- 未提交、未推送、未部署，也未触发 Paper/Live 或任何交易 mutation；生产亚秒 P99 仍需发布后的真实日志证明。

## 2026-07-22：15m RSI 放量横盘突破与背离 V2

- v1 与冻结 v36 均未覆盖；新增规则版本 `kline15m_rsi14_volume_bollinger_macd_divergence_structure_stop_v2`，只用于 ResearchBar 回放。
- 横盘背景改为信号前 96 根布林带宽的 20% 低分位，同时要求前一根 MACD(12,26,9) 主线和信号线绝对值均不超过价格的 0.15%；当前 RSI 必须严格高于 70 或低于 30，且收盘真实突破前一根上轨或下轨。
- RSI 背离独立于 30/70 极值门槛：当前价格相对最近一个由左右各 3 根已完成 K 线确认的同向价格枢轴创新低/新高，当前 RSI 至少反向改善 3 点；回看 48 根，不允许价格和 RSI 使用不同时间点，也不读取信号后的 K 线。
- 所有分支共用当前成交量至少为前 5 根均量 2 倍、反向长影线不超过实体、结构止损有效且不超过 3% 的门禁；优先级固定为背离、压缩突破、原 96 根趋势反转。
- 同样本为当前 live-only Top60、2025-07-01 至 2026-07-19、15m 已完成 K 线、单边手续费 5bps、单边滑点 3bps、48h 上限；原始候选 886,607，信号通过 10,413，实际 8,585 笔覆盖 42 个币。
- 总体净 EV/PF 为 `-0.493619R/0.578133`，胜率 `28.7711%`、Sharpe `-7.54864`、最大单币隔离回撤 `77.5016%`。空背离 2,491 笔 `-0.260404R`，多背离 2,781 笔 `-0.374244R`；向上压缩突破 151 笔 `-0.108579R`，向下压缩突破 122 笔 `-0.294241R`。
- 15 项策略定向测试和 427 项回测模块回归通过；二进制构建、格式、diff 与文件上限检查通过。现有仓库 warning 未扩大。
- v2 结论为研究失败；本轮没有 `--save-backtest-detail`，无回测 ID、数据库写入、Paper/Live、部署或真实交易 mutation。

下一步：不要把这次逻辑直接用于生产，也不要在已见结果上扫描 30/70、20% 分位、0.15%、48 根或 3 点邻域。若继续，先对亏损持仓做时序诊断，再预登记一个独立确认机制并用未见窗口验证。

## 2026-07-22：15m RSI 量价反转与 ATR 止损 V3

- v1、v2 和冻结 v36 均未覆盖；新增规则版本 `kline15m_rsi_divergence_breakout_net8_atr15_v3`，仅用于直接已完成 15m K 线 ResearchBar 回放。
- 全局量比改为当前成交量除以前 4 根已完成 K 线均量，最低 1.5；底/顶背离继续使用最近 48 根和左右各 3 根确认的同一价格枢轴，但要求当前 RSI 严格低于 30/高于 70，RSI 反向改善至少 1 点。
- 横盘背景继续使用信号前 96 根布林带宽最低 20% 与前一根 MACD 双线距零轴不超过价格 0.15%；向上/向下收盘突破只看方向 K 线、价格和放量，当前 RSI 不参与。
- 96 根分支只计算首开到末收净幅：净跌至少 8% 做多，净涨至少 8% 做空；移除 R² 替代，不要求当前 K 线颜色。全部分支统一执行做多上影线/做空下影线不超过实体 45%，零实体阻塞。
- 同一 K 线多个同向分支只开一次并组合触发原因，反向分支冲突直接不交易。初始止损固定为信号收盘价反方向 `1.5 * ATR14`；选择器对该版本禁止固定 3% 回退，止盈仍为既有 Volume-ATR `1.8R～3R`。
- 25 项策略边界测试、438 项回测模块回归通过，专用二进制与格式检查通过；策略文件 1,398 行低于 2,000 行硬上限，行数脚本仅报告 1,000 行目标提醒。
- 同 v2 样本仍为 current-live Top60、2025-07-01 至 2026-07-19、单边手续费 5bps、单边滑点 3bps、48h 上限。原始候选 886,607，信号通过 17,347，实际 8,992 笔覆盖 42 个币，约 713.5 笔/月。
- 总体净 EV/PF 为 `-0.133195R/0.845078`，胜率 `25.9453%`、Sharpe `-4.25877`、最大单币隔离回撤 `89.6022%`、总利润 `-1054.6972U`。相对 v2 的 `-0.493619R/0.578133` 明显改善，但仍不具备正优势且频率严重超标。
- 单分支均未达到职业门槛；只有“底背离 + 96 根净跌”组合 151 笔为正，EV/PF `0.121435R/1.158036`、Sharpe `0.79232`，仍远低于 `0.6R/2.2/1.5` 联合门槛。
- 应用户要求补充 ResearchBar 落库：先修正 v3 不要求 K 线颜色后 legacy 明细仍按涨跌幅反推方向的问题，并补齐主表 Sharpe/回撤、明细 ATR 初始止损、初始风险和净 R 映射。最终完整记录为 `back_test_log.id=17169`，包含 8,992 笔开仓与 8,992 笔平仓、42 个币；8,992 组开平仓无缺失、止损与风险无错配，旧的不完整记录 `17166` 已标记由 `17169` 替代。
- 本次仅写本地 `quant_core` 回测表，没有进入 Paper/Live、部署或真实交易 mutation。v3 状态仍为 `research_rejected_no_edge_and_excess_frequency`。

下一步：不要直接把 v3 用于生产，也不要在已见结果上把唯一正向组合后验裁成新策略。若继续，必须先预登记独立版本、固定未见窗口和事件聚类口径，再验证该组合是否具有样本外增量。

## 2026-07-22：15m RSI 放量反转 V4 移除压缩突破

- 新增独立规则版本 `kline15m_rsi_divergence_net8_atr15_no_sideways_breakout_v4`；v3 代码和已落库结果保持原样可重放。
- v4 候选集合只包含 RSI 极值背离与 96 根净幅反转，不再读取布林带压缩、MACD 零轴或上下轨突破条件。
- 全局门禁保持为当前量除以前 4 根均量至少 1.5；做多上影线或做空下影线不超过实体 45%；方向冲突不入场；初始止损为 `1.5 * ATR14`。
- 对照测试确认同一压缩突破 setup 在 v3 仍可触发、在 v4 多空均被拒绝；v4 背离与 96 根分支继续有效。
- `cargo fmt --all -- --check`、7 项 v4 定向测试及 research-only 身份测试通过，仅有仓库既有 warning。
- 本轮没有运行或落库 v4 回测，没有回测 ID，也没有进入 Paper/Live、部署或真实交易 mutation。

下一步：如需量化比较，应以 v4 独立 manifest 在冻结样本及未见窗口回放，再决定是否继续研究。

## 2026-07-22：15m RSI 放量反转 V5 因果异常量过滤

- 新增规则版本 `kline15m_rsi_divergence_net8_filtered_volume10_x2_atr15_v5`；v4 及其四根 1.5 倍量比保持可重放。
- v5 对最近 10 根历史 K 线逐根使用“该根自己的前 10 根原始均量”做因果标记；成交量 `>= 2.0` 倍即标记，所以 2 倍、2.1 倍和 3 倍都属于被剔除范围。
- 当前均量只统计最近 10 根中未标记的历史 K 线；当前 K 线不进入均量分母，完整保留为分子，当前量比也必须 `>= 2.0`。
- 历史标记阶段不递归剔除更早的异常量，避免顺序依赖；最近 10 根若全部被剔除则失败关闭，不回退原始均量。
- v5 的 RSI 极值背离、96 根净幅、45% 反向影线、方向冲突和 `1.5 * ATR14` 止损全部继承 v4；压缩突破仍保持关闭。
- RSI 策略族 31 项测试和 v5 ATR 止损测试通过；格式、差异及相关文件硬上限检查通过，仅保留仓库既有 warning 和两个低于 2000 行的目标提醒。
- 应用户授权按 v3 同一 current-live Top60、2025-07-01 至 2026-07-19、双向、单边手续费 5bps、单边滑点 3bps、48 小时上限执行全量回放，落库为 `back_test_log.id=17170`。
- 主记录包含 5,756 笔交易，`back_test_detail` 为 5,756 开仓 + 5,756 平仓，共 11,512 行且全部一一配对，覆盖 42 个币；初始止损、初始风险和 5,756 条平仓净 R 全覆盖。
- 净 EV/PF 为 `-0.081238R/0.901686`，胜率 `24.5309%`、Sharpe `-2.209516`、最大单币隔离回撤 `90.9215%`、总利润 `-631.5504U`、约 `457.29` 笔/月。前后半段 EV 均为负，42 个币仅 11 个盈利。
- 相比 v3 的 8,992 笔，v5 减少 `35.99%`；EV、PF 与 Sharpe 只从更差的负值收敛，胜率更低且最大单币隔离回撤更高，不能晋级。唯一略正的“底背离 + 96 根净跌”组合也只有 `+0.063203R/PF 1.081111`。
- 回测后按同方向相邻触发不超过 30 分钟聚类为 3,049 个事件，属于结果后诊断而非预注册有效事件证据。
- 落库审计暴露真实语义缺口：1,159 笔（20.14%）被通用 `max_loss_percent=3%` 选为比 ATR 更紧的保护位；全部 5,756 笔 `stop_loss_source` 为空；逐笔 `signal_value` 没有保存过滤量比的分子、分母或剔除数量；6 条月末 `结束平仓` 有 PnL/R 但 `close_price` 为空。
- 因此 `17170` 只代表当前实现下的诊断结果，不代表已经严格实现“纯 1.5 ATR 且逐笔可审计量比”的最终 v5；没有进入 Paper/Live、部署或真实交易 mutation。

下一步：先修复 ATR 被通用 3% 上限截断和逐笔量比/止损来源证据缺失，再以新规则版本重跑同样本；由于当前净 EV/PF 仍为负，修复后也必须继续保持 research-only。

## 2026-07-22：过滤量比 + RSI/EMA/MACD 15m V1

- 新增独立 Research-only 策略 `market_filtered_volume_rsi_ema_macd_15m_v1`，规则版本 `kline15m_filtered_volume3_rsi_ema_macd_atr15_v1`。策略只消费当时已完成的 15m K 线，未继承 96 根涨跌、布林带、BOS、FVG、CHoCH 等旧入口，也未接入 Paper/Live。
- 成交量门禁严格按冻结语义执行：最近 10 根历史逐根以各自此前 10 根原始均量标记 `>=3` 倍异常量；当前量比以排除异常量后的至少 5 根历史为分母，当前 K 线只作分子，要求 `>=3`。
- RSI 分支包含因果枢轴背离与极值主导影线；EMA 分支要求 EMA12/144/696 顺序、价格位置、方向大实体和 1%～3% 温和实体；MACD 分支使用归一化 DIF 背离且 RSI 40～60 失败关闭。三个分支同向合并，反向冲突不交易。
- 入场价格为信号 K 线收盘价；初始止损为纯 `1.5 * ATR14`，不受固定 3% 上限；同一冻结过滤量比决定 `1.8R/2.4R/3.0R` 三档目标。逐笔明细保存过滤量比、保留根数、RSI、DIF、EMA、目标 R 和止损来源。
- 13 项新策略定向测试与 459 项回测模块回归通过，二进制构建、格式检查、`git diff --check` 和相关文件 2000 行硬上限检查通过；只存在仓库既有 warning 及两个历史文件的 1000 行目标提醒。
- 同 current-live Top60、2025-07-01 至 2026-07-19、单边手续费 5bps、单边滑点 3bps、最长持仓 48 小时回放已落库为 `back_test_log.id=17171`。共 7,232 笔，`back_test_detail` 为 14,464 行，覆盖 42 个币。
- 主结果为净 EV `-0.141419R`、PF `0.822623`、胜率 `32.2179%`、Sharpe `-4.724372`、最大单币隔离回撤 `64.0420%`、净和 `-1022.7452R`、总利润 `-755.5536U`。前后半段 EV 为 `-0.170311R/-0.112544R`，13 个月中只有 2 个月净 R 为正，42 个币仅 11 个为正。
- 去除 255 笔多分支重叠后，RSI-only 1,338 笔，EV/PF `-0.100281R/0.873671`；EMA-only 1,170 笔，`-0.099267R/0.865661`；MACD-only 4,469 笔，`-0.159776R/0.803399`；重叠组 `-0.228968R/0.718430`。三个分支都没有正边际，MACD 是最大亏损来源。
- 落库审计：7,232 组开平仓配对错配 0；策略身份、量比/保留根数、目标档、止损、分支关键边界和旧入口污染错配均为 0；185 笔止损距离超过 3%，证明纯 ATR 未被旧上限截断。6 条窗口末结束平仓的 `close_price` 为空，但 `net_profit_r` 全部存在。
- 本版本结论为 `research_rejected_no_edge`，没有部署、没有 Paper/Live、没有真实交易 mutation。有效市场事件聚类未在本轮结果前预登记，因此不补做事后聚类，也不作为晋级证据。

下一步：不要围绕已见结果继续扫描 RSI、EMA、MACD 或量比阈值。若继续，应先提出独立的入场确认假设、冻结未见窗口与事件聚类规则，再创建新版本验证。

## 2026-07-22：同币种周 `vol_ccy` 过滤 + 1% 风险定仓 V3

- 新增独立 Research-only 策略 `market_filtered_volume_weekly_base_volume_rsi_ema_macd_15m_v1`，规则版本 `kline15m_filtered_volume3_weekly_base_volume_p90_rsi_ema_macd_structure_stop_fixed_atr_tp_v3`；旧 V1/V2 没有被覆盖，也未接入 Paper/Live。
- 成交量必须同时满足过滤后量比至少 3 倍，以及当前 `vol_ccy` 达到同币种此前 672 根连续 15m 的 nearest-rank P90。数据直接来自每币 15m 分表，不读取或补抓 `volCcyQuote`。
- RSI、EMA、MACD-DIF 分支、方向冲突和因果枢轴按最新版文档实现；成交价为下一根开盘。形态参与最终方向时使用吞没/影线结构止损，否则以实际成交价重锚 `1.5*ATR14`。
- 止盈为固定 ATR 价格距离：量比 `[3,4)/[4,6)/[6,+∞)` 对应 `2.7/3.6/4.5*ATR14`；形态止损不改变止盈距离。V3 显式关闭通用 3% 止损收紧，并检查成交 K 线内风险。
- 若结构止损相对实际成交价已经失效，回测在成交价立即退出，计入双边手续费和滑点，并保存 `Invalid_Structure_Stop_At_Fill`，不伪造 R 收益。
- 单笔仓位按入场前币种隔离权益的 `1% / 实际初始止损距离` 计算；落库 2,204 笔全部保存初始止损和风险金额，风险占比最小/最大均为 1%，偏差 0。
- Top60 全窗口 MACD 关闭基线已写入 `back_test_log_id=17172`：2,204 笔、42 币、4,408 行明细；开平仓配对错配 0，全部明细都保存当前 `vol_ccy` 和周 P90。
- 结果为净 EV `-0.161325R`、PF `0.800122`、胜率 `29.9909%`、Sharpe `-4.8145`、净和 `-355.559956R`，明确保持 Research-only。
- `final_fund=3867.496270` 是 42 个各 100U 隔离权益的合计值，初始合计 4,200U；统一组合资金、最大并发风险、数量精度和杠杆上限仍未补齐。

下一步：不要围绕已见负结果后验修改量比、P90、RSI/EMA 或止盈止损。若继续 MACD 网格，应按已冻结九组完成剩余开发段，样本不足 100 笔则直接判定 MACD 增量不可晋级；统一组合资金与交易所约束另立实现任务。

## 2026-07-22：周基础成交量策略 BB(12,2.6) 冲突缓冲 V4

- 在 V3 之后新增独立 Research-only V4：已有空候选且当前最低价触及下轨时增加多候选，已有多候选且当前最高价触及上轨时增加空候选；冲突不交易，BB 自身不能开仓。
- 布林带只用当前及此前 12 根已完成 15m 收盘，采用总体标准差 2.6 倍；V3 策略身份和历史结果未覆盖。
- EIGEN 原信号柱 OHLC `0.2303/0.2304/0.2256/0.2270`，BB 下轨 `0.227177390623`，V4 已取消 `2026-07-17 21:15` 的原空单；该时点前后 45 分钟无 V4 入场。
- 验证完成：V4 5 项、策略族 33 项、全库 784 项及沙箱外单测复跑通过；构建、格式、diff 和文件硬上限门禁通过。
- 同 Top60、同窗口、同成本回测已落库为 `back_test_log.id=17173`：1,457 笔、42 币、2,914 行明细；1,457 组开平仓完整，BB 快照和 1% 风险复算全部一致。
- V4 净 EV/PF `-0.173468R/0.789385`、胜率 `29.7186%`、Sharpe `-4.1501`、最大单币隔离回撤 `40.4499%`、总利润 `-234.1356U`。相对 V3 少 747 笔并减少 98.3681U 绝对亏损，但每笔质量指标反而略差。
- 结论为 `research_rejected_no_edge`；未接入 Paper/Live、未部署、未触发任何真实交易 mutation。完整证据见 `docs/MARKET_FILTERED_VOLUME_WEEKLY_BASE_VOLUME_BOLLINGER_CONFLICT_RSI_EMA_MACD_15M_V4_EVALUATION_MANIFEST.md`。

下一步：不在同一已见样本上扫描布林周期或倍数；先诊断 756 笔被移除交易并冻结新的反转确认假设，再做独立窗口验证。

## 2026-07-22：过滤量比策略 MACD 枢轴背离 V2 修复

- v1 规则与已落库 `17171` 保持可复现；新增同策略家族 Research-only 规则版本 `kline15m_filtered_volume3_rsi_ema_macd_pivot_atr15_v2`，未接入 Paper/Live。
- 已纠正旧实现把当前柱 `t` 与历史枢轴直接比较的问题：v2 固定在 `t` 收盘确认 `p=t-3`，价格与指标均比较 `p/q`，`q` 为 `p` 前 48 根内最近同类型严格价格枢轴。
- MACD 只使用 `DIF=EMA12-EMA26`；顶/底背离分别要求 `p/q` 严格位于各自 `Z*ATR14` 零轴带外侧，使用 `DIF/close` 的两枢轴差值检查 `D_min`，RSI 极值也读取 `p`。
- `Z` 和 `D_min` 新增为成对显式参数，缺失、非有限或不大于 0 时 MACD 分支失败关闭；未给默认值，RSI、EMA、量比、入场价与 ATR 风控逻辑未改。
- 回测明细审计证据可还原 `p/q` 时间、价格、RSI、DIF、归一化改善和两个阈值；信号与成交时间保持在确认柱 `t`，没有未来数据回填。
- 验证完成：MACD/策略专项 22 项通过，整个回测模块 468 项通过，`cargo check -p rust-quant-cli` 通过，相关文件均低于 2000 行硬上限。
- 回测前新增评估清单，冻结 `Z={0.10,0.20,0.30}`、`D_min={0.0005,0.0010,0.0020}`、开发/未见切分、100 笔 MACD-only 门槛和 30 分钟同向事件聚类规则；结果见 `docs/MARKET_FILTERED_VOLUME_RSI_EMA_MACD_15M_V2_EVALUATION_MANIFEST.md`。
- 九组开发集的三个 `Z` 结果完全一致；MACD-only 仅 5～15 笔。仅作证伪选择 `Z=0.20,D_min=0.0010`，未见段完整策略 `-0.130231R/PF 0.833924`，略差于关闭 MACD 的 `-0.129767R/PF 0.834528`。
- 完整窗口 3,176 笔、42 个币，净 EV `-0.140171R`、PF `0.821720`、胜率 `31.2343%`、Sharpe `-3.505425`、最大单币隔离回撤 `55.2408%`、净和 `-445.183058R`；四分位全部负 EV。
- 独立重放 RSI/EMA/MACD-only 分别为 1,649/1,515/14 笔，EV/PF 为 `-0.133530R/0.834887`、`-0.147239R/0.811698`、`0.482055R/1.863569`；MACD-only 样本、EV、PF 和 Sharpe 均未同时达标。
- 交易按预注册规则归并为 1,914 个事件；修复确认删除了 v1 大量错误 MACD 信号，但没有产生可交易优势。结论 `research_rejected_no_edge`，本轮无回测 ID、无数据库写入、无 Paper/Live 或交易 mutation。

## 2026-07-22：周基础成交量策略 EMA144 距离门禁 V5

- 新增独立规则版本 `kline15m_filtered_volume3_weekly_base_volume_p90_ema144_distance_atr1_rsi_ema_macd_structure_stop_fixed_atr_tp_v5`，V3/V4 行为和历史证据保持可重放。
- V5 仅在 EMA 延续候选存在时应用 `abs(signal_close-EMA144)/ATR14 <= 1.0`；阈值包含边界，RSI/MACD 独立候选不受影响，未叠加 V4 布林带。
- 验证覆盖长短镜像、1 ATR 边界、EIGEN 复现、独立 RSI 分支和未来 K 线隔离；策略族 41 项通过，全库除沙箱监听限制外 790 项通过，受限项在沙箱外复跑通过。
- 同 Top60、同窗口、同成本回测已落库为 `back_test_log.id=17174`：1,109 笔、42 币、2,218 行明细，开平仓配对和 1,109 条距离证据完整。
- V5 净 EV/PF `-0.213129R/0.751294`、胜率 `27.8629%`、Sharpe `-4.3486`、最大单币隔离回撤 `37.5170%`、总利润 `-220.1199U`。
- V5 是 V3 严格子集：删除 1,095 笔且无新增；删除集合平均约 `-0.111638R`，优于保留集合的 `-0.213129R`。被保留的 22 笔 EMA 交易为 `-0.321468R/笔`，假设被证伪。
- 与 V4 比，V5 虽少亏 14.0158U、最大单币回撤低 2.9328 个百分点，但 EV、PF、胜率和 Sharpe 全部更差。V4 是两者中更好的研究过滤，但仍不能晋级生产。
- 完整证据见 `docs/MARKET_FILTERED_VOLUME_WEEKLY_BASE_VOLUME_EMA144_PROXIMITY_RSI_EMA_MACD_15M_V5_EVALUATION_MANIFEST.md`；未部署、未进入 Paper/Live、未触发真实交易 mutation。

## 2026-07-23：DOGE 背离、长下影与盈利观察 V6-V8

- DOGE `2026-07-05` 图中可见背离是 `12:00 -> 13:30`，并在 `14:30` 才因右侧三根完成而可确认；DOGE `2026-06-25 01:45` 的背离在 `02:45` 才可确认。两例的价格/RSI14 背离均成立，失败点都是确认时成交量前置门禁，而非 RSI 改善 1 点或严格枢轴本身。
- 截图和回测看似冲突，来自四个口径差异：图表看枢轴发生时，代码看右侧确认时；UI 标注 K 线开盘时间，事件使用完成时间；图表关注枢轴量柱，代码检查确认 K；代码 RSI14，而用户第三张截图为 RSI12。
- 新增并验证三个独立 Research-only 版本：V6 只加中性 RSI 双重放量长下影做多；V7 保持 V3 入场，仅加目标完成比例盈利观察；V8 组合两者。共同 V3 信号在 V6/V8 中不改方向、止损或冲突结果。
- 有效落库 ID：V6 `17175`、V7 `17177`、V8 `17178`；`17176` 因旧版平仓证据重复写入 `varchar(5000)` 而失败且无明细，已修复为摘要留在 `signal_value`、完整保护历史单独存储，未删除该审计记录。
- V3/V6/V7/V8 净 EV 分别为 `-0.161325/-0.199152/-0.160452/-0.184569R`，PF 为 `0.800122/0.759961/0.694167/0.659830`；所有版本都没有正优势。
- V6 新增长下影分支 1,704 笔、胜率 `27.88%`、净和 `-450.3685R`、PF `0.7010`，42 个币仅 4 个为正、13 个月全部为负。DOGE 子集虽优于总体，仍为 76 笔合计 `-2.6880R`。
- DOGE `2026-07-01 21:30` 本身确实是一个有效新信号：过滤量比 `3.214638`、`vol_ccy` 超周 P90、RSI14 `65.826567`、下影占振幅 `75%`。V6 于完成后的 `21:45` 开多并获 `+1.007642R`；V8 同笔因盈利保护只获 `+0.494847R`，说明个例正确不能替代全样本验证。
- V7 把胜率提高到 `43.2675%`，最大单币隔离回撤降至 `28.9540%`，但 PF 降至 `0.694167`、Sharpe 降至 `-6.9591`，总净 R 比 V3 多亏 `1.9279R`。匹配事件层面，它救回原亏损约 `788.0132R`，却同时削减原盈利约 `782.3984R`；提前平仓释放出的 24 笔交易又亏 `7.5427R`。
- 当前状态机证明“利润回吐保护”方向有价值，但 1R 前单次收盘跌回 0.25R 太容易误杀最终赢家，1R 后盘中动态保护也会与邻近 ATR 目标竞争。下一轮只做退出消融并冻结未见窗口，不继续向入场叠加指标。
- 未部署、未晋级、未触发 Paper/Live 或真实交易 mutation；完整规则与结果见 `docs/MARKET_FILTERED_VOLUME_DOGE_WICK_PROFIT_OBSERVATION_15M_ABLATION_MANIFEST.md`。

## 2026-07-23：量比 2.5 + 周 P90 放量锚点 RSI 背离 V9

- 新增独立 Research-only 策略身份 `market_filtered_volume_weekly_p90_anchor_rsi_divergence_15m_v1`，规则版本为 `kline15m_filtered_vol2p5_weekly_p90_anchor_rsi_div_fixed_atr_tp_v9`；V3 身份与历史结果未覆盖。
- 历史 q 与当前 p 都独立执行因果过滤量比 `>=2.5` 和自身前 672 根 `vol_ccy` P90；最近 48 根内先锁定最近同方向锚点，再比较价格与 RSI，不向前挑选更有利旧锚点。
- 同一冻结 current-live Top60、同窗口、同费用/滑点和 1% 风险下，fresh V3=`17179`，最终独立身份 V9=`17181`。身份修复前的 `17180` 数值相同，但只保留为审计记录。
- V9 共 4,961 笔，胜率 `30.5785%`、净 EV `-0.181400R`、PF `0.771945`、Sharpe `-8.495876`、最大单币隔离回撤 `71.640724%`、净和 `-899.924623R`，显著差于 V3。
- 新锚点背离合计 2,640 笔，净 EV/PF `-0.205270R/0.744091`，净和 `-541.912245R`；做多和做空均为负，13 个月无正收益月份，42 个币仅 9 个为正。
- DOGE 目标案例已正确触发：q=`10:45`、p=`17:30`，q/p 量比 `2.806377/3.947500` 且周 P90 均通过；17:45 开多，20:30 止盈，净 `+1.487005R`，明细 ID `2244857`。
- 2,640 笔锚点交易全部在信号完成后的下一根 15m 开盘成交，时序错配为 0；4,961 条平仓净 R 全覆盖。
- 8 项 V9 测试、9 项 V3 回归、构建、局部格式、`git diff --check` 与硬行数门禁通过；无部署、无 Paper/Live、无真实交易 mutation。
- 结论为 `research_rejected_no_edge`。完整结果见 `docs/MARKET_FILTERED_VOLUME_WEEKLY_P90_ANCHOR_RSI_DIVERGENCE_15M_V9_RESULT.md`。

## 2026-07-23：锚点 RSI 背离下一根收盘确认 V10

- 新增独立 Research-only V10：只让 V9 锚点背离进入紧邻下一根收盘确认；底背离要求收盘严格高于 `p.high`，顶背离严格低于 `p.low`，失败立即过期，确认后再下一根开盘成交。
- 同 V9 冻结 Top60、时间窗、5/3 bps 成本和 1% 风险回放已落库为 `back_test_log.id=17182`。454 个开仓与 454 个平仓完整配对，454 条净 R 和确认时序证据完整。
- 结果为胜率 `33.4802%`、净 EV `-0.063616R`、PF `0.917649`、Sharpe `-0.857140`、最大单币隔离回撤 `18.044188%`、净和 `-28.881840R`；41 币中 19 币为正，13 个月仅 2 个月为正。
- V10 只保留 V9 锚点交易的 17.20%，说明下一根价格确认有效削减噪声，但做多仍为 `-0.023495R/PF 0.969279`，做空为 `-0.099392R/PF 0.872476`，未形成可交易优势。
- 盈亏差异主要来自确认突破幅度和当前量比，不来自 RSI 改善点数；结果后 `>=0.75%` 确认幅度切片虽为正，但属于事后诊断，不得直接晋级或原地写入 V10。
- DOGE 17:30 的原 V9 盈利案例被 V10 淘汰，因为紧邻 17:45 收盘没有突破 `p.high`；首次突破在第 5 根，不能按单个赢家回改等待窗口。
- V10 专项 6 项、策略族 57 项、构建、局部格式和硬行数门禁通过；未部署、未进入 Paper/Live、未触发真实交易 mutation。结果见 `docs/MARKET_FILTERED_VOLUME_ANCHOR_RSI_NEXT_CLOSE_CONFIRMED_15M_V10_RESULT.md`。

## 2026-07-23：锚点影线立即入场 / 下一根盘中触价 V11

- 新增独立 Research-only V11，未覆盖 V9/V10：方向性长下影底背离与长上影顶背离在锚点完成后的下一根开盘成交；其他锚点只允许紧邻下一根盘中严格触及 `p.high/p.low`，未触发即过期。
- 盘中跳过触发价时按下一根开盘成交，否则按锚点高/低点成交；逐笔保存触发模式、影线比例、触发价格、触发 K、成交来源和保守同根路径口径。
- 同一 Top60、同窗口、单边手续费 5bps、滑点 3bps、1% 隔离风险回测已落库为 `back_test_log.id=17183`：1,017 笔、41 币、2,034 行明细，入场证据和开平仓配对完整。
- V11 胜率 `29.1052%`、净 EV `-0.212154R`、PF `0.739640`、Sharpe `-4.538888`、最大单币隔离回撤 `26.2327%`、净和 `-215.760828R`；明显弱于 V10 的 `-0.063616R/PF 0.917649`。
- 影线下一开盘分支 339 笔，净 EV/PF `-0.227886R/0.722744`；盘中触价分支 678 笔，`-0.204288R/0.748783`。即使乐观删除全部盘中触价同根止损歧义，剩余 913 笔仍为 `-0.106735R/PF 0.862824`。
- HBAR 原 `back_test_detail.id=2245940` 的锚点是实体仅 `3.33%` 的长下影十字星，按既定“十字星不走影线直接分支”进入触价模式；V11 在 `20:45` 以 `p.high=0.06668` 成交，仍于后续止损，说明提前成交本身未修复信号质量。
- V11 专项 6 项、策略族 64 项及专用二进制构建通过；结论为 `research_rejected_no_edge`，无 Paper/Live、部署或真实交易 mutation。完整证据见 `docs/MARKET_FILTERED_VOLUME_ANCHOR_RSI_WICK_OR_NEXT_TOUCH_15M_V11_RESULT.md`。

## 2026-07-23：趋势止盈 + 持仓放量阶梯保护 V12

- 新增独立 Research-only 策略 `market_filtered_volume_anchor_rsi_trend_managed_15m_v1`，完整复用 V11 的 q/p、影线下一开盘与非影线紧邻下一根盘中触价，不注册 Paper/Live。
- 信号时点按连续三根 EMA12/144/169/696 顺序和 EMA696 三次同向斜率，或已确认且未收回的平台放量破位冻结趋势；顺势/中性使用量比目标，逆势默认 `1 ATR`，极端 96 根例外恢复量比目标。
- 持仓后只使用已完成 15m K 的因果过滤量比；第一次合格放量推进到双边成本真实保本，后续按冻结 ATR 逐级收紧，且当前 K 先检查旧止损和止盈。
- 首次落库暴露两个实现问题：旧信号载荷超过 `varchar(5000)`，以及框架只传 1 根 K 导致持仓量比永远未就绪。V12 改用紧凑审计载荷，止损历史保留在独立字段；回放滑窗修正为量比算法必需的 21 根，并新增端到端回归。
- 最终有效落库 ID 为 `17186`：1,020 笔、2,040 行明细、41 币、659 个有效市场事件；单条 `signal_value` 最大 3,357 字符，开平仓、初始风险与净 R 无缺失。
- V12 胜率 `38.9216%`、净 EV `-0.227954R`、PF `0.644265`、Sharpe `-6.139853`、最大单币隔离回撤 `24.9450%`、净和 `-232.513076R`，弱于 V11 的 EV/PF。
- 135 笔至少发生一次保护，共 184 次更新；共同 1,017 笔相对未启用保护改善 `+6.147650R`，其中首次保本组贡献 `+9.399863R`，第二级 `1 ATR` 反而损失 `-4.989173R`。
- V11 的共同 1,017 笔在 V12 中成交时间与价格全部一致；提前退出释放出 3 个额外信号且全部亏损。唯一正期望分组是 90 笔逆势极端例外，EV/PF 仅 `+0.166582R/1.263829`，不足以晋级。
- `17184` 是无明细失败记录，`17185` 是动态保护未生效的无效中间记录；删除属于破坏性数据库操作且未获单独授权，暂时保留并明确禁止当作最终 V12。
- 验证完成：26 项 equity、5 项 V12 趋势规则、结构化止损证据、legacy JSON、CLI check、格式和 2,000 行硬上限门禁通过。完整结果见 `docs/MARKET_FILTERED_VOLUME_ANCHOR_RSI_TREND_MANAGED_15M_V12_RESULT.md`。

## 2026-07-23：ATR 审计 + 普通逆势 1.5 ATR V13

- ATR14 已确认使用标准真实波幅与 Wilder RMA：信号只读取已经完成的锚点 `p`，并把同一个 `ATR14[p]` 冻结给实际成交后的初始止损、目标和持仓保护。新增前收跳变与递推数值测试通过。
- 41 个实际交易币在加载窗口内共 923,913 根 15m K；非法 OHLC、非正价格、时间未对齐、重复/逆序均为 0。ACT/GIGGLE 有两处长期断档，但恢复后的前 696 根内均无交易，不能解释本轮低胜率。
- ATR 加载器尚未失败关闭时间断档，且共享 ATR 以 `current==0` 识别种子状态；后者只会在首 14 根 TR 全零时偏离标准递推。本轮仅 12 根零振幅 K 且分散在 3 币，两项都未污染当前结果，后续须独立修复。
- 新增 V13 规则 `kline15m_filtered_vol2p5_anchor_rsi_wick_or_touch_trend_tp_counter15_volume_trail_v13`，只把普通逆势目标从 1.0 ATR 调到 1.5 ATR；V12 身份和 `17186` 保持不变。
- V13 有效落库 ID 为 `17187`：1,020 笔、2,040 行明细、41 币、659 个有效市场事件。全部共同交易的币种、方向、时间、成交价、初始止损、市场状态和趋势关系一一相同。
- V13 胜率 `35.7843%`、净 EV `-0.212383R`、PF `0.685039`、Sharpe `-5.483132`、最大单币隔离回撤 `23.4394%`、净和 `-216.630636R`。相对 V12 改善 `15.882440R`，但仍明显不达标。
- 普通逆势 419 笔从 `-0.228050R/笔` 改善到 `-0.190144R/笔`；201 个共同赢家因目标扩大增加 `66.9795R`，32 个原赢家转亏损减少 `51.0970R`，没有原亏损被改成赢家。
- 中性 381 笔的核心问题不是 ATR：317 笔是多空趋势证据冲突，纯中性仅 64 笔且更差。三个目标档、四种入场模式全部负期望；239 笔直接打初始止损，272 个亏损中 134 个从未达到 0.25R，另有 69 个曾达到 1R 后回吐。
- 完整报告见 `docs/MARKET_FILTERED_VOLUME_ANCHOR_RSI_TREND_MANAGED_COUNTER15_15M_V13_RESULT.md`。V13 结论为 `research_rejected_no_edge`，未注册 Paper/Live、未部署、未触发任何真实交易 mutation。

## 2026-07-23：三个独立 15m 策略家族

- 将原混合思路拆成 `market_momentum_exhaustion_reversal_15m_v1`、`market_volume_anchor_rsi_divergence_reversal_15m_v1` 与 `market_volume_platform_break_trend_15m_v1`；三者拥有独立 strategy key、slug、规则版本、预设、manifest、信号模块和审计快照。
- 三个家族只共享 15m 数据加载、过滤量比/周 P90 基础设施和固定风险执行器，不共享入场假设。逐笔合同审计中，家族身份缺失、MACD 分支证据、跨家族条件混入均为 0。
- 同一 current-live Top60、`1751328000000..=1784470500000`、5/3 bps 成本、1% 风险、实际成交价两侧 `1.5 ATR14`、固定毛 `1R` 与 48 小时口径完成回放。
- 动量衰竭反转落库为 `17188`：2,515 笔、42 币，胜率 `46.9583%`、净 EV/PF `-0.142040R/0.752168`、Sharpe `-7.132672`、最大单币隔离回撤 `36.380176%`。
- 放量锚点 RSI 背离落库为 `17189`：1,020 笔、41 币，胜率 `47.1569%`、净 EV/PF `-0.205382R/0.660656`、Sharpe `-6.557436`、最大单币隔离回撤 `21.008430%`。
- 放量平台破位趋势落库为 `17190`：1,373 笔、42 币，胜率 `47.9971%`、净 EV/PF `-0.154756R/0.732361`、Sharpe `-5.742009`、最大单币隔离回撤 `26.689631%`。
- 三个 ID 共 9,816 行 `back_test_detail`，开平仓一一对应，初始止损、初始风险与平仓净 R 无缺失。前后半段均负 EV，13 个月各仅 1/2/1 个月盈利。
- 三个家族均为 `research_rejected_no_edge`；没有注册 Paper/Live、没有部署、没有真实交易 mutation。结果分别见三个 `*_V1_RESULT.md`。

## 2026-07-23：动量衰竭反转 V2 + 水平平台破位趋势 V2

- 新增 `market_momentum_exhaustion_reversal_15m_v2`：方向性长影线在 `p.low/p.high` 最多等待 12 根限价成交，非影线继续使用紧邻下一根严格触价；冻结 `ATR14[p]`，量比档位对应 `2.7/3.6/4.5 ATR` 目标。V1 与其他指标家族未覆盖。
- 新增 `market_volume_platform_break_trend_15m_v2`：平台宽度只除以 `ATR14[b-1]`，增加前五/后五收盘重心、20 根收盘回归漂移、上下沿至少两次且分散触碰；V1 的放量破位、两根接受、EMA696 斜率与固定 1R 保持不变。
- 回测已完整落库：动量 V2 为 `17191`，2,096 笔、42 币、1,174 个有效事件，净 EV/PF `-0.187622R/0.754882`；平台 V2 为 `17192`，126 笔、34 币、103 个有效事件，净 EV/PF `-0.273161R/0.576139`。
- `17191` 有 4,192 行、`17192` 有 252 行 `back_test_detail`；开平仓一一对应，平仓净 R、开仓初始止损和初始风险缺失均为 0。
- EIGEN 旧明细 `2263912` 属于平台 V1。其破位前窗口在 V2 复算为宽度 `4.109387 ATR`、中心偏移 `1.826394 ATR`、`R²=0.709430`、拟合漂移 `2.428359 ATR`、上沿仅一次触碰，因此 V2 不再把该持续下移窗口识别为平台。
- 专项 11 项测试、格式、构建与文件硬上限通过；全库 856 项通过，唯一失败来自沙箱禁止 localhost `TcpListener`，与策略改动无关。
- 两个 V2 均为 `research_rejected_no_edge`，没有注册 Paper/Live、部署或真实交易 mutation。完整结果见 `docs/MARKET_MOMENTUM_EXHAUSTION_REVERSAL_15M_V2_RESULT.md` 与 `docs/MARKET_VOLUME_PLATFORM_BREAK_TREND_15M_V2_RESULT.md`。

## 2026-07-24：放量锚点 RSI 背离周期门禁 V2

- 新增独立策略 `market_volume_anchor_rsi_divergence_reversal_15m_v2`，规则版本为 `kline15m_filtered_vol2p5_anchor_rsi_gap4_swing_reset_wick_or_touch_fixed1r_v2`；V1 与回测 `17189` 未覆盖。
- V2 固定最近合格 q 后要求 q/p 中间至少 4 根完整 K；底背离途中 RSI `>60`、顶背离途中 RSI `<40` 时 q 失效，恰好 60/40 允许，中间 RSI 缺失时失败关闭，门禁失败不向前寻找旧锚点。
- 同一 current-live Top60、`1751328000000..=1784470500000`、5/3 bps 成本、1% 风险、`1.5 ATR14` 止损/止盈和 48 小时口径已落库为 `back_test_log.id=17194`。
- `17194` 共 296 笔、592 行明细、40 币、230 个 60 分钟同方向事件；开平仓一一对应，初始止损、初始风险和平仓净 R 缺失均为 0。
- V2 胜率 `44.9324%`、净 EV `-0.249429R`、PF `0.607387`、Sharpe `-4.262597`、最大单币隔离回撤 `10.704554%`、净和 `-73.831025R`。
- 2,753 个 V1 背离候选中，1,266 个因间隔不足、71 个因做多 RSI 重置、36 个因做空 RSI 重置被拒绝；1,380 个通过 setup，1,084 个下一根未触价，最终成交 296 笔。
- V2 全部交易都是 V1 的共同交易，逐笔结果完全一致；V2 保留组 EV `-0.249429R`，删除组 EV `-0.187374R`，因此不是退出差异，而是门禁筛出了更差子集。
- V1 的间隔 1 根组接近盈亏平衡，间隔 16 根以上组净 EV `-0.390775R`；“至少 4 根”删除近锚点却继续允许陈旧锚点，是本轮失败的主因。RSI 重置只改变 13 笔 V1 实际成交。
- 专项 5 项、策略族 91 项、manifest 风险合同、CLI check、二进制构建、格式、`git diff --check` 与 2,000 行硬上限通过。
- 结果归档于 `docs/MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_15M_V2_RESULT.md`；结论为 `research_rejected_no_edge`，未进入 Paper/Live、未部署、未触发真实交易。

## 2026-07-25：TradingView RSI 背离收回门禁消融

- 本轮只修改 Research 图表和策略规范，版本冻结为 `rsi_divergence_structure_v2`；Core 策略、数据库、Paper/Live 与生产发布均未触碰。
- 同口径逐项消融显示：关闭 `0.5 ATR` 创新或 `1 ATR` 中间摆动时结果仍为 6 笔；只取消“信号棒必须收回锚点”后恢复到 11 笔，净利润/PF 为 `+186.10 USDT / 3.1738`，最大回撤 `52.35 USDT`。
- `0.35%` 价格创新、`0.5 ATR` 创新、`1 ATR` 中间反向摆动、RSI 50 中轴连续性、最近完整量能 q 与禁止回退保持不变；收回/跌回状态继续在悬浮信息中显示，但不再阻断结构背离。
- 恢复的 5 个背离信号创新幅度为 `0.53%～1.36%`；入场标签审计没有 `2026-07-13 22:15` 的原 7 根 / 0.29% 噪声。TradingView 编译 0 错误，本地与编辑器源码逐字符一致。
- 当前样本未计手续费和滑点，只有 11 笔且 Sharpe 仍为负；本次只能说明硬收回门禁在该窗口过严，不能证明新版本具备跨币种正收益。

## 2026-07-25：独立放量锚定区间上破做多 V1

- 新增 Research-only 信号身份 `volume_anchor_range_upside_break_long_15m_v1`，只写入 `docs/strategy_list/15min_velocity_all_symbol_strategy_research.pine` 与对应中文策略规范；Core、数据库、Paper、Live、部署和真实交易 mutation 均未触碰。
- V1 在突破棒收盘时只读取此前20根已完成K线，要求阳线收盘严格上破区间高点、过滤后量比 `>=3`、原始 `vol_ccy` 通过同币种周P90、`EMA12 > EMA144 > EMA696` 且 `RSI14 < 70`。没有加入实体比例、区间宽度或触碰次数等事后门槛。
- 首个合格上破同时产生一次做多候选并启动既有8根假突破失败观察；同一周期继续冲高不会重复做多或移动边界。单独命中时使用 `1.5 ATR` 止损，并按突破量比使用 `2.7 / 3.6 / 4.5 ATR` 目标；悬浮提示新增区间、边界、突破收盘、量比、趋势门禁和版本。
- 目标 `OKX:ETHUSDT 2026-07-14 18:45` 已确定性命中：冻结区间 `1778.71～1795.05`、收盘 `1800.15`、量比 `6.55x`、RSI `69.47`；最早 `19:00` 以 `1800.16` 成交，`1823.78` 止盈。前一根 `18:30` 因量比约 `2.62x` 不合格。
- 临时标签审计共得到6个本分支独立入场，既有 RSI、EMA696 和 EMA压缩扩张候选均为假；2次止盈、4次止损，固定1单位且零费用下直接合计 `+17.60 USDT / PF 1.5575`。
- 组合图表基线为11笔、净利润 `+186.10 USDT`、PF `3.1738`、最大回撤 `52.35 USDT`；加入V1后为16笔、`+211.72 USDT`、PF `2.9395`、最大回撤 `59.00 USDT`。净利润增加但PF下降、回撤上升，且新持仓改变后续信号可成交时点，不能把组合差额当作纯分支收益。
- TradingView Pine 编译0错误，本地源码与编辑器源码逐字符一致；Markdown代码围栏成对、两份策略文件无行尾空白且均低于2,000行。V1 保持 `research_only_not_promoted`，下一步必须做费用后风险归一化与跨币种、多月份、未见窗口验证。

## 2026-07-25：独立大型上升三角放量突破做多 V1

- 新增 Research-only 信号身份 `volume_large_ascending_triangle_break_long_15m_v1`，只修改 TradingView Pine、中文策略规范和研究进度文档；Core、数据库、Paper、Live、部署及真实交易 mutation 均未触碰。
- 为避免逐长度隐性参数搜索，固定检查突破棒前 `96 / 120 / 144 / 168 / 192` 根并取最长有效结构。窗口三等分后要求三段峰差不超过阻力的 `0.5%`、上沿至少3组独立触碰且相隔8根、低点逐段至少抬高 `0.25 ATR`、总抬高至少 `1 ATR`、末段/首段宽度不超过 `70%`。
- 动量门禁固定为 `RSI[t-1] < 70`、`70 <= RSI[t] <= 80`、EMA12在当前棒刚上穿EMA144且EMA144高于EMA696；量能继续要求过滤后量比 `>=3` 与周P90。只在信号棒收盘确认并于下一根开盘模拟成交，不补造历史入场。
- `OKX:ETHUSDT 2026-07-10 09:45` 已确定性命中。TradingView 临时诊断标签为 `LARGE_TRI|192|1763.94|0.24|63.25|4`，分别表示192根窗口、冻结阻力、峰差百分比、收敛百分比和4组独立触碰。
- 目标多单下一根以 `1767.79` 模拟成交，在 `1798.79` 达到量比档位对应的 `4.5 ATR` 目标。主图继续只展示绿色入场箭头，完整结构数据放在悬浮提示中。
- 新鲜同图消融中，关闭分支为16笔、净利润 `+211.72 USDT`、PF `2.9395`、最大回撤 `59.00 USDT`；开启后为17笔、`+242.72 USDT`、PF `3.2235`、最大回撤仍为 `59.00 USDT`。
- 当前只新增1笔已见ETH盈利样本，不能据此计算有意义的独立PF或宣称优势。版本保持 `research_only_not_promoted`，待费用后风险归一化、未见月份、跨币种和参数邻域验证。

## 2026-07-25：TradingView 弱背离收回 + 背离反转退出 V3

- Research 图表新增 `rsi_divergence_weak_reclaim_regime_exit_v3`，未覆盖 Core 独立 RSI 背离策略，也未进入 Paper、Live、部署或真实交易 mutation。
- 弱背离定义为最近量能锚点与信号棒仅相隔 `5～7` 根，顶背离必须收盘跌回 `q.high`、底背离必须收盘站回 `q.low`；`8～32` 根完整背离继续使用既有创新幅度、独立摆动、RSI 中轴连续性与禁止回退门禁。
- 退出按制度拆分：严格逆势使用开仓前冻结的横盘结构目标；中性/过渡制度的纯背离反转使用 `1R` 后近似保本、`1.5R` 全平；严格同向 EMA 趋势仍使用原 ATR 延续目标，且与其他独立信号重合时不由背离覆盖退出。
- 同图同窗复测中，BTC 由 `-125.70 / PF 0.9604 / 10笔` 改善到 `+1296.10 / PF 1.5600 / 9笔`，ETH 保持 `+249.86 / PF 3.6474 / 16笔`；固定1单位，手续费与滑点均为0。
- BTC 的 5 根未收回弱顶背离原亏损 `-483.60` 被过滤；28 根完整顶背离由原止损 `-375.30` 改为 `1.5R` 止盈 `+562.90`。该两笔解释了本轮 BTC 的主要变化。
- TradingView 编译0错误，本地与编辑器源码逐字符一致，临时 `DBG_` 标签已清除，文件1404行且静态检查通过；仍只可视为已见窗口 Research 结果。

## 2026-07-25：突破多单接受期普通超买空单保护 V1

- Research 图表新增 `post_breakout_long_acceptance_short_guard_15m_v1`，只修改 Pine 研究制品；Core、Paper、Live、部署和真实交易 mutation 均未触碰。
- 20根确认箱体、大型水平箱体或大型上升三角真正生成多单入场订单后，冻结信号时最高有效突破线；后续新突破可以冻结新的最近阻力，但普通价格新高不会移动边界。
- 价格确认收盘仍高于冻结线时，只屏蔽普通 RSI 超买吞没/长上影空单；确认收盘跌回突破线，或既有重复扫高并收回空头结构成立后解除。结构背离、EMA、锚区失败与重复扫高空单均不受该保护阻断。
- ETH 同窗诊断记录4次启动、1次扫高解除、2次收盘跌回解除；BTC无启动。两币当前窗口均未出现位于保护期内的普通超买空单，因此成交数量、净利润和 PF 均未改变。
- 最终同窗结果为 BTC `+1296.10 USDT / PF 1.5600 / 9笔`、ETH `+249.86 USDT / PF 3.6474 / 16笔`；通过“不退步”保留门槛，但不能声称收益已改善。

## 2026-07-26：TradingView 布林带迁移到 K 线主图

- Research 主策略使用真实价格尺度绘制 `BOLL(20,2)` 上轨、中轨、下轨和高透明度带状填充，最终实例以 `overlay=true` 覆盖在 BTC 15m K 线价格窗格。
- 副图删除 BOLL 的 `%B`、带宽、价格轨道、分区线和表格字段，RSI 使用释放后的 `50～100` 显示区；MACD 与 `volume == vol_ccy` 语义保持不变。
- TradingView 新编辑器一度让脚本更新继承错误窗格，已删除错误实例并通过脚本标题菜单分别绑定、保存、重新添加；最终图表只有1个主策略和1个副图。
- 两份 Pine 均为0个编译错误，保存状态为 `saved`，编辑器与本地源码逐字符一致。当前主图数据窗口可读取 BOLL 三轨，副图数据窗口没有任何 BOLL plot。
- 视觉迁移未改变策略结果：BTC 当前窗口仍为6笔、净利润 `-662.40 USDT`、PF `0.554988`；未触碰 Core、Paper、Live、部署或真实交易。
- 副图原有 RSI 50 中轴线因浅灰点线与网格重叠不易辨认，已改为蓝色2像素实线。组合副图的 RSI 显示区为 `50～100`，因此真实 RSI=50 的绘图坐标为75；该映射与原始 RSI 数值均未改变。

## 2026-07-26：开仓价 ±1 ATR 主图风险范围

- Research 主策略新增持仓期风险可视化：实际 `strategy.position_avg_price` 为中心，上下分别为冻结 ATR14 的 `+1 ATR/-1 ATR`；多仓绿色、空仓红色，空仓阶段不绘制。
- 因策略在信号完成后的下一根开盘成交，ATR 使用信号棒的已确认值冻结，避免用成交棒尚未完成的高低点重写入场风险尺度；该状态不参与现有保护单计算。
- BTC 15m 的 2026-06-29 空单持仓段已在主图确认显示三条固定红线与淡红填充；同根开平的交易因没有可见持仓区间而不补造历史线段。
- TradingView Pine 编译0错误、离线静态分析0问题，保存状态为 `saved`，编辑器与本地源码逐字符一致；主脚本1493行。
- 策略指标保持6笔、净利润 `-662.40 USDT`、PF `0.554988`，证明本次没有改变信号、退出或风控；未触碰 Core、Paper、Live、部署或真实交易。

## 2026-07-26：放量卖出高潮后三棒强势反包做多 V1

- 新增独立 Research-only 身份 `volume_three_bear_bullish_engulfing_reversal_long_15m_v1`；没有覆盖 Core 策略，也未进入 Paper、Live、部署或真实交易 mutation。
- 原截图并非三根连续阴线：21:15 是小阳线。实现按“前三棒至少两阴且整体净下跌、当前实体吞没三棒实体包络”表达真实结构，避免为命中截图写入错误事实。
- 目标棒实体占全长 `91.25%`、约 `3.05 ATR`、收盘位置 `95.65%`；前一根是 `5.85x` 完整量能事件，当前过滤量比 `3.43x`，并从 RSI `28.92` 穿回 `52.45`，收盘重新站上 EMA12 与布林中轨。
- 退出独立使用四棒低点、1R 后近似保本和1.5R全平。BTC 同窗唯一新增 `Long 3Bar` 以 `63154.7` 入场、`64082.1` 止盈，贡献 `+927.40 USDT`；组合由 `-662.40 / PF 0.555 / 6笔` 变为 `+265.00 / PF 1.178 / 7笔`。
- ETH 开关消融均为 `+331.42 USDT / PF 8.709 / 14笔`，没有新增符合条件的样本。版本只可标记为 `research_only_not_promoted_insufficient_cross_symbol_samples`，不能宣称跨币种优势。
- TradingView 编译0错误；本地 Pine 1602行且无行尾空白。冻结规则与结果归档在 `docs/VOLUME_THREE_BEAR_BULLISH_ENGULFING_REVERSAL_LONG_15M_V1_EVALUATION_MANIFEST.md`。

## 2026-07-26：TradingView 当前策略同步 Rust 与等价审计

- 当前 Pine 以 FNV-1a 32 `66d3937e` 冻结；策略规范新增当前规则总表、20根确认箱体接受 V2、三棒反包、执行合同、结果对照与差异审计，避免把历史消融结果当成当前版本。
- 新增独立 Research-only Rust 入口 `tradingview_velocity_parity_15m_research_v1`，直接读取 OKX spot 15m 公共历史 `vol_ccy`，不计算 `volume × close`；旧 Core 策略、V1～V13、数据库、Paper/Live 和生产入口均未改变。
- Rust 已覆盖当前 Pine 的 RSI 背离/极值、EMA趋势、20根接受箱体、大型水平箱体、上升三角、突破失败、重复扫高、EMA压缩扩张和三棒反包家族，并复刻候选冲突、突破后普通超买空头保护、下一开盘反手、动态保本与保护单路径。
- BTC 30天7笔从信号、成交到退出逐笔一致，`+265.00 / PF 1.1780315754 / DD 976.20` 与 TradingView exact parity。ETH 接口可见最后11笔一致；固定窗口与图表隐式边界对账后仅余末笔止损 `0.03 USDT` 差异。
- 首差异审计发现并修正 EMA 长周期预热、RSI 首根 `ta.change` 缺失值和 TradingView 最大回撤公式；这些属于正确性修复，不根据收益选择阈值。
- 13个聚焦测试、目标 binary 编译、scoped rustfmt、工作区差异与文件硬上限检查通过；项目指定 `scripts/dev/check_code_file_line_limit.sh` 不存在，使用 `wc -l` 兜底，最大新文件 `signals.rs` 为1340行。
- 成本压力显示 BTC 30/60/90天分别为 `-434.47 / -2086.09 / +1553.84`，PF `0.770 / 0.410 / 1.317`；ETH 为 `+40.43 / +262.30 / +423.92`，PF `1.857 / 4.020 / 4.488`。跨币种稳健性不成立，结论保持 `research_only_not_promoted`。
- 完整证据位于 `docs/architecture/migrations/MIG-20260726-tradingview-velocity-rust-parity/evidence.md`；未提交、未推送、未部署、未触发任何真实交易 mutation。

## 2026-07-26：高位放量努力无结果 + 次高点布林拒绝做空 V1

- 新增独立 TradingView Research 身份 `volume_effort_no_result_lower_high_boll_reclaim_short_15m_v1`，没有覆盖现有 RSI 顶背离、普通超买空单或 Rust parity。
- 锚点只按 `t-20～t-80` 内最高高点确定，同高取最近且失败不回退；当前严格次高并按用户字面使用 `0.5% OR 0.5 ATR`，量能直接比较 `vol_ccy[t] >= 1.25×vol_ccy[q]`。
- 锚点 RSI 至少70，当前 RSI 为55～70且降低；当前阴线实体至少占振幅60%、至少0.5 ATR、收盘位于底部20%，并满足高点出布林上轨、收盘回到轨内。
- 信号收盘确认后下一根开盘使用独立 `Short ENR`；两高较高者上方1 tick止损，1R确认后近似保本、1.5R全平，退出注释与 RSI 背离隔离。
- 保留同棒冲突与冻结突破线保护；保护解除后独立旁路逆势横盘目标门禁。悬浮热区新增锚点、量价、RSI、阴线、布林、止损和版本详情。
- TradingView 源码为1754行、FNV-1a 32 `403adb2b`，编辑器写入1755行计数并编译0错误。
- BTC 当前窗口新增1笔：`2026-07-22 08:45` 确认，下一根 `66632.5` 开空、`66148.5` 止盈，毛收益 `+484.0 USDT`；组合从7笔 `+265.00 / PF 1.1780` 改善到8笔 `+749.00 / PF 1.5032`，最大回撤仍为 `976.20`。
- ETH 前后均为14笔 `+331.42 / PF 8.7092 / DD 22.06`，没有新增样本。状态保持 `research_only_not_promoted_insufficient_cross_symbol_samples`。
- 旧 Rust parity 仍明确冻结 `66d3937e`；本轮没有写数据库、注册 Paper/Live、部署或触发真实交易 mutation。评估见 `docs/VOLUME_EFFORT_NO_RESULT_LOWER_HIGH_BOLL_RECLAIM_SHORT_15M_V1_EVALUATION_MANIFEST.md`。

## 2026-07-26：放量布林下轨收回做多 V1

- 新增独立 TradingView Research 身份 `volume_bollinger_lower_reclaim_long_15m_v1`，没有覆盖 RSI 超卖形态、三棒反包或 Rust parity。
- 完整量能后要求低点出布林下轨、收盘回轨；下影至少50%、收盘位于振幅顶部75%、低点位于信号前48根区间底部15%，RSI 35～50连续两根回升且标准 MACD 负柱连续两根向零收缩。
- 信号在 t 收盘确认，下一根开盘使用 `Long BLR`；止损冻结在信号低点下方1 tick，目标冻结为信号时布林中轨，信号收盘预期不足1.1R时阻断。
- 新分支使用自身结构风控旁路逆势横盘目标，不复用旧 ATR 量比档，也不触发突破多单保护；同棒多空冲突仍不交易。
- BTC `2026-07-09 11:15` 诊断中，量比 `4.6084x`、下影 `55.11%`、收盘位置 `87.23%`、RSI `36.27→39.64→42.80`、MACD `-55.53248→-52.90089→-43.87334`、预期 `1.2152R` 均通过；前48根底部位置 `16.95%` 超过15%，最终没有开仓。
- BTC/ETH 同窗均为0个 BLR 原始信号和0笔成交；总策略仍为8笔 `+749.00 / PF 1.5032` 与14笔 `+331.42 / PF 8.7092`，只能判定“总策略仍为正”，不能判定新分支正收益。
- 最终 Pine 为1830行、FNV-1a 32 `90c8fc84`，TradingView 写入1831行且0编译错误；临时 `BLR_DIAG/BLR_SUM` 标签已清除，Rust 冻结快照哈希测试 `1 passed`。状态为 `research_only_not_promoted_no_samples`，未更新 Rust parity、Paper/Live、部署或真实交易。

## 2026-07-26：EMA596 收复后放量 HH/HL 离轨做多 V1

- 新增独立 TradingView Research 身份 `volume_ema596_reclaim_departure_hh_hl_long_15m_v1`；重合信号继续归属旧家族，未覆盖 Rust parity。
- 规则冻结为最近32根内收盘上穿EMA596且持续站稳，前一根离轨不超过0.5 ATR、当前至少1 ATR；当前收盘突破前4根高点并形成HH/HL，`vol_ccy` 同时高于周P90和前20根中位数2.5倍，阳线实体占振幅至少60%。
- `2026-07-02 16:00` 旧规则未入场是确定性门禁：上穿距今约26根、实体约0.30%低于旧1%门槛、过滤后量比约0.9008；不是展示或订单故障。
- 新分支在该棒收盘确认，下一根16:15以`60309.7`开多，冻结止损`60095.6`、2R目标`60737.8`，17:30完整止盈，固定1单位毛收益`+428.1 USDT`。
- BTC 新分支3笔、2胜1负、`+799.70`，组合改善为11笔`+1548.70 / PF 1.5854`，但最大回撤放大到`2133.20`。
- ETH 新分支4笔、1胜3负、`-86.57`，组合降为18笔`+244.85 / PF 2.7006 / DD 83.12`；跨币种验证失败。
- 最终源码快照为`26058470`，编辑器1914行、UTF-16长度113685；本地与TradingView编辑器哈希一致，编译0错误。状态为`research_only_not_promoted_cross_symbol_failure`，未改Rust、Paper、Live、部署或真实交易。
- 评估证据见`docs/VOLUME_EMA596_RECLAIM_DEPARTURE_HH_HL_LONG_15M_V1_EVALUATION_MANIFEST.md`。

## 2026-07-27：EMA596 收复接受后二次加速做多 V1

- 新增独立 Pine 与评估清单：`volume_ema596_reclaim_acceptance_reacceleration_long_15m_v1`，只属于 TradingView ResearchBar。
- BTC 目标样本按预注册结果执行：`06-22 09:15` 被前棒距离、当前离轨、完整空头排列与结构风险过滤；`19:00` 由持续接受后的二次加速成立，`19:15 → 20:00` 完成 2R，固定 1 单位毛收益 `+463.50 USDT`。
- BTC 当前窗口仅 1 笔、最大回撤 `88.00 USDT`；无亏损导致 PF/Sharpe 不可定义。ETH 为 0 笔，不能宣称跨币种正向收益。
- ETH 门禁漏斗已核对：初次收复 `73 → 55 → 3 → 0`（上穿/前棒距离/当前距离/双量能），二次加速 `288 → 23 → 4 → 0`（接受期/前棒距离/当前距离/均线过渡）。
- TradingView 编译 0 错误，本地/编辑器统一换行后 FNV-1a 32 均为 `86af0424`；独立脚本已保存。原 Research Chart 的误覆盖已恢复到本地 1914 行基线。
- 完整冻结规则、数据快照与结论见 `docs/VOLUME_EMA596_RECLAIM_ACCEPTANCE_REACCELERATION_LONG_15M_V1_EVALUATION_MANIFEST.md`；未提交、未推送、未部署、未触发真实交易。

## 2026-07-27：EMA596 收复接受后放量 HH/HL 离轨做多 V2

- ETH `2026-07-24 15:00` 的真实问题不是三条 EMA 全部向下，而是 EMA12 已被反弹拉成正斜率，EMA144 / EMA596 的预信号三棒斜率仍分别为 `-0.0344 / -0.00574 ATR/根`；V1 又允许 `reclaimAge=0`，并让 `-1.3295 ATR` 的负离轨天然通过 `<=0.50`。
- 新增 `volume_ema596_reclaim_departure_hh_hl_long_15m_v2`，要求收复至少发生在1根前、前棒严格位于 EMA596 上方 `0～0.5 ATR`；使用 `t-1～t-4` 已完成棒否决 EMA144 与 EMA596 同时强负，独立订单改为 `Long EMA596-D2`。
- 目标棒已被两层门禁独立过滤；若强制按 V1 在 `15:15` 约 `1908.46` 追入，`17:15` 会先触 `1878.73` 结构止损，固定1 ETH 毛亏约 `-29.73 USDT`。
- 保存并重载后的 TradingView 主研究图表编译0错误。BTC V2 为9笔、`+1177.10 / PF 1.7908 / DD 976.20`；ETH V2 为15笔、`+345.84 / PF 9.0447 / DD 40.84`。
- 相对关闭该分支的基线，BTC / ETH 分别增加 `+428.10 / +14.42`；相对 V1，ETH 全面改善，但 BTC 净利润减少 `371.60`，尽管 PF 和最大回撤改善。
- 预注册保留门槛因此失败，版本固定为 `research_only_not_promoted_preregistered_gate_failed`。V1 未覆盖，Rust parity、Paper、Live、部署和真实交易均未修改；完整证据见 `docs/VOLUME_EMA596_RECLAIM_DEPARTURE_HH_HL_LONG_15M_V2_EVALUATION_MANIFEST.md`。

## 2026-07-27：当前 Pine `3cbbc9d8` 同步 Rust V2 与 Top60 数据审计

- Rust 新增独立 `Current3cbbc9d8` 规则版本，旧 `Frozen66d3937e` 分支保持默认且可回放；新增 ENR、BLR、EMA596-D2 及对应优先级、下一开盘和独立退出，仍只属于 Research。
- BTC 30天9笔 `+1177.10 / PF 1.790796 / DD 976.20` 与当前 TradingView 完全一致；旧V1仍为7笔 `+265.00 / PF 1.178032`。ETH 在图表历史边界归一后首个规则/时序/成交差异为0，Rust 60天16笔 `+318.98`，TradingView 15笔已平仓 `+345.81`，另有未平仓空单。
- parity lib 29项、Top60 binary 2项测试及两个 Research binary 编译通过，scoped rustfmt完成；最大文件 `signals.rs` 1544行，低于2000硬上限但超过1000目标，行数脚本缺失而以 `wc -l` 兜底。
- Top60 strict 模式按冻结 manifest 完整覆盖 fail-close；原始结束时点完整成员为0/60。本地共同可用窗口只具备14/60成员，46个成员缺评价期或预热，因此没有伪造完整60品种结论。
- 显式 partial diagnostic 的1733笔结果为零成本 PF `1.0552`、平均 `-0.03893R`；每边5+3 bps事后成本压力 PF `0.7673`、平均 `-0.21341R`。固定1单位跨币 raw PnL 不具备组合可比性，current-live成员还带幸存者偏差。
- 用户明确要求跳过退市币并复用既有补 K 链路：新冻结 cohort 只取当次 OKX `current-live`；历史月包先补至2026-06-30，既有增量 REST backfill 再补7月尾段。60个成员全部成功，向本地 `quant_core` 写入/更新 `116334` 行，17个成员修复检测到的缺口。
- 重新审计达到 `sealed=true`、`60/60`，每币5760根预热、36826根评价 K 线，总覆盖2555160根；`vol_ccy`、confirm、原始 tick、首尾和内部15分钟连续性全部通过。
- 正式报告 `tradingview_velocity_strict_top60_okx_surviving_static_top60_15m_20260727_v1_b3aa75157a7d.json`：零成本7687笔、PF `1.1431`、平均 `-0.00044R`；每边5bps手续费+3bps滑点后 PF `0.8347`、平均 `-0.16292R`、净R `-1252.34`，仅12/60币盈利。
- 双成本逐币交易数、阻塞信号数、结束持仓和待成交状态一致；数据门禁已解除，但成本后表现明确不满足晋级标准。版本状态改为 `research_only_not_promoted_full60_cost_gate_failed`；未进入 Paper/Live、未部署、未触发交易所 mutation。

## 2026-07-28：EMA压缩扩张结构破位与多头死叉保护 V3

- 新增独立 Research V3 `tradingview_velocity_parity_15m_research_v3@7827654b`，
  V1/V2 冻结身份与默认入口均未覆盖。
- Pine 与 Rust 同步实现：EMA压缩扩张空必须收盘跌破前20根冻结低点；
  最近三根 MACD 死叉只阻断 RSI底背离多和 EMA压缩扩张多，缺历史失败关闭。
- 三笔方向审计完成：RSR 与 BCH 目标亏损已过滤；SATS 实际为空单，死叉和
  结构破位均支持空头，因此保留，避免把“不能做多”误写成“不能做空”。
- TradingView 编译0错误；Rust parity lib 58项、相关 CLI 6项测试与
  `cargo fmt --all -- --check` 全部通过。
- 冻结60/60 V3报告为7318笔：零成本 `+73.62R / 平均+0.01006R /
  PF 1.117`，相对V2改善 `+77.04R`；费用压力为 `-1107.31R /
  平均-0.15131R / PF 0.819`，相对V2少亏 `145.02R`，但最大回撤和PF均
  恶化。
- 结论固定为 `research_only_not_promoted_mixed_ablation_cost_gate_failed`：
  局部因果修复有效，费用后仍非正收益；未注册 Paper/Live、未部署、未触发
  真实交易 mutation。

## 2026-07-28：TradingView Velocity Research V4 逆势退出与背离确认

- Pine / Rust 已冻结独立 V4 身份
  `tradingview_velocity_parity_15m_research_v4@9ab73288`。主 Pine 在
  TradingView 编译0错误；V3继续绑定独立 `7827654b` 快照。
- V4 实现三项预注册变化：逆势结构实际成交1R单向移动保护、fresh横盘近边/
  96根大位移远边目标、RSI背离方向实体或25%方向影线门禁。保护只在完成棒
  后更新并于下一棒生效，不使用未来 K 线。
- 静态审计修正两个重叠优先级：fresh逆势空与 transition sweep 同棒时，
  Rust 现在与 Pine 一致地使用已冻结结构目标和信号棒高点止损；V1～V3行为
  保持冻结。
- Rust parity lib 67项、两个命令入口4项测试和全量格式检查通过。strict
  report schema升为2，每笔保存 `exit_policy`，成本路径一致性也比较该字段。
- V3 schema 2重跑为7318笔，零成本`+73.62R / PF 1.117`、压力成本
  `-1107.31R / PF 0.819`，与历史schema 1的聚合、逐币及剔除新增字段后的
  逐笔路径完全一致。
- V4-ABC 为7153笔：零成本`+50.76R / 平均+0.00710R / PF 1.167 /
  胜率36.66%`，压力成本`-1097.57R / 平均-0.15344R / PF 0.834`。
  零成本顺序权益回撤由11662.42降至9593.58，压力成本由22423.44降至
  21244.73，但整体未获得成本后正期望。
- 全部逆势结构仓零成本由`+116.58R`变为`+119.47R`，压力成本由
  `-164.80R`变为`-146.57R`；564笔移动保护贡献零成本`+424.67R`。
  同时结构止盈393→210、贡献`1135.59R→326.55R`，证明保护减少回吐也
  截断长尾盈利。
- RSI底背离零成本`-78.50R→-42.24R`、压力成本
  `-164.63R→-114.68R`，仍为负；RSI顶背离零成本
  `+61.14R→+5.48R`、压力成本`-42.06R→-75.82R`，明显恶化。
  被K线门禁删除的V3底/顶背离分别仍为`+7.32R / +23.79R`，门禁未证明
  能筛掉亏损。
- BTC 零成本 PF 与回撤改善，但净R`11.97→10.38`；压力成本净R
  `-21.77→-22.62`。ETH不在冻结60成员中，本轮不外推。
- 状态：
  `research_only_not_promoted_abc_cost_gate_failed_ablation_pending`。
  V4不替换V3、不进入Paper/Live、不部署。完整规则与结论见策略文档第34节，
  正式schema 2报告已保存到 `docs/backtest_reports/`。

## 2026-07-28：未来架构与迁移文档优化

- 仅审阅并修正 `docs/architecture/**` 与 `rust-quant-architecture` Skill；没有读取当前架构代码来反推目标设计，也没有修改业务代码、数据库或运行态。
- 业务入口已固定为 Web canonical `ExecutionRequest`，不存在 Core 自营账户；平台固定 `MarketDataAccessCredential` 仅用于 K 线、instrument、公开盘口/成交等 Market 公共只读 endpoint。
- Research/live 已分离为 `ResearchDecisionContextSnapshot -> ExecutionPlanningValue -> SimulationLedger`、Paper simulated OMS 与 live `ExecutionPlan` 三层，不再用最终 PnL 代替逐层 parity。
- live 前置已补齐稳定 `ExchangeAccountRef`、`ClaimExecutionRequestReceiptV1`、`ResolvedMarketEvidenceSetV1`、AccountFact/Recovery、RiskValuation、Exchange capability、SafetyMonitoring/Ack、外部仓位占用和 credential 撤销安全尾部。
- `MP-market-velocity-execution-v1` 的 15 个 Owner children 与 Markdown 表完全一致；`MP-vegas-research-parity-v1` 的 V0～V12 名称、Owner 和依赖完全一致。
- 验证结果：两个 TOML 可解析；Registry 3 个 Program、37 个 Contract、30 个 child，无重复 child、缺失依赖或环，共享 Contract schema 一致；179 个本地 Markdown 链接无缺失；目标四份主文档无同级重复 heading；`git diff --check` 与冲突/旧术语扫描无异常。
- 未暂存、未提交、未推送、未部署，也未触发任何真实交易 mutation。目标文档不等于迁移已落地；后续仍需逐 Owner Manifest/Evidence/Verdict 和显式 cutover 授权。

## 2026-07-28：V4 RSI背离亏损只读审计

- 已导出RSI底背离286笔、顶背离318笔完整压力亏损CSV；其中真亏损分别为
  244/253笔，仅成本转亏为42/65笔。
- matched审计确认：顶背离共同462笔从`-38.17R`恶化到`-75.23R`；
  33笔结构赢家被1R追踪截断少赚`83.32R`，31笔原止损被救回`52.08R`，
  净损失约`31.24R`。门禁移除的157笔压力合计原为`-3.89R`，不是主因。
- TradingView目检覆盖YGG底背离真失败、NMR/ADA顶背离真失败、ICP/ETC
  成本型亏损及YGG顶背离追踪截断价格路径。更早样本因客户端可见历史限制，
  只引用冻结逐笔回放，不宣称图表目检。
- 形成互斥研究分桶：逆势顶背离恢复V3远结构退出；中性纯RSI背离不交易；
  逆势底背离单独研究方向确认与1R保护；净保本覆盖成本；fresh目标独立消融。
- 本轮未修改策略代码、未运行参数优化、未注册Paper/Live。详见
  `docs/backtest_reports/tradingview_velocity_v4_rsi_divergence_loss_audit_20260728.md`。

## 2026-07-28：纯 RSI 背离 EMA 排列年龄 V5

- 独立实现
  `tradingview_velocity_parity_15m_research_v5@a36f0e19`，V1～V4 未覆盖；
  Pine 和 Rust 使用相同的 EMA 年龄、市场状态、结构目标与保护状态机。
- 中性/过渡纯 RSI 禁开；严格顺势沿用 V4；严格逆势 1～599 根使用近边
  短目标，>=600 根使用远边目标、结构确认净保本和确认后的 2R 宽追踪。
- Pine 在 TradingView 编译 0 错误；Rust parity 76 项、两个命令入口 4 项
  测试及全量格式检查通过。
- sealed Top60 严格报告达到 60/60、60 天预热、2,555,160 根覆盖。V5
  压力净 R 为 `-968.01R`，比 V4 少亏 `129.56R`，但 PF
  `0.834→0.810`、顺序权益回撤 `21244.73→21954.31`。
- 纯 RSI 压力亏损合计从 `-190.50R` 降到 `-56.80R`；主要贡献来自删除
  中性/过渡交易。新趋势近边目标只净改善 `1.14R`，且多空方向相反。
- 正式样本的最大 EMA 排列年龄为 578，>=600 分支 0 笔；BTC 结果恶化，
  ETH 不在冻结 cohort 中。
- 最终状态：
  `research_only_not_promoted_cost_gate_failed_mature_branch_unobserved`。
  保留 Research 证据，不注册 Paper/Live，不部署、不触发交易 mutation。

## 2026-07-28：当前亏损归因与 EMA 趋势多结构接受 V6

- V5 压力成本共 `6,662` 笔、`4,427` 笔亏损、净 `-968.01R`。精确
  单一家族累计亏损前三为 `ema_trend_long -272.21R`、
  `rsi_overbought_pattern -242.14R`、`rsi_oversold_pattern -189.54R`；
  样本不少于 30 笔时平均最差为
  `ema_compression_expansion_short -0.4809R/笔`。
- 本轮按预注册选择规则只优化 `ema_trend_long`。TradingView 代表样本
  ATOM/LTC/FLOW 显示，亏损集中于极端量能末端拉升、20 根局部突破碰撞
  更大供应、EMA 慢线未成熟和确认后继续追价；RSR 盈利对照具有更完整的
  突破前收缩与多棒结构接受。
- V6 冻结 20 根突破线与来源棒 ATR/量比/目标，只允许来源后 1～3 根完成棒
  连续站在线外并回到来源收盘后生成信号。接受棒不再被未独立满足量能门槛
  的 RSI 图形污染家族、详情或绝对止损。
- Rust parity `82/82`、strict Top60 `4/4` 通过；Pine V6 和恢复后的 V5
  均在 TradingView 编译 0 错误。正式回放达到 60/60、每币 60 天预热、
  `2,555,160` 根已完成 15 分钟 K 线，成交量为 `vol_ccy`。
- V6 全策略从 `6,662/-968.01R/PF 0.810` 变为
  `5,967/-795.85R/PF 0.825`；纯 EMA 趋势多从
  `1,070/-272.21R/-0.2544R/PF_R 0.690` 变为
  `322/-92.28R/-0.2866R/PF_R 0.656`。
- 累计少亏来自删除 748 笔目标交易，保留交易的平均 R 与 PF_R 均恶化，
  因此 V6 判定
  `research_only_rejected_target_quality_gate_failed`。主 Pine 保持 V5，
  V6 只保留可复现否证证据，不进入 Paper/ReadOnly/Live、不部署、不触发
  交易 mutation。

下一步：先补齐报告中的 setup 来源索引、冻结突破线、确认年龄和失效原因；
若继续，只能在未见窗口预注册真实回踩收回、EMA144/EMA696 正斜率、
48/96 根结构空间，并把 `>=6x` 极端量能拆为独立耗竭/二次突破假设。当前
V5 下一个累计亏损研究对象是 `rsi_overbought_pattern`，不得与后续 EMA
趋势多新版本同时修改。

## 2026-07-28：ATOM RSI 反向长影门禁 V7 与三棒阻力退出审计

- 用户口述的 ATOM `07-28 05:45` 已纠正为截图实际 `07:45`；`05:45`
  为阴线，截图信号棒为 `1.295 / 1.306 / 1.295 / 1.298`。
- 信号棒满足实体看涨吞没，但上影占振幅 `72.73%`、实体仅 `27.27%`，
  因此预注册 V7 只为四个 RSI 家族增加既有 60% 反向长影对称门禁。
- Pine/Rust 已实现独立 V7 候选；ATOM、镜像、恰好 60% 和低于 60% 边界
  测试通过，TradingView Pine 编译 0 错误。
- sealed Top60 达到 60/60。V7 从 6,662 笔降到 6,642 笔，共有 6,642
  笔逐笔漂移为 0；删除 20 笔零成本合计 `+7.5791R`，压力成本后仍约
  `+2.9329R`。
- V7 压力净 R `-968.0088→-970.9416`、平均 R
  `-0.145303→-0.146182`，违反预注册门槛，状态为
  `research_rejected_negative_ev_delta`。主 Pine 恢复 V5 `a36f0e19`；
  V7 仅保留 Research 快照、Rust 候选和 60/60 证据。
- ATOM 7 月 17 日正式交易为 `21:15` 信号、`21:30 @1.502` 成交；
  1.5R 目标为 `1.529`，最高 `1.527`，所以没有全平不是执行漏单。
  用户指出的 `1.526` 历史阻力未进入当前三棒退出合同，且主要位于信号前
  60～96 根，超出 48 根横盘扫描。本轮只做审计，不修改 1R/1.5R 退出。

下一步若继续，必须另立三棒退出 Research 消融：信号时冻结 48～96 根历史
供应区、至少两组独立触碰、排除不足 1.0～1.1R 的近端目标，最终目标使用
`min(1.5R, 冻结历史阻力)`；不得与已拒绝的 RSI V7 混改。

## 2026-07-28：LTC 慢均线带收复 V8 快速闭环

- LTC 目标棒并非刚上穿内部 EMA696；其在信号棒重新放量收复
  `max(EMA596, EMA696)`，原普通 RSI 超买长上影空最终止损。
- 新增 V8 Research 门禁：强放量阳实体收复慢均线带后，当前与后4根只要
  完成收盘持续站在线上，就拒绝普通 RSI 超买长上影空。
- Pine/Rust 已同源，Pine 身份 `252225ec`；TradingView 编译 0 错误，
  Rust focused `5/5`、完整 parity `91/91`、strict CLI `4/4` 通过。
- 同一 LTC 图表 V5/V8：交易 `18→17`、净利润 `-0.06→+0.17 USDT`、
  PF `0.9822→1.0541`、最大回撤不变。
- 用户要求停止数据库复审，因此未生成正式 60/60 结论。V8 当前只在
  TradingView Research 主图启用，不进入 Paper、ReadOnly、Live。

## 2026-07-29：五类入场质量 V10/V11

- 新增并冻结 V10 `06973f3c`、V11 `53ba4291`，历史 V1～V9 未覆盖。
- EMA 趋势多与压缩扩张多空改为来源棒冻结结构、后续 1～3 根回踩接受；
  普通 RSI 必须离开 30/70 极值、破前棒结构；EMA596 增加慢线斜率、
  8 根结构和最大离轨。
- V11 在结果前预注册：压缩多量比 `>=3`、普通 RSI 必须跨回 EMA12、
  压缩空接受 RSI `>=35`、EMA596 RSI `<=70`。
- TradingView 主 Pine 编译 0 错误；Rust Research parity `98/98`
  通过，Pine/Rust 均绑定 V11 源码身份。
- 既有数据库 43 个完整成员诊断中，五项目标集合从 V9
  `2,728/-588.04R/PF_R 0.717`，经 V10
  `425/-53.67R/0.807`，改善为 V11
  `160/+1.42R/1.018`。
- 全策略 V11 仍为 `-49.52R/PF_R 0.967`；普通 RSI 仍负、压缩多仅
  1 笔亏损，一单位价格 PnL 与回撤相对 V10 恶化。
- 最终状态：
  `research_only_partial_43_of_60_not_promoted`。未注册 Paper/Live，
  未部署，未触发数据库写入或交易 mutation。

## 2026-07-29：V12 统一去耦确认失败审计

- 预注册并实现 V12：EMA 趋势、压缩扩张、普通 RSI 采用 setup 后
  第 2～4 根确认；EMA596 恢复 V10 结构并取消 RSI 单一硬否决。
- Pine 快照 `34752685` 与 Rust `CandidateV12` 同源；TradingView 编译
  0 错误，Rust parity 相关 99 个测试通过。
- 同一 43 币诊断中，目标集合从 V11 的
  `160 笔/+1.42R/PF_R 1.018` 恶化为
  `138 笔/-50.39R/PF_R 0.589`；统一等待既未恢复交易数，也损害收益。
- V12 保留为 `rejected_research_diagnostic`；当前 TradingView 主图和
  主 Pine 已恢复 V11 `53ba4291`。没有 Paper/Live 注册、部署或数据库写入。

## 2026-07-29：V13 压缩扩张分阶段接受失败审计

- V13 `b81e5d25` 只修改 EMA 压缩扩张：原方向 setup 冻结 20 根结构，
  0～2 根等待放量 impulse，再用 1～3 根回踩确认；其他 V11 家族冻结。
- Rust 没有新增平行 `v13.rs`，而是在既有 V10/V11 状态 owner 内增加
  显式 policy，保持单一状态机；Research-only，无迁移或运行态切换。
- TradingView V13 编译 0 错误；Rust parity `100/100`、CLI `7/7`
  通过。四个冻结家族的 symbol、方向、时间、价格、止损和退出逐笔一致。
- 同一 43/60 诊断中，压缩多 `1→0`，压缩空仍为 MOVE 单笔
  `+5.22R`；压缩族仅 1 币/1 事件且单笔贡献毛利润 100%，失败于所有样本
  恢复和集中度门禁。
- 全策略 `-49.52R→-48.40R` 只是删除 MERL 一笔 `-1.12R` 止损，
  不能视为可推广改进。V13 冻结为 `rejected_research_diagnostic`，
  TradingView 已恢复 V11 `53ba4291`，未进入 Paper/Live/部署或交易 mutation。

## 2026-07-29：V14 无方向压缩制度失败审计

- V14 `45391eac` 把压缩改为无方向 setup，方向只由后续 8 根内的 EMA
  扩张、放量破位和 1～3 根回踩接受决定；规则在结果前预注册。
- Rust parity `101/101`、三个 CLI `7/7` 通过；TradingView 候选与恢复后的
  V11 均编译 0 错误。EMA 趋势多、普通 RSI 多空、EMA596 多与 V11 逐笔一致。
- 43/60 同口径回放只产生 WIF、ICP 两笔压缩空，均止损，合计
  `PF_R=0 / -2.31R`；压缩多为 0，覆盖仅 2 币/2 事件。
- 六目标方向从 V11 `+1.42R` 恶化到 `-5.00R`，全策略从 `-49.52R`
  恶化到 `-50.48R`。V14 失败于全部样本恢复和收益门槛。
- V14 冻结为 `rejected_research_diagnostic`；TradingView 主 Research
  与本地主 Pine 保持 V11 `53ba4291`，无 Paper/Live、部署、数据库写入或
  交易 mutation。

## 2026-07-29：V15 真实箱体突破接受失败审计

- 新增独立 V15 `28f2817f`，用 48 根真实箱体、冻结放量突破和 1～4 根
  回踩接受替代 EMA 接近；V11 主策略未覆盖。
- 退出合同独立为：结构止损、实际 1R 平 33%、收盘确认净保本、2R 后
  1 ATR 单向追踪、箱体重新进入退出和冻结量度目标。
- TradingView Pine 编译 0 错误；Rust parity `108/108`、Top60 CLI
  `3/3` 通过。
- 本地已有数据的 43 个完整成员产生 75 笔、34 币、74 个时间方向事件；
  零成本 `+8.85R / PF_R 1.226`，压力成本后
  `-44.97R / PF_R 0.371`。
- 25 笔在入场当根止损；达到 1R 保护的 34 笔压力成本后仍为
  `+21.11R`。失败主因是接受后实际入场质量与约 `0.267%` 的过窄中位
  结构风险，不是目标过小。
- V15 状态为 `research_only_rejected_cost_and_entry_quality_gate_failed`；
  不进入 Paper/ReadOnly/Live，不进行数据库补造、部署或交易 mutation。

## 2026-07-29：V16 右侧触发过度过滤审计

- 新增独立 V16 `5ac357c1`：V15 接受后不再直接市价入场，而是在后续
  1～3 根等待接受棒高/低点外 1 tick 的 stop-entry；未成交前的结构失效、
  箱体重入和超时会撤单。
- Pine 在 TradingView 编译 0 错误；Rust parity `111/111`，Top60、
  strict Top60 与单币 runner 合计 `7/7` 通过。
- 同一 43/60 本地完整成员诊断仅剩 VIRTUAL、1INCH 两笔多单；
  交易/事件/币种从 `75/74/34` 降为 `2/2/2`，空头为 0。
- 两笔压力成本后合计 `+1.94R`，无入场当根止损，但最大单币贡献
  `79.21%`，不能把两笔 100% 胜率视为可靠优势。
- V16 比 V15 净增 73 个阻塞，说明 `0.35 ATR + 1.80R + 0.30R`
  组合门禁主导了结果，右侧触发本身尚未得到独立验证。
- 当前 TradingView 主策略保存内容已恢复为本地 V11，逐字一致。
  V16 状态为 `research_only_rejected_overfiltered_two_trade_sample_not_promoted`；
  无 Paper/Live、部署、数据库写入或交易 mutation。

## 2026-07-29：V17 纯右侧触发消融审计

- V17 `7097ee03` 仅保留 V15 接受资格和 V16 的 1～3 根 stop-entry，
  移除 `0.35 ATR / 触发价1.80R / 成本0.30R` 三个组合门禁。
- TradingView V17 与恢复后的 V11 均为 0 编译错误、0 警告；Rust parity
  `112/112`、三个 CLI `7/7`、格式检查通过。
- 同一 43/60 本地完整成员恢复到 32 笔、31 个事件、22 个币种，多空
  17 / 15；全部 32 个事件都能与 V15 同币种、方向、信号时间对应。
- 入场当根止损从 V15 的 25 / 75 降为 0 / 32，证明有限右侧触发修复了
  最明显的追价错误；但 13 笔后续结构止损和 5 笔箱体重入仍形成负贡献。
- 零成本 `+4.08R / PF_R 1.262`，压力成本后
  `-7.70R / PF_R 0.659`；平均每笔毛期望 `+0.128R`，平均摩擦
  `0.368R`，多空压力结果均为负。
- 最大单币/事件正贡献为 `31.59% / 12.92%`，集中度通过；失败来自
  普遍的结构距离与 15 分钟摩擦不匹配，而不是单币异常。
- V17 状态为 `research_only_rejected_negative_net_edge_after_cost`；
  未覆盖 V11/V15/V16，未进入 Paper/Live、未部署、未写数据库或触发交易。

## 2026-07-29：V18 主策略组合审计

- V18 `9f26295a` 将冻结 V11 作为主策略、V17 作为低优先级补充家族；
  同棒冲突由 V11 优先，未成交 V17 stop-entry 遇 V11 入场会撤销。
- TradingView 当前主 Research 已加载 V18，编译 0 错误；Rust parity
  `112/112`、三个 CLI `7/7` 和格式检查通过。
- 同一 43/60 本地完整成员中，V11 的 2,262 笔交易逐笔身份与结果不变；
  V17 实际新增 29 笔、26 个时间方向事件，多空 15 / 14，总交易 2,291 笔。
- 增量零成本 `+5.40R / PF_R 1.398`，压力成本后
  `-5.47R / PF_R 0.726`；V18 总压力结果从 V11 的 `-49.52R`
  恶化到 `-54.99R`。
- 样本、方向、入场当根止损和集中度门槛通过，成本后净 R、平均 R、PF_R
  全部失败。V18 仅保留为图表 Research，未进入 Paper/ReadOnly/Live、
  部署、数据库写入或交易 mutation。

## 2026-07-30：EMA 空头趋势单变量消融完成

- 在 V19 同源基线上实现五个互斥的 Rust Research-only 变量，基线回放与既有
  V19 交易和指标完全一致；Pine、Paper、Live 与默认策略均未改动。
- 同一 43/60 完整成员、2025-07-01 至 2026-07-19、每边 8 bps 成本下，
  `structure_break` 把 EMA 空头由 `1301 笔/-19.38R/PF_R 0.980`
  改为 `821 笔/+38.90R/PF_R 1.065`。
- 全策略成本 R 从 `-51.65R/PF_R 0.966` 改为
  `+18.90R/PF_R 1.016`；入场当根止损率从 `9.84%` 降至 `7.43%`，
  一根内止损率从 `20.75%` 降至 `17.54%`。
- 被结构条件删除的 507 笔合计 `-69.15R`，路径释放后新增的 27 笔为
  `-10.87R`，说明过滤有效但组合持仓路径仍会稀释优势。
- BTC 4 笔仍为 `-1.84R/PF_R 0.479`，ETH 不在本轮完整成员内；每边总成本
  12 bps 时 EMA 空头转为 `-17.02R`，因此只晋级下一轮 Research。
- `slope_spread` 快速止损恶化；`right_side_retest` 与 `distance_guard`
  样本塌缩；`extreme_volume_acceptance` 改善极小且仍负期望，全部淘汰。
- Rust parity `121/121`、Top60 CLI `4/4` 通过；未部署、未写数据库、
  未触发任何交易 mutation。

## 2026-07-30：结构破位深度单变量验证

- 预注册 D0、D10 与 D20 三档，只改变信号收盘相对前 20 根低点的 ATR 深度；
  评价窗口、43 个完整冻结成员、60 天预热、每边 8 bps 成本与退出均冻结。
- 新增 Rust Research-only 版本和精确边界测试；D0 重跑与上一轮报告逐字段一致，
  未修改 TradingView Pine、Paper、ReadOnly、Live 或生产。
- D10 EMA 空头 `786笔/+25.19R/PF_R 1.043`，低于 D0 的
  `821笔/+38.90R/PF_R 1.065`，且每边 10 bps 时转负。
- D20 EMA 空头 `745笔/+33.81R/PF_R 1.062`，全策略仅微增约 `0.22R`；
  目标家族净 R、平均 R 与 PF_R 仍全部低于 D0。
- D10 删除的 36 笔 D0 交易为 `+15.37R`；D20 删除的 78 笔为
  `+5.66R`。实际成交深度收益非单调，不能事后新增区间排除。
- 两档均按预注册门槛淘汰，D0 保持唯一 Research 候选；BTC 仍负、
  ETH 无同口径证据，2025-08/09 仍是主要失败窗口。
- 结果记录于
  `tradingview_velocity_v19_ema_short_structure_break_depth_result_20260730.md`；
  未部署、未写数据库、未触发交易 mutation。

## 2026-07-30：EMA676 斜率与目标亏损市场状态审计

- 预注册 S20 只要求 D0 结构破位时实际 EMA676 低于 20 根前；D0 重跑与上一轮
  报告忽略生成时间后逐字段一致。
- S20 EMA 空头为 `817笔/+43.41R/+0.0531R每笔/PF_R 1.073`，只比 D0
  删除 4 笔亏损、增加 `4.51R`；快速止损率略升，PF 与目标状态门槛失败。
- 2025-08/09 保持 `157笔/-124.69R`，BTC 保持
  `4笔/-1.84R/PF_R 0.479`；被删除交易与两者均无交集，因此淘汰慢 EMA
  斜率假设，保留 D0 Research 基线。
- 后验诊断显示目标两个月的 157 笔只构成 39 个同步市场事件，主要形态是多币种
  局部破位后快速反抽；BTC 4 笔则都位于 96 根区间底部 20%，更偏向末端追空。
- Research 测试 `9/9`、Top60 CLI `4/4`、Rust 格式检查及 2000 行硬上限通过。
  详细结果见
  `tradingview_velocity_v19_ema_short_ema676_slope_regime_result_20260730.md`；
  未修改 Pine、Paper、ReadOnly、Live，未写数据库或触发交易。

## 2026-07-30：迁移 V1 交易所范围与 SDK 前置审计

- V1 持续采集、readiness、SDK 验收和运行 inventory 固定为
  `binance_usdm + okx_swap`；Bitget、Bybit、Gate、KuCoin 仅保留 legacy
  历史解析与未来 backlog，不阻塞两家 V1。
- Binance 已有正确 anonymous `/fapi/v1/exchangeInfo`，但缺 typed snapshot、
  quota/error evidence 和 public-only root facade；OKX 存在正确 path 走签名、
  anonymous 入口 path 错误的双入口冲突。
- 并行审计发现 `rust_quant_alpha` 的 migration-check P1 把仓库根、role map、
  Release Unit、target path 和 `owner_repository` 固定为本仓库，不能验证
  `crypto_exc_all` Owner Manifest。
- 已在待提交 Registry 中登记 P2、Exchange SDK I1 和 Market F4C 三个
  `not_created` child，并把 I1 的治理依赖固定为 P2；未创建 Manifest、SDK
  源码或 runtime，未提交、部署或执行网络/交易 mutation。
- 已冻结 P2 的三仓上下文、显式 repository root、目标 repo Git/Cargo/path、
  registration/current Registry 校验与 non-deployable library profile 合同；
  `crypto_exc_all` 的既有 README 脏文件必须在 I1 前独立处理，不能扩大 allowlist。
- 修正 Program 状态规范：current-migration 的 planned/active Program 都可包含
  `not_created` child，但父 Program active 不传播实施或依赖资格；这消除了 MVE
  14 个未来 child 与 README 规则的矛盾。
- Owner 边界已矫正：SDK I1 只新增 Binance/OKX typed protocol capability，不禁用
  其他既有 SDK module；V1 inventory、`UnsupportedProvider` 与 source profile 归
  Market F4C。
- F4B 后继文档进一步拆清 completeness：SDK 证明 provider response 的协议/解码
  完整性；Market F4C 判定 source snapshot 的业务完整性与 readiness，避免双方
  同时拥有未限定的 `response completeness`。
- `rust-quant-architecture` 技能已同步两阶段跨仓库登记、三仓预检和 SDK/Market
  Owner 规则，防止后续再次把父 Program 状态或 SDK module 误当实施授权。
- 已修正 Registry 父计划链接：阶段 1、2、4 使用文档内稳定 `stage-*` anchor；
  当前 6 个 Program 的所有 `parent_plan` 文件和 fragment 均可解析。
- 已把跨 Program 依赖冻结为全局 child ID 图：当前 47 个 child、4 条跨 Program
  边无缺失、重复或环，且 predecessor owner repository 均在 successor Program
  的仓库集合内。
- F4B migration-check 复验通过：48 个 changed paths，0 error、0 warning，
  `verdict_generated=false`。

## 2026-07-30：D0 EMA 空头交易解剖与标准退出方向

- 先冻结交易解剖清单，再在 Top60 runner 增加独立模块；没有修改 V19 信号、Pine、
  Paper、ReadOnly、Live、数据库或执行路径。
- 同一 43/60 完整成员中，821 笔实际成交 EMA D0 全部生成可审计记录，0 笔无效；
  全样本 `+38.90R/PF_R 1.065`，2025-08/09 为 `-124.69R/PF_R 0.219`，
  其他月份为 `+163.59R/PF_R 1.370`。
- 536 笔亏损按退出前保守 MFE 分为：221 笔不足 0.5R、150 笔介于
  0.5～1R、165 笔达到 1R 后仍亏损。后者占 30.78%，触发预注册的第一优先级。
- 165 笔 1R 回吐中 157 笔是 `Fixed/StopLoss`；目标月份的 50 笔全部如此。
  这说明下一轮应隔离固定 EMA D0 的完成棒 1R 净保本，而非再调 EMA 距离或区间位置。
- 初始止损后 16 根内恢复 1R 仅 17.27%，目标月份仅 10.79%；破位线 2 根收回率
  目标/其他月份为 41.73%/40.55%，均不足以优先支持“止损太紧”或统一接受确认。
- 2025-08/09 的 157 笔只对应 39 个 60 分钟事件；21 个多币事件平均
  `-5.423R`、贡献 88.96% 亏损。其他月份多币事件平均 `+2.004R`，因此同步触发
  是制度依赖的组合风险，不是可无条件过滤的坏信号。
- BTC 4 笔为 `-1.84R/PF_R 0.479`，三笔亏损退出前 MFE 都小于 1R；
  下一轮净保本只处理全样本主要回吐问题，不宣称修复 BTC。
- 已预注册下一轮唯一变量：纯 EMA D0 `Fixed` 空单完成棒收盘达到 1R 后，从下一根
  启用覆盖双边各 8 bps 的 tick 对齐净保本，原目标不变，不分批、不追踪、不改入场。
- `cargo test -p rust-quant-cli --bin tradingview_velocity_top60` 8/8 通过；
  runner 989 行、诊断模块 926 行，均通过 1000 行目标和 2000 行硬上限。

## 2026-07-30：D0 EMA 空头完成棒 1R 净保本验证

- 新增 Research-only 隔离退出反事实模块；没有把新退出写进共享 broker，因而不会因
  提前释放仓位制造后续入场。旧/新报告 1,837 笔全策略交易 identity SHA256 均为
  `04978b13ae72000a26519f8bac14af5cb70ab943aab6246ee3507eb567cab564`。
- D0 EMA 空头保持 821 笔，0 笔身份漂移；803 笔 Fixed 交易中 279 笔完成棒收盘
  达到 1R，124 笔随后被净保本提前退出。
- 每边 8 bps 下，全部 D0 从 `+38.9011R / 0.0474R每笔 / PF_R 1.0647`
  变为 `-8.0411R / -0.0098R每笔 / PF_R 0.9846`。70 笔原亏损改善
  `80.7568R`，但 54 笔原赢家被截断 `127.6990R`。
- 2025-08/09 从 `-124.6911R` 改善为 `-108.3286R`，仅减亏 13.12%；
  其他月份从 `+163.5922R` 降为 `+100.2875R`，下降 38.70%。
- 每边 10 bps 时基线仍为 `+10.9401R`，变体为 `-36.0114R`；12 bps 时分别为
  `-17.0209R` 与 `-63.9818R`。BTC 从 `-1.8387R` 恶化为 `-3.5275R`。
- focused tests `14/14` 通过，覆盖盘中 1R 不激活、激活仅影响下一棒、跳空开盘成交
  及同棒目标/净保本路径顺序。该变量未通过任何经济晋级门槛，已淘汰。
- 详细 JSON 与中文报告分别为
  `tradingview_velocity_v19_d0_ema_short_completed_close_1r_net_be_43of60_20260730.json`
  和
  `tradingview_velocity_v19_d0_ema_short_completed_close_1r_net_be_result_20260730.md`；
  未修改 Pine、Paper、ReadOnly、Live，未写数据库或触发交易 mutation。

## 2026-07-30：D0 EMA 空头结构破位失败回抽净保本验证

- 新增 Research-only 三阶段退出反事实：完成棒确认 1R 后，冻结第一次收盘跌破此前
  20 根最低点的结构线，只有更晚完成棒回抽该线失败后，才从下一根启用净保本。
- 同一 43/60 回放保持全策略 1,837 笔与 D0 821 笔身份不变；规范化
  `per_symbol` SHA256 为
  `86e01fd3e1407ba52b8bb52de016c00fb05cc855839dff1658379a47d68695b3`。
- 803 笔 Fixed EMA 空头中，279 笔确认 1R、125 笔后续破位、74 笔失败回抽，
  26 笔实际提前退出。18 笔亏损改善 `20.5952R`，8 笔赢家损失
  `20.4998R`，最终仅增加 `0.0954R`。
- 2025-08/09 从 `-124.6911R` 改善到 `-121.2902R`，只减亏 `2.73%`；
  其他月份下降 `3.3055R`，BTC 4 笔完全不变。
- 该变量虽避免了无条件 1R 保本的巨大损害，但未通过目标期减亏 25% 门槛，已淘汰。
  下一轮应转向事前市场状态入场门禁，不继续后验调整退出 lookback 或容差。
- focused tests `18/18` 通过；详细结果见
  `tradingview_velocity_v19_d0_ema_short_structure_break_failed_retest_net_be_result_20260730.md`。
  未修改 Pine、Paper、ReadOnly、Live，未写数据库或触发交易 mutation。

## 2026-07-31：V19 EMA 趋势多突破深度邻域

- 在 A3 补充来源上新增 Research-only `0.20 / 0.30 / 0.40 ATR` 冻结前高突破深度；
  原 V19 基线、其他信号家族、确认状态机和退出合同未改。
- 43/60、60 天预热、双边 8 bps 同口径下，0.20 ATR 的新增成交为
  `18笔/-5.95R/PF_R 0.613`；0.30 ATR 为 `11笔/+2.49R/PF_R 1.359`；
  0.40 ATR 为 `7笔/+1.92R/PF_R 1.419`。
- 全策略最好的 0.30 ATR 仍为 `-49.15R/PF_R 0.968`，仅比 A2 改善
  `3.60R`，不满足晋级门槛，未合并 Pine、Paper、ReadOnly、Live 或主策略。
- 发现既有 distance 1.5 阶梯没有传入 Candidate V19 使用的 V10 状态机；
  ETH 07-14 的 `0.44 ATR` 突破会通过本轮门槛，但约 `1.29 ATR` 的 EMA12
  距离仍被硬编码 `1.25 ATR` 阻塞。
- 格式检查、Research helper 2/2、Top60 runner 18/18 通过；核心
  `signals.rs` 保持 2000 行硬上限。详细报告见
  `tradingview_velocity_v19_ema_long_body003_break_depth_result_20260731.md`。

## 2026-07-31：V19 EMA 趋势多 EMA12 距离邻域

- 修复 Research 距离参数此前只进入 V12、未进入 Candidate V19 所用 V10 的
  实验合同；默认路径仍固定为 `1.25 ATR`。
- 约 `1.29 ATR` 的专用契约测试证明 1.25 档拒绝、1.35 档接受；V10 8/8、
  Research helper 2/2、Top60 runner 18/18、release 构建和格式检查通过。
- 同一 43/60、60 天预热、双边各 8 bps 下，1.25 / 1.35 / 1.50 的全策略净 R
  为 `-49.15 / -59.19 / -85.71R`，PF_R 为 `0.968 / 0.962 / 0.946`。
- 1.25→1.35 新增 13 笔仅 2 盈、净 `-10.04R`；1.35→1.50 再新增
  43 笔仅 9 盈、净 `-26.53R`。两档均显著负期望，已淘汰。
- 失败样本大量在入场后四根内止损，说明“远离 EMA12 + 仅价格线回踩”
  仍是弱接受；下一轮应优先验证接受 K 的右侧阳线确认，而不是继续调距离。
- ETH 不在冻结 Top60，本轮只能确认其约 1.29 ATR 来源不再被硬编码距离阻塞，
  不能从 43/60 报告宣称该单最终成交。
- 详细结果见
  `tradingview_velocity_v19_ema_long_break_depth_0_30_distance_result_20260731.md`；
  未修改 Pine、Paper、ReadOnly、Live、数据库或交易执行。

## 2026-07-31：V19 EMA 趋势多阳线回踩接受

- 新增独立 Research-only 版本，只对 0.30 ATR 补充来源冻结“回踩接受 K 必须
  收阳”；原 V19 来源、其他信号和退出保持不变。
- 当前二进制重跑冻结基线与旧报告逐字段一致，排除状态字段导致的基线漂移。
- 同一 43/60、60 天预热、双边各 8 bps 下，全策略净 R 从
  `-49.1495R` 恶化到 `-50.1726R`，PF_R 从 `0.9680` 降到 `0.9673`。
- EMA 趋势多从 `23笔/+3.9094R/PF_R 1.2607` 降到
  `21笔/+2.8864R/PF_R 1.2081`，4 根内快速亏损仍为 7 笔。
- 交易审计显示门禁删除了 `+2.1504R` 的 DOGE 赢家和 `-1.1281R` 的 ORDI
  亏损；两笔 LTC 赢家只延后一根成交，边际合计 `-1.0230R`。
- 纯阳线门禁已淘汰，不合并 Pine、主策略或运行路径。详细结果见
  `tradingview_velocity_v19_ema_long_bullish_acceptance_result_20260731.md`。

## 2026-08-01：HYPE 放量杀跌后低位拒绝与强阳收回评估

- 目标形态确认由三段组成：14:30 放量长阴扫出布林下轨、随后 5 根收盘拒绝接受
  更低价格、16:00 强阳放量收回 EMA12 与前四根高点。
- 目标棒收盘 57.76 仍低于 EMA576 59.13，且 EMA576 低于 12 根前的 59.20；
  因此它属于慢均线下方的逆势卖压衰竭反弹，而不是 EMA576 收复顺势多。
- 冻结版本未补造目标交易；43/60 回放新增家族 0 笔，候选与基线零成本、成本后
  和逐币结果完全一致。当前数据只支持 `partial_data_diagnostic`。
- 按预注册规则淘汰该定义，不并入 Pine 或主策略。规则、目标审计和原始报告见
  `VOLUME_SELL_CLIMAX_BASE_RECLAIM_LONG_15M_V1_EVALUATION_MANIFEST.md`。

## 2026-08-01：V20 锚区上破后扫高失败接受空单

- 新增 Research-only `volume_anchor_upthrust_failed_acceptance_short_15m_v1`：仅在
  放量上破后的第1～2根完成棒识别扫高后跌回冻结上沿，不改变 V19 晚确认分支。
- 冻结止损为突破棒与拒绝棒较高点上方1 tick，100%结构目标为突破时冻结锚区下沿；
  早期信号消费 pending，防止随后跌破下沿重复开仓。
- BTC 07-13 验收样本在 08:15 确认、08:30 最早成交；止损 `64398.1`、目标
  `63639.0`、信号收盘估算约 `2.13R`，未读取 08:30 之后 K 线决定信号。
- Pine 编译 0 错误；Rust 聚焦测试 5/5、V5～V20 源码身份测试 1/1 通过；当前
  Pine 与冻结 V20 指纹同为 `a755168d`，`signals.rs` 1944 行未越过2000行硬上限。
- 已重新打开已保存脚本，使后台 Pine 模型与可见 Monaco 模型归一；V20 另存为
  `15m Velocity All Symbol - Research Chart V20` 并加载到 BTC 15m 图表，V19 保存脚本
  仍保留。07-13 图中可见 `volume_anchor_upthrust_failed_acceptance_short_15m_v1`，
  随后在冻结下沿 `63639.0` 以 `TP_STRUCT` 退出，完成可见图表验收。
- 当前 BTC 可见窗口8笔、5胜3负、PF `3.406`、净利润 `3767.3 USDT`；由于佣金为0、
  样本仅8笔，该结果只证明加载、信号时序和结构目标生效，不作为多币种盈利或晋级结论。
- 已在结果前冻结同一 current-live Top60、60 天预热、`vol_ccy` 与每边
  `5 bps fee + 3 bps slippage`；本地 43/60 完整成员完成 V19/V20 同范围回放。
- 新家族 217 笔压缩为 190 个 60 分钟事件；零成本 `+27.5745R/PF_R 1.1863`，
  成本后为 `-10.0412R/-0.0463R每笔/PF_R 0.9424`，21 个币种正、21 个币种负。
- 全策略成本后 V19 为 `-51.6540R/PF_R 0.9663`，V20 为
  `-59.6819R/PF_R 0.9649`，paired delta `-8.0279R`；因此 V20 不晋级。
- 交易路径审计还发现 216 笔 V20 独占交易、7 笔 V19 消失交易和 6 笔退出路径变化；
  早期空单可反手旧多仓，同棒 RSI 顶背离也可能被新结构接管，需在下一独立版本拆开。
- 结果见
  `docs/backtest_reports/tradingview_velocity_v20_anchor_upthrust_multisymbol_result_20260801.md`；
  未修改 Paper、ReadOnly、Live、生产注册或真实交易执行。

## 2026-08-01：V21 扫高失败右侧确认多币种验证

- V20 失败解剖显示 217 笔中有 43 笔入场后 0～1 根止损、75 笔在 4 根内止损；
  RSI、量比和初始风险阈值无稳定单调性，问题集中在第一次扫高回落即反手的时序。
- 预注册并实现独立 Rust Research V21：V20 拒绝棒只建立 setup；紧邻下一根完成棒
  必须收盘跌破拒绝低点且未触及冻结止损，才允许再下一根开盘成交。其余入场条件、
  冻结止损、结构目标和最低 1.5R 均未改变。
- 同一 current-live Top60、43/60 完整成员、60 天预热、`vol_ccy` 与双边各 8 bps
  下，V21 家族为 52 笔、22 胜 30 负、胜率 `42.31%`，成本后
  `+13.2731R / +0.2553R每笔 / PF_R 1.3832`。
- V20 同族为 217 笔、胜率 `31.80%`、成本后 `-10.0412R / PF_R 0.9424`；
  右侧确认减少 40 笔 0～1 根止损，并把成本后净 R 提高 `23.31R`。
- V21 覆盖 27 个币种、45 个 60 分钟事件簇、13 个月份；移除最大 3 笔后仍为
  `+2.69R`。BTC 仅 1 笔亏损，ETH 不在冻结币种池，不能外推为两者均有效。
- 全策略成本后从 `-59.6819R / PF_R 0.9649` 改善为
  `-34.8837R / PF_R 0.9777`，仍未转正；因此 V21 只保留为 Research，不进入
  Pine、主策略、Paper、ReadOnly、Live 或生产执行。
- 预注册与结果分别见
  `docs/backtest_reports/tradingview_velocity_v21_right_side_confirmation_evaluation_manifest_20260801.md`
  和 `docs/backtest_reports/tradingview_velocity_v21_right_side_confirmation_result_20260801.md`。

## 2026-08-02：V22 确认棒结构目标消耗验证

- 已按结果前冻结口径实现确认棒目标消耗审计，并新增 V22A 25%、V22B 33%、V22C
  50% 三个独立 Rust Research 版本；V21 旧行为保持不变。
- 新 V21 审计基线与旧 V21 在忽略新增审计字段后逐笔哈希一致，排除代码漂移。
- 同一 current-live Top60、43/60 完整成员、60 天预热、`vol_ccy` 与双边各 8 bps
  下，52 笔确认棒消耗均值 `9.78%`、中位数 `8.64%`、P90 `16.85%`、最大
  `19.51%`。
- 25% / 33% / 50% 三档均未触发；V21/V22A/V22B/V22C 的完整逐笔 SHA-256
  同为 `23d9a85d12c13f461f6df02f047e44e25ee8ea210df8268635a3035d3508dd16`。
- 四组家族结果均为 52 笔、22 胜 30 负、成本后 `+13.2731R`、平均
  `+0.2553R`、`PF_R 1.3832`；全策略均为 `-34.8837R/PF_R 0.9777`。
- 预注册晋级条件未满足，V22A/B/C 判定为冗余门槛并拒绝晋级；没有修改 TradingView
  主 Pine、Paper、ReadOnly、Live、数据库或生产交易路径。
- 完整预注册、分桶、集中度与版本判定见
  `docs/backtest_reports/tradingview_velocity_v22_target_consumption_evaluation_manifest_20260802.md`
  和 `docs/backtest_reports/tradingview_velocity_v22_target_consumption_result_20260802.md`。

## 2026-08-02：策略研究工作流分级与 Top60 运行时加速

- 新增 `docs/STRATEGY_RESEARCH_WORKFLOW.md`，把日常研究固定为 L0～L3 四级；截图和
  单笔默认只分析，L1 先做无标签覆盖扫描，L2 缺失成员跳过，只有正向结果才进入 L3。
- `tradingview_velocity_top60` 已增加阶段计时、严格身份 baseline 缓存、候选交易账本和
  单进程多变体批次；单变体顶层 JSON 保持兼容，多变体才使用批次外壳。
- 候选账本将 `time_visible_features`、`frozen_risk` 与 `outcome` 分栏，并保存 60 分钟
  事件簇及阻塞事件；本地 V20 baseline 验收生成 2,520 个候选和 8,608 个阻塞事件。
- 最终 release 首次基线总耗时 `83,242ms`：数据加载 `3,182ms`、指纹 `362ms`、
  回放 `79,619ms`、账本 `4ms`、分析 `12ms`；确认真正热区是完整 broker 回放。
- 同身份第二次运行 `3,773ms`，`baseline_cache_hit=true` 且回放、账本、分析均为
  `0ms`；删除运行诊断字段后与首次基线 JSON 完全一致。
- baseline 与 V21 右侧确认同进程运行 `80,965ms`：一次数据加载、一个数据指纹、
  baseline 命中缓存，V21 独立完整回放 `77,214ms`；两份账本都含冻结退出政策。
- focused tests `23/23`、release build、Rust 格式和 2000 行硬上限通过；当前 main
  `1,210` 行仅超过 1,000 行目标、未超过 2,000 行硬上限。
- 本轮只读本地 `quant_core`，43/60 可用成员仅用于运行时验收；未补造 K 线、未修改
  Pine、Paper、ReadOnly、Live、数据库或生产执行路径。

## 2026-08-02：L0～L3 已升级为项目硬规则

- umbrella 根 `AGENTS.md` 已新增 L0～L3 强制流程；后续新形态、阈值、过滤、退出和策略优化
  默认从 L0/L1 开始，只有逐级正向证据才能进入 L3。
- `strategy-iteration` 技能已重写并与硬规则归一：L0 不改代码，L1 先做无标签覆盖与候选账本，
  L2 使用本地可用成员且不自动补数据，L3 才执行 OOS、集中度和 Pine/Rust parity。
- 旧技能中探索阶段直接写原型、自动同步缺失 K 线、立即完整回测和为中间阈值分散建文档的
  冲突入口已删除。
- 本次只修改协作规则、技能和任务记录；未触碰策略实现、TradingView、数据库或生产路径。

## 2026-08-02：15 分钟动量布林长影回归 L1 已停止

- 目标策略已从相邻的 TradingView Velocity 口径纠正为
  `market_momentum_exhaustion_reversal_15m_v2` 方向长影 cohort；错误的临时 Velocity
  探针和产物已删除。
- 新增固定参数的只读扫描入口：量比、周 P90、96 根 8%、60% 长影和 V2 风险身份保持不变，
  只计算信号 K 是否触及总体标准差 `Bollinger(20, 2.5)` 外轨。
- 本地 current-live Top60 返回 60 个成员，44 个具备完整预热与评价窗口；机器产物数据指纹为
  `0c3d1e6ce33187fbc0fd528486d837574fe176b73a748b1f44dedd3c14c328f5`。
- 896 个来源方向长影 setup 中 673 个触轨，保留 `75.1116%`；做多 340、做空 333，
  聚类后 369 个有效市场事件，覆盖 44 个币种与 13 个月。
- 唯一失败门槛为预注册保留比例 `10%~60%`；按 L1 停止条件没有读取任何成交后结果，也没有
  实现趋势过滤、中轨过滤、分批退出、Pine、Paper、ReadOnly、Live 或生产执行。
- focused tests 5/5 与格式检查通过；项目声明的 `scripts/dev/check_code_file_line_limit.sh`
  当前仓库不存在，已直接核对触碰文件行数：父模块 1,999 行，新扫描模块 822 行，新二进制 38 行。

## 2026-08-02：15 分钟动量布林长影“趋势刚形成”L1 已停止

- 后续用户明确指出趋势应通过 EMA 均线排列确定；本批次使用 96 根净移动年龄的前提错误，最终
  状态修正为 `rejected_definition_mismatch`。错误 Research 模块和二进制已删除，下列统计仅作审计。
- 用户已确认基础形态为长上影且信号 K 高点触上轨做空、长下影且低点触下轨做多；此前
  `0.5R` 的真实含义是中轨回归时平掉原始仓位 50%，机器合同改用
  `middle_band_partial_close_fraction=0.5` 避免与风险倍数 R 混淆。
- 新批次只研究来源 96 根净移动达到正负 8% 后的连续趋势年龄，预注册拒绝前 4/8/12 根；
  量比、周 P90、60% 方向长影、Bollinger(20,2.5) 触轨和数据身份均保持不变。
- 同一 44/60 完整成员与 `0c3d1e6ce33187fbc0fd528486d837574fe176b73a748b1f44dedd3c14c328f5`
  指纹下，基础触轨账本仍为 673 个 setup。
- 前 4 根拒绝 327 个（48.5884%，多/空 176/151，206 个事件）；前 8 根拒绝 415 个
  （61.6642%，214/201，249 个事件）；前 12 根拒绝 458 个（68.0535%，238/220，
  261 个事件）。
- 三个变体的币种、月份、多空与事件门槛均通过，但拒绝比例都超出预注册 5%～25%，因此
  统一判定 `stop`，未读取成交、未来 K 线、MFE、MAE、退出、R、胜负或 PnL。
- 年龄恰为 1 的无标签 setup 有 164 个（24.3685%），更贴近“刚形成”字面定义，但未在本批次
  预注册；如继续必须另立单变量批次并完成目标图表审计，不能事后改写当前结果。
- focused tests 9/9、新二进制 `cargo check`、`cargo fmt --check` 均通过；新模块 542 行、
  新二进制 36 行，既有父入口仍为 1,999 行。未修改 Pine、Paper、ReadOnly、Live、数据库或生产路径。

## 2026-08-02：15 分钟动量布林长影 EMA 趋势新形成 L1 已停止

- 用户最终确认不需要连续三根排列和 EMA696 斜率，因此本节批次状态已改为
  `rejected_definition_mismatch`；以下 5 个样本只保留审计，不得用于晋级或运行。
- 已按用户纠正复用 Core 现有长期趋势口径：连续三根严格
  `EMA12 > EMA144 > EMA169 > EMA696` 为多头、完全反向为空头，并要求最近四个 EMA696
  逐根同向；当前确认成立而前一根未成立才是“刚形成”。
- 新扫描器只读取 673 个 Bollinger 长影基础 setup 的信号时 EMA；44/60 完整成员均具备
  699 根慢线输入预热，扩展数据指纹为
  `45496fd9db8998652e4e65c813d031b416abac99e73b9ef30cb053bfb960c2c5`。
- 673 个 setup 全部 EMA-ready，487 个处于对向确认趋势；其中年龄 1 为 5 个、2～4 为 8 个、
  5～12 为 27 个、至少 13 根为 447 个，另有 186 个未确认对向趋势。
- 精确首次确认过滤只影响 5 个 setup（0.7429%），做多/做空 1/4，覆盖 5 个币种、4 个月、
  5 个事件；命中数、影响比例和双向覆盖三项预注册门槛失败，结论为 `stop`。
- 仅作下一轮无标签定义参考：确认年龄前 4/8/12 根分别覆盖 13/24/40 个 setup；`age<=8`
  是第一个至少覆盖 5 个多单的窗口，但未预注册且未读取收益，不能回填到当前批次。
- focused tests 10/10、新二进制 `cargo check`、`cargo fmt --check` 均通过；EMA 研究模块 684 行、
  新二进制 36 行，父入口仍为 1,999 行。未修改 Pine、Paper、ReadOnly、Live、数据库或生产路径。

## 2026-08-02：15 分钟动量布林长影单根 EMA 排列刚形成 L1 已停止

- 用户后续明确最终只使用 EMA12/144/576，因此本节 `EMA12/144/169/696` 批次状态已改为
  `rejected_definition_mismatch`；以下 1 个命中只保留审计，不得作为当前策略证据。
- 最终口径已冻结为当前信号 K 单根严格 `EMA12/144/169/696` 排列，上一根只用于判断
  `false -> true`；代码与机器身份明确排除连续三根确认和 EMA696 斜率。
- 独立 Research-only 扫描器只读取信号 K 与上一根已完成 K 的 EMA；基础形态、量比、
  Bollinger(20,2.5)、止损和已登记退出语义均未改变。
- focused tests 11/11、独立二进制 `cargo check` 与格式检查通过；测试证明单根排列即可成立、
  EMA696 反向移动不阻塞、上一根已排列不重复拒绝、缺值失败关闭。
- 同一 current-live Top60 返回 60 个成员，44 个完整成员进入扫描；16 个不完整成员跳过，
  673 个基础 setup 全部具备当前与上一根 EMA 状态。
- 494 个 setup 当前处于对向排列，其中 493 个为 `true -> true`，179 个为 `false -> false`，
  只有 1 个为 `false -> true`，影响比例 `0.1486%`；没有 `true -> false`。
- 唯一命中是做空方向，只有 1 个币种、1 个月和 1 个事件；除 EMA-ready 外所有预注册门槛
  均失败，L1 状态为 `stop`，未读取后续行情或任何收益字段。
- 机器报告 SHA-256 为 `ece2e11bdd8982029fbf727b090d4647232e736097ded274731baf7331249159`；
  未修改 Pine、Paper、ReadOnly、Live、数据库或生产执行。
- 如继续，下一独立批次应只冻结“信号 K 收盘接近中轨”的距离定义和阈值，不能从本轮唯一
  命中样本的结果反向选参。

## 2026-08-02：15 分钟动量布林长影 EMA12/144/576 单根排列刚形成 L1 已停止

- 已复核并固定信号来源：`market_momentum_exhaustion_reversal_15m_v2` 先产生既有方向候选；
  扫描器只接受 `directional_wick_limit_12_candles`，再检查对应 Bollinger(20,2.5) 外轨。
  EMA 只能否决，不能产生空头、多头或改写既有方向。
- 既有 V2 空头仍要求过滤量比至少 2.5、当前 `vol_ccy` 达到此前 672 根 P90、此前 96 根
  净移动至少 +8%、实体占比大于 10%、上影至少 60% 且严格长于下影，并以高点限价 12 根；
  多头完全镜像。本轮没有用 EMA 替代这些条件。
- 最终 EMA 口径只使用当前信号 K 的 EMA12/144/576 严格排列，上一根仅判断 `false -> true`；
  EMA169、EMA696、连续确认、斜率和均线距离全部排除。EMA576 使用 576 根 SMA 种子递推。
- 同一 current-live Top60 返回 60 个成员，44 个完整成员进入扫描，16 个缺口成员跳过；673 个
  基础触轨 setup 均可判定当前与上一根 EMA 状态。
- 514 个 setup 当前处于对向排列，其中 512 个上一根已经排列，159 个始终未排列，只有 2 个
  在信号 K 首次排列，影响 `0.2972%`；两者均为做空，覆盖 2 个币种、2 个月和 2 个事件。
- 命中数、影响比例、事件、币种、月份和双向覆盖均未通过预注册门槛，且 2 个命中触发 L1
  立即停止条件；没有读取后续 K 线、MFE、MAE、退出、R、胜负或 PnL。
- focused tests 14/14 与 `cargo fmt --check` 通过；新增版本分派测试证明固定调用既有 V2，机器
  身份记录完整来源链。报告 SHA-256 为
  `05de0fa466be6b40d8a7c1cd2b6ce6897eeb4636b62932f45729c740603a08d0`。
- 中轨回归平原始仓位 50%、余仓止损移到实际开仓价、对侧外轨或 +5R 完全退出的合同只登记，
  未执行回放；未修改 Pine、Paper、ReadOnly、Live、数据库或生产执行路径。

## 2026-08-02：V23 最近有效横盘首次突破失败接受 L1 已停止

- 已把用户纠正后的锚点语义实现为独立 V23 Research 版本：只允许最近已完成的稳定横盘区，
  当前放量棒必须是该上沿的首次完成收盘突破；冻结后仅观察第 1、2 根扫高回落。
- V23 使用独立状态，不会让旧横盘下沿假突破事件泄漏进新家族；V20、V21、V22 的版本身份
  与行为保持不变，未修改 TradingView 主 Pine 或任何运行态入口。
- 真实 SHIB 15m 回归证明：旧 V20 在 2026-07-06 06:30 形成候选，而 V23 因 06:15 之前
  已有完成棒突破旧横盘上沿而拒绝重新锚定，目标误报不再触发。
- 同一冻结 Top60 身份与 13 个月窗口中，43/60 成员可用；无标签候选由 V20 的 217 个变为
  V23 的 7 个，分布于 7 个币、5 个 UTC 月份和 7 个独立事件，没有保留原 V20 候选。
- L1 只读取信号时可见字段和冻结结构，没有读取任何 outcome；因覆盖过低，结论为
  `stop_at_l1_keep_research_only`，不进入 L2，不创建 Pine，不接 Paper、ReadOnly、Live 或生产。
- 策略库回归 148/148、Top60 入口 23/23、目标 Rust 格式、JSON 解析、尾随空白和 2,000 行
  硬上限均通过；现有编译警告与本次任务无关。

## 2026-08-02：V24 跌回横盘上沿但不要求再创新高已停止在 L2

- 用户指出“确认棒必须再次超过突破棒高点”过严；已将其作为唯一变量新增 V24，未覆盖冻结
  V23。V24 只要求突破后第 1/2 根满足原失败接受质量门槛并收盘跌回横盘上沿。
- 横盘定义本轮保持不变：最近 48 根内由近到远滑动连续 8 根，宽度实际为
  `(high-low)/low <= 3%`，上下沿各两组触碰，前后半段漂移受限且当前棒必须首次收盘突破。
- 新边界测试证明同一根未超过突破高点的回落棒被 V23 拒绝、被 V24 接受；SHIB 趋势延续
  反例在两个版本中都不建立横盘 pending。
- L1 同身份覆盖由 7 增至 14 个候选，原候选全部保留并新增 7 个；V24 分布于 12 币、7 个月
  和 14 个事件，预注册覆盖门禁通过后才进入 L2。
- V24 14 笔为 4 胜 10 负，零成本 `-3.1140R`，成本后 `-8.1333R`、平均
  `-0.5809R`、`PF_R 0.4156`；新增 7 笔为 2 胜 5 负、成本后 `-4.2096R`。
- 新增交易分散在 7 个独立事件但自身仍为负，故不是集中度或纯成本问题；结论为
  `stop_at_l2_keep_research_only`，未创建 Pine，未接入任何运行态或生产路径。
- 策略库 149/149、Top60 入口 23/23、格式、JSON、尾随空白和 2,000 行硬上限检查通过。
- 已从本地 `quant_core` 逐笔提取 14 个信号前后确认 15m K 线并生成交易路径图；10 个亏家
  全部在入场后 5 根内打冻结止损，其中 4 个在同根或下一根、5 个在两根内。
- V23 保留组与 V24 新增组均为 7 笔、2 胜 5 负，证明删除“再次创新高”只扩大覆盖，没有造成
  两组胜率差异；核心问题是单根跌回上沿后经常立即恢复上涨。
- 14 笔初始风险仅为价格的 0.130%～0.932%，成本平均侵蚀 `0.359R/笔`；毛保本胜率约
  36.7%，压力成本后约 49.0%，均高于实际 28.6%。

## 2026-08-02：15 分钟动量布林长影中轨过滤与首次减半已停止在 L2

- “信号 K 收盘接近中轨”已作为独立 L1 完成：主定义为绝对收盘到中轨距离不超过 0.25 个
  半带宽，只命中 8/673（`1.1887%`）；0.10 命中 3 个、0.50 命中 56 个，但没有在看到
  覆盖后把主阈值改成 0.50。主定义覆盖不足，未打开任何 outcome。
- 用户所说 `0.5R` 已按真实含义实现为首次因果回归中轨时平掉原始数量 50%；余仓仍使用
  V2 的实际成交价反方向 `1.5*ATR14[p]` 止损和量比分档 `2.7/3.6/4.5 ATR` 目标。
- 入口完整复用既有 Momentum V2 多空：量比、周 P90、前 96 根正负 8%、实体和 60% 方向
  长影先定方向，再检查对应 Bollinger(20,2.5) 外轨；EMA 没有产生或替代空头定义。
- L1 几何覆盖为 673/673，做多 340、做空 333，覆盖 44 个币、13 个月和 369 个事件；通过后
  才预注册并执行 L2 成交与退出回放。
- 新增独立 Research-only L2 模块和 CLI；5 个 focused tests 全部通过，覆盖止损优先、因果
  中轨、只减半一次、中轨与目标同棒，以及旧挂单盘中成交先于收盘新 setup 替换。
- L2 共有 283 个限价成交，持仓冲突后 279 笔全部完成；中轨减半触发 110 笔，做多/做空
  47/63，覆盖 35 个币、13 个月和 89 个有效事件。
- 成本后基线为 `-52.2604R / -0.1873R每笔 / PF 0.7644`；中轨减半为
  `-48.1010R / -0.1724R每笔 / PF 0.7407`。虽然总净 R 改善 `+4.1594R`，PF 下降且
  做多方向恶化 `-0.6737R`，失败两项查看结果前冻结的门禁。
- 机器结果 SHA-256 为
  `ba66886f662188c57af1b4867599d78dcf1c427087d2b5e57f242101a38d1563`；账本数量、候选
  唯一性、共享最终路径及 R 汇总一致性已复核。
- 结论为 `stop_at_l2_keep_research_only`：不再用余仓保本或对侧外轨/`+5R` 事后挽救；
  未修改 Pine、Paper、ReadOnly、Live、数据库、策略注册、调度或生产交易路径。

## 2026-08-02：V25 横盘方向效率已停止在 L2

- 用户指出 ICP 2026-06-29 09:00 前是方向性修复而非横盘；当前 V24 实际选择 05:45～07:30
  的 8 根区间，孤立上下影让极值边界检查通过，但收盘方向效率为 `0.52`。
- 已在打开结果前冻结方向效率为唯一变量，主阈值 `0.35`，邻域 `0.30/0.40`；新增 V25A/B/C
  独立 Research 身份，V24 及其余信号、风险、成交和退出合同保持不变。
- 同一 current-live Top60 数据指纹、43/60 完整成员下，无标签候选由 V24 的 14 个变为
  V25A/V25B/V25C 的 9/10/11 个；V25B 过滤 4 个，覆盖 8 币、6 月和 10 个事件。
- ICP 在三档全部拒绝，SHIB 趋势延续反例继续拒绝；邻域候选集合单调扩张且没有新增异常身份。
- L1 通过后只打开预注册 V25B：10 笔 4 胜 6 负，零成本 `+0.886R / PF 1.148`，成本后
  `-3.259R / -0.326R每笔 / PF 0.640`；平均成本拖累约 `0.414R/笔`。
- V25B 删除的 BLUR、ADA、SSV、ICP 四笔后验均为止损，但该结果没有参与阈值选择；由于
  成本后边际仍为负，结论为 `stop_at_l2_keep_research_only`。
- parity library 151/151、Top60 CLI 3/3、Rust 格式和 2,000 行硬上限通过；未创建 Pine，
  未接 Paper、ReadOnly、Live、数据库或生产执行。
- 三份研究产物位于 `docs/backtest_reports/tradingview_velocity_v25_horizontal_direction_efficiency_*_20260802.*`。

## 2026-08-02：15 分钟动量布林长影近期 EMA12 领先双慢线已停止在 L1

- 已将旧版“严格三线必须在信号 K 恰好 `false -> true`”改成独立 Research 版本：做空检查
  `EMA12>EMA144 && EMA12>EMA576`，做多镜像，EMA144/576 相对顺序不参与判断。
- 对向状态连续年龄从首次成立的 1 开始，中断归零；预注册 48 小时即 `1..=192` 根为近期，
  该时限来源于基础策略冻结的最长持仓时间，不读取斜率或连续三根确认。
- Momentum V2 仍先按量比、周 P90、96 根净移动、实体和 60% 方向长影产生候选，随后检查
  Bollinger(20,2.5) 外轨；EMA 只否决，未重新定义空头或多头。
- AGLD、YFI、NMR、ORDI、SATS 五个用户样本的机器年龄为 3/62/158/53/29，与预注册值
  完全一致且全部被拒绝，说明旧漏判已修复。
- 同一 44/60 完整成员与 673 个基础 setup 全部 EMA-ready；640 个当前处于对向 EMA12 领先，
  其中 537 个年龄不超过 192 根，过滤占比 `79.7920%`。
- 其余覆盖门槛均通过，但影响比例超过 45% 上限；该定义会否决绝大多数逆势候选，故按 L1
  状态 `stop`，没有读取成交后行情、R、胜负或 PnL，也没有进入 L2。
- 新增模块 6 个边界测试通过；机器报告 SHA-256 为
  `93711524b2bd33d6dcf4ab93759807d8cbfc5ad7857dcb9c07d97c81cf5d447d`。未修改 Pine、
  Paper、ReadOnly、Live、数据库、策略注册或生产路径。

## 2026-08-02：15 分钟动量布林长影近期 EMA12 领先 96 根已停止在 L1

- 已新增独立 `age<=96` 规则身份和只读 CLI；旧 192 根版本继续保留。共享实现只接受两个
  冻结版本，不开放任意阈值调参。
- 唯一变量是连续年龄上限由 192 根改成 96 根；Momentum V2 方向、Bollinger(20,2.5)、
  EMA12 对 EMA144/576 的关系、量比、影线、限价、止损和退出合同全部不变。
- 同一 44/60 完整成员、673 个基础 setup 中拒绝数由 537 降到 423，影响比例由 79.7920%
  降到 62.8529%；仍超过预注册 45% 上限。
- 96 根版本做多/做空拒绝 204/219，覆盖 43 币、13 月和 245 个有效事件；覆盖下降并未解决
  “过滤绝大多数逆势候选”的核心问题。
- 目标样本年龄继续精确匹配 3/62/158/53/29；AGLD、YFI、ORDI、SATS 被拒绝，NMR 因
  158>96 被放行，原 5/5 目标门禁失败。
- 结论为 `stop`；机器报告确认未读取 outcome，不进入 L2，不创建 Pine，不接 Paper、
  ReadOnly、Live、数据库或生产执行。报告 SHA-256 为
  `63f9c36660b18386660bfa63443f1801a2b40b5ba5d298631e8d8aac9f994e8e`。

## 2026-08-02：15 分钟动量布林长影首次重测收回确认已停止在 L1

- 新增独立 Research-only 候选 `market_momentum_bollinger_wick_reentry_confirmation_15m_v1`；
  来源 setup 不再直接成交，首次重测收回当根 `BB(20,2.5)` 后才在下一根允许激活。
- 673 个来源 setup 中 283 个首次重测，197 个确认、86 个带外失效；确认保留率 69.6113%，
  做多/做空确认 81/116，覆盖 43 币、13 月和 148 个事件。
- 十个固定失败目标全部精确复现，但只有 YFI、EIGEN、OP 被拒绝，低于预注册 5/10 门槛；
  动态带扩张让 NMR、SATS、WIF、BABY 在越过来源高点后仍被视为带内。
- 唯一失败门禁为 `target_rejected_at_least_5_of_10=false`，结论固定为 `stop`；没有读取
  确认后的入场价格、R、胜负或 PnL，也没有进入 L2。
- 定向测试 5/5、2,000 行硬上限和机器 JSON 结构复核通过；机器报告 SHA-256 为
  `d087732b342039bb5dde635da64ba8c02639ac741591c35ffbc3a35cdd5a5e51`。未创建 Pine，
  未接入 Paper、ReadOnly、Live、数据库写入或生产执行。

## 2026-08-02：V26 最长有效父横盘已停止在 L2

- 已新增独立 V26 Research 身份，V25B 原行为继续保留。新锚点只读取突破前完成 K：8 根为
  成形下限、96 根为 24 小时上限，使用 P90/P10 稳健边界并优先选择最长有效父横盘。
- 选择父横盘发生在检查突破收盘之前；若最长父横盘尚未突破，不允许退化选择更短、更低的
  微型区间。确认棒继续只需第 1/2 根跌回冻结上沿，不恢复突破棒高点复扫门槛。
- ALGO 2026-07-15 指定误判已修复：V26 选择 87 根父横盘、上沿 `0.08520`，13:45 收盘
  `0.08512` 未突破，因此 14:00 不建立空单；ICP 和 SHIB 指定反例继续拒绝。
- 同一 43/60 完整成员和同一数据指纹下，V26 产生 203 个候选，覆盖 41 币、13 月和 167 个
  事件；相对 V25B 保留 5、删除 5、新增 198，说明新定义改变了候选宇宙而非单纯过滤。
- L1 正式目标与覆盖门禁通过后打开 L2：203 笔 58 胜 145 负，零成本 `-20.687R`、
  `PF 0.857`；成本后 `-64.356R`、每笔 `-0.317R`、`PF 0.634`。144 笔止损、58 笔止盈、
  1 笔反向开盘退出，所有父横盘长度档均为负。
- parity library 153/153、Top60 CLI 23/23、release 构建和格式检查通过；候选账本升级为 v2，
  保存父横盘起止时间、长度、上下沿、方向效率与突破 tick。机器结果 SHA-256 为
  `3b6586f34dadec2bd30eb9c3530ccb578eda7615c955c2af46f2dbd8d9d74a09`。
- 结论为 `stop_at_l2_keep_research_only`：形态归属已纠正，但“第 1/2 根跌回上沿即做空”在
  成本前已经负期望；未创建 Pine，未接 Paper、ReadOnly、Live、数据库或生产路径。

## 2026-08-02：V27 突破实体完全否定已停止在 L2

- 已新增独立 V27 Research 身份，唯一变化是第 1/2 根失败确认收盘必须不高于冻结突破棒
  开盘价；父横盘、放量、确认窗口、收盘位置、量能、1.5R、止损、目标和退出均沿用 V26。
- L1 只读取信号时字段：保留 V26 的 65/203 个候选（32.02%），覆盖 36 币、13 月、62 个
  事件簇；没有新增身份，三个指定反例继续拒绝，所有 V27 证据字段完整。
- 65 个保留身份与 V26 的信号、冻结风险和 outcome 漂移均为 0，证明实现只过滤浅回踩，
  没有隐式改变下一根成交或退出路径。
- L2 65 笔为 24 胜 41 负；零成本改善为 `+11.298R`、每笔 `+0.174R`、`PF 1.276`，但
  成本后降为 `-2.107R`、每笔 `-0.032R`、`PF 0.957`，触发预注册停止条件。
- 成本后赢家分布在 19 币、10 月和 23 个事件簇，但只有 5/13 月为正；43/60 部分成员结果
  不能升级为正式 Top60 或全市场结论。
- parity library 154/154、Top60 CLI 23/23、release 构建通过；候选账本 v3 保存突破开盘、
  确认收盘和实体否定深度。机器结果 SHA-256 为
  `bf9752dfd789274d4c80b4a120c836ceef264dbc60d2d9499d606bb60ed732ef`。
- 结论为 `stop_at_l2_keep_research_only`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库
  写入或生产路径。若继续，必须另开单变量版本研究父横盘高度归一化深度，不能事后挑 tick。

## 2026-08-02：V28 父横盘归一化实体否定 10% 已停止在 L2

- 已新增独立 V28 Research 身份，唯一变化是 V27 首个有效确认相对突破棒开盘的跌回深度，
  必须至少达到冻结父横盘高度的 10%；父横盘、确认、成交、止损、目标和退出全部保持不变。
- L1 在 outcome 前冻结阈值并只读取信号字段：保留 V27 的 38/65（58.46%），覆盖 24 币、
  9 个有信号月份和 36 个事件簇；三个指定反例继续拒绝，归一化证据完整。
- 初次回放发现 TURBO 第 1 根已满足 V27 但深度不足时继续等待第 2 根会制造新身份；在读取
  V28 outcome 前修正为立即消费 setup。最终新增身份、信号/风险/outcome 漂移均为 0。
- L2 38 笔为 12 胜 26 负；零成本 `-0.296R`、每笔 `-0.008R`、`PF 0.989`，成本后
  `-8.168R`、每笔 `-0.215R`、`PF 0.734`，成本共拖累 `7.872R`。
- 10% 门禁保留的队列成本后亏 `-8.168R`，删除的 27 笔反而赚 `+6.061R`；胜率由 V27 的
  36.92% 降至 31.58%，只有 2/9 个有信号月份为正，属于反向选择而非仅仅过严。
- parity library 155/155、Top60 CLI 23/23、release 构建通过；机器结果 SHA-256 为
  `a3f32962c72afaf0a485dfd9e0405756021d80ef09c688576ec013d96ff1edfc`。
- 结论为 `stop_at_l2_keep_research_only`；不再搜索 5%、15% 或 20%，未创建 Pine，未接
  Paper、ReadOnly、Live、数据库写入或生产路径。

## 2026-08-02：V29 父横盘浅突破 10% 已停止在 L2

- 已新增独立 V29 Research 身份，唯一变化是 V27 首次突破棒完成收盘超出冻结父横盘上沿的
  幅度不得超过父横盘高度 10%；没有叠加 V28 深度、RSI、EMA、止损或目标变化。
- L1 在查看分布和 outcome 前冻结 10%；无标签保留 27/65（41.54%），覆盖 21 币、9 个
  有信号月份和 26 个事件簇，三个指定反例继续拒绝。
- 初次实现因提前拒绝强突破 pending 释放状态，产生 1 个 V27 不存在的 ALGO 身份；在读取
  V29 outcome 前修正为推进同一 V27 pending 并在基线发信号位置过滤，最终新增/漂移为 0。
- L2 27 笔为 12 胜 15 负；零成本 `+11.666R / +0.432R每笔 / PF 1.778`，标准成本后
  `+5.128R / +0.190R每笔 / PF 1.277`，是 V26～V29 首个成本后正向版本。
- V29 保留队列成本后 `+5.128R`，删除的 38 笔为 `-7.235R`；成本后 6/9 个有信号月份为正，
  12 笔赢家分布在 11 币、6 月和 11 个事件簇。
- 移除最大一笔后仍 `+2.089R`，移除最大两笔后转为 `-0.565R`；当前净边际仍依赖头部两笔，
  且 EV/PF 距离职业门槛明显，故集中度门禁失败、不进入 L3。
- parity library 155/155、Top60 CLI 23/23、release 构建通过；机器结果 SHA-256 为
  `1adf5ba6253af4c253a2a4359a8d3babe748e07f3a106a60f73b4c9757b33c74`。
- 结论为 `stopped_at_l2_keep_research_only`；冻结 V29 等待新 forward OOS，未创建 Pine，
  未接 forward shadow、Paper、ReadOnly、Live、数据库写入或生产路径。

## 2026-08-02：15 分钟动量布林长影来源极值收回确认停止在 L2

- 新增独立 Research-only 候选 `market_momentum_bollinger_wick_source_extreme_reclaim_15m_v1`；
  唯一确认边界是首次重测收盘严格回到来源 setup 方向极值以内，不与动态布林外轨叠加。
- L1 仅转换冻结无标签账本：283 个首次重测中确认 143、拒绝 140，确认保留率 50.5300%；
  做多/做空确认 56/87，覆盖 41 币、13 月、114 个事件。
- 十个固定近期失败 setup 全部存在，YFI、NMR、SATS、WIF、BABY 被拒绝，达到 5/10 门禁；
  L1 机器报告 SHA-256 为 `ab22aa5c485e660cb0dd32baf9d0327eb6927e8a2bb34e089972a0a949546925`。
- L1 通过后才打开 L2，配对比较同一确认 cohort 的来源极值被动成交与确认后下一根开盘；
  两侧共享 setup ATR14、1.5 ATR 止损、2.7/3.6/4.5 ATR 目标、8 bps 单边成本和冲突集合。
- 143 个 pair 全部成功解析，共同同币种锁过滤 4 个；139 个完整 pair 覆盖 41 币、13 月、
  110 个事件，做多/做空 55/84，合同一致性校验通过。
- 条件配对基线成本后为 `+27.1246R / +0.1951R每笔 / PF 1.2897`；候选降为
  `-12.4691R / -0.0897R每笔 / PF 0.8834`，总净增量 `-39.5938R`，多空均恶化。
- 下一根开盘相对来源影线极值 137/139 笔为不利成交，平均牺牲 `0.4806R`；13 笔从基线
  止盈翻为候选止损，只有 2 笔反向改善，说明右侧追价直接消耗均值回归入场边际。
- L2 多项预注册绩效与集中度门禁失败，机器报告 SHA-256 为
  `0de5c628d416ca2b487dfe5553bdd67eaf712f3dbb36f2cfbd49815565d74a10`；状态固定为
  `stop_at_l2_keep_research_only`。
- L1 定向测试 5/5、L2 定向测试 4/4 通过，新增 Rust 文件均低于 1,000 行目标；未创建 Pine，
  未改 Paper、ReadOnly、Live、数据库、策略注册、调度或生产交易路径。

## 2026-08-02：EMA144/576 重扩张稳定成交额面板 V12 停止在 L2

- 用户三张图的 V6 形态保持不变：EMA144/576 历史资格永久独立保存，最新 0.75 ATR 重扩张
  武装回踩单，上一根已完成 EMA144/ATR14 形成 0.30 ATR 因果限价，跨 EMA576 再穿越仍存活。
- V11 的完整面板同 K 排名确认保留 41,564/54,837，但仅命中 1/3。只读核对证明两个 BTC
  样本本身为上涨 K、排名 1、`delta_rank=0`，漏判来自 CVX 使面板只有 43/44 可用。
- V12 Research-only 加载器允许当时稳定的至少 95% 可用面板，并要求相邻快照成员集合完全
  一致；成员进入或离开时跳过，不会制造伪排名变化。生产完整面板入口未改。
- V12 L1 候选 48,048，减少 12.3803%，多空 19,824/28,224，覆盖 44 币、13 月、3,917 个
  事件；实际面板 42/43/44 的候选分别为 1,123/5,361/41,564，用户目标 3/3。
- L2 48,048 个授权全部映射，同币种锁后 16,941 笔完整交易。毛收益 `+404.649R`、
  `EV +0.023886R / PF 1.083335`；8 bps/side 后 `-272.671R`、
  `EV -0.016095R / PF 0.946614`。
- 多头成本后 `EV -0.039634R / PF 0.876428`；空头 `EV +0.002157R / PF 1.007527`，不满足
  双方向正边际。移除头部两笔或最高事件后仍分别为 `-273.632R`、`-299.415R`。
- L1/L2 机器报告 SHA-256 分别为
  `201114b4ae1e519793f2988f000e00b5751c05fffc710ad0146e28340eb14dd1`、
  `843f6c50b9903688543ab53e476c3b464658141f7f74a6dbc2f9c107657154a3`。
- 排名测试 8/8、EMA 聚焦测试 38/38 通过；新增 Rust 文件低于 1,000 行。V12 固定为
  Research-only `stop`，没有 Pine、Paper、ReadOnly、Live、生产 preset、数据库写入或实盘变更。

## 2026-08-02：EMA144/576 稳定面板结构目标 V13 停止在 L2

- 新增独立 V13 Research-only 身份；唯一变化是把 V12 固定 `0.52R` 换成信号前 96 根
  已完成 K 内最近的已确认 2 左 2 右盈利侧摆动高/低，入场、止损、持仓与成本均不变。
- L1 无标签构造 48,019/48,048 个目标，覆盖 99.9396%；多空 19,808/28,211、44 币、
  13 月、3,918 个事件，用户图 3/3。目标 R 中位数 0.301026、P90 0.750370。
- L2 48,019 个授权全部解析，同币种锁后 18,337 笔完整交易；覆盖多空 7,662/10,675、
  44 币、13 月、4,026 个事件，无 forward 不完整，逐笔结构目标与成本合同一致。
- 毛结果为 `+162.4853R / EV +0.008861R / PF 1.033335`；8 bps/side 后降为
  `-570.6150R / EV -0.031118R / PF 0.888715`。
- 多头成本后 `EV -0.060354R / PF 0.806191`，空头 `EV -0.010134R / PF 0.960539`；
  移除最优两笔或最高事件后仍分别为 `-581.1554R`、`-601.9829R`。
- L1/L2 机器报告 SHA-256 分别为
  `d0cac4f5650d0f9df579081c4db62cedc8d6194288ab21aab5fa5663d7c38ec0`、
  `5ba2aae968e8ec52313e8d41d38ce3008008b01d22a7706aa6192cecb489e550`。
- EMA144/576 聚焦回归 42/42、成交额排名回归 8/8、Rust 格式检查和定向代码行数检查通过；
  V13 L1/L2 核心模块分别为 904/529 行，父 L2 模块为 981 行。
- V13 固定为 Research-only `stop`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库、
  策略注册、调度或生产执行。后续不得从本轮 outcome 事后搜索结构目标 R 阈值。

## 2026-08-02：rust_quant_alpha 首个 Vegas 冻结迁移闭环完成

- 完成范围严格限定为 `vegas / legacy-mysql-1 / ETH-USDT-SWAP / 4H` 同输入行为迁移；旧
  14,372 根混源 K 线只属于不可晋级 `LegacyParityDatasetSnapshotV1`。
- alpha Feature 与最终决策逐棒匹配 legacy：10,773 次决策、240 个多头、334 个空头、
  1,113 个过滤候选，最终指纹分别为 `b6957f...e392` 与 `5a71e0...4d22`。
- Research `simulation/legacy_vegas_v1` 隔离旧仓位、风险、退出、权益与报表口径；Strategy
  只输出 Vegas 决策和候选保护证据，Quant Backtest/Analytics 不再承载业务状态机或隐式默认值。
- 447 笔已平仓交易、894 条明细、447 个结算权益点逐字段/逐 bit 一致；最终资金
  `4421.393678523676`，胜率 `0.5302013422818792`，Sharpe `2.3763532924784836`，其余
  年化、总收益、最大回撤与波动率也逐 bit 一致。
- 全工作区 351 个测试通过、9 个明确依赖一次性数据库或真实样本的测试保持忽略；W2 scoped
  Clippy `-D warnings`、格式、依赖方向、Release Unit 和 `arch-check` 均通过。
- `wave-check --wave W2` 仍按设计失败于 31 个未完成 capability；这防止把首个冻结闭环误报为
  整个 W2、研究晋级或生产切换完成。后续应转向 canonical OKX/Binance Dataset 与正式能力闭环。

## 2026-08-03：EMA576 首次突破后 EMA144 回踩守稳 V1 停止在 L1

- 新增独立 Research-only 候选
  `market_momentum_ema576_first_breakout_ema144_pullback_hold_15m_v1`，没有覆盖旧 V6/V12/V13。
- 冻结形态为 144 根长期状态、80% 价格侧、两收盘 EMA576 首次突破、首次
  `EMA144±0.30 ATR14` 守稳、576 根有效期及一次性消费；空头完全镜像。
- 状态机聚焦测试 9/9 通过。L1 返回 2,066 个无标签候选，多空 1,221/845，覆盖 44 币、
  13 个 UTC 月份和 1,129 个一小时事件；`outcome_evaluation_performed=false`。
- 三张正样本命中 NMR 7 月 1 日 09:00、BTC 7 月 2 日 08:30；BTC 7 月 12 日未命中。
  ALGO 17:30 与 MERL 18:45 两个错误空单时间均为零候选，反例修正成立。
- BTC 7 月 12 日表明一次性消费与用户历史正样本冲突；同时该窗口首次触碰 K 略收于 EMA144
  下方，后续出现更深跌破，“不有效跌破”可能也不是同 K 收回口径。按单变量规则停止在 L1。
- 机器报告 SHA-256 为
  `03b9162be3c09d16d9ab1509638a29cf7eb125928ddd25314574bea6add4a769`；未创建 Pine，未接
  Paper、ReadOnly、Live、数据库写入、策略注册、调度或生产执行。

## 2026-08-03：EMA576 / EMA144 用户定义 V2～V4 停止在 L1

- V2 仅改变生命周期为“最新活动方向独占且可重复回踩”，生成 44,781 个候选、多空
  25,517/19,264、3,032 个一小时事件；NMR 与 BTC 7 月 2 日通过，BTC 7 月 12 日失败，
  ALGO/MERL 错误空点均拒绝。报告 SHA 为
  `61f8bec3b76670cf3336a8b8b2e842b8c88b65ce0a73f25f1b54b4b2647ca615`。
- V3 仅把 EMA144 同 K 守稳放宽到现有 `0.30 ATR14` 缓冲，候选增至 49,282，但目标仍为
  4/5；状态追踪确认 BTC 漏判来自 7 月 8 日空头活动接管，而非 `-0.0544 ATR` 的轻微收线。
- V4 仅把活动所有权改成多空分别保存，三张正样本全部命中；同时 ALGO 17:30、MERL 18:45
  两个错误空点精确复现，候选增至 92,661，超过预注册 80,000 上限。报告 SHA 为
  `2a8e063ac10abaf13572277966205dd71e2ef60149b9ca44fb5016b19ee1239f`。
- V2～V4 使用同一行情指纹
  `67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997`；新增双向生命周期、
  缓冲镜像和 outcome-blind 门禁测试通过，所有新增核心文件低于 1,000 行。
- 冲突已压缩为一个未定义业务边界：最新方向独占会漏掉 BTC 7 月 12 日；双方向保留会恢复
  两个错误空点。当前不得进入 L2，也不得用收益反推活动寿命或叠加其他过滤器。

## 2026-08-03：TradingView Velocity V30 三次边界交替停止在 L2

- 新增独立 Research-only 候选
  `volume_active_parent_horizontal_edge_transitions_3_shallow_breakout_excess_10pct_short_15m_research_v30`；
  只在 V29 已选父横盘上计算突破前边界交替，不改父区间或交易合同。
- L1 从 V29 的 27 笔保留 20 笔，覆盖 18 币、9 月和 20 个 60 分钟事件簇；严格子集新增 0，
  冻结风险、父区间边界、浅突破证据与同身份 outcome 漂移均为 0。
- L2 零成本 `+2.687R / +0.134R每笔 / PF 1.207`，成本后降为
  `-2.155R / -0.108R每笔 / PF 0.865`；被删除 7 笔反而是 5 胜 2 负、成本后
  `+7.284R / +1.041R每笔 / PF 3.818` 的强队列。
- 结论为 `stopped_at_l2_keep_research_only`：边界交替可改善视觉箱体，但不能作为该空头家族
  的入场门禁；未创建 Pine，未接 Paper、ReadOnly、Live、数据库写入或生产策略。
- Release 回放和 V30 聚焦测试完成；后续全量回归被并行未完成的 `finite_episode_v5.rs`
  缺失测试模块及 `V5*` 定义阻塞，本轮未修改该文件。机器结果 SHA-256 为
  `536e812a4805d5cdf559425738a58699cd9ef32f7db82f1f623fb5eb44620135`。

## 2026-08-03：严格视觉横盘 V1 与 V29 交易边界完成代码隔离

- 新增独立指标 `15min_strict_visual_consolidation_v1.pine`。它只画成熟横盘区、首次确认点及
  收盘离区点，没有 `strategy()`、订单、收益读取、告警或运行态入口。
- 默认视觉标准为 P90/P10、宽度不超过 3%、双边至少两组触碰、容纳率至少 80%、上/下沿
  漂移不超过 0.25/0.35、方向效率不超过 0.35，且至少完成三次上下沿交替。
- 脚本只在 `barstate.isconfirmed` 后创建或更新对象；同棒同时触及两侧不计算切换，避免用
  OHLC 补造盘中先后；首次确认标签明确区间何时才因果可见。
- V29 的 `model.rs`、`anchor_failed_acceptance.rs`、`ranges.rs` 实施前后 SHA-256 完全一致；
  因交易合同未变，本轮不重跑或重解释 V29 结果，仍沿用已冻结 L2 证据。
- 已改用本地 TradingView Desktop MCP 保存并加载脚本；Pine 编译为 0 错误、0 警告，保存脚本
  276 行与仓库源码按换行归一后逐字一致，图表可读取 30 个箱体、60 条边界线及成对确认/离区标签。
- 图表隔离检查发现离区后会复用旧重叠窗口并连续标记；视觉脚本已增加“新区间起点必须晚于
  上次离区棒”的生命周期约束，并让离区标签随最多 30 个区间一起回收。修复后连续推进段不再
  连续生成离区噪声，ADAUSDT.P 2026-08-01～08-03 放大图只保留完成往返的平台。
- 本地 TradingView Basic 方案限制为 5,000 根 K 线，因此不能从 2026-08-03 直接加载
  2026-05-28 的 15m 旧图；该限制不影响当前窗口的本地编译、保存和画图验证，也未据此改动 V29。

## 2026-08-03：EMA576 / EMA144 V10 L1 定义门禁通过

- V5 的任意反向 EMA576 两收盘会误杀交叉前回踩；V6/V7 的失败回踩与永久资格分别漏掉正样本
  或保留 MERL；V8 的任意 EMA144 收盘失效会误杀 BTC 7 月 12 日；V9 修正 BTC 中断但仍保留
  交叉后迟到建立的 MERL 空头。各版本均保留独立预注册和机器结果，不覆盖旧行为。
- V10 冻结市面标准时序：历史 144 根 EMA144/576 关系与 80% 价格侧资格；价格两收盘先突破
  EMA576；EMA144/576 随后完成方向交叉；离开 EMA144 0.30 ATR14 后再武装回踩。
- Episode 只在已武装回踩的收盘有效失守时终止；影线单独越界只消费武装。均线已完成方向交叉
  后，价格反向两收盘穿越 EMA576 无条件终止；失效后必须有新的合格突破才能重建。
- BTC 证据链为北京时间：资格 7 月 16 日 14:45；空头突破 7 月 17 日 07:00、09:45；方向
  交叉 17 日 18:15；09:45 再武装；18 日 00:45 回踩消费；18 日 01:15 反向确认中断；到
  19 日 21:15 无新空头 episode。
- 六张定义图 6/6；候选 26,656，多空 15,037/11,619，3,333 个事件，44 币、13 月。
  行情指纹 `67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997`，机器报告
  SHA-256 `1d780c35eef54b71490073323b722ce09ad1b38416e1062130125b1401cc05be`。
- `outcome_evaluation_performed=false`；当前仅可进入独立预注册 L2，不创建 Pine，不接
  Paper/ReadOnly/Live，不写数据库，不改生产配置。
- V3～V10 生命周期聚焦回归 31/31 通过，V9/V10 核心文件均低于 1,000 行，定向代码行门禁通过。

## 2026-08-03：严格视觉横盘首次放量上破做多 V1 停止在 L2

- 新增独立 Research-only 身份
  `volume_strict_visual_consolidation_breakout_long_15m_research_v1`，接入 Candidate V20 主 Rust
  回放，但不覆盖 V20/V29，不进入 Pine、Paper、ReadOnly、Live、生产注册或调度。
- 因果状态机只用完成棒：最长 `8/12/16/24/32/48/64/96` 根有效区间、P90/P10 边界、双边
  触碰、80% 容纳、漂移、方向效率和三次边界交替；首次确认不补造同棒突破，父边界升级只向后
  生效，任何收盘离区都会消费活动范围。
- L1 无标签结果为 36,213 个确认区间、1,969 个合格上破候选、43 币、13 个上海时区月份、
  1,083 个 60 分钟事件簇，覆盖门禁通过；没有读取 MFE/MAE/退出/胜负/R/PnL。
- L2 新家族 1,377 笔，零成本 `-54.079R / -0.0393R每笔 / PF 0.944`；每边
  `5 bps + 3 bps` 成本后为 `-340.731R / -0.2474R每笔 / PF 0.706`，950 笔初始止损。
- 13 月只有 2 月为正，43 币只有 7 币为正；聚合主回放成本后每笔 R/PF 从
  `-0.0239R/0.907` 恶化为 `-0.1047R/0.757`，顺序权益最大回撤增加 5,240.645 固定价格单位。
- L1/L2/V20 基线报告 SHA-256 分别为
  `275163cffa2238a9e074887c60aea7258860055918a303db7cdd077559aacb6f`、
  `a516b4f2141d5117a6224da02ec0866f39ec6fa1660a82ae61a7d70150322273`、
  `097566fef54e8dfafc957106709f007c0606c01744348880510429d3c55e974d`。
- 定向测试 26/26、两个相关二进制 `cargo check` 和 Release 构建通过；全库测试仍被既有且无关的
  `armed_close_post_cross_recross_v9/tests.rs` 缺失阻塞。本版本固定为
  `stopped_at_l2_keep_research_only`，不得从已查看 outcome 事后拼接 EMA/RSI/确认棒/退出补丁。

## 2026-08-03：EMA576 价格先突破、EMA144 回踩 V10 停止在 L2

- 按独立预注册把 V10 回踩确认收盘接到下一根连续 15m 开盘；继续冻结 4% 止损、0.52R、
  24 小时、8 bps/side、同棒止损优先和同币种单持仓锁，且未建模资金费。
- L1 文件 SHA、候选键、规则版本、无 outcome 状态和行情指纹通过；重载后 26,656 个候选账本
  逐字段一致，所有候选均能解析下一根连续开盘。
- 同币种锁后得到 8,671 笔完整交易，多空 4,900/3,771，覆盖 44 币、13 月和 3,396 个事件；
  17,985 个信号因持仓重叠阻塞，forward 不完整为 0。
- 零成本为 `-69.7213R / EV -0.008041R / PF 0.973661`；成本后为
  `-416.3624R / EV -0.048018R / PF 0.850443`。多头 `-0.072461R/PF 0.784900`，
  空头 `-0.016257R/PF 0.945907`，交叉前和交叉后回踩也都为负。
- 目标命中 4,806 笔、止损 2,252 笔、超时 1,611 笔；0.52R/1R 合同所需的成本后盈亏平衡
  胜率约 68.42%，实际成本后胜率仅 60.12%，且超时平均 `-0.2354R`。
- 44 个有交易币种仅 6 个净正，13 月仅 3 月净正；低集中度不能挽救总体负边际，移除最好两笔
  或最好事件后仍为负。ETH 不在冻结 Top60 返回成员中，当前没有 ETH 层证据。
- L2 机器报告 SHA-256 为
  `ff97d13e0375db0898eca45d79fdbb191ae93e19c74af821cbd23b770a77834f`；V2～V10 与 L2 聚焦测试 43/43、
  `cargo check`、定向行数门禁通过，L2 主文件 976 行。
- 状态固定为 Research-only `stop`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库写入、
  策略注册、调度或生产。若继续，应新开 L0/L1 单变量“每个 episode 只取第一次有效回踩”。

## 2026-08-03：严格视觉横盘上破回踩接受 V2 停止在 L2

- 新增 Research-only 身份
  `volume_strict_visual_consolidation_breakout_retest_acceptance_long_15m_research_v2`；唯一变量是
  V1 合格突破后等待 1～3 根轻触冻结上沿并收在上方，源棒 ATR、量比和目标档位全部冻结。
- L1 只读当时可见字段：1,969 个突破源中接受 770 个、跌回失效 457 个、超时 742 个；覆盖
  43 币、13 月和 568 个一小时事件，覆盖门禁通过。
- L2 家族 550 笔，零成本 `-50.065R / EV -0.091027R / PF 0.871957`；每边
  `5 bps + 3 bps` 后为 `-167.088R / EV -0.303796R / PF 0.647108`，391 笔初始止损。
- 13 月仅 2026-01 为正，43 币仅 7 币为正；第 1、2、3 根接受队列全部负期望，禁止根据
  outcome 改搜等待窗口。
- 511 笔同源配对中，V2 入场价平均改善 `0.180598` 源 ATR，但净结果比 V1 再少
  `4.226121R`；被接受的 V1 源队列 `-0.291909R/笔`，也差于未接受队列 `-0.219450R/笔`。
- V2 聚合结果虽好于全量 V1，却仍差于 V20 基线：成本后每笔 R/PF 为
  `-0.076478/0.847141` 对 `-0.023901/0.907259`。结论是减少交易而非获得正向选择边际。
- 定向测试 28/28，历史 `signals.rs` 已拆至 1,987 行；机器报告 SHA-256 为
  `25e6a5ce4fa9c1d2799939162f3c76ea19694580e701d236bd0280effc26bd3b`。拆分后的当前 Release
  重跑结果除运行元数据外与拆分前逐字段一致。
- 状态固定为 `stopped_at_l2_keep_research_only`；未创建 Pine，未接 Paper、ReadOnly、Live、
  数据库写入、策略注册、调度或生产。下一轮必须返回新 L0，不继续微调本形态。

## 2026-08-03：严格视觉横盘突破实体中点守稳 V3 停止在 L2

- 新增 Research-only 身份
  `volume_strict_visual_consolidation_breakout_body_midpoint_hold_long_15m_research_v3`；唯一变量为
  V2 首次接受确认收盘必须守住冻结突破棒实体中点，失败时立即消费来源。
- L1 在相同行情指纹下从 V2 的 770 个候选保留 627 个、拒绝 143 个，覆盖 43 币、13 月、
  483 个一小时事件簇；新增原始信号 0，143 个删除项与拒绝记录逐笔一致。
- BNB 2026-07-16 14:00 与 2026-07-19 01:45 分别因 `583.0 < 583.3`、
  `571.0 < 571.15` 被拒绝；ADA 2026-07-18 08:15 因 `0.1672 >= 0.1671` 被保留。
- 同一 Release 可执行文件和数据身份下，V3 家族 445 笔：零成本
  `-23.945R / EV -0.053809R / PF 0.923006`；每边 8 bps 压力后
  `-118.606R / EV -0.266531R / PF 0.684766`，仍未转正。
- 过滤删除的 106 笔已执行 V2 队列为 `-45.752R / EV -0.431622R / PF 0.529463`，但共同
  保留的 444 笔仍为 `-121.336R / EV -0.273279R / PF 0.677511`，入场家族没有可交易边际。
- ADA 样本下一根开盘 0.1673、止损 0.1659，盘中在 10:30 达到 1R 价 0.1687，但冻结目标
  0.1714，最终成本后 -1.1904R；该事实不用于修改本轮 outcome 后的退出政策。
- V3 L2 机器结果 SHA-256 为
  `f8a981358319a7414fec345989f2c5e5308964d992ea16b0891ae1931791cfbb`。状态固定为
  `stopped_at_l2_keep_research_only`，不创建 Pine、不接运行态，也不启动横盘长度止盈批次。

## 2026-08-03：EMA576 首信号后 24 小时交叉确认超时 V11 停止在 L2

- 新增 Research-only 身份 `market_momentum_ema576_post_signal_cross_timeout_15m_v11`；旧资格链
  第一笔交叉前信号启动 96 根不可重置计时，到期 K 先检查方向性交叉，失败才消费 episode 与资格。
- ALGO 日期链按北京时间通过：07-15 06:00 首信号保留，07-16 06:00 超时失效，07-16
  07:15 的陈旧资格再突破未建立 episode；BTC 既有 07-18 中断链及六张目标图均保持通过。
- L1 从 V10 26,656 个候选保留 17,041 个，删除 9,615、新增 0；多空 9,042/7,999，覆盖
  44 币、13 月和 3,108 个事件；L1 未读取成交后标签。
- L2 逐字段重建 L1 账本并复用冻结成交/退出：5,791 笔，零成本
  `-111.439R / EV -0.019244R / PF 0.938159`，成本后
  `-342.920R / EV -0.059216R / PF 0.819124`。
- V10/V11 共同 5,543 笔为 `EV -0.060101R/PF 0.816887`；V10 独有或因锁顺序消失的
  3,128 笔为 `EV -0.026606R/PF 0.913727`，确认 24 小时条件删除了相对较好的负交易，单笔质量恶化。
- BTC 119 笔小样本为 `EV +0.010773R/PF 1.059548`，但缺少 ETH、OOS 与 point-in-time
  币池，不能升级为 BTC 专用结论；其他币种 5,672 笔仍为负。
- 状态固定为 `stop / Research-only`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库或生产。
  L2 机器报告 SHA-256 为 `4a0c9245ca5fe26f14b6bf2937ab0ec15273a7ed11629b714f8d39e09a4ebcb8`。

## 2026-08-03：严格视觉横盘短区间 1R 目标 V4 停止在 L2

- 原预注册的 20%～80% 影响比例门禁因无 outcome 扫描得到 502/627=`80.0638%` 而失败；
  保留原失败证据后，按用户确认新建修订预注册，以两个分支分别至少 15 笔、8 币、3 月和
  10 个事件簇作为绝对覆盖门禁。
- 新增 Research-only 身份
  `volume_strict_visual_consolidation_breakout_body_midpoint_hold_short_range_32_one_r_long_15m_research_v4`。
  V4 只在突破棒冻结横盘不超过 32 根时令 Fixed 目标等于初始止损 tick；确认状态、入场、
  止损、长区间目标、成本和特殊退出均复用 V3。
- V3/V4 使用相同数据指纹
  `eda87a30667f040cd74048e659def7453a5d49f6c81e062d012bebcb1c2ad5c4` 和可执行文件指纹
  `e36c382b8e8fd534fcd5c291d77b49d167157f453c6257d447466f8fedd124a3`。445 笔严格家族身份
  差异为 0，335 笔短区间 Fixed 目标合同错误为 0，长区间与特殊退出目标漂移为 0。
- V4 家族零成本 `-15.377R / EV -0.034556R / PF 0.937490`，成本后
  `-110.045R / EV -0.247293R / PF 0.630237`。成本后胜率由 29.21% 升至 44.04%，但 PF
  由 0.684766 降至 0.630237；1R 的成本后平均赢/亏约为 `0.793R/1.206R`，所需胜率约
  60.35%，实际不足。
- 335 笔目标变更队列中，65 笔由止损转 1R 止盈、1 笔由下一开盘反手转止盈，净改善
  130.442R；92 笔原量能目标赢家被截短，损失 121.881R；最终只改善 8.561R。ADA
  2026-07-18 08:15 正确从 `-1.1904R` 变为 `+0.8080R`。
- V4 提前退出另释放 ACT 与 BAT 两笔其他家族亏损，合计 `-2.368818R`。完整 Candidate V20
  成本后总净 R 约由 -179.809R 改善到 -173.617R，但总 PF 降至 0.807148、最大回撤升至
  14,400.88 固定价格单位。
- `cargo fmt --check`、V4 聚焦 18 个库测试、CLI 解析与账本测试、Release 构建和 V3/V4 Rust
  回放通过。全 `strict_visual` 过滤测试中存在一个与 V4 无关且原逻辑即可复现的旧状态测试失败；
  本轮未修横盘状态机，避免破坏唯一变量合同。仓库声明的行数脚本当前不存在，定向 `wc -l`
  显示 `signals.rs` 1,999 行，未超过 2,000 行硬上限。
- 修订预注册、V3 重跑、V4 机器结果 SHA-256 分别为
  `1eeebd8b149be456f165bbef895fb78a2a38a18e6cbddc59b54c8ef64e2728ff`、
  `f723bc938ada21e3a8bfcc00d67dc29f8efad5dfc879a79201df9445a5f40c05`、
  `173fe9d2363eeb9141dbc57f065a6fed851be6a8d53e2ed531863397be4f054f`。
- 状态固定为 `stopped_at_l2_keep_research_only`；不创建 Pine、不接 Paper、ReadOnly、Live、
  数据库或生产。下一轮不得在当前 outcome 上搜索长度/目标，若继续只能另立新 L0 并使用
  未查看过的前向/OOS 数据。

## 2026-08-03：EMA576 首笔真实成交与 EMA144 结构止损诊断

- 新增独立 Research-only V12
  `market_momentum_ema576_first_filled_entry_per_setup_15m_v12`。成交 broker 以
  `symbol × direction × setup_ts` 记录已真实成交的 setup；持仓锁或数据阻塞不消费资格，
  首笔成交后后续 L1 信号只保留审计，不再进入持仓回放。
- V12 的 17,041 个冻结 V11 候选中完成 2,684 笔，多空 1,516/1,168，覆盖 44 币、13 月和
  1,406 个事件；14,230 个候选因 setup 已成交被阻塞、127 个因同币种持仓锁阻塞。成本后
  `-164.693R / EV -0.061361R / PF 0.811146`，多头和空头仍分别为负。
- LAYER `setup_ts=1784319300000` 只成交北京时间 07-19 10:15 信号对应的 10:30 开盘；
  11:45 后续信号没有形成 12:00 第二笔。首笔按 V12 固定 4% 风险同棒到达目标，成本后
  `+0.479584R`，说明用户指出的重复入场生命周期已修正。
- 新增独立 Research-only V13
  `market_momentum_ema576_first_entry_ema144_structural_stop_15m_v13`，唯一变量为信号时冻结
  EMA144 外侧 `0.30 ATR14` 结构止损；下一根开盘已越过止损则明确阻塞，目标仍为实际初始风险
  的 0.52R，不启用保本、追踪、分批或反手。
- V13 完成 2,703 笔，37 个候选因结构止损不在入场失效侧而阻塞；止损距离中位数 0.4442%、
  P90 1.0841%，往返成本换算为中位 0.3603R、P90 0.9571R。成本后
  `-1768.009R / EV -0.654091R / PF 0.140181`，大量 0.52R 目标命中仍被成本转换为净亏损。
- 共用 L2 风险政策拆到独立模块，主文件 983 行；聚焦单测 8/8，V12/V13 Release 构建通过。
  V11 旧机器报告以及 V12/V13 已保存机器报告均在删除生成时间后逐字段一致，确认没有历史漂移。
- V12/V13 机器报告 SHA-256 分别为
  `c8bce8a28aaa742ab04d89db1eb93b89a616cb94a0d93b050a4b1ca17bbc28b3`、
  `6e231adbe25c4fb018cfb0c89e6d13e47904cf2dd7c364fe329fa5a56674818a`。两版均固定为
  `stop / Research-only`，未创建 Pine、未接数据库写入、策略注册、调度或生产。

## 2026-08-03：严格视觉横盘突破强度 V5 与弱离区观察期 V6 停止在 L2

- 新增两个独立 Research-only 身份。V5 要求突破棒实体占振幅至少 60%、方向实体涨跌幅至少
  0.25%；V6 只把弱离区生命周期改为紧邻一根完成棒观察期，上下方向完全镜像。
- V5 L1 拒绝 688/1,969 个 V3 来源，保留 351 个候选；V6 的 18,301 次弱离区中有
  6,729 次下一根回区，恢复后得到 421 个候选。两轮均覆盖 43 币和 13 月，未读取 outcome。
- V6 L2 严格家族 290 笔：零成本 `-1.2195R / EV -0.0042R / PF 0.9940`，每边
  `5 bps + 3 bps` 后为 `-61.0147R / EV -0.2104R / PF 0.7502`。
- 相比 V5，V6 成本后平均 R 和 PF 小幅改善，但净 R 再减少 5.2654R；新增 58 笔在成本后为
  `-3.6660R`，删除的 12 笔为 `+1.5994R`。完整 Candidate V20 成本后净 PnL 由
  -2,482.96 降至 -3,386.57。
- 结果覆盖 43 币、13 月和 250 个事件簇；32 币、8 月、174 个事件簇亏损，不能解释为少数样本
  拖累。V6 按预注册停止在 L2，不进入 L3，不同步正式 Pine 或任何运行态。
- 两个相关二进制 `cargo check`、V5/V6/镜像/CLI 聚焦测试及 L1 二进制 6/6 通过；完整
  `strict_visual_breakout` 模块仍有既有 `old_range_cannot_reappear...` 用例失败，本轮未混入
  重叠窗口候选选择这一第二变量。

## 2026-08-03：EMA576 确认收盘距 EMA144 上限 V14 停止在 L2

- 新增独立 Research-only 身份
  `market_momentum_ema576_first_entry_ema144_structural_stop_close_distance_15m_v14`；唯一变量为
  信号完成 K 的确认收盘绝对距离不得超过预注册 ATR14 cap。
- L1 在冻结 V11 SHA `ebc2b886...e1e6` 上一次性扫描 0.50/0.75/1.00 ATR，不读取 outcome。
  三者分别删除 41.91%、22.77%、11.84%；均拒绝 LAYER 10:15 并保持 44 币、13 月和双方向覆盖，
  因而按事前最大 cap 规则选中 1.00 ATR，主账本为 15,024 个候选。
- L2 保持 V13 的首笔真实成交、下一根开盘、EMA144±0.30ATR 结构止损、0.52R、24 小时、
  8 bps/side 和同币种锁。2,651 笔完整交易成本前
  `-307.160R / EV -0.115866R / PF 0.723030`，成本后
  `-1923.961R / EV -0.725749R / PF 0.106533`；多头和空头均负。
- LAYER 10:15 的 `1.463463 ATR` 信号已过滤，但过滤不消费 setup；11:45 的
  `0.298203 ATR` 信号成为首笔，12:00 入场后 12:30 止损，成本后 `-1.279745R`。
- V13/V14 配对归因：共同 2,312 笔的成交、风险与退出漂移为 0；被过滤的 391 笔 V13 首笔为
  `EV -0.203669R/PF 0.517180`，反而显著好于共同队列；过滤后释放的 339 笔后续首笔为
  `EV -0.694945R/PF 0.100968`。V14 成本中位数由 0.3603R 升到 0.4073R，P90 由
  0.9569R 升到 1.0416R，确认更靠近 EMA144 会进一步放大结构止损下的成本占 R。
- V14 代码文件 788 行；4 个距离/绝对值/setup/outcome 边界测试、Release 构建、L1/L2 当前实现
  parity 均通过。L1 与合并机器报告 SHA-256 为
  `23edf396fe1b82681789c9323d38a30b17e1defa03d9d8b4551ac2f059e7b475`、
  `099827aa7924f2e754fa5926e6b50299eaedfc03aa9c5ddc284ce2bf4fed51c1`。
- 状态固定为 `stop / Research-only`；不创建 Pine、不接 Paper、ReadOnly、Live、数据库、调度或生产。
  禁止继续搜索距离阈值；下一步仅在用户确认后研究“第一次信号机会是否无论成交都消费 setup”。

## 2026-08-04：严格视觉横盘突破多单净保本激活 V1 停止在 L2

- 新增隔离退出反事实，只比较完成棒最高价达到 `0.5R` 或 `1.0R` 后，从下一根开始把止损提高到
  覆盖每边 8 bps 成本并向上取整到 tick 的净保本价；不改变 V6 的 290 笔交易身份。
- 0.5R 激活 180 笔、实际改写 158 笔退出，成本后由基线 `-61.0147R / PF 0.7502` 变为
  `-62.2187R / PF 0.5249`；1R 激活 129 笔、改写 92 笔，变为 `-72.7651R / PF 0.5899`。
- 0.5R 救回 105 笔原止损的 `+117.4345R`，同时截断 52 笔原止盈的 `-118.6593R`；1R 的
  对应贡献为 `+67.9956R / -79.7460R`。胜率升高但净 R、PF 与回撤均恶化。
- 6 个因果/成本单测与 CLI 全部 30 个测试通过；Rustfmt 2021、定向 2,000 行硬门禁与 Release
  43/60 本地成员回放通过。机器结果 SHA-256 为
  `e2ddc000fdb6b9765b0fb3ebeec37ee51a2e40ba2e03da7a832ddd907e1b3676`。
- 最近交易审计已追加到同一结论报告：12 笔最新原亏损中 0.5R 改善 8 笔、1R 改善 4 笔；但
  0.5R 全样本误伤 52/81 笔原止盈。净保本价中位数位于横盘上沿上方 0.538R，与允许回踩上沿的
  入场合同直接冲突。
- 状态固定为 `stopped_at_l2_negative_exit_edge`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库
  写入、调度或生产。下一研究只能另立“完成棒收盘确认站上 1R”的单变量版本。

## 2026-08-04：严格视觉横盘突破多单完成收盘 1R 净保本 V2 停止在 L2

- 新增 Research-only V2 隔离退出对照：只有已完成 15m K 线收盘达到 1R 才激活覆盖双边
  8 bps 成本的 tick 对齐净保本，确认棒自身不得触发，保护从下一棒生效。
- 同一行情指纹和 290 笔 V6 基线中，V2 激活 101 笔、覆盖 91 个事件簇、改变 58 笔退出；交易
  身份变化为 0。成本后由 V6 的 `-61.0147R / PF 0.7502` 变为
  `-75.1793R / PF 0.6297`，相对基线少 `14.1646R`。
- 34 笔原止损被救回 `+41.6654R`，但 24 笔原止盈被截断 `-55.8300R`；收盘确认较最高价 1R
  保留 11 笔赢家的同时漏救 23 笔止损，最终还比 V1 少 `2.4142R`。
- 新增 3 个完成收盘因果边界测试；focused `9/9`、完整 CLI `33/33`、格式和 2,000 行硬门禁
  通过。机器结果 SHA-256 为
  `557dd276f3121bebba0264a804ea6db322a1f7c3830be3ddfc5c57f223b50217`。
- 状态固定为 `stopped_at_l2_negative_exit_edge`；未创建 Pine，未接 Paper、ReadOnly、Live、数据库
  写入、调度或生产。下一轮只能另立结构失效保护的 L0/L1，禁止继续追逐固定 R 阈值。

## 2026-08-04：EMA576 EMA144 结构止损净 2R 目标 V15 停止在 L2

- 新增独立 Research-only 身份
  `market_momentum_ema576_first_entry_ema144_structural_stop_close_distance_net_target_2r_15m_v15`；
  唯一变量为把 V14 毛 0.52R 目标替换成按 8 bps/side 反解的净 2R 目标。
- L1 对 V14 的 2,651 笔冻结计划只读取入场时字段；全部目标合法，44 币、13 月、双方向覆盖，
  三个指定样本净目标误差均小于 `1e-9R`。污染 complete/exit/gross/cost/net 等 outcome
  字段不会改变目标几何。
- L2 成本前改善到 `+187.843R / EV +0.070857R / PF 1.101154`，但平均成本拖累
  `0.609721R/笔`，成本后仍为
  `-1428.526R / EV -0.538863R / PF 0.526433`。794 笔净 +2R 目标无法覆盖
  1,857 笔平均净 `-1.6244R` 的结构止损。
- XRP、1INCH、UMA 初始风险仅占价格 0.0650%、0.1179%、0.1397%，对应成本分别为
  2.4595R、1.3559R、1.1449R；三笔均止损，净结果为 -3.4595R、-2.3559R、-2.1449R。
- 共用目标政策保持 V10～V14 显式走原毛 0.52R 分支；V14 重跑的 2,651 笔账本、指标和身份
  完全一致。EMA 模块 106 个测试、格式与行数门禁通过。
- 机器结果 SHA-256 为
  `802527ed3c58627077337aca5a03a46cf57102a3f9d2b3a68eaa67dc3bed161c`。V15 固定为
  `stop / Research-only`，未创建 Pine，未接运行态或生产；下一轮不得在当前 outcome 上搜索
  目标 R，只能另立入场经济性门禁假设。

## 2026-08-04：严格视觉横盘突破多单 1R 后上沿收盘失效退出 V3 停止在 L2

- 新增 Research-only 隔离退出：完成收盘达到 1R 后，后续完成棒严格收盘跌破信号时冻结的
  横盘上沿，才在下一根真实开盘退出；原初始止损和冻结目标继续优先参与 broker 路径。
- 同一行情指纹、43/60 本地成员和 290 笔 V6 基线身份下，283 笔 Fixed 多单可应用；101 笔
  完成收盘达到 1R，22 笔形成结构失效并实际改变退出，覆盖 22 个 60 分钟事件。
- 20 笔原止损合计改善 `+9.8288R`，NMR 与 XRP 两笔原止盈因仅 `0.01R/0.02R` 的浅跌破被
  错误退出，合计损失 `-5.4603R`；8 bps 下净改善只有 `+4.3684R`。
- V3 零成本为 `+3.1525R / PF 1.0162`，但 8 bps 后为
  `-56.6463R / EV -0.1953R / PF 0.7597`，未通过净 R 为正和 PF 大于 1 两项门禁。
- 归因显示 182 笔从未完成收盘达到 1R 的 Fixed 多单贡献 `-173.5169R`，而 101 笔已激活队列
  本身为 `+112.2234R`；进一步调 1R 后保护无法修复主亏损源。
- 专项 6 个因果测试与 CLI 39 个完整测试通过，行数硬门禁通过。机器结果 SHA-256 为
  `6ef2936b937e8d3921771bd3984bfe930a1cfc7880a569f9f1c1bfa44b741bed`，结论报告 SHA-256 为
  `28e19de1f25823f0101c17a39fbd3542d818478036f9bef2d199037b76b367b0`。
- 状态固定为 `stopped_at_l2_negative_cost_adjusted_edge`；未创建 Pine，未接 Paper、ReadOnly、
  Live、数据库写入、调度或生产。下一轮应回到入场前接受质量的独立 L0/L1，并使用新窗口验证。

## 2026-08-04：EMA576 EMA144 入场止损成本 0.50R 门禁 V16 停止在 L2

- 新增独立 Research-only V16；唯一变量为下一根连续 15m 开盘时，若 EMA144±0.30ATR14
  结构止损成交的双边成本超过 `0.50R`，则在真实成交前拒绝且不消费 setup。
- L1 账本保存全部 15,024 个候选的因果入场证据。14,987 个合法机会中拒绝 6,255、保留
  8,732；三个指定失败样本成本为 `1.355888R / 1.144888R / 2.459450R`，全部准确拒绝。
- L2 完成 2,258 笔、1,373 个事件；毛 `EV +0.044726R / PF 1.065324`，成本后
  `-616.687R / EV -0.273112R / PF 0.697804`。多头、空头、13 个月均负，44 币仅 7 币净正。
- 相比 V15，平均成本从 `0.609721R` 降到 `0.317838R`，但 31.53% 命中率仍低于 39.76%
  盈亏平衡线。成本门禁是有效风险卫生措施，不是足以形成正边际的入场规则。
- 共用回放显式保留 V10～V15 无门禁政策；EMA144/576 相关测试 111/111、Rustfmt、定向行数门禁、
  V16 bin 编译与 V15 重跑逐字段 parity 通过；共用主文件 917 行、入场模块 110 行。
- 机器报告 SHA-256 为
  `0c199ad758becefab58a03da53837e85c022b6e4d0e510fa7b34c26332307cf2`。状态固定为
  `stop / Research-only`；未创建 Pine、未接运行态或生产。下一轮只能另立入场确认质量 L0/L1。

## 2026-08-04：严格视觉横盘突破确认收盘 ATR 接受余量 V7 停止在 L1

- 新增 Research-only 预注册，唯一变量为确认收盘高出冻结横盘上沿的距离除以突破棒冻结 ATR；
  阈值网格事前固定为 `0.05/0.10/0.15/0.20 ATR`，选择不得读取 outcome。
- V6 无标签 L1 账本包含 421 笔、43 币、13 月和 338 个事件；余量 P25 为 0.3049 ATR、
  中位数 0.5197 ATR，说明原网格只触达左尾。
- 0.20 ATR 只拒绝 61/421=`14.4893%`，低于预注册 15% 最小影响；其余阈值更低，故没有合法
  selected threshold，不能四舍五入或临时追加更强阈值。
- 只读数据库覆盖审计确认当前 43 个可用成员均有 813 根前向 15m 完成 K，统一覆盖上海时间
  07-19 22:30 至 07-28 09:30；约 8.47 天，不足正式 OOS。
- 机器结果 SHA-256 为 `54ded4c292258ff58967e2c1277247bde32cd4b5467623b02dab7c205c918944`，
  结论报告 SHA-256 为 `5c2a301b61e36749a7e3400f6e3dc40dfcd9a62906e187ab465395c1b1a5fa2e`。
- 状态固定为 `stopped_at_l1_insufficient_preregistered_impact`；未写 V7 交易代码、未运行 L2、
  未创建 Pine、未接 Paper/ReadOnly/Live 或生产。下一轮只能另立更强接受余量网格的无标签研究。

## 2026-08-04：严格视觉横盘突破确认收盘强接受余量 V8 停止在 L2

- 依据 V7 无标签分布另立 V8，事前网格仅 `0.30/0.40/0.50 ATR`。三档均通过覆盖门禁，按拒绝
  比例最接近 30% 的固定规则唯一选中 `0.40 ATR`，未读取交易 outcome。
- 新增 Research-only 身份
  `volume_strict_visual_consolidation_stronger_acceptance_margin_0_40_atr_long_15m_research_v8`；
  保留 V6 的横盘、弱离区观察、强突破、三棒接受与实体中点顺序，只在首次合法确认棒消费低余量来源。
- Rust L1 精确复现 421 笔 V6 候选中的 144 笔拒绝和 277 笔保留；保留候选覆盖 43 币、13 月、
  234 个事件，V6/V8 共同来源的上沿、来源 ATR、突破/信号时间漂移为 0。
- L2 严格家族成交从 290 降到 193，总净 R 改善 `+20.3351R`，但平均净 R 与 PF 分别恶化
  `0.000380R / 0.002242`，V8 仍为 `-40.6796R / EV -0.210775R / PF 0.747981`。
- 97 笔删除子集本身为 `EV -0.209640R / PF 0.754590`，略好于保留子集；净改善只是减少负期望
  笔数。全 Candidate V20 净 PnL 再恶化 `776.17`，PF 恶化 `0.028395`，最大回撤增加 `782.62`。
- 专项 V8 测试、L1 runner、CLI 身份、Rustfmt、Release L1/L2 和 2,000 行硬门禁通过；扩大名称
  过滤测试仍暴露 1 个不经过 V8 的 Baseline 横盘重建旧断言，未冒充全绿。
- L2 机器结果 SHA-256 为
  `9eb41ba7a10c9763157144303932871bd15b48589b3f925cc4b97100cfc94220`。状态固定为
  `stopped_at_l2_no_quality_separation_and_total_portfolio_regression`；未创建或修改 Pine，未接
  Paper、ReadOnly、Live、数据库写入、调度或生产。下一轮禁止继续搜索接受余量阈值。

## 2026-08-04：EMA576 资格周期修复与突破强化 V17～V19

- ADA 根因确认：V7 的 `reset_run()` 在 EMA144/576 关系失败时只清空当前 144 根统计，明确不清除
  `latched`；V11 的 96 根/24 小时超时又从首次交易信号而不是资格完成时启动。因此 07-03 08:30
  的空头资格跨过 07-08 18:15 下穿、07-18 10:45 上穿，仍支持 07-19 00:30 突破和 01:15 信号。
- V17 独立检查 setup 到突破的关系周期，ADA 在 07-08 18:15 以
  `qualification_relation_cycle_broken_before_breakout` 拒绝。L1 删除 2,620 个 V16 可入场机会，
  保留 6,112 个、44 币、13 月、2,143 事件；L2 完成 2,032 笔，净
  `-578.377R / EV -0.284635R / PF 0.686411`，状态 `stop`。
- V18 独立要求确认收盘距离 EMA576 至少 2.50 ATR。L1 删除 93.6555%，只留 554 个；ADA 与
  ONT 分别只有 0.8867/1.1054 ATR。L2 完成 179 笔，毛
  `EV -0.057388R / PF 0.919747`，净 `EV -0.353104R / PF 0.617412`，状态 `stop`。
- V19 独立要求第一根越线开始连续 8 根同侧完成收盘，接受完成前回线或提前触及 EMA144 回踩区
  即失效。ADA/ONT 只有 2/6 根；全样本只影响 37.5859%，未进入预注册 50%～95% 区间，停止
  在 L1，未运行 L2。
- 三个批次共用冻结 V14/V16 SHA 与行情指纹校验，只读取信号时可见字段完成 L1；V17/V18 仅在
  L1 通过后各运行一次冻结 L2。EMA144/576 专项 114/114、三个 bin 编译、Rustfmt 和定向行数
  门禁通过，共用模块 955 行。
- 三份机器报告 SHA-256：V17
  `9cc9ad63c4f4230d5cde94c0236c13f4ba7d11d29b5cb6f4b944909382e5e77b`，V18
  `9445d82dc1ff898662a82d627220c4caf69cb91ecb3beccc5c652e33c8cee977`，V19
  `032d345888af7b20ed209198426697466c72f8377e97d5b0d5c8bc333629c9a0`。未创建 Pine，未接
  Paper、ReadOnly、Live、数据库写入、调度或生产；不把三个失败/停止候选事后合并。

## 2026-08-04：EMA576 组合质量合同 V20 停止在 L1

- 用户明确要求把 V17 资格周期、2.50 ATR 突破距离和连续八根接受叠加，因此新增独立 Research-only
  V20。该版本只评估组合整体，不声称其中任一单项的因果归因，也不覆盖 V17～V19。
- 预注册冻结组合影响 `93.5%～99.9%`、多空各至少 100、至少 8 币/6 月/100 事件；保持 V16
  next-contiguous-open、首笔成交、EMA144±0.30ATR 止损、净 2R、每边 8 bps 和 0.50R 成本门禁。
- L1 在同一 V14/V16 SHA 和行情指纹下评估 15,024 个候选、8,732 个 V16 合格机会；三项全过
  325 个，拒绝率 `96.2781%`。保留覆盖 41 币、13 月、214 事件，多头 230、空头 95。
- 三位位图按资格周期=`1`、距离=`2`、八根=`4` 保存逐项证据；位图 0～7 的数量依次为
  `1070/2177/7/28/1349/3582/194/325`。2.5 ATR 仍是主要瓶颈，只有 554 个机会通过距离。
- ADA 位图 0，07-08 18:15 先失效；ONT 位图 1，只通过资格周期。目标审计、影响区间、币种/月/事件
  均通过，唯一失败门禁是空头 95 小于冻结下限 100。
- 按 L1 停止合同没有读取 outcome 或运行 L2，不事后把门禁改为 95。状态固定为
  `stopped_at_l1_insufficient_short_coverage`，机器报告 SHA-256 为
  `d4724c475099f04b8ddc6d852b47e3f5310927b7e3ec924ee0724839bb103552`。
- EMA144/576 专项 115/115、V20 bin、Rustfmt、定向行数和 JSON 合同检查通过。未创建 Pine，未接
  Paper、ReadOnly、Live、数据库写入、调度或生产；继续时只能使用新增非重叠窗口保持规则不变复验。

## 2026-08-04：严格视觉横盘突破外部结构上沿门禁 V9 停止在 L2

- 新增 Research-only V9，严格区分绘图/回踩用 P90 `visual_upper` 与交易突破用
  `trade_breakout_upper`；后者只在外部高点尚未被完成收盘解决时提高到横盘前
  `min(range_length, 32)` 根的最高价。
- 全部外部证据在突破棒完成时冻结，被拒绝来源在 V8 原决策时点消费，不释放状态补造后续信号。
  L1 得到 139 笔保留、138 笔拒绝、V9 新增 0，目标 BTC 06:15 因未越过 65,100 正确拒绝。
- 43/60 本地成员 L2 中，严格家族成交由 193 降至 116，总净 R 改善 12.5960R，但平均净 R、
  PF、胜率和零成本净 R 全部恶化；删除集合 PF 0.797701 高于保留集合 0.716759。
- 删除结果覆盖 35 币、13 月和 73 个事件，并非只解释 1～2 笔；但 96 根横盘被删集合为
  `+4.9868R / PF 3.0622`，证明固定 raw-high 回看范围没有跨持续时间稳定性。
- Candidate V20 总组合虽从 `-4162.74 / PF 0.849281` 改善到
  `-2946.64 / PF 0.887995`，仍为负且不能覆盖唯一研究家族的质量门禁失败。
- 状态固定为 `stopped_at_l2_no_quality_separation`。未创建 Pine，未接运行态或生产；机器结果
  SHA-256 为 `e89e8453408422128196275d7bb6d19440706f7cac5be11bccdf561a3820f271`。

## 2026-08-04：EMA576 2.00 ATR 与 EMA576 八根盘中保持 V21/V22

- 为避免同时修改距离与接受边界，先冻结 V21：相对 V20 只把突破确认距离从 2.50 ATR 放宽到
  2.00 ATR；再在任何 V21 结果可见前冻结 V22：相对 V21 只把 EMA144 回踩区提前否决替换为
  EMA576 盘中严格穿越。
- V21 无标签保留 623/8,732，方向 398/225，较 V20 增加 298；实际影响 `92.8653%` 超出事前
  `93.5%～96.3%` 区间下沿，按纪律停止在 L1，没有 outcome 或 L2。
- V22 无标签保留 613/8,732，方向 388/225、44 币、13 月、381 事件，全部 L1 门禁通过；
  EMA576 边界相对 V21 最终纯删除 10 个多头，组件层删除 1,109、释放 72，净减少 1,037。
- V22 冻结 L2 完成 224 笔、196 个事件，67 目标/157 止损；毛
  `-2.7977R / EV -0.01249R / PF 0.98218`，净
  `-70.0868R / EV -0.31289R / PF 0.65658`，多空 EV/PF 均为负。
- 117 个 EMA144/576 测试、两个研究入口编译、Rustfmt、定向 1,000/2,000 行门禁及两个 JSON
  合同检查通过。机器报告 SHA-256：V21
  `d73a9d6e45f7ad5a723edbd338655825cdcf18294cb0c454edef5c2197b56f6b`，V22
  `99d490ee5e074179a569f9693e99a7d4207428532802e3ff78696ee3cb1453ee`。
- V22 固定为 `stopped_at_l2_negative_entry_edge / Research-only`；未创建 Pine，未接 Paper、
  ReadOnly、Live、数据库写入、调度或生产。下一轮若继续，应只删除盘中 wick 否决，保留
  2.00 ATR 与 8 根完成收盘同侧。

## 2026-08-05：EMA576 信号前关系周期重置 V23

- 用户补充 WOO、LTC 与 ACT 三个反例后，确认 V22 的根因是资格关系只校验到
  breakout，未在 breakout 到 signal 期间消耗已交叉的旧事件。
- V23 独立身份仅延长关系终点到 signal close；其他 V22 的 2.00 ATR、8 根接受、
  EMA576 盘中穿越否决、next-open、结构止损、0.50R 成本门禁和净 2R 目标不变。
- 目标审计确认 WOO/LTC/ACT 的首次失效时间为 07-15 02:30、07-13 14:30、
  07-11 02:15；三者均为位图 6/7，只失败关系周期位，不再产生入场。
- L1 对 8,732 个 V16 合格机会保留 123，多空 101/22、32 币、12 月、91 事件；
  影响 98.5914%。空头低于事前 100 个下限，分散覆盖也未通过，故停在 L1，不运行 L2。
- 多空镜像与盘中边界定向测试 8/8 通过，Release L1 成功；定向行数检查无硬门禁错误，
  共用文件 1,064 行超过 1,000 目标但低于 2,000 硬上限，本轮不做无关拆分。
- 机器报告 SHA-256 为
  `a538880d277865ce316d0c5c0a002fc3f9a846f87d9e1295aa6014d721ec50b2`；结论报告 SHA-256 为
  `44afd27c4d5f49d307f333bd7927e87863a7b6b22d75e9ecc35361c04fc36f6e`。未创建 Pine，未接运行态或生产。

## 2026-08-05：严格视觉横盘突破 1.0H 净保本退出 V4

- 研究只改变纯严格视觉横盘 Fixed 交易的退出保护：冻结横盘高度来自信号 intent，完成棒达到
  `1.0H` 与 8bps 成本净保本价中的更远者后，止损从下一根开始生效；同棒不追溯，跳空按开盘。
- L1 通过：176 笔全部唯一匹配冻结高度，覆盖 43 币、13 月、157 个 60 分钟事件；L2 交易身份、
  入场、初始止损和原目标漂移均为 0，原 V8 核心回放保持一致。
- L2 中 39 笔激活、20 笔改变退出。9 笔原止损被保护，改善 `10.1255R`；11 笔原止盈在达到 1H、
  回踩开仓附近后被截断，损失 `23.8817R`，净变化 `-13.7561R`。
- 8bps 下净 R `-39.8712R→-53.6273R`、EV `-0.22654R→-0.30470R`、PF
  `0.73319→0.61600`、最大回撤 `48.9607R→60.0966R`，全部核心质量闸门失败。
- 新合同 6/6、Top60 45/45、原严格视觉短区间目标 1/1 通过。机器报告 SHA-256 为
  `546d015d0246b6e63d64bd8957ed9a6f72be4a4e82d9e393f96131e056d97d1c`；结论报告 SHA-256 为
  `aaecb2ef88cca0256ae0f13cef1ef16bd97b6abb0ffc1b5a3f83508b035d5842`。
- V4 停止在 L2，保持 Research-only；没有接入主策略、Pine、Paper、ReadOnly、Live、调度或生产。

## 2026-08-05：严格视觉横盘 1H 后两次连续失守 V5

- 新增独立 Research-only L1 决策扫描：达到信号时冻结 `1H` 后，只在更晚两根连续完成 K 都
  严格失守冻结上沿时形成决策；第二根收盘是标签边界，之后的 K 线与最终交易结果不进入 L1。
- 纯严格视觉 Fixed 交易 176 笔，39 笔达到 1H、37 个事件；两次连续失守仅 4 笔、4 个事件、
  4 个币种、3 个上海月份，占 armed 交易 10.2564%。
- 预注册要求至少 8 笔/8 事件/6 币/4 月且占比 15%～45%；五项覆盖门禁失败，状态固定为
  `stopped_at_l1_insufficient_two_close_failure_coverage`，没有读取 V5 决策后的 outcome 或运行 L2。
- 因果时序测试 6/6 通过；机器结果 SHA-256 为
  `912ffde937a117c7c1ac85e318b1ea99948ab465dc3af8bd46084515e10ccfbd`；结论报告 SHA-256 为
  `f6b4687dc1aa44deb467d31b3a92148cfd83e1d7562b9b46476ab5f15e61c773`。
- 未创建 Pine，未接 Paper、ReadOnly、Live、数据库写入、调度或生产；下一轮不得继续搜索三根确认。

## 2026-08-05：EMA576 八根接受窗口极值 2ATR V24

- 用户纠正 V23 的动量距离时点：不再要求第 2 根确认 K 的收盘距 EMA576 达到 2ATR；改为
  第一根越线收盘起的 8 根接受窗口内，任一完成 K 的顺势最高/最低价达到当根 2ATR 即可。
- V24 保留两根收盘确认、8 根 EMA576 同侧收盘、确认后盘中严格反向穿越否决、EMA144/576
  原关系保持到信号、next-open、结构止损、0.50R 成本门禁和净 2R 目标。
- L1 无标签结果由 V23 的 123 个恢复到 715 个，新增 592；多空 470/245、44 币、13 月、
  440 个事件，所有预注册覆盖门禁通过。V23 的 123 个身份全部保留。
- 原关系与 8 根接受/盘中保持同时通过的上限集合为 857；V24 保留其中 715（83.4306%），
  说明第 2 根收盘口径确实是此前过度过滤的重要来源。
- WOO、LTC、ACT 虽均满足新 2ATR 窗口距离，仍因原均线关系提前结束被拒绝；旧资格没有复活。
- 12/12 定向测试、Rustfmt 和 2,000 行硬门禁通过；共用文件 1,204 行，仅触发 1,000 行目标告警。
- 机器报告 SHA-256 为
  `e0c3ed91ba38ca6782d2e45bef35461898ca888d08ab2de1daa93f28a329055d`；结论报告 SHA-256 为
  `d69998593340fd3520c345323657433388744ddf77288c315a585e62d8821cb1`。当前停在 L1、
  `l2 = null`，未创建 Pine，未接 Paper/ReadOnly/Live、数据库写入、调度或生产。

## 2026-08-05：EMA576 八根接受窗口极值 2ATR V24 L2 成本后回放

- 独立预注册把 V24 L1 完整文件、payload、715 个合格 candidate ID 集合、多空 470/245、
  V14/V16 源报告与行情指纹全部绑定 SHA；L2 不重新运行 V24 门禁，也不允许等价替换 L1 文件。
- 新入口逐条核对 symbol、方向、setup/breakout/signal 时间及 V16 eligibility 后，715/715 全部
  匹配；L1 SHA 仍为 `e0c3ed...55d`，候选与数据漂移为 0，风险/退出合同复核为 true。
- 下一根连续开盘全部可解析；首笔成交政策阻塞同 setup 后续候选 318 个，实际 397 笔全部具有
  完整 24h 内退出证据，多 248、空 149、44 币、13 月、324 个事件。
- 129 笔净 +2R 目标、268 笔止损；毛结果 `+29.0685R / EV +0.07322R / PF 1.10846`，
  `123.3525R` 成本后变为 `-94.2840R / EV -0.23749R / PF 0.73236`，胜率 32.4937%。
- 多头成本前已经为负，成本后 `-87.9370R / EV -0.35458R / PF 0.62086`；空头毛正但成本后
  `-6.3470R / EV -0.04260R / PF 0.94726`。44 币仅 13 个正、13 月仅 3 个正，负边际广泛分布。
- 总体成本后、多空分别成本后、移除最佳两笔、移除最佳事件四项门禁失败，状态固定为
  `stopped_at_l2_negative_cost_adjusted_edge`。按预注册纪律不调整止盈止损或 2ATR/8 根阈值，
  不进入 L3，不接 Pine/Paper/ReadOnly/Live/worker/调度/生产。
- 13/13 定向测试、Release 回放、Rustfmt、JSON 合同与 2,000 行硬门禁通过；注释复核只补充了
  outcome 解封顺序和候选授权边界，未改变业务逻辑。共用文件 1,494 行，仅触发 1,000 行目标告警。
- L2 机器报告 SHA-256：
  `5fa3323a02a0558d5721b008c59f479e516563a7ab1a42d97f71d6a6557283fa`；结论报告 SHA-256：
  `330fc3de2629337cfb680d835e4057b0ed878ca3925795a8b49f39469e734924`；L2 清单 SHA-256：
  `8a56daf1450e2a74c96bedfaac0b659d450d75a1012bd447a03264626c21aabd`。

## 2026-08-05：严格视觉横盘双向保留突破合同 V1

- 新策略身份为 `strict_visual_consolidation_symmetric_retained_breakout_15m_research_v1`；旧 V1～V9
  未原地覆盖。实现了 50%/15 bps 突破棒、五棒首次确认、25% 越界保留、无量能门禁和多空镜像。
- Rust 状态机定向测试 18/18 通过，量度目标多空意图测试通过；L1/Top60 Release 入口编译运行成功。
- L1 在 43/60 partial 完整成员上得到 15,998 候选，多空 7,857/8,141，覆盖 43 币、13 月、
  4,126 个方向事件；弱离区生命周期全部在一棒内闭环，无旧拒绝路径或未武装强突破。
- L2 严格家族共 15,197 笔：多头零成本已为负，成本后 `EV -0.26152R / PF 0.61689`；
  空头毛 EV 仅 `+0.03058R`，成本后 `EV -0.17231R / PF 0.73041`。43 币成本后全部为负。
- 结论固定为 `stopped_at_l2_negative_cost_adjusted_edge / Research-only`。未修改 Pine，未接入
  Paper、ReadOnly、Live、worker、调度、数据库写入或生产。
- 预注册/L1/L2 SHA-256 分别为 `435a1cd6...ab3`、`34f0267a...663`、`ba2ce3d7...cf0`；
  结论见 `docs/backtest_reports/tradingview_strict_visual_symmetric_retained_breakout_v1_result_20260805.md`。

## 2026-08-05：严格视觉横盘突破棒极值止损 V1

- 新增 Research-only V11：多头在突破棒整根最低价外一 tick 止损，空头在最高价外一 tick；止损在
  突破棒完成时冻结。确认后实际开盘已越过保护位时阻塞入场，不反转止损也不误平已有仓位。
- L1 43/60 partial 无标签复核得到 15,998 候选，多空 7,857/8,141；V10/V11 删除止损字段后的
  候选账本哈希同为 `26208990...f307`，非法/缺失 V11 止损 0，证明信号合同没有漂移。
- L2 严格视觉已平仓 15,260 笔。零成本由 V10 的 `EV -0.01115R / PF 0.97952` 改善为
  `+0.00303R / 1.00507`，但 5bps 手续费 + 3bps 滑点/边后由 `-0.21644R / 0.67239`
  恶化为 `-0.29854R / 0.62703`。
- V11 实际风险中位数为 `1.219 ATR`，5,254 笔低于 `1 ATR`、722 笔低于 `0.5 ATR`；后者平均
  成本侵蚀 `0.91692R`。最窄 ALGO 样本仅 `0.00858 ATR`，成本侵蚀 `16.4152R`。
- V11 停止在 L2，不创建 Pine、不接运行态或生产。预注册、机器报告、结论报告 SHA-256 分别为
  `4fda8d308af1680abcef2f74f552713358e6af4c97989ce66a6c951f09cdfd09`、
  `13717e7a19f985229b195d03b12fbd4ed805d7b85d59fdb2220f29a24be3bfc9`、
  `822ac1db5e6963027a44d84df50f8e10e0856be0df80544060bd75512f47e23e`。

## 2026-08-05：严格视觉横盘突破棒结构止损 + 最小 1ATR 风险 V12

- V12 独立保留 V11 的突破棒极值结构保护，并给实际开仓风险增加确认棒 1ATR 下限；多头取结构
  与 ATR 止损中更低者，空头取更高者。实际开盘越过结构位仍阻塞，ATR 不复活失效信号。
- L1 中 V11/V12 均为 15,998 个候选，多空 7,857/8,141；删除新增 ATR 字段后的候选账本
  SHA 完全一致，信号合同、横盘、回踩、目标和成本没有漂移。
- L2 已平仓 15,238 笔，V11 的 5,254 笔 `<1ATR` 风险降为 0；V12 最小风险
  `1.000002ATR`、中位数 `1.22111ATR`，5,263 笔由 ATR 下限提供更远止损。
- 平均成本拖累从 V11 的 `0.30156R` 降到 `0.23971R`，成本后 EV 改善 `+0.05286R`；但最终仍为
  `EV -0.24568R / PF 0.65780`，多头 `-0.29153R`、空头 `-0.20092R`，无法晋级。
- 账本另暴露 13 笔下一开盘已到达或越过冻结目标；这属于独立入场几何问题，本轮未同时修改目标。
- V12 固定为 `stopped_at_l2_negative_cost_adjusted_edge / Research-only`，不继续搜索 ATR 阈值，
  不进入 L3，不创建 Pine，不接运行态或生产。
- 预注册、机器报告、结论报告 SHA-256 分别为
  `c6564447a332c0c8c039e2ac96ba23745b781551c8b7270bfe3108b97ffcec5a`、
  `f9638cc8d8918de0140fc57b5e3d9d6e8914ceea3bee78772402211502bb7cd4`、
  `98cfad4a316fd6aa33d0fb12519346ebaa17788e0e70217dbefaff37f59292ad`。

## 2026-08-05：EMA576 六根确认 + EMA144±1ATR 结构止损 V26

- V25 将 V24 接受窗口从八根缩短为六根；2ATR 顺势极值、EMA576 盘中保持和关系周期合同不变。
- V26 独立使用多单 `signal EMA144-1ATR14`、空单 `signal EMA144+1ATR14`；不覆盖 V24/V25。
- L1 冻结 1,207 个 V25 质量候选，1ATR 风险与 0.50R 成本门禁保留 1,157 个，多空
  754/403；相对旧 0.30ATR 门禁释放 437 个，旧 720 个合格候选丢失为 0。
- 配对 L2 中，1ATR 把入场 K 止损占比从 35.2313% 降到 12.5683%，60 分钟内止损占比从
  74.7331% 降到 51.0929%；初始风险/ATR 中位数由 0.81862 增至 1.41239。
- 基线成本后 `EV -0.32217R / PF 0.65007`；V26 改善到 `EV -0.19592R / PF 0.76219`，
  但多头与空头仍分别为 `-0.30722R / -0.01828R`，无法形成成本后正边际。
- 状态固定为 `stopped_at_l2_negative_cost_adjusted_edge / Research-only`。V26 L1/L2/结论报告
  SHA-256 分别为 `56e3eb7d...ac478`、`91f95abf...24f05`、`627bb244...41e39`；未接运行态或生产。
- V26 结论已补最近 10 笔完整亏损信号，统一按开仓时间倒序并打印 BJT 日期时间；研究工作流新增
  L2/L3 强制字段合同，机器账本缺失字段必须写 N/A，不得省略或用后续 K 线推测。

## 2026-08-05：V26 TradingView 独立合并与严格对账

- 使用本地 TradingView MCP 创建独立已保存脚本
  `15m EMA576 Six-close Structural Stop V26 Strict Parity Research`，Pine v6 编译无错误；对应本地
  源文件为 `docs/strategy_list/15min_ema576_six_close_structural_stop_1atr_v26_strict_parity_research.pine`。
- Pine 只实现 Research-only 手工审计账本：下一连续 K 开盘、0.50R 成本门禁、EMA144±1ATR
  初始止损、净 2R、入场棒纳入、同棒止损优先和 24h 超时均显式表达；没有 alert、Paper、Live
  或真实订单调用。
- TradingView 编辑器的“新建”不会自动创建独立保存身份，首次操作曾让旧视觉脚本指向 V26
  源码；现已通过“复制脚本”建立独立 ID，并把旧 `15m Strict Visual Consolidation V1` 恢复为
  本地 276 行源码，编辑器与本地逐字一致。旧脚本只因 Basic 指标上限从当前布局移出，仍保留在脚本库。
- Rust V26 最近 10 笔完整成交覆盖 LAYER、ONT、MEW、LTC、NMR、GPS、ACT、WIF、LAYER、
  RENDER。TradingView 的 52/52 个唯一 setup/breakout/rearm/signal/entry/exit 时间均可解析；
  信号收盘 10/10、next-open 入场 10/10 精确相等，8 个止损与 2 个目标的退出棒穿越也全部成立。
- 上述只证明所抽取事件的 OKX 15m 行情层一致，不能替代完整状态机 parity。Rust 冻结加载起点
  `2025-06-21 17:15 UTC` 超出当前 Basic 图表历史深度，实际最早为 `2026-07-11 23:00 UTC`；
  V26 因而显示“冻结起点未命中 / 严格数据未就绪”并输出 0 信号，这是预期安全门禁。
- 当前状态仍为 `stopped_at_l2_negative_cost_adjusted_edge / Research-only`；未 promote，未接
  Paper/ReadOnly/Live、worker、调度、数据库写入或生产。

## 2026-08-05：V12 严格视觉横盘突破 TradingView 图表审计版

- 用户在 V12 已停止于 L2 后明确要求把该策略合并到 TradingView；因此创建独立 Research-only
  Pine，而不是改变既有研究晋级结论。源码为
  `docs/strategy_list/15min_strict_visual_consolidation_breakout_v12_strict_parity_research.pine`。
- 图表脚本复刻 8/12/16/24/32/48/64/96 横盘窗口、P90/P10 边界、弱离区观察、50% 实体占比、
  15bps 方向实体、1～5 根首次接受、25% 越界保留、突破棒极值外一 tick 结构保护、最小
  1ATR 风险、多空镜像和一个冻结区间高度目标；没有量能门禁、告警或订单 API。
- 已通过本地 TradingView 保存为独立脚本
  `15m Strict Visual Consolidation Breakout V12 Strict Parity Research`，Pine v6 编译 0 错误；
  本地 746 行源码与编辑器除 CRLF 外逐字一致。
- Basic 套餐当前只能同时加载两个指标；当前 BTCUSDT.P 15m 图表为 V20 + V12。V26 仅从当前
  布局移除，已保存脚本和本地源码仍完整保留。
- 当前可见历史只能运行 `V12_DIAG`：确认横盘 113、强突破 77、确认/入场/退出各 46，退出分布
  为止损 30、目标 15、反转 1，每边 8bps 后净值 `-20.97998R`。严格加载起点 `2025-05-02`
  不可见，因此这些数字只用于图形审计，不作为新的严格回测或完整 parity 结论。
- Rust V12 的冻结 L2 结论仍为 15,238 笔、成本后 `EV -0.24568R / PF 0.65780`，状态保持
  `stopped_at_l2_negative_cost_adjusted_edge / Research-only`；未接 Paper/ReadOnly/Live、worker、
  调度、数据库或生产。23 个 `strict_visual` 定向测试、Pine 行数门禁和源码差异检查均通过。
