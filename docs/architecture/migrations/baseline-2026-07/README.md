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

ratchet 语义:读 `legacy-allowlist.toml` 冻结基线,运行违规数 **> 基线即 FAIL**,`≤` 通过。当前 HEAD 跑通即 PASS(违规数 == 基线,自洽)。

### dependency-rules.md §13 覆盖状态

| §13 项 | 规则 | 状态 |
|---|---|---|
| 1/2 | 跨库直连 quant_web/quant_news 禁令 | ✅ 已实现(生产源码连接串扫描,排除测试 fixture;运行时门禁在 sqlx_pool.rs) |
| 3/4(方向) | 依赖方向 / owner-agnostic 反向依赖 | ✅ 已实现(cargo metadata 分层 + V2 显式判定) |
| 10 | 文件行数闸门(1000 WARN / 2000 硬失败) | ✅ 已实现(复用 check_code_file_line_limit.sh 阈值) |
| 11 | quant/* 依赖业务 Domain | ✅ 由方向检查覆盖(V1 analytics→strategies) |
| 17 | legacy signed read-only 账户直读 | ✅ 存续核对(V3/V4/V5 文件存在性) |
| §10.1 | SDK DTO 泄漏(业务 crate `use okx`) | ✅ 已实现(文件级 ratchet,基线 9 文件) |
| §10.2 | 交易/风控热路径 panic(unwrap/expect/panic) | ✅ 已实现(execution/risk 生产区扫描,基线 5 文件) |
| §10.3 | 运行时 DDL(CREATE/ALTER TABLE) | ✅ 已实现(文件级 ratchet,基线 3 文件) |
| 8 | Contract 未声明变化 | ⏳ TODO(需 Contract snapshot 基线,属阶段 2) |
| 13 | evaluator 读账户配置/生成订单数量 | ⏳ TODO(需 AST/语义分析) |
| 18 | 零字段 Service/Manager/Calculator | ⏳ TODO(需 AST) |
| 19 | Aggregate 可变不变量 / Model 读系统时间 | ⏳ TODO(需 AST) |
| 20 | backtest/live 重复实现 | ⏳ TODO(需语义分析) |
| 23/24 | 镜像 binary allowlist / package 依赖闭包 | ⏳ TODO(需 release-units + 镜像检查) |
| 其余 | 5/6/7/9/12/14/15/16/21/22/25/26/27 | ⏳ TODO(需运行时证据或语义分析,留待后续) |

TODO 项不在本次实现,也**不假装已覆盖**(遵守 dependency-rules.md:405)。随迁移推进按需补齐检查器。

## CI 接入

在 CI pipeline 加一步 `cargo xtask arch-check` 即可(退出码非零阻断)。本仓 CI 配置若不在可改范围,则本工具支持本地/pre-commit 运行,不谎称已接 CI。
