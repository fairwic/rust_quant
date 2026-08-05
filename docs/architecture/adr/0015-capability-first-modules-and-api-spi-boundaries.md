# ADR-0015：采用 capability-first 模块、API/SPI 双门面与 Port 完整性门禁

- 状态：已接受
- 日期：2026-07-29
- 决策者：Rust Quant Core
- 上位文档：[长期目标架构](../target-architecture.md)、[依赖与代码归属规则](../dependency-rules.md)、[AI 架构迁移执行协议](../ai-migration-execution-protocol.md)
- 当前解释：capability-first、API/SPI、Port 完整性和文件预算继续有效；逐切片 Manifest/Verdict 流程由 [ADR-0017](0017-capability-catalog-and-domain-wave-migration.md) 取代

## 背景

Owner、crate 和五类物理目录已经解决“业务属于谁”和“依赖向哪里流”的一级边界，但仍不足以约束 Domain/Adapter 内部持续增长。若每个 Domain 都只按 `model/`、`use_cases/`、`ports/` 横向分层，容易出现：

- 同一业务能力被拆散到多个横向大目录，维护者必须跨目录拼接完整链路；
- `model.rs`、`types.rs`、`enums.rs`、provider 级 `okx.rs` 持续吸收不相关定义；
- Domain 根门面把 Model、Use Case、Port 全部重导出，其他 Domain 和 Adapter 看到同一张过宽 API；
- 先写 Trait 和 Fake、后补业务的“空 Port 架构”，长期没有生产 Adapter；
- Use Case 持有越来越多 Port，重新变成万能 Service；
- App、Outbox publisher、Reconciliation 和数据库 Adapter 之间没有明确恢复责任；
- 文件虽然没有达到仓库 2000 行硬上限，但在 600～1000 行时已经失去单一修改理由。

目标不是追求更多目录，而是让一个真实业务能力能够被局部理解、局部验证和局部替换，并让机器在代码变成大文件之前阻止继续堆积。

## 决策

### 1. Domain 内部采用 capability-first

Domain 的一级内部目录先按业务能力或子领域拆分，再在能力内部按需要建立 Model、Policy、Use Case 和 Port：

```text
crates/domains/<owner>/src/
├── api.rs | api/                    # 消费方稳定门面
├── spi.rs | spi/                    # Adapter/组合根门面
├── <capability-a>/
│   ├── model/
│   ├── policies/
│   ├── commands/
│   ├── queries/
│   ├── consumers/
│   └── ports/
├── <capability-b>/
└── lib.rs
```

目录只在存在真实代码时创建。某个能力只有一个模型和一个查询时，可以直接使用 `<capability>/model.rs`、`<capability>/query.rs`，不为满足图示创建空目录。

能力边界以业务语言和独立变化原因命名，不以技术层、provider 或任意文件大小命名。当前建议的首批边界是：

- Market：`reference`、`stream/bars`，后续按真实能力增加 `trades`、`order_book`、`funding`；
- Strategy：`evaluation`、`signal_handoff`、`release`；
- Account：`projection`、`exchange_session`、`admission`；
- Execution：`planning`、`order_lifecycle`、`mutation`、`protection`、`safety_obligation`；
- Reconciliation：按检测到的差异与发往原 Owner 的恢复命令组织，不建立直接修表能力。

以上是导航约束，不授权提前创建未实施能力。

### 2. Domain 只有 API 与 SPI 两个公共门面

- `api` 是其他 Domain、Research、Quant App 业务调用方可见的稳定门面，只暴露业务输入、输出、只读值、明确的业务动作和稳定错误；
- `spi` 是 Adapter 与 App 组合根可见的实现门面，只暴露消费方定义的 Port、Adapter 构造所需的强类型输入和必要装配入口；
- 其他 Domain、Research 和普通业务调用方只能导入 `<domain>::api`，不得导入 `<domain>::spi` 或私有 capability module；
- Adapter 只能导入其实现对象所属 Domain 的 `<domain>::spi`；不得导入该 Domain 私有 `model`、`use_cases` 或 sibling capability；
- App 只允许在已登记的运行装配入口中导入 `spi` 完成装配，Handler、Consumer、Scheduler loop 只能调用 `api`；
- `lib.rs` 只公开 `api` 与 `spi`，不从 crate 根重导出 Model、Port、Adapter 或 Wire DTO；
- 接收 Port 的构造函数属于 `spi`/装配边界。稳定 `api` 的函数签名不得泄漏 Port Trait、数据库类型、SDK 类型或 Adapter 配置。

`api.rs`、`spi.rs`、`lib.rs` 和 `mod.rs` 是门面/导航文件，不是业务实现文件。

### 3. Port 必须由完整业务闭环证明

非测试 Port 只有同时具备以下证据时，所属 capability 才可进入 `implemented`：

1. 有一个明确 Owner 的生产 Use Case 实际调用它；
2. 方法名表达业务动作或业务查询，而不是技术 CRUD；
3. 至少有一个非测试 Adapter 实现；
4. 失败分类、幂等/原子性、超时和恢复 Owner 已写入用例测试或 Adapter/恢复测试；
5. Port 只包含当前调用方需要的方法。

Fake、Mock 或测试替身只能证明可测试性，不能证明生产边界存在。只有测试实现的 Port 可以在 capability 为 `implementing` 时短暂存在，但必须记录同一 Wave 缺失的 Adapter 和完成条件；该状态不得进入 `implemented` 或被其他能力当作已完成依赖。

纯 Policy、单一确定性算法或“以后可能替换”的实现不创建 Port/Trait。只有真实生产多实现、进程边界适配或外部副作用隔离需要 Trait；测试替身本身不是创建 Trait 的理由。

### 4. Use Case 以业务动词、原子结果和恢复 Owner 为边界

一个公开 Use Case 只表达一个业务动词、一个主要结果和一个明确恢复 Owner。读取、校验、状态转换、同 Owner 原子写和 Outbox 可以属于同一个 Use Case；第二个独立业务结果、第二个事务提交、等待外部 receipt 或跨 Owner 状态机必须拆成后续 Use Case/Consumer 或该 Owner 的 durable process manager。

出现以下任一情况必须在 Review 中阻塞并重新说明边界：

- 一个 Use Case 注入四个及以上有副作用 Port；
- 使用 `EverythingPort`、`Context`、`Services` 等组合对象隐藏 Port 数量；
- Use Case 同时拥有两个可独立重试、独立补偿或独立审计的业务结果；
- App callback 决定重试、补偿或状态迁移，而原 Owner 没有恢复用例。

该规则不以机械拆小函数代替业务内聚；同一原子动作需要多个读取 Port 时可以保留，但必须解释为何仍是一个业务结果。

### 5. Enum、错误和表示类型按语义放置

禁止建立 Domain 级全局 `enums.rs`、`types.rs`、`common.rs` 或 `shared.rs` 收纳不相关定义。

- Aggregate/Value Object 状态枚举与其不变量放在同一 capability 的 `model`；
- Command/Query 的选择枚举放在对应用例输入附近；
- Port 的技术失败分类放在该 Port 附近，并映射为稳定 Domain 错误；
- 跨进程枚举放在 owner 的版本化 Contract；
- 数据库枚举/Row 表示保持在 owner-scoped Postgres Adapter 私有模块；
- 交易所 wire 枚举保持在 `crypto_exc_all` 对应官方 API domain，Gateway 显式映射；
- `api`/`spi` 只重导出经过审查的类型，不复制或重新定义另一套枚举。

同名概念若具有不同 owner 或不同状态机，使用不同类型并显式映射，不通过一个“共享枚举”抹平语义。

### 6. Adapter 先按 capability，再按 provider

Adapter crate 的一级目录先表达安全能力和依赖方向，再表达交易所/provider：

```text
crates/adapters/exchange-gateway/src/
├── public_market/
│   └── okx/
│       ├── source.rs
│       ├── request.rs
│       ├── candle_mapper.rs
│       └── error.rs
├── private_account/
└── fenced_mutation/
```

公共行情、用户私有只读和 fenced mutation 不得因为同属 OKX 被重新合并进一个 provider 大文件。请求构造、DTO 映射、错误归一、源实现和测试按独立变化原因拆分；业务 finality、风险和执行门禁仍留在对应 Domain。

### 7. Outbox、幂等、事务和恢复责任固定

| 责任 | 唯一位置 |
| --- | --- |
| 业务事件/命令含义、幂等 identity、何时需要重试或补偿 | 原 Owner Model/Use Case |
| State + Inbox/幂等 + Outbox + Audit 的原子写集 | 原 Owner Write Port 定义，owner-scoped Postgres Adapter 实现 |
| 通用轮询、投递、Ack、退避和 transport telemetry | Messaging/Postgres Adapter 或 Platform 机制 |
| 配置、依赖注入、循环监督、graceful shutdown | App |
| 失败后的业务恢复决定 | 原 Owner Recovery Use Case |
| 跨 Owner 差异检测 | Reconciliation；只发送 typed owner command，不直接修表 |

通用 Outbox publisher 不拥有业务重试决策；App 不通过 `match error` 临时发明状态机；Adapter 不根据 SQL/HTTP 错误自行改变业务 identity。

### 8. 文件预算在“大文件形成前”生效

目标仓库新增或触碰的 Rust 文件采用以下预算：

| 范围 | Warning | Error |
| --- | ---: | ---: |
| Domain/Adapter `src` 生产代码行 | `> 400` | `> 600` |
| 任意 Rust 文件总行数 | 无 | `> 1000` |
| `lib.rs`、`mod.rs`、`api.rs`、`spi.rs` 总行数 | `> 100` | `> 150` |
| 独立测试文件或 `tests.rs` | `> 250` | `> 500` |

生产代码行不计 `#[cfg(test)]` 测试模块；总行数仍包含测试。生成代码只能通过 role map 中精确登记的生成路径豁免，禁止用宽 glob 排除业务源码。门面文件只允许模块声明、文档、稳定 re-export 和极薄装配类型；出现业务分支、SQL、SDK 映射或大段测试即使未超行数也属于违规。

触碰已超过 Error 的目标文件时，必须先在同一 capability 内按真实职责拆分并固定行为，再增加新业务。不能通过把单个大文件拆成多个无业务边界的 `part1.rs`/`helpers.rs`，也不能把测试全部移出以掩盖生产大文件。

## 迁移顺序

本决策不能通过一个跨 Owner 的大批次同时修改治理工具、Market、Strategy 和 Adapter：

1. 先提交本 ADR 与规范性文档，形成新的 `rust_quant` governance baseline；
2. Strategy 的 `MIG-MVE-A1R-REMOVE-FAKE-ONLY-PORT-V1` 先以调用点闭包证明没有生产 Adapter/入口，再从生产编译面移除当前 Fake-only signal-handoff Port/Use Case；blocked A1 的 Contract/边界发现记录保留给未来 successor；
3. Architecture Governance 的 `MIG-20260729-MODULE-BOUNDARY-GUARDRAILS-P0-1` 只在 `rust_quant_alpha` 修改 `xtask`/role map/注入式门禁；
4. Market 的 `MIG-MKT-F2R-CAPABILITY-API-SPI-V1` 拆分 canonical bar、public Kline API/SPI 与 exchange-gateway public-market/OKX，保持输出、错误、Decimal、时间和请求语义不变；
5. 上述结构门闭合后再进入 Market storage F3A，避免在已知大文件和过宽门面上继续叠加数据库职责。

任何步骤都不得把未提交工作树当成 legacy 基线，也不得因 CI 延后把 `implementing` 写成 `implemented`。

## 后果

### 正面影响

- 维护者可以沿 capability 阅读完整业务链路；
- 其他 Domain、Adapter 和 App 的可见面被物理分开；
- Port、Use Case、Outbox 和恢复责任能被逐 capability 和 Domain Wave 验收；
- provider/枚举/错误不再自然汇聚成单文件；
- 大文件在 600 行生产代码前触发拆分，而不是等到仓库 2000 行硬上限。

### 代价

- Rust re-export 和装配入口需要更明确；
- 初次拆分会产生纯结构迁移和路径调整；
- 部分当前只有测试 Fake 的 Port 会被明确标记为未完成，而不能继续冒充已落地能力；
- 静态门禁只能证明结构和依赖，Port 的业务完整性仍需调用点、生产 Adapter、测试和 Wave Evidence 共同证明。

## 验收条件

1. `target-architecture.md`、`dependency-rules.md`、`business-code-and-data-access.md`、AI Guardrails 与架构 Skill 使用同一套 capability/API/SPI 术语；
2. `arch-check` 有注入式失败测试覆盖文件预算、API/SPI 越界、空 Port/Fake-only Port 登记和 façade 规则；
3. Market 与 public-market/OKX 的纯结构拆分有同输入同输出 characterization；
4. 其他 Domain 只能依赖目标 Domain `api`，Adapter 只能依赖目标 Domain `spi`，App 的 `spi` 依赖只出现在组合根；
5. 新增 Port 所属 capability 在进入 `implemented` 前具备真实 Use Case 消费方、生产 Adapter 和失败/恢复证据；
6. F3 不在已知超限文件、过宽 crate 根 API 或未完成 Port 之上继续实施。
