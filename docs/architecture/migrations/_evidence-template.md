# Migration Evidence：MIG-XXXX

> 复制到 `docs/architecture/migrations/<migration-id>-<slug>/evidence.md` 后填写。没有新鲜证据的项目必须标记“未验证”，不得写成通过。

## 1. 身份

| 字段 | 值 |
| --- | --- |
| Migration program / Owner child | `MP-XXXX / MIG-XXXX / Owner` |
| Program registry / owner repository | `docs/architecture/migrations/programs/registry.toml / <repo>` |
| 历史记录可作依赖 | `false`（`historical_characterization` 必须为 `false`） |
| Migration ID | `MIG-XXXX` |
| Manifest | `docs/architecture/migrations/.../manifest.toml` |
| Manifest SHA-256 | 待填写 |
| Evidence SHA-256 | 由最终 checker 计算；正文变更后必须重算 |
| Machine Verdict | `verdict.json`（由 `migration-check` 生成，禁止人工编辑） |
| Verdict schema / checker revision | 待填写 |
| 被测试代码 commit revision | 待填写 |
| 被测试范围补丁 SHA-256 | 无未提交补丁则填 `none` |
| 架构基线 revision | 待填写 |
| 规范文档 / Release Unit / Contract snapshot hash | 待填写 |
| Contract snapshot objects | `[{contract_id, version, path, sha256, producer, consumers, compatibility_window}]` |
| 迁移模式 | `structure_only / behavior_change / cutover / legacy_delete` |
| 技术 state | `draft / baseline_frozen / implementing / verified / ready_for_cutover / completed / blocked` |
| Promotion status | `not_applicable / research_only / candidate / promoted / rejected` |
| Cutover status | `not_required / not_ready / ready / in_progress / completed / rolled_back` |
| Verification mode | `manual / machine` |
| Evidence 生成时间 | 待填写 |

### 1.1 可重放工件

| 类型 | 运行前/后 | 不可变 ref | SHA-256 | 用途 |
| --- | --- | --- | --- | --- |
| 输入 | 运行前 | 待填写 | 待填写 | DatasetManifest / Snapshot / fixture |
| 输出 | 运行后 | 待填写 | 待填写 | report / trace / Verdict |

`dynamic_input_artifacts` 只能列出运行前固定的输入，不能引用本文件或本次运行会改写的输出。缺少 code revision、补丁 hash、输入或输出 hash 时，只能记为“未验证/阻塞”，不得把该 Evidence 当作当前 HEAD Verdict。

### 1.2 Predecessor Verdict

| 前置 Migration | Manifest SHA-256 | Evidence SHA-256 | Verdict ref / SHA-256 | Tested revision | 计算结果 |
| --- | --- | --- | --- | --- | --- |
| 待填写 | 待填写 | 待填写 | 待填写 | 待填写 | `satisfied / blocked` |

依赖是否满足只能由 `migration-check` 读取这些不可变对象计算；不得抄写 Program Registry 中的人工布尔值。

## 2. 范围核对

- 唯一 Owner：
- 父迁移计划与 `depends_on`：
- Source paths：
- Target paths：
- 实际修改文件：
- Manifest 外修改：
- 受影响 Cargo package：
- 受影响 Release Unit：
- Capability / `api` / `spi` 可见面：
- Contract/表/运行入口：
- Contract producer/consumer/version/兼容窗口：
- Claim/Renew/Release/Outcome command 与 receipt 方向、current `claim_fence`、expiry/CAS 字段：
- 本 Owner 本地写集、事务、Outbox/InBox：
- 跨 Owner 交接（不得有跨库大事务）：

结论：`通过 / 阻塞 / 未验证`

## 3. 迁移前基线

### 3.1 真实调用链

```text
待填写
```

### 3.2 Characterization

- 固定输入：
- 配置/Snapshot/Context identity：
- Research execution artifact / Promotion receipt（若适用）：
- 当前输出：
- 错误与状态迁移：
- 默认值、单位、舍入和时间语义：
- 数据库事务、唯一约束和 Outbox：
- 当前 Cargo 依赖和 binary：

### 3.3 基线命令

```bash
# 待填写可重复执行命令；不得包含 Secret。
```

## 4. 实施结果

- 目标路径：
- 保持的业务不变量：
- 允许差异：
- 实际差异：
- Owner/capability/API-SPI/Port/Adapter 变化：
- 非测试 Port 的生产 Use Case 调用方、生产 Adapter、失败/原子性/恢复证据：
- 文件预算：生产代码行 / 总行数 / façade / tests；超限文件的 structure-only 处置：
- Contract/Schema 变化：
- 本 Owner 事务、Inbox/Outbox 与恢复变化：
- 跨 Owner Contract/版本交接：
- live 前置 Contract/Verdict（若适用）：ExecutionAccountBinding、RequiredMarketEvidence、BarFinalization/MarketDecisionReadiness/ResolvedMarketEvidenceSet、AccountAdmission/Fact/Recovery、RiskValuation、ExchangeCapability、SafetyMonitoring/Ack：
- 外部副作用：
- 固定 Market 公共 Key access evidence（若使用）：非敏感 key ref、owner、endpoint/method、时间、evidence ref/hash、无用户 credential fallback：

## 5. 验证结果

| 验证 | 命令/证据 | 结果 | 首次差异或备注 |
| --- | --- | --- | --- |
| Unit | 待填写 | 未验证 | |
| Integration | 待填写 | 未验证 | |
| Contract | 待填写 | 未验证 | |
| Parity | 待填写 | 未验证 | |
| Recovery | 待填写 | 未验证 | |
| Build impact | 待填写 | 未验证 | |
| Deploy contract | 待填写 | 未验证 | |
| `git diff --check` | 待填写 | 未验证 | |
| API/SPI + Port completeness + file budget | 待填写 | 未验证 | |

## 6. Parity

- 四个 Policy Snapshot identity：
- Context identity：live 使用 `ExecutionDecisionContextSnapshot`；Research 使用 `ResearchDecisionContextSnapshot`，不得混写：
- ResearchExecutionArtifactRef / PRNG / scheduler version：
- 动态 Market/Account/Instrument Evidence：
- EvaluationState before：
- Clock/Seed：
- 比较层：
- 首次差异层：
- Exact parity / Scenario comparison：
- 若为 Scenario comparison，不能作为 Exact parity 的缺失输入：
- B0 test-only evidence provider（若适用）：bundle ref/hash、Market/Account/Instrument input hash、Clock/Seed、确认无网络/DB 写入/运行时装配：

## 7. 架构与工件

- `arch-check`：
- Cargo 反向依赖：
- Release Unit build-impact：
- 生产 binary allowlist：
- forbidden package：
- 实际镜像内容：

## 8. Shadow、Cutover 与 Rollback

- Shadow 是否无双副作用：
- 接受阈值：
- 切换前事实源：
- 切换后事实源：
- Cutover 授权：
- Rollback 入口：
- Rollback 窗口：
- 生产状态：`未切换 / 已授权待切换 / 已切换并验证`
- 研究验证时必须单列：`state = verified`、`promotion_status = research_only`、`cutover_status = not_required`；不要把三者拼成一个状态字符串。

## 9. Legacy Ratchet

| 项目 | 迁移前 | 迁移后 | 证据 |
| --- | --- | --- | --- |
| Allowlist 条目 | | | |
| 旧调用方 | | | |
| 旧配置 | | | |
| 旧表写入 | | | |
| 旧任务/监控 | | | |

## 10. 阻塞与未完成项

- 阻塞原因：
- 需要的决策：
- 未验证项：
- 不在本切片范围的债务：

只有技术 `state = blocked` 时，阻塞原因才可以非空；研究未晋级、策略门禁失败或 Cutover 不适用分别写在 `promotion_status`、研究结论和 `cutover_status`，不能伪装成技术阻塞。

## 11. Verdict

- 技术 state：`draft / baseline_frozen / implementing / verified / ready_for_cutover / completed / blocked`
- Promotion status：`not_applicable / research_only / candidate / promoted / rejected`
- 技术结论：`通过 / 阻塞 / 未验证`
- Cutover eligibility：`允许 / 不允许 / 不适用`
- Legacy delete eligibility：`允许 / 不允许 / 不适用`
- Machine Verdict ref / SHA-256：
- Predecessor Verdict 计算摘要：
- 是否含敏感数据：`否`
- 结论依据：
