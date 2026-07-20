# AI 编码与架构防腐护栏

- 状态：已接受
- 日期：2026-07-20
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 放置规则：[业务代码与数据访问放置规范](business-code-and-data-access.md)

## 1. 目标

目录只能降低混乱概率，不能阻止 AI 把业务规则写进 Handler、把 SQL 写进 Use Case，或创建新的万能 Service。本规范将架构从“建议”变成可审查、可测试、可渐进自动执行的约束。

## 2. 规则权威顺序

发生冲突时按以下顺序处理：

1. 系统、用户和仓库安全规则；
2. 已接受 ADR；
3. `target-architecture.md`；
4. `dependency-rules.md` 与 `business-code-and-data-access.md`；
5. 子目录 `AGENTS.md` 的增量规则；
6. 模板和示例。

子目录规则只写相对上层的差异，并链接权威文档。禁止复制整份架构规范，否则多份文本会在后续修改中漂移。

## 3. 编码前强制输出

任何新增功能、重构或跨文件修改开始前，AI 必须先声明：

```text
业务目标：
唯一 Owner：
切片：Command / Query / Event Consumer / Pure Policy
要保持的不变量：
输入入口与 Contract：
Use Case：
Model / Policy：
Ports：
Adapters：
运行模式与替换的 Adapter：
事务边界：
幂等身份：
失败与恢复 Owner：
验证：
```

出现以下情况时必须停止并请求确认：

- 同一事实可能由两个 owner 写入；
- 需求要求跨库直写或 Admin/Web 绕过 Core；
- 需要改变 Order、Fill、Protection、Risk、Release 等状态机语义；
- 需要实盘 mutation；
- 现有 Contract 无法表达且可能影响其他仓库；
- 迁移同时改变目录和策略/风控/执行行为，无法分开验证。
- Strategy evaluator 需要账户余额、用户风险配置、最终下单数量或环境变量才能运行。

## 4. 默认生成单位：垂直切片

AI 不按“先建所有 model，再建所有 repository，再建所有 service”的横向批量方式生成。一次只完成一个最小垂直切片：

```text
入口映射
  -> Use Case
  -> Model/Policy
  -> Port
  -> Adapter
  -> 必要 Contract
  -> Tests
```

每个切片必须能独立说明：输入、业务结果、数据库变化、外部副作用、失败状态和恢复方式。

## 5. 三个 Golden Template

目标仓库只维护三个模板：

- `templates/command-slice`：状态变化、事务、幂等和 outbox；
- `templates/query-slice`：只读模型、索引、分页和陈旧度；
- `templates/event-consumer`：合同版本、inbox、顺序、ack 和重放。

模板只包含最小骨架和一个通过测试的示例，不包含业务万能基类。新增第四种模板必须有重复模式证据和 ADR。

## 6. 自动门禁

目标命令为：

```bash
cargo xtask arch-check
```

在实现前，文档只能称其为“目标门禁”，不能宣称已经存在。实现后至少检查：

1. 路径与 crate 分区；
2. Domain 对 SQLx、Redis、Reqwest、SDK、环境变量和 Contract 的禁止依赖；
3. Contract 对 Domain、SQLx 和 SDK 的禁止依赖；
4. App 以外的环境变量读取；
5. 跨 owner SQL 和无 owner migration；
6. 新增 `common`、`utils`、`helpers`、`BaseService`、泛型 Repository；
7. Testkit 被生产代码依赖；
8. 未版本化跨进程 payload；
9. 新增/触碰文件行数上限；
10. 必需的 owner 声明、数据库注释与 Contract snapshot。
11. `quant/*` 依赖业务 Domain、Adapter、数据库或环境变量；
12. Strategy evaluator 接收账户风险配置，或产生最终仓位/订单数量；
13. 策略状态 key 缺少 RuntimeSnapshotId、MarketStreamId 或等价版本身份。
14. 生产 Domain 依赖 Research，或 Research 绕过其他 Domain 公开 API；
15. ResearchBar 声称覆盖 lease、outbox、Unknown、保护恢复或 Reconciliation；
16. 多币种回测按 symbol 遍历顺序逐个分配资金；
17. ResearchEvidence 由 Strategy 表直接拥有，或跨对象存储/Postgres 宣称全局原子。

## 7. 渐进 Ratchet

当前 Workspace 已有扁平 crate、跨层依赖和 legacy 数据路径，不能直接以最终规则扫描全仓并长期红灯。

实施顺序：

1. 生成只读违规基线；
2. CI 禁止新增违规；
3. 每迁移一个 golden slice，删除对应白名单；
4. 白名单必须包含 owner、原因、删除条件和最晚复查日期；
5. 违规总数只能下降，不能通过扩大 glob 或忽略目录恢复绿灯；
6. 最终删除 legacy allowlist。

小型 legacy bugfix 可以留在原位置，但不得新增跨层依赖、扩大 API 或把新能力继续堆入 legacy。新增业务能力默认进入目标架构。

## 8. Review 检查表

### 8.1 边界

- 是否只有一个事实 owner；
- 是否放入正确 Domain 和切片；
- 是否出现跨 Domain 私有依赖、跨库 SQL 或共享 Row；
- 是否把技术失败误写成业务状态。

### 8.2 数据库

- SQL 是否只在 Postgres Adapter；
- Port 是否使用业务语言；
- 事务是否覆盖状态、幂等和 outbox；
- 查询是否有索引、范围、分页和锁评估；
- 新表/列是否有数据库注释；
- 删除是否符合事实保留规则。

### 8.3 交易安全

- 是否先持久化稳定订单身份；
- 是否区分 read-only、dry-run、paper、shadow、canary、live；
- 是否保留 lease、精度、余额、凭证、新鲜度和保护门禁；
- 部分成交后保护数量是否正确；
- `Unknown`、撤单/成交竞态和重启是否可恢复；
- 没有有效止损计划时是否 fail-closed。

### 8.4 Contract

- Owner 与版本是否明确；
- Domain 是否与 Wire DTO 解耦；
- 是否有旧 payload、未知字段和 snapshot 测试；
- event/correlation/causation/idempotency/aggregate/sequence 是否完整。

### 8.5 测试

- Model/Policy 单元测试是否固定业务不变量；
- Adapter 集成测试是否覆盖 SQL、约束和事务；
- Contract test 是否覆盖跨仓库兼容；
- Recovery test 是否覆盖重复、超时、崩溃、乱序和对账；
- Parity test 是否证明 backtest/paper/live 使用相同业务规则。

### 8.6 策略与回测防漂移

- Strategy evaluator 是否只消费 Market 证据、Strategy Runtime Snapshot 和自己的 Evaluation State；
- 候选止损/失效价是否仍是信号证据，而不是偷偷完成最终风险审批；
- 资金分配比例、真实 leverage、最大亏损与订单数量是否由不同 owner 明确建模；
- Research use case 是否调用与 paper/live 相同的 Strategy、Portfolio、Risk 和必要 Execution 公开 API；
- `quant/backtest` 是否仍保持无 Domain 依赖的纯模拟内核；
- 是否固定 DatasetManifest、预热长度、SimulationProfile、费用、滑点、资金费、Seed 和所有政策版本；
- EvaluationStateKey 是否包含 EvaluationScopeId，确保并行 Run 不共享可变状态；
- 同一 decision time 的多币信号是否先收集后统一分配，symbol 重排是否不改变结果；
- parity 是否逐层比较 Signal、Target、RiskDecision、OrderIntent 和 FillEvent，而不是只比较最终 PnL；
- ResearchBar、PaperEvent、RecoveryHarness 是否各自只声称覆盖其精度边界；
- 模拟成交是否只进入 SimulationLedger/ResearchEvidence，未污染生产订单/账户事实；
- Evidence 是否通过内容寻址对象 + Research owner Completed manifest 实现原子可见，而非虚构跨存储原子；
- 环境变量是否只在 `quant-lab` App 解析后映射成强类型 ExperimentSpec。

Vegas 的具体基线与迁移门见 [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md)。

## 9. 文档与实现同步

以下变化必须同步文档或 ADR：

- 新 Domain、App、Contract major version；
- Owner 转移；
- 新的跨仓库写入链路；
- 状态机语义改变；
- 事务原子性改变；
- 新消息中间件或新一致性模型；
- `risk-worker`、`portfolio-worker` 等新运行角色；
- Postgres Adapter 拆 crate；
- 任何架构例外。
- SimulationProfile 能力边界、ResearchEvidence owner 或 Evidence 发布协议变化。

普通函数新增不要求更新架构文档；文档不应成为逐文件清单。

## 10. 禁止用文档代替执行

以下表述只有获得新鲜证据后才能使用：

- “架构门禁已启用”；
- “所有依赖已符合目标”；
- “Web 不再拥有订单事实”；
- “恢复测试已覆盖全部状态”；
- “迁移完成”。

在对应代码、CI、Contract、测试和生产迁移完成前，必须明确写成“目标”“计划中”或“legacy 仍存在”。
