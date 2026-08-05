# ADR-0017：能力总账与 Domain Wave 迁移治理

- 状态：Accepted
- 日期：2026-08-02
- 最近修订：2026-08-03
- 决策范围：rust_quant legacy 到 rust_quant_alpha 的迁移组织、目录归属与验收方式
- 取代范围：ADR-0016 的活跃迁移流程，以及 ADR-0014、ADR-0015 中按细粒度切片重复维护 Manifest、Registry 和源码哈希的流程

## 1. 背景

此前迁移按最小纵向切片推进，每个切片重复创建 Manifest、Evidence、Verdict、Registry 条目并维护源码哈希。该方法能证明单个切片的来源，却产生了三个问题：

1. 迁移顺序容易被局部可实现性牵引，先出现行情同步、回测骨架或执行局部能力，而不是按完整业务闭环推进。
2. 同一 Domain 的业务语义被拆散到很多迁移登记中，难以判断能力是否完整、是否重复实现、是否遗漏 owner。
3. 哈希维护成本高，但不能代替业务语义、因果时序、状态机、恢复责任和真实 parity 验证。

迁移仍未统一切流，因此当前阶段应先冻结完整能力地图和目标目录，再按 Domain 闭环成批迁移。

## 2. 决策

### 2.1 唯一活跃能力总账

rust_quant_alpha/architecture/business-capability-catalog.toml 是目标仓库唯一活跃的能力登记表。

每个能力必须登记：

- 稳定且唯一的 capability id；
- 业务事实 owner；
- 能力种类；
- 唯一目标目录；
- 所属 Domain Wave；
- 迁移状态；
- legacy 处置方式；
- 复用策略；
- legacy 来源；
- 已知消费者。

禁止为同一业务概念另建第二份活跃 Registry、目录表或能力清单。人类可读文档必须引用该总账，不得复制一份容易漂移的平行事实源。

### 2.2 一次 L1 全量盘点，按 Wave 做 L2 深挖

迁移开始前只做一次全量 L1 业务能力盘点，覆盖：

- legacy 入口与主要实现位置；
- 当前业务目的；
- owner；
- 目标 capability id；
- preserve、optimize、defer、retire 或 new 的处置结论；
- 明显的共享能力和外部依赖。

进入某个 Domain Wave 时，再对该 Domain 做 L2 语义深挖：

- 输入、输出与调用方；
- 状态机和不变量；
- 数据表、事件与外部 API；
- 幂等、事务、Outbox、重试和恢复 owner；
- 精度、时序、失败降级与安全门禁；
- legacy golden cases 与 parity 口径。

不再要求每个小切片重复完成整套全仓扫描。

### 2.3 Domain Wave 是迁移批次

迁移批次按业务闭环组织，不按单个文件、trait 或技术组件组织。

冻结顺序如下：

1. W0：能力总账、详细目录、依赖规则、自动校验基线；
2. W1：Market Data 与 Instrument Reference 闭环；
3. W2：Strategy 与 Research/Backtest 闭环；
4. W3：Portfolio、Account 与 Risk 闭环；
5. W4：Execution 与 Reconciliation 闭环；
6. W5：运行入口、运营控制、全链路 parity 与统一切换。

Wave 内仍按 capability-first 拆分代码，但同一批次必须交付可验证的业务闭环，不能以创建空 trait、空目录或局部 DTO 作为完成。

### 2.4 目录按能力细化

详细物理目录以 target-directory-layout.md 为准。

基本规则：

- Domain 内第一层必须是稳定业务能力；生产路径不得使用 services、common、utils、helpers、support、
  `*_helpers` 或 `*_support` 作为业务容器；
- model、policy、commands、queries、ports、consumers 只能出现在具体 capability 内；
- 跨 Domain 只共享明确的 contract 或无业务归属的 quant/platform primitive；
- Adapter 按外部机制和协议拆分，不能承载用户授权、风控、强制止损、lease 等业务判断；
- App 只装配、调度和管理生命周期，不承载业务决策；
- 目录只在有真实代码时创建，禁止预生成空架构。

### 2.5 共享能力采用封闭登记

能力复用策略只能取以下一种：

- owner_only：业务规则只在 owner Domain 实现；
- canonical_shared：仅适用于 quant 或 platform 中无业务 owner 的通用实现；
- boundary_contract：跨边界只共享 contract；
- boundary_adapter：共享外部机制实现，但业务判断仍由调用方 Domain 持有；
- composition_only：仅用于 App 装配。

新增所谓通用能力前，必须先查询能力总账。已存在 canonical_shared 能力时必须复用；不存在时先判断其是否其实属于某个 Domain。只有确实无业务 owner 且至少存在真实消费者时，才允许登记新的 canonical_shared 能力。

### 2.6 哈希只保护必要事实

取消以下日常硬门禁：

- 每个迁移切片的整文件源码哈希；
- 每个 Evidence 文档哈希；
- 每次微调都更新 Registry 哈希。

保留以下哈希或指纹：

- 数据集和 point-in-time universe；
- 策略版本、研究产物和回测身份；
- 跨服务事件或 contract schema；
- 发布制品、镜像 revision 与数据库 migration；
- 明确安全关键且需要防漂移的 golden fixture。

哈希用于证明输入或制品身份，不用于代替业务验收。

### 2.7 每个 Wave 的统一验收

每个 Wave 必须同时满足：

1. 目录归属：能力总账无重复 id、重复 target 或非法 owner；
2. 依赖方向：Domain 不依赖 Adapter 或 App，跨 Domain 仅经允许的 contract；
3. 代码形态：无空架构、万能 Service、超大文件或枚举与业务流程混装；
4. 业务语义：状态机、不变量、精度、因果时序和恢复责任有测试；
5. legacy parity：冻结 golden cases，并记录首个差异层和处置结论；
6. 数据边界：优先复用既有生产表；改表前明确 DDL owner、回填与回滚；
7. 运行安全：实盘路径保留只读预检、幂等、lease、精度过滤、风控和保护单；
8. 可观测性：关键失败、阻塞与恢复证据可定位；
9. 文档一致性：能力状态和 Wave 证据更新；
10. 自动校验：目标仓库架构检查、相关测试和 Wave readiness 全部通过。

静态检查通过只证明结构合规，不能替代行为 parity、数据库验证或运行态证据。

### 2.8 历史迁移登记只读归档

既有 docs/architecture/migrations 下的 Manifest、Evidence、Verdict 和 programs/registry.toml 只在 legacy 源仓库或 Git 历史中保留为历史证据：

- 不在目标仓库继续复制；目标仓库已有副本和对应命令应删除；
- 不为新的 Domain Wave 继续新增同类登记；
- 不要求为非语义变更回填哈希；
- 未完成条目可被新 Wave 重新吸收，其最终状态由能力总账和 Wave 验收决定；
- migration-check 与 migration-registry-check 不再保留运行代码；需要追溯时读取 legacy 源仓库或 Git 历史。

## 3. 迁移完成定义

单个 capability 完成必须满足：

- 状态改为 implemented；
- 目标代码真实存在；
- 旧语义已被 preserve、optimize 或 retire 的明确结论覆盖；
- 至少有一个真实入口或消费者；
- focused tests 通过；
- 不存在第二份并行实现，或并行期有明确截止条件。

单个 Wave 完成必须满足：

- Wave 内所有非 deferred 能力完成；
- Domain 闭环测试和 parity 通过；
- 数据、恢复与可观测性责任闭合；
- 无越权依赖；
- 切流前仍保持 legacy 为事实源，除非该 Wave 获得单独的显式切流授权。

全量迁移完成不等于自动切换。统一切换仍需独立的 readiness 审查、回滚计划和用户授权。

## 4. 影响

### 正向影响

- 迁移进度以业务能力和闭环衡量，而不是以文件数量衡量；
- 新代码有稳定、可查询、可自动校验的唯一归属；
- 减少重复文档、重复 Registry 和无业务价值的哈希维护；
- 共享能力有封闭名单，降低 common/utils 扩散；
- 每个 Domain 可以在完整语义背景下重构，而不是机械复制 legacy。

### 代价

- W0 需要一次性完成较全面的能力盘点和目录冻结；
- 每个 Wave 开始前仍需完成该 Domain 的 L2 语义深挖；
- 能力总账和真实代码必须同步维护；
- 自动检查只能发现结构问题，业务 parity 仍需要高质量测试与证据。

## 5. 被否决方案

### 继续按最小切片维护 Manifest 与源码哈希

被否决。其证据粒度过细，维护成本与业务风险不匹配，并会掩盖 Domain 闭环是否完整。

### 先一次性复制全部 legacy，再统一整理

被否决。会把跨库直连、万能 Service、执行安全缺口和历史耦合直接复制到目标仓库。

### 只冻结目录树，不维护能力总账

被否决。目录只能说明位置，不能说明 owner、处置结论、复用策略、消费者和迁移状态。

### 让 AI 根据当前文件自由判断放置位置

被否决。缺少稳定目录与能力登记时，同一概念会被重复创建，Domain 边界会随任务漂移。
