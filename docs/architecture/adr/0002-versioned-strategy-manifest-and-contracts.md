# ADR-0002：分离策略定义、研究证据、发布与跨进程合同

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
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

显式管理 Research、Paper、Shadow、Canary、Live、Retired 状态及 promote、rollback、pause、retire 记录。Release 是有审计的状态机，不嵌入不可变 Definition。

### StrategyRuntimeSnapshot

数据面实际消费的不可变快照，固定：

- Definition 与参数；
- Portfolio/Risk policy 版本；
- Contract 版本；
- Release generation、有效期与 kill-switch generation。

控制面发布 Snapshot；数据面不在热路径临时组合定义、配置和默认值。

### 显式编译期 Registry

内置策略通过明确列表注册 Definition 与 factory。不通过大小写、去分隔符、去版本号或 JSON 猜测策略身份。不引入动态链接库或隐式自动注册。

### 版本化 Wire Contract

Strategy Signal、Portfolio Target、Risk Decision、Execution Request、Order Intent、Order Event、Fill Event、Readiness 与 Catalog Sync 等跨进程结构必须有明确 owner/version。

Domain 不依赖 Wire Contract。App/入站 Adapter 显式完成：

```text
Wire Contract <-> Use Case Input/Output
```

Signal 至少携带 strategy/definition version、definition hash、instrument、timeframe、observed time 和 evidence cutoff。订单链路保持稳定 event/correlation/causation/idempotency/aggregate/sequence identity。

### 跨仓库 Owner

- Core Strategy 拥有 Definition、StrategyArtifact 技术身份、Release 和 Runtime Snapshot；
- Core Research 拥有 Experiment、BacktestRun、DatasetManifest、Checkpoint 和 ResearchEvidence；
- `quant_web` 拥有产品标题、营销描述、订阅可见性、会员/combo 和用户配置；
- Web 的执行资格交接使用 `ExecutionRequest` Contract，不把产品事实写入 Core 策略定义；
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
