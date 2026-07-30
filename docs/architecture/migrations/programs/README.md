# Migration Program Registry

`registry.toml` 是跨仓库父 Migration Program 的机器可读索引；它只编排 Owner 子 Manifest 和 Contract，不拥有代码、表或跨 Owner 事务。

每个 `[[program]]` 必须提供：

- `program_id`、`kind`、`state`、`evidence_scope`、`parent_plan` 和参与 `repositories`；
- `historical_dependency_eligible`：只有 `current_migration` Program 才可能为 `true`；`historical_characterization` 永远为 `false`；
- `[[program.contracts]]`：Contract ID/version、producer -> consumer 方向、snapshot object（`status/ref/sha256`）、request/receipt 字段及 N/N-1 `compatibility_window`；
- `[[program.children]]`：child ID、`manifest_kind`、唯一 owner、`owner_repository`、`depends_on`、`manifest_path`、`evidence_path`、`verdict_path`、`manifest_revision`、`evidence_revision` 与 `verdict_revision`；路径必须含 `<repository>:` 前缀。

仓库字段语义：

- `repositories` 列出该 Program 的 legacy 来源、目标实现和其他参与 owner 仓库；
- `owner_repository` 是 child Manifest 与目标实现的实际仓库，不是 legacy `source_paths` 所在仓库；
- current-migration Contract `direction` 的仓库限定符使用实际目标 producer/consumer；Core 端为 `rust_quant_alpha::<Owner>`；
- Core greenfield 当前迁移使用 `rust_quant_alpha`，legacy 来源通过 Manifest 中钉住 revision 的 `rust_quant@<sha>:<path>` 表达；
- 历史 characterization 保持其实际产物仓库，不随目标仓库决策改写。

`manifest_status` 的合法值为：

- `not_created`：Manifest 不存在，三个 revision 均为 `not_created`；
- `created`：current-migration Manifest 已提交，当前 Registry 用 `sha256:<content-hash>` 记录 Manifest/已有 Evidence，尚无 Verdict 时 `verdict_revision = "not_created"`；
- `recorded`：仅用于 `historical_record`。

跨仓库创建 child 使用两阶段登记，避免 Registry hash 与 Manifest 的 `registry_ref` 循环引用：

1. 先提交 `not_created` Registry 条目，冻结 child ID、owner repository、实际 artifact path 和 `depends_on`；该提交是 registration revision；
2. 目标仓库 Manifest 的 `registry_ref` 钉住 registration revision，然后提交 Manifest/Evidence；
3. 再由 Registry 后续提交把 `manifest_status` 更新为 `created`，记录目标提交中 Manifest/Evidence 的内容 hash；
4. Manifest 不为追逐第 3 步的观察性 hash 回写而改动；checker 必须证明当前 Registry 相对 registration revision 未改写 child identity/owner/path/依赖，并且当前记录的 hash 与目标仓库内容一致。

Checker 规则：

1. `not_created` child 必须使用 `manifest_revision = evidence_revision = verdict_revision = "not_created"`，只能位于 `evidence_scope = "current_migration"` 且 `state = "planned"` 或 `"active"` 的 Program；它不能满足任何 `depends_on`、不能作为“实施输入门”的 predecessor，也不能证明实现、发布或切换资格。`active` 只表示父编排已有 child 开始推进，不会把实施授权传播给尚未创建的 sibling child；
2. Registry 不保存或信任 `depends_on_satisfied`。只有 `migration-check` 读取 predecessor 的 Manifest/Evidence/Verdict hash、tested revision 与状态后，才能在当前 child 的 `verdict.json` 中计算依赖是否满足；
3. `manifest_kind = historical_record` 必须属于 `historical_characterization` Program，永远不能被 current-migration child 引用；
4. Claim/Renew/Release command 固定为 Execution -> Web，receipt 固定为 Web -> Execution；Outcome 固定为 Execution -> Web，再由 Web 返回 receipt；
5. Claim receipt 必须返回单调 `claim_fence` 与 `claim_expires_at`；Renew/Release/Outcome 必须携带 current fence，迟到旧 fence 由 Web CAS 拒绝；
6. 父 Program 不得把多个 owner 的本地事务合并；跨仓库交接只允许已登记的 versioned Contract；
7. Program Registry 与每个 child Manifest 的 `migration_program_id`、`registry_ref`、`owner_repository` 和 `depends_on` 必须双向一致。
8. `owner = "QuantKernel"` 只用于 `quant/*` 的 owner-neutral 纯机制技术维护责任：不得拥有业务事实、数据库表、Contract payload 或发布资格；一旦切片包含 Strategy/Risk/Execution/Research 语义，必须拆回对应业务 Owner child。
9. 同一 owner Contract 被多个 Program 复用时，重复的 `contract_id` 记录必须具有完全相同的 version、direction、required/forbidden fields、compatibility window 与 snapshot ref/hash；checker 必须拒绝任一 Program 私自改写副本。`ExecutionPlanningValueV1` 等共享 Contract 只允许复用，不允许形成 Program 私有方言。
10. Core 当前迁移必须遵守 [ADR-0014](../../adr/0014-greenfield-target-repository-migration.md)：Registry 的目标 child 指向 `rust_quant_alpha`，Manifest 的 source path 再独立钉住 `rust_quant` legacy revision；两者不得互换。
11. current-migration 的 `planned` / `active` Program 可以同时包含 `created` 与 `not_created` child；每个 child 的可实施性只由自身 registration、Manifest/Evidence、依赖闭包和 Verdict 决定，不能由父 Program 状态推断。`historical_characterization` Program 只能包含 `recorded` child；Program 进入终态前不得仍有 `not_created` child。
12. 面向单个 Manifest 的 `migration-check` 必须阻断目标 child、其传递依赖闭包和全局 child ID 唯一性错误；不得把无关 Program 中不属于该闭包的未完成 child 伪装成目标 child 的依赖失败。全 Registry 的独立一致性检查仍须报告并阻断重复 ID、非法 Program/child 状态矩阵和不可解析路径。
13. `depends_on` 按整个 Registry 中全局唯一的 `child_id` 解析，不限于当前 Program。跨 Program 依赖只允许连接两个 `current_migration` Program，predecessor 的 `owner_repository` 必须列入 successor Program 的 `repositories`；缺失、重复、指向 `historical_record` 或形成全局依赖环必须 fail closed。父 Program 的状态不能代替 predecessor child 的 Manifest/Evidence/Verdict，也不得为了规避跨 Program 查找而复制 child 登记。

当父计划明确记录用户决定延后受跟踪 CI 时，可以启用“实施输入门”，但它不改变 Checker 对 `depends_on` 的判定：

- predecessor 必须属于当前迁移、已提交、Registry 为 `created`，Manifest/Evidence hash 匹配并记录本地验证；`not_created`、`draft`、`blocked`、`historical_record` 不可作为实施输入；
- successor 必须在 `dynamic_input_artifacts` 钉住 predecessor 的代码 revision、Manifest/Evidence 和实际消费的 API/schema/lockfile hash；`predecessor_verdicts` 仍为空；
- successor 最高停在 `implementing`，只可实现源码、schema、纯/离线逻辑、公共只读 Adapter 与 disposable integration test；不得部署 runtime、切换事实源、删除 legacy、写生产数据库、触发跨 Owner 副作用或交易所 mutation；
- predecessor 工件漂移时，所有 descendant Evidence 失效；获得 current-revision `pass` Verdict 后，才可计算真正的依赖闭合并进入后续状态。

未来创建 child Manifest 时，必须先有已提交的 `not_created` registration 条目和实际路径；目标 Manifest 提交后再把 Registry 更新为 `created` 并记录内容 hash。不要通过增加一个未登记目录绕过 Program 依赖图，也不要让 Manifest 和 Registry 互相追逐最新 commit 形成不可解的自引用。
