# AI 架构迁移执行协议

- 状态：已接受
- 首次接受：2026-07-23
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 通用护栏：[AI 编码与架构防腐护栏](ai-coding-guardrails.md)
- 阶段计划：[Rust Quant 架构迁移计划](migration-plan.md)
- Manifest 模板：[Migration Manifest TOML](migrations/_template.toml)
- Evidence 模板：[Migration Evidence](migrations/_evidence-template.md)

## 1. 目的

本协议控制 AI 如何把 legacy 实现迁入目标架构。它不重新定义业务边界，而是把已接受 ADR、目标架构、依赖规则和迁移计划转换成每个迁移切片都必须满足的执行合同。

单纯要求 AI“阅读并遵守文档”不构成控制。每次迁移必须同时具备：

1. 版本化 `Migration Manifest`；
2. 可重复执行的基线和验证证据；
3. 机器门禁产生的 Migration Verdict；
4. 对行为变化、事实源切换和生产动作的独立授权。

在目标门禁尚未实现前，只能称为“协议已接受、门禁计划中”，不得宣称迁移已经自动受控。

## 2. 适用范围

以下工作必须执行本协议：

- legacy 目录、crate、module 或运行入口迁移；
- Owner、Port、Adapter、事务或事实源调整；
- Strategy/Portfolio/Risk/Execution/Research 共享逻辑收敛；
- Backtest/Paper/Live 双实现合并；
- App Cargo package、Release Unit 或生产工件拆分；
- Contract、数据库写入方、配置快照或状态机迁移；
- shadow、cutover、rollback 和 legacy 删除。

普通局部 bugfix、只新增纯函数或不改变架构边界的单文件修改不要求创建 Migration Manifest，但仍遵守通用 AI 护栏。

## 3. 权威顺序与基线锁定

迁移执行时的权威顺序为：

1. 系统、用户、仓库安全与生产授权规则；
2. 已接受 ADR；
3. 目标架构；
4. 依赖、代码放置、生产运行和数据访问规范；
5. 本协议；
6. 已批准的 Migration Manifest；
7. 迁移计划、模板和示例；
8. 当前 legacy 实现。

Manifest 必须固定：

- `architecture_baseline_git_sha`；
- 本切片采用的 ADR；
- 规范性文档路径及其内容 hash；
- Release Unit 及其 manifest hash；
- 相关 Contract/Schema snapshot 的版本、路径及内容 hash；
- 当前行为 fixture、DatasetManifest、动态输入与输出工件的不可变 identity/hash；
- 被测试代码的 commit SHA；若测试基于未提交补丁，还必须固定该范围补丁的 hash。

用于正式迁移的架构基线必须来自已提交 revision。工作树中未提交的规范性文档不能被 AI 自行视为已批准基线。

如果代码与文档冲突：

- legacy 代码违反已接受决策：迁移代码，不修改目标规则迁就 legacy；
- 新需求确实推翻旧决策：停止当前切片，先提交替代 ADR；
- 无法判断哪一方正确：进入 `blocked`，请求明确选择；
- 禁止 AI 在实现后修改规范文档，为已经写出的代码补理由。

## 4. 三个强制控制工件

### 4.1 父迁移计划与 Owner 子 Manifest

跨 Owner 的迁移先在父迁移计划中建立一个 `migration_program_id`，并在 [`migrations/programs/registry.toml`](migrations/programs/registry.toml) 登记机器可读的 Program。Program registry 必须列出参与仓库、每个 Owner 子 Manifest 的 ID/owner repository/`depends_on`/创建状态，以及每个跨仓库 Contract 的 version、方向、snapshot object、字段和兼容窗口。父计划只负责编排业务结果、子 Manifest、依赖图、共享 Contract 及版本、兼容窗口和切换顺序；父计划没有代码或表 Owner，也不是可以混合修改多个 Owner 代码的“大 Manifest”。

Registry 只声明依赖边，不能保存一个可人工改成 `true` 的 `depends_on_satisfied` 缓存作为授权。`migration-check` 必须读取每个 predecessor 的不可变 Manifest hash、Evidence hash、当前 revision 机器 Verdict 及其 `verdict_schema_version`，确认状态/范围/兼容窗口满足后计算依赖是否闭合；任一引用缺失、hash 漂移、Verdict 非 current revision 或仍是 historical/not-created 时 fail closed。

跨仓库 child 创建采用两阶段登记：父 Registry 先以 `not_created` 提交并冻结 child identity、目标 owner repository、artifact path 与 `depends_on`，目标 Manifest 的 `registry_ref` 钉住这次 registration revision；目标 Manifest/Evidence 提交后，Registry 再以 `created` 记录其内容 hash。Manifest 不回写去追逐这个观察性 Registry revision，否则会形成循环 hash。Checker 必须同时验证 registration revision 与当前 Registry：identity/owner/path/依赖不得被改写，当前 Registry 记录的 Manifest/Evidence hash 必须匹配目标仓库，依赖仍按当前 Verdict 计算。

#### 延后受跟踪 CI 时的实施输入门

“可以作为后续源码实施输入”不等于 `depends_on` 已满足。只有父迁移计划明确记录用户决定延后受跟踪 CI，且同时满足以下条件时，successor 才可在 predecessor 尚无 current-revision `pass` Verdict 的情况下进入 `implementing`：

1. predecessor 属于当前迁移、已提交、Registry 状态为 `created`，Manifest/Evidence hash 与当前内容匹配，且 Evidence 已记录要求的本地验证；`not_created`、`draft`、`blocked` 和 `historical_record` 均不合格；
2. successor 的 `dynamic_input_artifacts` 钉住 predecessor 的代码 revision、Manifest/Evidence hash，以及实际消费的公开 API、schema 或 `Cargo.lock` hash；`predecessor_verdicts` 必须保持为空，并在 Evidence 中明确记录依赖尚未满足；
3. 只允许实现目标仓库源码、schema、纯/离线逻辑、公共只读 Adapter 和 disposable integration test；禁止部署 App/runtime、切换事实源、删除 legacy、执行生产数据库写入、触发跨 Owner 副作用或交易所 mutation；
4. successor 最高只能停在 `implementing`；在 predecessor 获得 current-revision `pass` Verdict 前，不得进入 `verified`、`ready`、`completed`，不得生成 `pass` Verdict 或声明可切换；
5. predecessor 任一被钉住的 revision/hash 漂移，必须使 descendant Evidence 失效，并重新执行受影响的本地验证与后续 CI。

该门只解决“CI 延后期间能否继续编写有依赖的目标源码”，不改变 DAG，也不把本地测试冒充依赖闭合。若父计划未显式启用该门，仍按正常规则保持 `blocked`。

每个可实施的子 Manifest 必须只有一个 `scope.owner`。`secondary_owners` 不是合法字段：原先需要写在该字段中的内容必须改为父计划中的子 Manifest、`depends_on` 或版本化 Contract。子 Manifest 只可声明该 Owner 的代码、表、App 装配和本地事务边界；消费者的适配、投影或状态转换由消费者自己的子 Manifest 完成。

Core 采用跨仓库 greenfield 迁移时，`owner_repository` 表示子 Manifest 和目标实现的**实际落点**，不能填 legacy 来源仓库。当前约定是：规范基线和 legacy `source_paths` 可引用已提交的 `rust_quant@<sha>`，但 Core 当前迁移的 `owner_repository`、Manifest/Evidence/Verdict 与目标代码必须位于 `rust_quant_alpha`。`program.repositories` 应同时登记 legacy 来源、目标仓库和其他参与 owner；历史 characterization 仍保留在它实际产生的仓库。完整决策见 [ADR-0014](adr/0014-greenfield-target-repository-migration.md)。

跨 Owner 协作只能经版本化 Command/Query/Event Contract：

- 生产者先在自己的本地事务内写自己的事实与 Outbox；
- 消费者在自己的本地事务内做 Inbox 去重、状态转换和自己的事实写入；
- 不得跨库/跨服务开启大事务、共享 ORM 事务或以远程调用替代 Outbox/InBox 语义；
- Contract 变更必须写明 producer、consumer、当前/目标版本、N/N-1 兼容窗口和各自的 `depends_on`。

固定服务 API Key 只能作为 Market 公共只读数据采集能力；它不得成为 `ExecutionRequest`、Account、Risk、用户 credential 验证或任何执行前置条件，也不得用于交易所 mutation。声明该 Key 时，Manifest 必须以 `fixed_service_api_key_access_evidence` 记录非敏感 key ref、Market owner、允许 endpoint、只读 method、观察时间、响应/权限 evidence ref + hash，及 `no_user_credential_fallback = true`；Research 只能消费 Market 发布的 DatasetManifest，不能持有或回退到该 Key。

#### 用户执行 Claim Contract（固定方向）

Web 是 canonical `ExecutionRequest`、用户 credential 状态和 lease 的唯一 owner。Execution 只能发起 Claim/Renew/Release，不能创建 Core 自营请求、直接写 Web 表或把固定 Market Key 当作执行输入。上述 Web command/receipt Contract 都必须带 `idempotency_key` 与 `correlation_id`，且不携带 credential secret；不可变 Owner Value/Event 另按自身 identity/hash/Envelope 规则定义，不能为了满足此规则伪造业务幂等字段。

| Contract | 固定方向 | 必填 Command 字段 | 必填 Receipt 字段 |
| --- | --- | --- | --- |
| `ClaimExecutionRequestV1` | Execution -> Web | `execution_request_id`、`worker_identity`、`claim_id`、`requested_lease_until`、`idempotency_key`、`correlation_id` | `ClaimExecutionRequestReceiptV1`（Web -> Execution）：request/claim ID、单调 `claim_fence`、`claim_status`、`claim_expires_at`、`ExecutionAccountBindingV1` ref、`risk_profile_ref + risk_profile_version`、`blocker_code`、幂等/correlation ID；账户与 credential 字段只从冻结 binding 解析 |
| `RenewExecutionRequestClaimV1` | Execution -> Web | request/claim ID、worker identity、current `claim_fence`、`requested_lease_until`、幂等/correlation ID | `RenewExecutionRequestClaimReceiptV1`（Web -> Execution）：request/claim ID、current `claim_fence`、`renewal_status`、`claim_expires_at`、`blocker_code`、幂等/correlation ID |
| `ReleaseExecutionRequestClaimV1` | Execution -> Web | request/claim ID、worker identity、current `claim_fence`、`release_reason`、幂等/correlation ID | `ReleaseExecutionRequestClaimReceiptV1`（Web -> Execution）：request/claim ID、current `claim_fence`、`release_status`、`released_at`、幂等/correlation ID |
| `ReportExecutionRequestOutcomeV1` | Execution -> Web | request/claim ID、current `claim_fence`、`outcome_type`、`outcome_at`、`outcome_evidence_ref`、`outcome_evidence_sha256`、幂等/correlation ID | `ExecutionRequestOutcomeReceiptV1`（Web -> Execution）：request/claim ID、current `claim_fence`、`acceptance_status`、`blocker_code`、幂等/correlation ID |

`claim_fence` 是 Web 为同一 `execution_request_id` 分配的单调 claim generation。Renew/Release/Outcome 必须由 Web 以 request + claim ID + current fence CAS 接受，旧 fence 的迟到结果只能返回 stale/rejected，不能改写当前请求状态。Claim receipt 中只能给稳定绑定和 opaque reference；Execution 在自己的 owner API/Port 边界解析必要能力，不能取得或记录用户 secret。Claim/Renew/Release 与 Outcome 各自是本地事务后的跨服务 Contract，不能合并成跨库大事务。

对新增风险，`execution_request_id + claim_id + claim_fence + claim_expires_at` 必须进入 live Context subject binding、最终门禁、`GatewayCredentialCapability` 和 `MutationPermit`。capability/permit 的有效期不得晚于 claim；Gateway consume 时必须同时验证 current claim fence/expiry。claim 失效只冻结新开/加仓，不得中断已持久化 `SafetyObligation` 的 Query/Reconciliation/Cancel/Protect/Reduce/Close。

### 4.2 Migration Manifest

每个迁移切片创建：

```text
docs/architecture/migrations/<migration-id>-<slug>/
├── manifest.toml
├── evidence.md
└── verdict.json                     # migration-check 生成，禁止人工编辑
```

`manifest.toml` 是机器可读的执行合同。`manifest_kind = "owner_child"` 才是实施型子 Manifest；`manifest_kind = "historical_record"` 只能保存历史 characterization，必须配 `evidence_scope = "historical_characterization"` 与 `historical_dependency_eligible = false`，不得出现在 current-migration child 的已满足 `depends_on`。实施型子 Manifest 至少声明：

- migration program identity、`depends_on`、技术状态与唯一 Owner；
- Program registry ref、owner repository 与 `historical_dependency_eligible`；
- legacy source repository、target owner repository 与 governance baseline 必须可区分；跨仓库迁移不得把三者压成同一个可变路径；
- 迁移模式与 `behavior_change`；
- source、target、允许和禁止修改路径；
- 受影响的 Cargo package、Release Unit、`contract_snapshots` object array、表和本地事务/InBox/Outbox；
- 必须保持的业务不变量；
- capability、`api`/`spi` 可见面、生产 Use Case/Port/Adapter 完整性和文件预算；
- legacy 完整调用链与语义处置工件；每项旧行为必须记录业务目的、前置状态、时序/失败语义、`保留/优化/废弃/延期` 决策、理由、目标落点和验证方式；
- 配置、事务、外部副作用和恢复边界；
- 必需测试、parity 层和证据；
- shadow、cutover、rollback 和 legacy 删除门；
- 是否需要显式授权及授权来源。

Manifest 未填写完整、字段互相冲突或无法映射到真实调用链时，不得开始修改。

#### 4.2.1 语义继承门禁

目标架构允许重新分层和重写实现，但不得把“没有复制旧代码”误写成“旧业务语义不存在”。每个涉及 legacy 业务代码的 Owner 子迁移，在写目标代码前必须形成独立的语义处置工件，并由 Manifest/Evidence 的 `required_artifacts` 引用。工件至少覆盖：

1. 先枚举全部 App/CLI/Scheduler/Consumer 入口及绕过 Service 的直接 SDK/SQL/env 调用，再追到 Use Case、数据库、外部副作用和最终消费者；
2. identity、状态机、时间边界、默认值、排序、幂等、事务、失败、重试、恢复、限频、并发、审计和清理行为；
3. 每项 legacy 行为的真实业务目的，而不是只描述函数或目录；
4. `保留`、`优化`、`废弃` 或 `延期` 的唯一处置；`废弃` 必须说明为何没有业务价值或为何不安全，`延期` 必须指向承接该语义的后续 Manifest/Gate；
5. 目标语义、首个差异层、允许差异、验证命令/fixture 和 legacy 删除门。
6. 调用点搜索式、命中数量和闭包证明：每个命中必须绑定一个矩阵 ID；同一目的存在互相冲突的 legacy 实现时，必须选出 canonical 语义并把其余实现标为优化或废弃。

只列 source path、hash、类型名或“已参考旧代码”不满足该门禁。分析范围必须先闭合，实施范围才允许按 Owner/阶段缩小；切片只实施链路的一部分时，未实施的闭环语义仍必须进入处置矩阵并绑定后续切片，不得借切片边界静默丢弃。目标实现与 legacy 行为不同时，必须先在矩阵中把差异批准为 `优化` 或 `废弃`，再写测试固定新语义。

### 4.3 Evidence

`evidence.md` 保存人可读、可复核的结果；它不能作为机器依赖边的唯一输入：

- 迁移前真实调用链和 characterization；
- legacy 语义处置矩阵及所有 `延期` 项的后续承接关系；
- 迁移前后 Cargo 依赖图摘要；
- 实际变更文件与 Manifest allowlist 对比；
- unit/integration/contract/parity/recovery 命令和结果；
- build-impact 与工件内容结论；
- shadow 差异和解释；
- legacy 调用方、配置、表写入和白名单变化；
- rollback 证据；
- 未完成项与阻塞项。

不得只写“测试通过”。必须记录 Manifest SHA-256、Evidence SHA-256、被测试的代码 commit/范围补丁 hash、可重放命令、不可变输入 identity/hash、输出工件 identity/hash、关键输出和失败时首次差异层。`contract_snapshots` 必须是可机读 object array（ID/version/path/hash/producer/consumers/compatibility window），`dynamic_input_artifacts` 只能引用在运行前已经固定的 DatasetManifest、Snapshot 或外部工件；不得引用本 Manifest 的 `evidence.md`，也不得以会被本次运行改写的文件作为动态输入。Secret、凭证和敏感用户数据不得进入 Evidence。Evidence 后续发生变化时，最终 CI 必须在新 HEAD 重新生成 Verdict；不能让旧 revision 的 Evidence 自动证明新代码。

### 4.4 Migration Verdict

目标命令：

```bash
cargo xtask migration-check \
  --manifest docs/architecture/migrations/<migration-id>-<slug>/manifest.toml
```

目标输出规范 JSON 到 Manifest 声明的 `verdict_file`。`verdict.json` 由 checker 原子覆写，禁止 AI 或实施者手工填写，至少包含：

```text
verdict_schema_version
migration_program_id
migration_id
manifest_sha256
evidence_sha256
tested_revision
tested_patch_sha256
generated_at
checker_revision
architecture_baseline
scope_match
predecessor_verdicts[]
contract_compatibility
architecture_checks
required_test_results[]
parity_result
recovery_result
build_impact
legacy_ratchet_delta
cutover_eligibility
blocking_reasons[]
verdict
```

Verdict 只能由当前 revision、Manifest、Evidence hash、不可变 predecessor Verdict 和新鲜验证计算。`verdict` 只允许 `pass` 或 `blocked`；任何 required suite 缺失、skipped、hash/revision 漂移或 compatibility window 不闭合都必须是 `blocked`。AI 的文字总结不是 Verdict。

现有 `arch-check` 的 legacy ratchet PASS 也不是 target-layout Verdict。任何 source/target 含 `apps/`、`crates/domains/`、`crates/quant/`、`crates/contracts/`、`crates/adapters/` 或 `crates/platform/` 的 Manifest，在以下条件全部具备前不得生成 `pass` Verdict。正常路径必须保持 `blocked`；若父计划明确延后受跟踪 CI，并满足上文“实施输入门”，则只允许停在 `implementing`，仍不得进入 `verified`：

1. 当前 revision 的 workspace package/path 已由机器 role map 分类，未知 package fail-closed；
2. 依赖、文件大小、SDK DTO、panic、DDL、跨库规则覆盖目标源码根和 `apps/`；
3. baseline/allowlist 的内容变更已由独立 Manifest、Evidence 和当前 revision 的完整性检查授权；
4. 每种 target role 至少有一个注入违规的失败证据；
5. 受跟踪 CI 已执行相同检查，或 Manifest 明确记录 CI 尚不可用并停在 `verified` 之前。

## 5. 迁移模式必须互斥

每个 Manifest 只能有一个主模式：

| 模式 | 允许 | 禁止 |
| --- | --- | --- |
| `structure_only` | 移目录、拆 crate、提取公开 API/Port/Adapter，保持行为 | 修改策略条件、风险阈值、订单语义或默认值 |
| `behavior_change` | 按已批准需求/ADR 修改单一 Owner 的业务规则 | 冒充结构迁移、同时切换事实源 |
| `cutover` | 切换入口、读写事实源、运行角色或部署指针 | 顺便重构业务实现 |
| `legacy_delete` | 删除已满足删除门的旧入口、配置和白名单 | 在调用方或回滚窗口未闭合时提前删除 |

同一需求同时包含多种模式时，必须拆成多个有先后依赖的 Manifest。典型顺序为：

```text
structure_only
  -> behavior_change（仅在确有需求时）
  -> cutover
  -> legacy_delete
```

`structure_only` 必须设置 `behavior_change = false`。任何业务输出、默认值、舍入、时序、事务原子性或错误语义变化都不再属于纯结构迁移。

## 6. 技术状态、策略晋级与 Cutover 状态

```text
state: draft -> baseline_frozen -> implementing -> verified -> ready_for_cutover -> completed
                                                          \-> blocked

任一非终态也可转入 blocked；解除阻塞后必须回到能重新验证的前一技术状态。
```

- `state` 只允许为 `draft`、`baseline_frozen`、`implementing`、`verified`、`ready_for_cutover`、`completed`、`blocked`；不得拼接业务结果，例如 `verified_research_only`；
- `promotion_status` 与技术状态独立，只允许为 `not_applicable`、`research_only`、`candidate`、`promoted`、`rejected`；研究回放验证完成但没有产品/生产晋级时，必须写 `state = "verified"` 与 `promotion_status = "research_only"`；
- `cutover_status` 与技术状态独立，只允许为 `not_required`、`not_ready`、`ready`、`in_progress`、`completed`、`rolled_back`。只有需要切换的 Manifest 才能从 `not_ready` 进入 `ready`；
- `state = "blocked"` 时 `[blocking].reasons` 必须非空；其他技术状态时它必须为空。`required_decisions` 只能补充已列出的阻塞原因，不能代替原因；机器检查必须拒绝二者不一致。
- `evidence_scope = "historical_characterization"` 可保存已发生的研究记录，但不能证明当前 HEAD、目标目录、Release Unit 或 Cutover 已验证，也不能作为未来 Owner 子 Manifest 的已满足 `depends_on`；未来迁移必须以 `current_migration` 重新冻结输入和输出工件。

下表是 `mode`、`state` 和 `cutover_status` 的机器检查真值表；未列出的组合 fail closed。

| `mode` | `behavior_change` | 合法的非阻塞完成前状态 | `cutover.required` / 合法 `cutover_status` | 禁止 |
| --- | --- | --- | --- | --- |
| `structure_only` | `false` | `verified` 或 `completed` | `false` / `not_required` | 修改业务输出、使用 `ready_for_cutover` 或把它当事实源切换 |
| `behavior_change` | `true` | `verified` 或 `completed` | `false` / `not_required`；要切换时另建 `cutover` 子 Manifest | 同一 Manifest 同时切换事实源/运行入口 |
| `cutover` | `false` | `verified -> ready_for_cutover -> completed` | `true` / `not_ready -> ready -> in_progress -> completed`，或 `rolled_back` | 顺手改变策略、风险、默认值或数据模型语义 |
| `legacy_delete` | `false` | `verified` 或 `completed` | `false` / `not_required` | 在前序 cutover、调用方归零或回滚窗口未闭合时删除 |
| 任意 mode + `historical_characterization` | 与历史记录一致 | 仅 `verified` | `false` / `not_required` | 作为 `depends_on`、`ready_for_cutover`、`completed` 或当前 HEAD Verdict |

- `draft`：只允许补充范围和证据，不允许迁移代码；
- `baseline_frozen`：真实调用链、行为和依赖基线已固定；
- `implementing`：只允许修改 Manifest 声明的路径；
- `verified`：必需测试和门禁通过，但不代表已切换生产；
- `ready_for_cutover`：shadow、rollback 和授权齐全；
- `completed`：目标事实源已稳定，删除门按本 Manifest 范围闭合；
- `blocked`：保存原因和待决策项，不通过扩大范围绕过。

AI 可以根据确定性证据更新技术状态，但不能自行填写 `approved_by`、伪造外部授权或把自己的评论当作授权来源。

## 7. 执行前强制 Preflight

AI 在修改文件前必须完成：

1. 确认 owning child repo、分支和工作树；
2. 读取 Manifest 引用的全部规范文档和 ADR；
3. 从当前入口反向追踪真实调用链；
4. 建立 Owner、数据表、Contract、运行角色和 Release Unit 映射；
5. 执行 characterization tests，冻结业务输出；
6. 计算允许修改路径和禁止修改路径；
7. 声明外部副作用能力和生产 mutation 权限；
8. 验证回滚和 legacy 删除条件；
9. 把 Manifest 状态推进到 `baseline_frozen`。

仅在聊天中输出放置声明，不能替代已保存的 Manifest。

## 8. 分阶段执行

### 8.1 冻结行为

迁移前至少固定：

- 输入、输出、错误和状态迁移；
- 默认值、单位、Decimal scale、舍入和时间语义；
- 环境变量和动态配置来源；
- 表、唯一约束、锁、事务与 Outbox；
- Contract payload 和兼容窗口；
- Cargo 依赖、运行 binary 和 Release Unit；
- Backtest/Paper/Live 的业务 symbol、Snapshot/Context 和逐层输出。

不能为无法解释的 legacy 行为自动建立永久兼容层。应记录真实调用方和删除条件。

### 8.2 实施最小 Owner Slice

一次只迁移一个可验证的垂直切片：

```text
入口映射
  -> Owner capability
  -> API Input / Output
  -> Use Case + Model / Policy
  -> SPI Port
  -> production Adapter
  -> 必要 Contract
  -> Tests
```

禁止：

- 按目录横向搬完全部 Model/Repository/Service；
- 顺手清理 Manifest 外的无关代码；
- 建立没有真实调用方的抽象；
- 把跨 Owner 大函数整体移动到新 `impl`；
- 为通过编译临时增加生产到 Research 的反向依赖；
- 同时维护两套会独立演化的业务规则。

目标 Domain/Adapter 的内部拆分遵守 [ADR-0015](adr/0015-capability-first-modules-and-api-spi-boundaries.md)：

- 其他 Domain/Research 只经 `api`，Adapter 只经 `spi`，App 只在 wiring/composition root 使用 `spi`；
- 非测试 Port 进入 `verified` 前必须有生产 Use Case 调用方、生产 Adapter 和失败/原子性/恢复证据；Fake-only Port 只能作为有承接项的 `implementing` 中间态；
- Domain/Adapter 生产代码、任意 Rust 文件、façade 与测试文件执行 ADR-0015 预算；
- Owner 级 `enums.rs`/`types.rs`/`common.rs`、provider 级 Gateway 大文件和万能 Use Case 不属于有效拆分。

如果发现新规则尚未进入已提交 governance baseline，必须先停止业务切片并提交 ADR/规范；不得在同一未冻结工作树中一边修改规范、一边让目标 Manifest 引用这些规则。Architecture Governance 的门禁修改、Market 结构拆分、Strategy Fake-only Port 处置属于不同 Owner/范围，必须使用独立 Manifest，不能合并为一个“P0.1 大切片”。

### 8.3 B0：不可变 test-only Evidence Provider

确定性 Dry-run parity 前，必须有独立 B0 Owner 子 Manifest。B0 是 Execution owner 的 test-only Adapter，不是新 Domain owner，也不拥有 Market、Account 或 Instrument 事实；它只能读取各 owner 已发布、内容寻址的 fixture/DatasetManifest/Snapshot，组合为 `ImmutableDecisionEvidenceBundleV1`。

B0 bundle 至少固定 `market_evidence_ref/hash`、`account_evidence_ref/hash`、`instrument_evidence_ref/hash`、Clock identity、Seed 和四个 Policy Snapshot ref/hash。B0 禁止网络 fetch、数据库写入、环境变量业务 fallback、用户 credential、固定 Market API Key 和任何 App/runtime wiring；它只能被 B1/B2/B3 的 test/dry-run 装配消费。缺少任一输入 hash 时，B 只能 `blocked`，不得用“当前数据”补齐。

`ExecutionPlanningValue` 是 B0/B1/B2/B3、Research 和 Paper 可保存的纯规划值；它不是 OMS aggregate。只有 C1/live Execution owner 在本地事务中将经批准的规划落实为持久 `ExecutionPlan`、Order/Attempt/Protection，才允许使用 `ExecutionPlan` 名称。

#### C1 live 前置 Owner 子 Manifest

C1 不能只由“Risk approval -> Execution recovery”两项组成。首个允许 live mutation/cutover 的 Execution 子 Manifest 必须在 Program 中显式依赖并固定以下 owner Evidence：

| 前置能力 | 唯一 Owner | 至少证明 |
| --- | --- | --- |
| `ExecutionAccountBindingV1` 与 claim fence | Web | 稳定 `ExchangeAccountRef`、产品/子账户/保证金与持仓模式、credential revision、claim CAS/expiry |
| `RequiredMarketEvidenceV1` | Strategy | 在 RuntimeSnapshot 中冻结全部必需 exchange/instrument/timeframe/source profile、finality、最大年龄和显式 fallback |
| `BarFinalizationV1` / `MarketDecisionReadinessV1` / `ResolvedMarketEvidenceSetV1` | Market | 逐来源 final/revision/continuity、迟到/源切换、逐项 readiness 与一次决策的完整聚合 hash/TTL |
| `AccountAdmissionEvidenceV1` / `AccountFactV1` / `AccountRecoveryClosedV1` | Account | 私有流只由 Account 持有，投影先于 owner fact，cursor/snapshot/query 闭合、session generation 与 zombie fence |
| `RiskValuationSnapshotV1` | Risk | 合约乘数、结算/mark/FX、保证金与清算缓冲，以及 `ObservedExternalPosition` 的默认风险占用 |
| `ExchangeExecutionCapabilityProfileV1` | Execution + exchange-gateway Adapter | stable client identity、缺席证明、保护/reduce-only、position/margin mode、限频与时钟能力 |
| `SafetyMonitoringV1` / `SafetyMonitoringAckV1` 与 `SafetyObligation` 闭合 | Execution / Account | add/update/remove fence、Outbox/Inbox、current-fence ack/replay、无监测空窗和不可变闭合谓词 |
| RecoveryHarness | CI-only integration artifact | claim/permit 竞态、Unknown、部分成交保护、会话抢占、凭证撤销与安全尾部 |

表中各业务能力必须拆为各自唯一 Owner 的 child Manifest；Program 只记录 Contract 和依赖边。RecoveryHarness 是 `C1-execution-recovery` 的 required verification suite/Evidence，不是新的业务 Owner、独立 predecessor child、Release Unit 或部署单元：可以在 C1 实现后验证同一 live 代码路径，其通过结果进入该子 Manifest 的 machine Verdict。任一业务 predecessor Verdict 缺失/`blocked`，或 RecoveryHarness required suite 未通过时，`production_mutation_allowed`、`cutover_eligibility` 和 live capability issuance 必须保持 false。

C1 的运行消息方向也固定：Risk 只以 `RiskApprovalV1` Outbox 向 Execution 交付不可变审批；Execution 只把 `OrderSubmissionRequestedV1`、`OrderCancelRequestedV1`、`ProtectionSubmissionRequestedV1` 发布给自己的 Dispatcher，不直接把 Broker delivery 当成 Gateway 调用授权；Fenced Gateway 只以绑定原 event generation、aggregate version、attempt 与 permit 的 `ExchangeMutationOutcomeV1` 回到 Execution Inbox。字段与兼容窗口以 Program Registry 为准，任何缺少 current claim receipt、mutation generation/version 或 permit identity 的“简化消息”都不得进入 live。

### 8.4 Shadow 与 Parity

新旧路径并行验证时，必须保证不会产生双副作用。交易链路按首次差异层比较：

```text
StrategyEvaluationState after
StrategySignal / ExitIntent
PortfolioTarget
RiskDecision
OrderIntent
ExecutionPlanningValue（test/dry-run/research/paper）
ExecutionPlan（仅 live OMS aggregate，若本切片涉及）
ProtectionPlan
decision trace
```

只有四个 Policy Snapshot、Decision Context、动态 Evidence、EvaluationState before、Clock 和业务 API identity 一致时，才能声称 exact parity。test/dry-run/research/paper 的 parity 比较 `ExecutionPlanningValue`，不得伪装成已经持久化的 `ExecutionPlan`。Fill/PnL 只在相同 SimulationProfile 下要求确定性重放。

### 8.5 Cutover

Cutover 与代码结构迁移分开执行，必须具备：

- 单一新事实源和旧写入冻结方案；
- feature flag/release generation 或等价切换身份；
- shadow/parity/recovery 证据；
- 生产工件和 deploy contract 证据；
- 明确回滚入口、数据兼容和允许窗口；
- 用户或授权责任人的显式切换批准。

CI 变绿、镜像构建成功或 AI 判断安全，都不能替代生产切换授权。

### 8.6 Legacy 删除

删除前必须证明：

- 所有真实调用方已迁移；
- 旧入口不再读写业务事实；
- 旧配置、任务、表写入、监控和 allowlist 已归零；
- 回滚窗口已结束或替代回滚路径成立；
- 删除后 contract/parity/integration/recovery 仍通过；
- legacy ratchet 条目实际减少。

仅改名、移动文件或隐藏旧入口不算删除完成。

## 9. 一致性门禁

`migration-check` 目标上必须组合以下结果：

1. Manifest schema、`manifest_kind`、mode/state/cutover 真值表、合法状态枚举、`blocked`/`[blocking]` 一致性与字段完整性；
2. Program Registry/父计划/Owner 子 Manifest 图、owner repository、legacy source/target repository、child revision、唯一 Owner、`depends_on` 和 Contract/version 是否闭合；依赖状态必须由 predecessor Manifest/Evidence/Verdict hash 计算，`not_created`、`historical_record` 或 Registry 中人工缓存的布尔值不得满足 current-migration dependency；
3. 当前架构基线、规范文档 hash、Release Unit/`contract_snapshots` object array 是否一致；
4. Git diff 是否完全落在允许路径；
5. 新增依赖是否符合 Domain、Quant、Contract、Adapter 和 App 方向；
6. capability/API-SPI 可见面、façade 内容、文件预算、Port/生产 Adapter 登记与 Fake-only 中间态是否符合 ADR-0015；
7. Cargo 反向依赖与 Release Unit build-impact；
8. 生产镜像 binary allowlist 与 forbidden package；
9. Owner、本地事务、表、Outbox/InBox 和跨 Owner Contract 边界；Claim/Renew/Release/Outcome 的方向、current `claim_fence`、expiry、CAS 与 receipt 是否符合 Registry，最终 capability/permit TTL 是否被 claim 截断；
10. Strategy/Portfolio/Risk/Execution 快照与 Decision Context 边界；Market Velocity Signal 是否由 Strategy owner 产生；
11. B0 immutable test-only Evidence Provider 是否只消费 hash 输入、且物理不可达 runtime/Paper/Live/scheduler；
12. 固定 Market 公共 Key 是否只有 Market owner 的只读 access evidence，且无用户 credential fallback；
13. unit、integration、contract、parity、recovery 和 deploy contract；
14. legacy allowlist 增减；
15. cutover、rollback 和生产授权；
16. Evidence 的 Manifest/Evidence hash、代码 revision/补丁 hash、动态输入和输出工件是否可重放，且动态输入未自引用 `evidence.md`；规范 `verdict.json` 是否由当前 HEAD 的 checker 生成、引用全部 predecessor Verdict，并且不存在 skipped required suite。
17. C1 live 子 Manifest 是否依赖 Web account binding/claim、Market required/resolved evidence、Account admission/fact/recovery、Risk valuation、Exchange capability、Safety monitoring 的 current-revision Verdict；`C1-execution-recovery` 自身 Verdict 是否包含 CI-only RecoveryHarness required suite 且无 skipped。

任一无法判定的架构、依赖或工件结果必须 fail closed。纯历史债务使用 ratchet：保存基线、禁止新增、逐切片减少，不允许一次把全仓永久置红。

## 10. 文档同步规则

迁移执行期间，文档分为两类：

### 规范性文档

包括 ADR、目标架构、依赖规则、生产运行、数据访问和本协议。

- 默认由 `architecture_baseline` 锁定；
- 迁移实现不得静默修改它们；
- 需要改变已接受语义时，先停止并单独更新/替代 ADR；
- 规范修改获批后，重新生成 Manifest 基线，再继续迁移。

### 执行性文档

包括 Manifest、Evidence、迁移状态和 legacy ledger。

- 必须随实际进度更新；
- 不得预先写成已完成；
- 每条完成结论必须链接新鲜测试或运行证据；
- 状态、代码 revision 和 Evidence identity 必须一致。

## 11. 强制停止条件

出现以下任一情况，AI 必须停止当前切片并设为 `blocked`：

- Owner、事实源或写入责任无法唯一确定；
- 需要同时改变目录和业务语义；
- 实际 diff 超过 Manifest allowlist；
- 需要新增跨库读写、共享 ORM 或绕过 owner API；
- Contract、状态机、事务原子性或恢复协议需要改变但没有批准；
- 新旧结果出现无法解释的首次差异；
- exact parity 所需 Snapshot、Context 或 Evidence identity 不一致；
- 需要扩大 legacy allowlist 或关闭测试才能继续；
- 架构基线、ADR 或 Release Unit Manifest 已漂移；
- 计划使用尚未提交的 ADR/规范作为实现基线，或把 Architecture Governance、Market/Adapter、Strategy 的结构修改塞进同一 Owner Manifest；
- 需要真实下单、撤单、平仓、生产写入或拓扑切换但没有显式授权；
- 回滚路径、数据兼容或删除门无法证明。

AI 不得通过添加 fallback、读取“最新配置”、复制旧实现或降低验证标准绕过阻塞。

## 12. 实施与验证职责分离

- 实施者只在 Manifest 范围内修改；
- 验证者以只读方式复核 diff、调用链、依赖图和证据；
- 确定性 CI/测试是最终技术裁判，AI 自评不是；
- 高风险切片应由不同上下文的只读 AI 或人工 Review 再验证；
- 生产授权人只批准 cutover/mutation，不替代技术门禁；
- 一个角色不能用自己生成的文字结论代替缺失证据。

## 13. 完成标准

只有同时满足以下条件，Manifest 才能进入 `completed`：

- 实际修改范围与 Manifest 一致；
- 目标 Owner、目录和依赖方向成立；
- `behavior_change` 与真实 diff 一致；
- 必需测试和门禁使用当前 revision 通过；
- parity/recovery 达到 Manifest 声明层级；
- build-impact 和工件边界正确；
- 没有新增 legacy 违规；
- 事实源、rollback 和删除门在本切片范围内闭合；
- Evidence 完整且不包含敏感信息；
- 需要的授权有真实来源；
- 没有把“verified”误写成“已生产切换”。

未实现 `migration-check` 前，可以人工执行相同检查并记录 Evidence，但必须明确标记 `verification_mode = "manual"`。

## 14. 渐进实施

协议工具按以下顺序落地：

1. 提供 Manifest 模板和人工 Evidence；
2. 实现 `migration-check` 的 schema、基线和 diff scope 只读报告；
3. 接入 `arch-check`、build-impact、Contract 和测试结果；
4. CI 先阻止新增违规和越界 diff；
5. 再逐步收紧 parity、工件和 legacy 删除门；
6. 所有活跃切片切换到机器 Verdict 后，停止接受无 Manifest 的架构迁移。

不得因为自动化尚未完成就跳过 Manifest，也不得把尚未实现的目标门禁写成已经生效。
