# ADR-0016：迁移风险分级与低风险快车道

- 状态：已被 ADR-0017 取代
- 日期：2026-07-31
- 决策者：Rust Quant Core / Architecture Governance
- 历史上位文档：[旧 Manifest 执行协议](../archive/legacy-manifest-execution-protocol-2026-07.md)、[Greenfield 目标仓库迁移](0014-greenfield-target-repository-migration.md)
- 替代决策：[ADR-0017](0017-capability-catalog-and-domain-wave-migration.md)

> 本 ADR 只解释 2026-07 历史 Manifest 为什么出现低风险快车道。新的迁移不再声明 migration_tier，不再创建逐切片 Manifest；活跃流程统一使用能力总账和 Domain Wave。

## 背景

`rust_quant` 全量重写进 `rust_quant_alpha`（见 [ADR-0014](0014-greenfield-target-repository-migration.md)）由 [AI 架构迁移执行协议](../ai-migration-execution-protocol.md) 驱动。协议对**每一个迁移切片**施加同等重量的执行合同：

- §8.2 要求"一次只迁一个可验证垂直切片"，并禁止按 capability 批量推进；
- 每个切片都要产出全套工件：`manifest.toml`（含十几个 normative 文档逐一 SHA-256 绑定）、`evidence.md`、`characterization.md`、`callsite-closure.md`、`business-semantics-matrix.md`；
- §4.2.1 要求即使是纯重写也先形成完整 legacy 语义处置矩阵与调用点闭包。

这套严格性对**实盘 execution、risk、cutover、删除 legacy** 是必要的：这些切片一旦出错会造成真实资金损失或不可逆的事实源丢失。但把同一把尺子套在"公共匿名只读行情采集"这类切片上是过度投入——拉一条公共 K 线被拆成 F1→F2→...→F5C 十几个切片，每个都背负全套重工件，交付速度与切片风险严重不匹配。截至本 ADR，Market 数据地基的全部切片都停在 `implementing`，尚无一个进入生产。

问题不在协议的严格性本身，而在于它**缺少按风险分级**：低风险切片不应承担与高风险切片相同的证据负担。

## 决策

### 1. 引入两档迁移风险分级（tier）

Manifest 顶层新增 `migration_tier` 字段，枚举 `low_risk_fast_track` 与 `full_rigor`。缺省与非法值一律视为 `full_rigor`（fail-closed）。

`full_rigor` 是既有的完整档，行为与本 ADR 之前完全一致，承载所有高风险迁移。`low_risk_fast_track` 是新增的低负担快车道。

### 2. tier 是安全字段的纯函数，声明只能更严不能更宽

tier **不是**独立可信输入。门禁基于 Manifest 已有的客观安全字段派生"最宽松可享受档"（`derive_max_eligible_tier`），八项维度**全部命中**才够格 `low_risk_fast_track`：

| 维度 | 字段 | 低风险要求 |
| --- | --- | --- |
| 无实盘 mutation | `production_mutation_allowed` | `false` |
| 只读副作用 | `side_effects.read_only` 及 database/network/exchange/production 写标志 | 只读、全部写标志 `false` |
| 无 cutover | `cutover.required` / `cutover_status` | `false` / `not_required` |
| 无行为变化 | `behavior_change` / `mode` | `false` 且 `mode == structure_only` |
| 不删 legacy | `mode` | 非 `legacy_delete` |
| 不写生产持久化 | `persistence.changed` | `false` |
| 不改跨仓 contract | `contracts.changed` | `false` |
| 不碰 credential | `side_effects.fixed_service_api_key_scope` | `not_used` 或 `market_public_read_only` |

生效 tier = 声明与派生上限中**更严格者**。声明 `low_risk_fast_track` 但派生为 `full_rigor` 时，门禁报 `MIG-TIER-ESCALATION-REQUIRED` 并 fail closed。因此任一低风险维度漂移（某天开始写生产表、改 contract、删 legacy、碰 mutation）都会自动 escalate，恢复全套 `full_rigor` 校验——高风险切片无法伪装成低风险。

### 3. 低风险快车道的豁免范围

生效 `low_risk_fast_track` 时豁免以下负担：

- **逐文档内容 hash**：`authority.normative_document_hashes` 与 `release_unit_manifest_hashes` 可为空；`architecture_baseline_git_sha` 仍必填并锁定基线 revision，足够复现。
- **语义继承工件**：§4.2.1 的 characterization、callsite-closure、business-semantics-matrix 改为可选（公共只读无 legacy 语义继承风险）。
- **schema 必填字段**：`[behavior]` 的差异刻画、`[authority]` 的 hash 类、`[configuration]` 的策略/组合/风险/执行快照 refs、`[shadow]` 整节改为存在即校验、缺失不报错。
- **同 Owner 连续能力合并**：可选字段 `scope.merged_capabilities` 允许一个切片覆盖同一 Owner 的多个 capability 目录，免去逐能力拆切片。
- **推进门槛**：`verification_mode = manual` 时可凭本地验证停在 `implementing`。

### 4. 低风险快车道**不可**豁免的部分

以下即使 low tier 也完全不放松，是 fail-closed 的地基：

- mode/state/cutover/blocking/historical 五张安全真值表；
- `side_effects.*`、`persistence` owner 与跨 owner 事务禁令、固定服务 Key 只读证据校验；
- `evidence_file`/`verdict_file` 存在性与"不得只写测试通过"；
- tier 派生的全部输入字段本身必填；
- 推进上限为 `implementing`——进入 `verified`/`completed`、执行 cutover 或删除 legacy 仍需按 full 规则补齐 verdict 与授权门（§4.1/§4.2.1/§13）。

## 关系与影响

- **[ADR-0014](0014-greenfield-target-repository-migration.md)**：tier 分级作用于 greenfield 迁移的切片粒度，不改变 owner_repository/legacy source/baseline 三者可区分的约束。
- **[ADR-0013](0013-user-execution-request-and-public-market-data-credentials.md)**：`market_public_read_only` 作为低风险 Key scope 直接引用其公共行情 credential 边界；任何用户 credential/claim 都使切片脱离低风险。
- **[ADR-0007](0007-owner-scoped-persistence-and-transaction-boundaries.md)**：`persistence.changed = true`（写生产事实）即 escalate，低风险切片不触及 owner 持久化事务。
- **[ADR-0006](0006-at-least-once-idempotency-and-recovery.md)**：低风险切片无副作用，不涉及幂等与恢复语义；一旦涉及即 escalate。

## 回滚条件

若出现以下任一情况，撤销本 ADR、把 `migration_tier` 收敛为恒 `full_rigor`：

- 发现某个被判定低风险的切片实际造成了生产事实变更或不可逆副作用（派生逻辑存在漏判）；
- 快车道被用于绕过高风险切片的证据要求（escalation 门被规避）。

机器门禁 `derive_max_eligible_tier` / `MIG-TIER-ESCALATION-REQUIRED` 与本 ADR 的豁免/不可豁免清单一一对应，实现变更必须同步修订本 ADR。
