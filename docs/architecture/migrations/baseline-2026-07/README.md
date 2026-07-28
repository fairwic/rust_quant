# 阶段 0/1 迁移前准备产物(baseline-2026-07)

- 冻结日期:2026-07-27
- `architecture_baseline_git_sha`:`660407f438db52acde8318e705b26ad05948aa0a`
- 上位:[迁移计划 阶段 0/1](../../migration-plan.md)、[依赖规则 §13](../../dependency-rules.md)

本目录是迁移正式开始前的准备产物:冻结当前基线 + 落地防腐 ratchet 闸门。**不搬任何业务、不建目标目录空壳。**

## 产物清单

| 文件 | 内容 |
|---|---|
| [dependency-graph.md](dependency-graph.md) | 阶段 0:14 crate 依赖图、规模、已知违规基线 V1–V5 |
| [owner-ledger.md](owner-ledger.md) | 阶段 0:现有 crate/module → 目标 owner 映射矩阵 + services 拆分线 |
| [runtime-topology.md](runtime-topology.md) | 阶段 0:七个 quant_core_* 角色 bin → app → 装配冻结 |
| [legacy-allowlist.toml](legacy-allowlist.toml) | 阶段 1:ratchet 输入基线(V1–V5 + 删除条件 + 复查日期) |
| [duplication-and-wheel-reinvention.md](duplication-and-wheel-reinvention.md) | 重复造轮子 / 未用外部标准版实证台账(W1–W6 未用外部版 + D1–D8 内部重复 + 规模速览) |

## 防腐闸门 `cargo xtask arch-check`

只读架构检查器(`xtask/` crate,不进生产依赖图)。用法:

```bash
cargo xtask arch-check          # 人读摘要
cargo xtask arch-check --json   # 机器可读
# 等价(无需 alias):
cargo run -p xtask -- arch-check
```

`cargo xtask` 别名定义在 `.cargo/config.toml`（该路径被 `.gitignore` 忽略，不随仓库分发）。clone 后如需该别名，自行在 `.cargo/config.toml` 加：

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

或直接用上面的 `cargo run -p xtask -- arch-check`，无需别名。

ratchet 语义:读 `legacy-allowlist.toml` 冻结基线,运行违规数 **> 基线即 FAIL**,`≤` 通过。这里的 `PASS` 只表示**当前已扫描的 legacy 规则没有新增违规**，不表示目标目录、目标依赖图、Release Unit 或 CI 已受保护。2026-07-28 在 `9952c748d029` 加当时未提交工作树运行结果为 PASS（2 个依赖方向、3 个 legacy 路径、9 个 SDK DTO、5 个热路径 panic、3 个 runtime DDL 均为基线内，43 个文件行数 WARN）；输出同时提示 baseline SHA 为 `660407f438`。该结果必须在提交后的 revision 重新生成，不能充当首个物理迁移切片的 Verdict。

## 目标目录 anti-corruption P0（首个物理迁移前必须完成）

当前 `xtask` 的有效范围仍是 14 个 legacy package：依赖方向表按旧 package 名硬编码，未知 package 直接跳过；文件大小只扫描 `crates/`；SDK DTO、panic 和 runtime DDL 扫描也只列出旧目录；baseline SHA 漂移只提示，修改 allowlist 本身不会要求 Manifest。因此，在第一次把代码放入 `apps/` 或 `crates/{domains,quant,contracts,adapters,platform}` 之前，必须先完成以下 P0：

1. 以机器可读 role map 覆盖每个 workspace package/path 和 Release Unit；除 `xtask` 等显式技术豁免外，未知 package 必须 FAIL，不可静默忽略；
2. 让文件大小、SDK DTO、热路径 panic、runtime DDL、跨库访问和依赖方向扫描同时覆盖 legacy 与目标源码根，尤其是 `apps/`；
3. baseline/allowlist 变更必须关联独立 Migration Manifest、冻结架构基线和当前 revision 的 Evidence，不能靠扩白名单恢复绿灯；
4. 为每种 target role 和 `apps/*` 注入一条故意违规的测试，证明检查器真能失败；
5. 在可追溯 CI 中执行门禁。当前仓库版本未发现受跟踪的 `.github`、`.gitlab-ci.yml`、Jenkins 或 Azure pipeline 配置，不能宣称已接入 CI。

P0 完成前，`arch-check PASS` 只能作为 legacy ratchet 证据；任何 target-layout `structure_only` Manifest 都应保持 `blocked`。

### dependency-rules.md §13 覆盖状态

| §13 项 | 规则 | 状态 |
|---|---|---|
| 1/2 | 跨库直连 quant_web/quant_news 禁令 | ✅ 已实现(生产源码连接串扫描,排除测试 fixture;运行时门禁在 sqlx_pool.rs) |
| 3/4(方向) | 依赖方向 / owner-agnostic 反向依赖 | ⚠️ legacy package 名已实现；target package role map/未知 package fail-closed 为 P0 |
| 10 | 文件行数闸门(1000 WARN / 2000 硬失败) | ⚠️ legacy `crates/` 已实现；`apps/` 与 target root 覆盖为 P0 |
| 11 | quant/* 依赖业务 Domain | ✅ 由方向检查覆盖(V1 analytics→strategies) |
| 17 | legacy signed read-only 账户直读 | ✅ 存续核对(V3/V4/V5 文件存在性) |
| §10.1 | SDK DTO 泄漏(业务 crate `use okx`) | ⚠️ legacy 目录已实现(文件级 ratchet,基线 9 文件)；target Domain/Quant 覆盖为 P0 |
| §10.2 | 交易/风控热路径 panic(unwrap/expect/panic) | ⚠️ legacy execution/risk 生产区已实现(基线 5 文件)；target execution/risk 覆盖为 P0 |
| §10.3 | 运行时 DDL(CREATE/ALTER TABLE) | ⚠️ legacy 列表目录已实现(基线 3 文件)；target Adapter/App 覆盖为 P0 |
| P0 | target role map、unknown package、`apps/` 扫描、baseline 完整性 | ❌ 未实现；见本 README 的 P0 节和 [xtask 路线图](xtask-roadmap.md) |
| 8 | Contract 未声明变化 | ⏳ TODO(需 Contract snapshot 基线,属阶段 2) |
| 13 | evaluator 读账户配置/生成订单数量 | ⏳ TODO(需 AST/语义分析) |
| 18 | 零字段 Service/Manager/Calculator | ⏳ TODO(需 AST) |
| 19 | Aggregate 可变不变量 / Model 读系统时间 | ⏳ TODO(需 AST) |
| 20 | backtest/live 重复实现 | ⏳ TODO(需语义分析) |
| 23/24 | 镜像 binary allowlist / package 依赖闭包 | ⏳ TODO(需 release-units + 镜像检查) |
| 其余 | 5/6/7/9/12/14/15/16/21/22/25/26/27 | ⏳ TODO(需运行时证据或语义分析,留待后续) |

TODO 项不在本次实现,也**不假装已覆盖**(遵守 dependency-rules.md:405)。随迁移推进按需补齐检查器。

## CI 接入

在 CI pipeline 加一步 `cargo xtask arch-check` 才能让退出码阻断合并；P0 完成后还必须执行 target role 注入测试和 Manifest/baseline 完整性检查。当前仓库版本没有发现受跟踪的 CI pipeline 配置，因此这里只提供本地/pre-commit 用法，不谎称已接 CI。
