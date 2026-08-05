# ADR-0014：采用 `rust_quant_alpha` 作为 Core 目标实现仓库

- 状态：已接受
- 日期：2026-07-29
- 决策者：Rust Quant Core
- 上位文档：[长期目标架构](../target-architecture.md)、[架构迁移计划](../migration-plan.md)、[AI 架构迁移执行协议](../ai-migration-execution-protocol.md)
- 当前解释：源仓库与目标仓库边界继续有效；Registry、Owner 子 Manifest 和逐工件哈希机制已被 [ADR-0017](0017-capability-catalog-and-domain-wave-migration.md) 取代

## 背景

目标架构最初按 `rust_quant` 原仓原位迁移设计，Migration Program Registry 因此把 Core Owner 子 Manifest 指向 `rust_quant`。实施阶段已经明确创建独立仓库 `rust_quant_alpha`，并在其中提交 target-layout P0 与 `migration-check` P1。若继续沿用旧 Registry，会出现三套互相冲突的事实：

- `rust_quant` 同时被解释为 legacy 来源和目标实现仓库；
- `rust_quant_alpha` 中的 Manifest 引用一个 Registry 未登记的父 Program；
- 后续业务切片可以在错误仓库通过局部检查，却无法形成 Registry 双向一致的 Verdict。

## 决策

### 1. 固定源仓库、目标仓库与过渡期事实

- `rust_quant` 是 legacy 源仓库，也是迁移切换前现有 Core 生产实现的事实来源；现有代码可继续维护，但不得承载目标架构的新业务包。
- `rust_quant_alpha` 是 Core 目标实现仓库。所有新的 `apps/*`、`crates/{domains,quant,contracts,adapters,platform}/*`、目标 migration SQL、Release Unit、能力总账和当前 Wave 实施证据都只能落在该仓库。
- `rust_quan_web`、`rust_quant_news`、`crypto_exc_all` 和 `rust_quant_admin` 继续保留各自 owner 仓库，不随 Core 代码迁入 `rust_quant_alpha`。
- 本目录中的架构规范、ADR 和 legacy 盘点在过渡期继续由 `rust_quant` 承载；它们不因此成为目标业务代码位置。旧 Migration Program Registry 只读归档。

### 2. 历史 Registry 的仓库字段使用目标落点（已被 ADR-0017 取代）

- `program.repositories` 同时列出参与迁移的 legacy 来源、目标实现和其他 owner 仓库。
- `program.children.owner_repository`、`manifest_path`、`evidence_path`、`verdict_path` 必须指向该子切片实际实施和保存治理工件的目标仓库。
- current-migration Contract 的 `direction` 仓库限定符也使用实际 producer/consumer 目标仓库；Core 端统一写为 `rust_quant_alpha::<Owner>`，不能继续写成 legacy `rust_quant::<Owner>`。
- Core 当前迁移的 Owner 子切片统一指向 `rust_quant_alpha`；历史 characterization 继续指向其实际保存位置 `rust_quant`。
- `MP-rust-quant-alpha-migration-v1` 只登记目标仓库治理 P0/P1，不拥有 Strategy、Risk、Execution 等业务事实，也不能替代业务 Program。
- 跨仓库 child 先以 `not_created` registration revision 冻结 identity/owner/path/依赖，目标 Manifest 钉住该 revision；Manifest 提交后 Registry 再记录内容 hash。Manifest 不回写追逐观察性 Registry commit，避免循环 hash。

### 3. 历史 Core 子 Manifest 跨仓库边界（已被 ADR-0017 取代）

Core 子 Manifest 必须同时声明：

- `scope.source_paths` 使用 `rust_quant@<commit>:<path>` 冻结 legacy 来源；
- `scope.target_paths` 和 `scope.allowed_change_paths` 只使用 `rust_quant_alpha` 当前仓库相对路径；
- `scope.owner_repository = "rust_quant_alpha"`；
- `program.registry_ref`、`authority.architecture_baseline_git_sha` 与规范 hash 指向同一个已提交 `rust_quant` 架构基线；
- 不读取或复制 legacy 未提交工作树作为正式基线，不通过跨仓库工作区状态伪造可复现输入。

### 4. 仓库切换是独立 Cutover

业务 parity、数据迁移和运行入口迁完不自动改变生产事实源。最终从 `rust_quant` 切换到 `rust_quant_alpha` 必须进入独立 W5 Cutover Gate，并满足：

- current-revision CI、Contract、parity、恢复和 deploy contract 证据完整；
- 生产镜像、compose、部署和回滚入口明确指向目标 revision；
- 不存在双重外部副作用或双写交易所 mutation；
- 获得显式生产切换授权；
- 回滚窗口与 legacy 删除门已记录。

在用户决定统一延后 CI/CD 的迁移阶段，各 capability 只能按当前证据保持 `planned`、`implementing` 或带真实原因的 `blocked`；未通过对应 Wave Gate 不得伪造 `implemented`，未通过 W5 和显式授权不得声称已切换。

## 后果

### 正面影响

- legacy 来源与目标代码位置不再混淆；
- 能力总账、目标目录和 Domain Wave 对源仓库与目标仓库使用同一套归属口径；
- 旧仓库的大量在途策略修改不会污染目标架构基线；
- 最终生产切换仍保留独立授权和回滚门。

### 代价

- 每个 Wave 必须固定 legacy 基线 revision，并以 current target revision 生成验证证据；
- 架构规范在过渡期仍由旧仓库托管，需要在 W5 中完成最终文档归属迁移；
- 跨仓库 Contract 和 build-impact 需要按各 owning repo 分别验证。

## 验收条件

1. 能力总账的 legacy source 明确限定 `rust_quant@<revision>`；
2. target 路径全部位于 `rust_quant_alpha`；
3. 迁移计划、执行协议和架构技能明确区分 legacy source、target implementation 与 governance baseline；
4. 历史 Manifest/Registry 只读归档，不再决定活跃迁移状态；
5. 在独立 Cutover 获批前，`rust_quant` 的现有生产事实源和运行入口不被静默替换。
