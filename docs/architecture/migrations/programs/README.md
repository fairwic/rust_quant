# Migration Program Registry

`registry.toml` 是跨仓库父 Migration Program 的机器可读索引；它只编排 Owner 子 Manifest 和 Contract，不拥有代码、表或跨 Owner 事务。

每个 `[[program]]` 必须提供：

- `program_id`、`kind`、`state`、`evidence_scope`、`parent_plan` 和参与 `repositories`；
- `historical_dependency_eligible`：只有 `current_migration` Program 才可能为 `true`；`historical_characterization` 永远为 `false`；
- `[[program.contracts]]`：Contract ID/version、producer -> consumer 方向、snapshot object（`status/ref/sha256`）、request/receipt 字段及 N/N-1 `compatibility_window`；
- `[[program.children]]`：child ID、`manifest_kind`、唯一 owner、`owner_repository`、`depends_on`、`manifest_path`、`evidence_path`、`verdict_path`、`manifest_revision`、`evidence_revision` 与 `verdict_revision`；路径必须含 `<repository>:` 前缀。

Checker 规则：

1. `not_created` child 必须使用 `manifest_revision = evidence_revision = verdict_revision = "not_created"`，只能位于 `planned` Program；它不能满足任何 `depends_on`；
2. Registry 不保存或信任 `depends_on_satisfied`。只有 `migration-check` 读取 predecessor 的 Manifest/Evidence/Verdict hash、tested revision 与状态后，才能在当前 child 的 `verdict.json` 中计算依赖是否满足；
3. `manifest_kind = historical_record` 必须属于 `historical_characterization` Program，永远不能被 current-migration child 引用；
4. Claim/Renew/Release command 固定为 Execution -> Web，receipt 固定为 Web -> Execution；Outcome 固定为 Execution -> Web，再由 Web 返回 receipt；
5. Claim receipt 必须返回单调 `claim_fence` 与 `claim_expires_at`；Renew/Release/Outcome 必须携带 current fence，迟到旧 fence 由 Web CAS 拒绝；
6. 父 Program 不得把多个 owner 的本地事务合并；跨仓库交接只允许已登记的 versioned Contract；
7. Program Registry 与每个 child Manifest 的 `migration_program_id`、`registry_ref`、`owner_repository` 和 `depends_on` 必须双向一致。
8. `owner = "QuantKernel"` 只用于 `quant/*` 的 owner-neutral 纯机制技术维护责任：不得拥有业务事实、数据库表、Contract payload 或发布资格；一旦切片包含 Strategy/Risk/Execution/Research 语义，必须拆回对应业务 Owner child。
9. 同一 owner Contract 被多个 Program 复用时，重复的 `contract_id` 记录必须具有完全相同的 version、direction、required/forbidden fields、compatibility window 与 snapshot ref/hash；checker 必须拒绝任一 Program 私自改写副本。`ExecutionPlanningValueV1` 等共享 Contract 只允许复用，不允许形成 Program 私有方言。

未来创建 child Manifest 时，先把 registry child 从 `not_created` 改为其实际路径与初始 revision；不要通过增加一个未登记的目录绕过 Program 依赖图。
