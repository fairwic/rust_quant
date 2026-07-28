# ADR-0002：分离策略定义、研究证据、发布与跨进程合同

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-28
- 决策者：Rust Quant Core

## 背景

当前策略身份、注册、支持范围、运行参数、Catalog 和 Signal payload 分散在枚举、Registry、CLI、Service、环境变量和数据库配置中。第一版“不可变 Strategy Manifest”又同时放入生命周期、数据指纹、研究证据和运行能力，混合了不可变定义、评估产物与可变发布状态。

策略进入 paper、shadow、canary 或 live 后，任何原地覆盖都会使历史信号、回测结果、执行请求和生产行为无法准确追溯。

## 决策

### StrategyDefinition

不可变技术定义，至少声明：

- strategy key、version、entry rule version；
- 参数 schema、输入数据要求和输出语义；
- 支持的交易所、instrument、timeframe 和方向；
- 执行能力、保护能力与兼容的 policy contract；
- definition hash 与规范序列化版本。

### StrategyArtifact

Strategy 拥有的不可变可执行技术产物，至少包含：

- Definition identity 与构建输入；
- 代码 revision、编译/模型 artifact hash；
- 参数 schema 与兼容能力；
- 规范序列化和供应链身份。

### ResearchEvidence

Research Domain 拥有的不可变实验与验证事实，至少包含：

- Definition identity；
- 数据指纹、Universe 版本、样本和 evidence cutoff；
- 费用、滑点、资金费与仓位模型；
- 代码 revision、回测/评估结果和集中度证据；
- promote 建议或拒绝理由。

StrategyArtifact 和 ResearchEvidence 都不决定当前是否上线，也不修改 live pointer。Release 只能引用状态为 Completed、内容引用完整且满足门禁的 ResearchEvidence。

### StrategyRelease

Strategy owner 显式管理 Research、Paper、Shadow、Canary、Live、Retired 状态及 promote、rollback、pause、retire 记录。Release 是有审计的状态机，不嵌入不可变 Definition，也不等于“当前运行范围选择了它”。`Research` 只表示 Research owner 已完成的证据尚未获得运行资格，不能被任何 ActivationPointer 选用。

### ActivationPointer 与 KillSwitchSnapshot

Control owner 管理可变 `ActivationPointer`：它只选择某个已发布 `StrategyRuntimeSnapshot` 在明确 activation scope（例如 `strategy_key + deployment_channel`）中的当前引用，并为该 scope 维护单调 `activation_generation`。Strategy 不能通过修改 Release 自行激活，Control 也不能修改 StrategyRelease 的生命周期。

Strategy owner 还必须发布不可变 `ActivationEligibilityV1`，作为 Control 写 Pointer 的唯一资格输入。它绑定 `runtime_snapshot_id/hash`、`strategy_release_id/generation`、Completed ResearchEvidence 引用、eligible channel 集、eligibility generation 与 revoked 标记。固定 channel×stage 规则为：`Research -> none`、`Paper -> paper`、`Shadow -> shadow`、`Canary -> canary`、`Live -> live`、`Retired -> none`。Control 只能验证并审计该资格，不能自行从可变 Release 状态推断；资格撤销或 Retired 必须触发新的 Control catalog generation，使旧 Pointer 不再被数据面接受。

Control 还拥有 `KillSwitchSnapshot`。它使用独立的全局 `kill_switch_catalog_generation` 与每个 scope 的 `scope_generation`；不得复用 activation/release generation。RiskAction、Account 失败或 Web 商业停用只能提交 typed request，不能绕过 Control 直接写 switch。Kill Switch 只阻断新增风险，保护、reduce-only 和恢复仍经各 owner 状态机。

### StrategyRuntimeSnapshot

数据面实际消费的不可变快照，固定：

- Definition/Artifact identity 与 Strategy entry/exit 参数；
- evaluator state schema、输入要求与策略能力；
- compatible policy/Contract schema 范围；
- 所属 StrategyRelease identity/version；
- 不包含 ActivationPointer、activation generation、Kill Switch 或其 generation。

Strategy 发布 RuntimeSnapshot 与独立的 ActivationEligibility；Control 发布指向其的 ActivationPointer/KillSwitchSnapshot。数据面本地读取已发布控制快照，不在热路径临时组合定义、配置和默认值，并拒绝 channel、eligibility generation 或 runtime snapshot hash 不匹配的 Pointer。

`StrategyRuntimeSnapshot` 不拥有或内嵌 Portfolio、Risk、Execution policy，也不包含 account、user、credential 或 risk profile。各 Domain 分别发布自己的 Policy Snapshot，Execution 通过 `ExecutionDecisionContextSnapshot` 绑定某次请求实际使用的四个稳定引用；完整规则见 [ADR-0011](0011-layered-runtime-snapshots-and-decision-context.md)。

### 显式编译期 Registry

内置策略通过明确列表注册 Definition 与 factory。不通过大小写、去分隔符、去版本号或 JSON 猜测策略身份。不引入动态链接库或隐式自动注册。

### 版本化 Wire Contract

Strategy Signal、Portfolio Target、Risk Decision、Execution Request、Order Intent、live Execution Plan、Order Event、Fill Event、Readiness、ActivationEligibility 与 Catalog Sync 等跨进程结构必须有明确 owner/version。纯 `ExecutionPlanningValue` 是进程内/Research parity 值，不是独立 OMS Contract；只有 live aggregate 的跨进程状态需要 Wire Contract。

Domain 不依赖 Wire Contract。App/入站 Adapter 显式完成：

```text
Wire Contract <-> Use Case Input/Output
```

Signal 至少携带 strategy/definition version、definition hash、instrument、timeframe、observed time 和 evidence cutoff。订单链路保持稳定 event/correlation/causation/idempotency/aggregate/sequence identity。

业务 payload 随其事实/command owner 的仓库发布：Core `crates/contracts` 只保存 Core owner payload；Web/News payload 由其 owner 发布，Core 只能在 Adapter 使用固定版本 binding。所有 wire body 都把 owner-neutral `ContractEnvelopeV1`（版本、message/correlation/causation/idempotency、aggregate/sequence、partition、transport time）与业务 payload 分开；Envelope 不承载 credential、风险配置、订单或策略等业务字段。两层分别执行 golden serialization 和 N/N-1 兼容测试。

### 跨仓库 Owner

- Core Strategy 拥有 Definition、StrategyArtifact 技术身份、Release 和 Runtime Snapshot；Control 拥有 ActivationPointer、KillSwitchSnapshot 与其各自 generation；
- Core Research 拥有 Experiment、BacktestRun、DatasetManifest、Checkpoint 和 ResearchEvidence；
- `quant_web` 拥有产品标题、营销描述、订阅可见性、会员/combo 和用户配置；
- Web 是 canonical `ExecutionRequest` 的唯一 creator。执行资格交接使用 Web owner Contract：Core 通过 `ClaimExecutionRequestV1`/`RenewExecutionRequestClaimV1`/`ReleaseExecutionRequestClaimV1`/`ReportExecutionRequestOutcomeV1` 与 Web 协作，不读取或轮询 Web `execution_tasks`；不把产品事实写入 Core 策略定义；
- Core 的订单结果通过 Core Contract/API 投影给 Web，不把 Web 结果表作为交易事实源。

## 结果

### 正面影响

- 不可变定义、技术 Artifact、研究证据和可变发布状态不再混淆；
- 每次运行可追溯到完整不可变 Snapshot；
- 历史信号、回测和订单不会因 promote/rollback 被覆盖；
- Web/Admin 消费稳定 Contract，不依赖策略内部类型；
- 回测、paper 和 live 共享相同版本身份。

### 代价

- 需要维护 Strategy 与 Research 两个 owner 间的引用完整性；
- 旧 Manifest/payload 需要边界兼容 Adapter；
- Core 与 Web 的 Catalog/Release 投影需要明确同步；
- hash 需要规范序列化与 snapshot 测试。

## 被否决的方案

### 一个 Manifest 承载所有内容

会把不可变定义、研究证据和可变 lifecycle 混合，难以表达 promote/rollback 与历史不可变性。

### 只使用数据库配置版本

不能替代代码 revision、数据指纹、输出 Contract 和执行能力声明。

### 动态插件自动注册

降低可审计性并扩大供应链和进程安全风险；未来用户自定义策略需另立沙箱 ADR。

### 原版本原地覆盖

破坏历史证据与回滚能力，禁止采用。

## 兼容与迁移

- 先从现有 RuntimeManifest/配置提取 Definition 和 StrategyArtifact，不改变策略行为；
- 从现有 backtest/progress/audit 表提取 Research Experiment、Run 和 Evidence identity；
- Release 与 Runtime Snapshot 通过新增表/Contract 建立；
- 旧 Manifest/payload 在边界映射，并有真实调用方和删除期限；
- Registry、Catalog、Signal builder 与 Worker 全部切换后停止旧版本输出。
