# ADR-0010：基于依赖图的构建影响与生产工件隔离

- 状态：已接受
- 首次接受：2026-07-23
- 最近修订：2026-07-28
- 决策者：Rust Quant Core
- 相关决策：[ADR-0001](0001-modular-monolith-and-business-modules.md)、[ADR-0003](0003-explicit-runtime-composition-roots.md)

## 背景

业务 Domain、Cargo package、运行进程、容器镜像和 CI/CD 发布单元不是同一概念。当前 `rust-quant-cli` 同时承载生产、Research、Backtest 与 Paper 入口，任何 `crates/**` 变更都会进入同一条生产构建与部署流水线；仅增加 path filter 不能证明生产二进制没有传递依赖 Research，也不能证明共享 Domain 变更没有漏掉生产回归。

立即把六个生产角色拆成六个服务或六个镜像，会增加发布矩阵和运维成本，却不能自动解决 Cargo 依赖耦合。本 ADR 固定“先拆 Cargo 根包与发布工件、暂不拆微服务”的目标。

## 决策

### 1. 三类 Release Unit

目标仓库定义三个明确发布单元：

| Release Unit | 内容 | 是否可自动部署生产 |
| --- | --- | --- |
| `core-runtime` | control-api、market-worker、signal-worker、account-worker、execution-worker、reconciliation-worker | 是 |
| `core-maintenance` | schema-tool 与经过批准的有界生产维护 Job | 否；只能显式 Job/运维动作 |
| `quant-lab` | Research、Backtest、Analytics、PaperEvent 和研究 CLI | 否 |

Release Unit 是构建和工件边界，不是业务 owner 或网络服务。六个生产角色继续共享一个 `core-runtime` 镜像；它们使用独立 App Cargo package 和独立组合根，但不因此增加六个微服务仓库或六个镜像。

`RecoveryHarness` 不属于第四个 Release Unit，也不是可部署 App。它是 CI-only integration test artifact：只在隔离 runner 中使用临时 Postgres/Redis、Fake/recorded Exchange boundary 和故障注入验证 lease、Outbox、Unknown、保护与 Reconciliation。它可以编译被测 `core-runtime` 业务包及专用 testkit，但不得进入任何镜像 binary allowlist、不得获得生产 Secret、不得连接生产账户或生产存储；每次运行结束后销毁临时存储。影响 Execution、Account、Reconciliation、消息运行时或恢复 Adapter 的变更，必须触发对应 RecoveryHarness suite。

### 2. App Cargo package 边界

目标 App 必须是独立 Cargo package：

```text
apps/
├── control-api/
├── market-worker/
├── signal-worker/
├── account-worker/
├── execution-worker/
├── reconciliation-worker/
├── schema-tool/
└── quant-lab/
```

每个 App package 只声明本职责所需依赖。不得再使用一个拥有全部 Domain、Adapter、SDK、Research 和 Backtest 依赖的总 CLI package 承载所有生产 binary。

允许同一 `core-runtime` 镜像复制六个生产 binary；禁止该镜像包含：

- `quant-lab`；
- backtest/research/optimizer binary；
- PaperEvent 收益或研究入口；
- strategy candidates；
- 通用 shell、数据库客户端或未列入清单的维护工具。

`core-maintenance` 与 `quant-lab` 使用独立工件。`schema-tool` 不进入长期生产容器的 binary allowlist。

### 3. Strategy Catalog 编译边界

Strategy owner 在确有候选/已发布独立生命周期证据时拆成：

```text
strategy-api
strategy-released
strategy-candidates
```

- `signal-worker` 只依赖 `strategy-api + strategy-released`；
- `quant-lab` 可以依赖 `strategy-api + strategy-released + strategy-candidates`；
- candidate 是 `strategy-candidates` 内的 module，不按策略建立 crate；
- promote 是将已批准实现纳入 released catalog、生成新的 Definition/Artifact/Release 身份并重新构建，不是让生产动态加载 candidate；
- production Release Unit 的传递依赖闭包中出现 `strategy-candidates` 时构建直接失败。

### 4. 版本化 Release Unit Manifest

仓库在 `release-units/` 维护机器可读的 Release Unit Manifest：

```text
release-units/
├── core-runtime.toml
├── core-maintenance.toml
└── quant-lab.toml
```

每份 Manifest 至少声明：

```text
release_unit
root_packages
binary_allowlist
forbidden_packages
container_image
production_deployable
required_test_suites
```

Manifest 是 CI、Dockerfile、Compose 和 deploy contract 的共同输入。不得分别手写四份相互独立的 binary/service 清单。

### 5. Build Impact 由依赖图决定

目标命令：

```bash
cargo xtask build-impact --base <sha> --head <sha>
```

算法：

1. 从 Git diff 找出变更文件；
2. 将文件映射到 owning Cargo package 或 Release Unit infrastructure；
3. 通过 `cargo metadata` 计算反向传递依赖闭包；
4. 将受影响 root package 映射到 Release Unit；
5. 输出必须执行的 verify、image build、deploy eligibility 和原因；
6. 无法归属、Cargo graph 解析失败或 Manifest 漂移时 fail closed，至少标记所有生产单元受影响。

Path filter 只允许作为“是否启动 impact job”的粗粒度优化，不能代替依赖图。

以下变更默认影响全部 Release Unit：

- `Cargo.lock`、toolchain、workspace 根 Cargo 配置；
- proc-macro、build script 或共享编译配置；
- Release Unit Manifest、公共 Docker base、CI impact 逻辑；
- 无法确定 owner 的构建输入。

纯文档变更不构建代码，但修改正式 Contract snapshot、migration 或部署清单时按其 owning Release Unit 处理。

### 6. CI/CD 矩阵

| 变更 | 必须验证 | 可以构建/部署 |
| --- | --- | --- |
| quant-lab、Research、Backtest、Analytics | quant-lab tests、replay、Evidence | 只构建 quant-lab 工件；不得部署生产 |
| strategy-candidates | candidates、quant-lab、Research parity | 不构建 core-runtime |
| strategy-released、Strategy API | Strategy、quant-lab、受影响生产 App、parity | 构建 core-runtime；主分支可部署 |
| 共享 Market/Portfolio/Risk/Execution/Contract/Platform | 受影响生产 App、Research、parity、contract/recovery | 构建受影响生产工件 |
| 单个生产 App 组合根 | 该 App、deploy contract | 构建 core-runtime |
| schema/migration | schema compatibility、目标 owner integration | 构建 core-maintenance；是否构建 runtime 由依赖图决定 |

Research job 不得获得 production environment、SSH deploy Secret、生产交易凭证或生产部署权限。只有 `core-runtime` 新工件及其所有生产门禁通过后，`deploy-stable` 才可运行。

### 7. 工件身份与内容证明

每个工件发布以下不可变身份：

- Git revision；
- Cargo.lock hash 与 Rust toolchain；
- Release Unit Manifest hash；
- root package 和 binary allowlist；
- 传递依赖图 hash；
- binary checksum；
- 容器镜像 digest；
- SBOM/供应链元数据。

Deploy contract 必须验证镜像中 binary 集合与 allowlist 完全一致：缺少必需 binary 或出现 Research/Backtest/Paper/候选 binary 都失败。

Research 每次可用于正式 Evidence 的运行必须把实际执行工件固定为 `ResearchExecutionArtifactRef`，至少包含：

- Git revision；若使用未提交补丁，还包括精确 patch hash；
- Cargo.lock、toolchain、target triple、build profile、启用的 Cargo features；
- owning package、入口 binary/test target 与其 checksum；
- Release Unit Manifest（若适用）和传递依赖图 hash；
- PRNG 算法/版本与 deterministic scheduler 版本。

`ResearchRunSpec` 必须引用该对象，不能只记录“代码版本”字符串，也不能在运行后用新的构建结果补写。只有同一 `ResearchExecutionArtifactRef`、同一输入和同一数值后端，才可要求字节级重放；跨 target/数值后端比较必须使用事前声明的数值容差和首次差异层。

候选策略进入 `strategy-released` 时会产生新的生产构建，不能假设它与被研究的 candidate binary 自动等价。Strategy owner 必须发布不可变 `PromotionReceiptV1`，串联：

- candidate Definition/Artifact 与 `ResearchExecutionArtifactRef`；
- Completed ResearchEvidence 及其评价门结果；
- released Definition/Artifact、source revision、binary/image digest；
- candidate 与 released 构建之间的公开业务 API、规范 planning 输出和允许数值容差 parity；
- 批准身份、时间、目标 release stage/channel 与回滚引用。

缺少该 receipt、跨构建首次差异未解释，或 released 工件无法追溯到被研究源码时，不得发布 `ActivationEligibilityV1`。`Completed ResearchEvidence` 只表示运行与工件完整可见，不自动表示通过评价门或具备 promotion eligibility。

## 结果

### 正面影响

- 新增候选策略或 Research 模拟不会无意义重建、发布生产镜像；
- 共享业务规则变化仍会进入生产构建和 parity；
- 生产镜像不携带 Research、Backtest 和候选执行入口；
- 保留六角色共享镜像，避免过早增加微服务和镜像矩阵；
- CI 结论可以由 Cargo 依赖图和版本化清单重放。

### 代价

- 需要将总 CLI 拆为独立 App package；
- 需要维护 Release Unit Manifest 和 build-impact 工具；
- Cargo.lock、toolchain 等基础变更会触发较宽验证；
- 迁移期新旧 Docker/CI 路径需并行验证后切换。

## 被否决的方案

### 只使用 GitHub Actions path filter

无法识别传递依赖，也无法证明生产工件不含 Research。

### 每个策略一个服务或镜像

会造成部署单元爆炸，并把代码 owner 问题变成网络和运维问题。

### 保持总 CLI，只在 Dockerfile 选择部分 binary

Cargo package 仍共享全部依赖，Research 变化仍会污染构建影响判断，无法提供编译级边界。

### 任何代码变更都构建并部署生产

安全但成本高，并让候选研究与生产发布生命周期继续耦合。

## 验证

- 修改 `strategy-candidates` 时，impact report 不包含 `core-runtime`；
- 修改 Risk/Execution 公共 API 时，impact report 同时包含 `core-runtime` 与 `quant-lab`；
- 生产镜像 binary allowlist 不含 Research、Backtest、Paper 或 schema-tool；
- production App 的 Cargo 传递依赖闭包不含 candidates、Research、Backtest、Analytics；
- Research workflow 无 production environment 与 deploy Secret；
- RecoveryHarness 只作为 CI 临时测试工件运行，不进入任何镜像或生产 Secret scope；
- `ResearchRunSpec` 可解析到完整 `ResearchExecutionArtifactRef`，相同执行工件可确定性重放；
- candidate 晋级 produced `PromotionReceiptV1`，且 released 构建与被研究候选的业务输出差异符合事前声明；
- Manifest、Dockerfile、Compose、runtime service list 和实际镜像内容一致；
- impact 无法判定时 fail closed，而不是静默跳过生产验证。
