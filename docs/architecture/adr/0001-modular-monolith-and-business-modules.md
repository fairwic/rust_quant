# ADR-0001：采用模块化单体与五类物理目录

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
- 决策者：Rust Quant Core

## 背景

当前 Workspace 同时存在 `common`、`core`、`domain`、`infrastructure`、`services`、`market`、`strategies`、`risk`、`execution`、`orchestration` 等横向层和业务 crate。`services`、`orchestration` 与 `infrastructure` 可以依赖大量模块，导致新增代码很容易进入大而模糊的容器。

第一版目标把 Market、Strategy、Portfolio、Account、Risk、Execution、Operations 直接平铺在 `crates/`。业务 owner 比现状清楚，但业务、量化通用能力、Contract、外部 Adapter 和进程基础能力仍混在同一级；同时每个 owner 一个 Postgres crate 会在尚无独立发布需求时制造过多包和装配成本。

立即拆微服务会提前引入网络耦合、分布式一致性和部署复杂度，不能解决代码 owner 不清的问题。

## 决策

采用模块化单体，并在 `crates/` 下使用五类物理分区：

```text
domains/     Market、Strategy、Portfolio、Account、Risk、Execution、Reconciliation、Research
quant/       Math、Indicators、Backtest Kernel、Analytics
contracts/   按 owner/version 组织的跨进程合同
adapters/    Postgres、Exchange、HTTP、Redis、对象存储、通知
platform/    Kernel、Messaging、Lifecycle、Observability、Security、Testkit
```

进程组合根放在 `apps/`，数据库迁移放在单一有序 `migrations/`。

### Domain 内部结构

```text
model/       实体、值对象、状态机和不变量
policies/    确定性纯决策
use_cases/   commands、queries、consumers
ports/       本 Domain 需要的外部能力
api/         允许其他 Domain 使用的稳定进程内 API
```

### Reconciliation 命名

使用 `Reconciliation` 取代含义过宽的 `Operations`：

- Reconciliation 只拥有差异、恢复任务和处置证据；
- 它通过 typed command 请求 Execution、Account、Risk 等 owner 恢复；
- 日志、通知、审计传输和通用运维工具不进入该 Domain。

### Adapter 粒度

- Postgres 默认是一个 crate，内部按 owner module 隔离；
- Contracts 默认是一个 crate，内部按 owner/version 隔离；
- 只有编译隔离、重依赖、独立 owner 或独立发布证据出现时才拆更多 crate；
- Exchange Gateway 包装 `crypto_exc_all`，不复制交易所 SDK。

## 依赖方向

```text
App -> Use Case/API + Adapter + Contract mapping + Platform
Adapter -> Domain Port/Model + External SDK
Production Use Case -> 本 Domain Model/Policy/Port + 上游 Domain API + Quant Math/Indicators
Research Use Case -> Production Domain 稳定 API + Quant Backtest/Analytics
Model/Policy -> Kernel 或批准的 Quant 纯计算 API
Quant -> Kernel/自身纯计算依赖，不依赖业务 Domain
Contract -> Wire primitives
```

Research 是终端离线 Domain，负责实验编排与证据；`quant/backtest` 和 `quant/analytics` 只提供 owner 无关的确定性机制，不直接编排业务 Domain。Domain 内部不依赖 Wire Contract；映射发生在 App 或入站 Adapter。禁止业务代码依赖具体 Adapter，也禁止跨 owner SQL。最终依赖由 [ADR-0009](0009-research-domain-and-tiered-simulation.md) 固定。

## 结果

### 正面影响

- 物理目录直接区分业务、纯量化、合同、外部实现和平台能力；
- AI 不能再把所有内容默认写进 `services/common/infrastructure`；
- Strategy、Portfolio、Account、Risk、Execution、Research owner 清晰；
- Reconciliation 不再成为运维杂物筐；
- Postgres 与 Contract 不会因 owner 数量产生无必要 crate 爆炸；
- 将来可以按证据从模块化单体提取进程或服务。

### 代价

- 边界需要显式 Input/Output、Port 和映射；
- 迁移期间新旧目录会并存；
- 单个 Postgres/Contracts crate 需要可见性和架构门禁防止 owner 穿透；
- 必须维护依赖检查和 golden templates。

## 被否决的方案

### 继续使用扁平业务 crate

比现状清楚，但不能从物理导航上区分 Domain、Quant、Contract、Adapter 与 Platform。

### 每个 Owner 一个 Postgres crate

在当前阶段增加 Cargo、装配和迁移成本。先用一个 crate + owner module；需要真实隔离时再拆。

### 保持通用四层 crate

无法解决 `services/infrastructure/orchestration` 持续膨胀和 owner 不清。

### 一次性重写

难以证明策略、实盘门禁、部署命令和跨仓库合同没有漂移。

### 立即微服务化

当前主要矛盾是代码边界，不是独立扩缩容；微服务会把本地耦合变成网络耦合。

### 每个策略一个 crate 或 Worker

会造成 crate 与部署单元爆炸。策略默认是 Strategy Domain 内的 module。

## 兼容与迁移

- 现有 crate 暂时保留，新增能力优先进入目标目录；
- 小型 legacy bugfix 可以原地修改，但不能扩大旧依赖；
- 使用 Golden Vertical Slice 和 CI ratchet 逐步迁移；
- 旧入口只有在调用方、Contract、release、恢复和生产证据全部迁移后才能删除。
