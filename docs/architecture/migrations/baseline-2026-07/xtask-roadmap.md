# xtask arch-check 强化路线图

- 状态:规划中(未实现)
- 制定日期:2026-07-27
- 上位:[baseline README](README.md)、[dependency-rules §13](../../dependency-rules.md)

当前 `cargo xtask arch-check` 已实现 4 类静态检查(依赖方向、跨库直连、文件行数、legacy 路径存续)。本文列出可继续静态落地的候选检查,以及明确做不到、需 AST/语义分析的项。每项标注**当前基线量级、误报风险、实现复杂度**,供决定实现批次。

所有新检查沿用现有 ratchet:先冻结当前违规基线(写入 `legacy-allowlist.toml`),只允许违规数下降,禁止新增。

## 一、可静态落地(regex / 行扫描 / Cargo 解析)

基线量级为 2026-07-27 当前 HEAD 快速扫描估值,实现时以精确扫描为准。

| 键 | 检查 | 查法 | 基线量级 | 误报风险 | 复杂度 | 对应坏习惯 |
|---|---|---|---|---|---|---|
| C | **SDK DTO 泄漏** | 业务 crate(domain/indicators/strategies/risk/execution/trading/orchestration)出现 `use okx` | ~11 | 低 | 低 | 类别 3-C(最干净) |
| D | **热路径 panic** | execution/risk 非测试代码 `.unwrap()`/`.expect(`/`panic!` | ~14(粗,含测试) | 中(需剔除 `#[cfg(test)]` 与内联测试) | 中 | 类别 2-D(实盘安全) |
| I | **运行时 DDL** | crates/ 出现 `CREATE TABLE`/`ALTER TABLE` 字面量 | 6 文件 | 低 | 低 | 类别 3-I |
| — | **SELECT \*** | SQL 里 `SELECT *` | ~32 | 低 | 低 | SQL 纪律 |
| L | **业务层散读 env** | domain/indicators/strategies 出现 `std::env::var` | 6 | 低 | 低 | 类别 3-L / env flag |
| B | **domain 端口返回 Value** | crates/domain/src/traits 签名含 `serde_json::Value` | 7 | 中(需区分参数/返回 vs 存证字段) | 中 | 类别 2-B |
| N | **git 依赖无 rev** | 解析 Cargo.toml,`git=` 无 `rev/tag/branch` | 1(hyperliquid_rust_sdk) | 无 | 低 | 类别 3-N |
| F | **glob 导入** | 生产 src `use ...::*`(排除 `super::*`/`crate::*`/测试) | ~158 | 中(测试模块 glob 合法) | 中 | 类别 2-F |
| — | **println 当日志** | src(非 bin/tests/examples)`println!`/`eprintln!` | ~323 | 高(CLI 输出、诊断合法) | 中 | 可观测性 M |

### 实现优先级建议

- **第一批(高价值三项)—— ✅ 已实现(2026-07-27)**:C(SDK DTO 泄漏,基线 9 文件)、D(热路径 panic,基线 5 文件)、I(运行时 DDL,基线 3 文件)。文件级 ratchet,基线冻结在 `legacy-allowlist.toml` 的 `[file_baselines]`,已验证注入新增 FAIL、清理 resolved。
- **第二批(SQL/配置纪律)**:SELECT \*、L(业务层 env)、N(git 无 rev)、B(domain Value 端口)。基线小、复杂度低到中。
- **第三批(需大量基线豁免,谨慎)**:F(glob 导入,158)、println(323)。基线大,冻结成本高,ratchet 收益偏弱,建议末位或先只 WARN 不计 ratchet。

### 重复造轮子 / 未用外部标准版(来自 [duplication 台账](duplication-and-wheel-reinvention.md))

| 键 | 检查 | 查法 | 基线量级 | 误报风险 | 复杂度 |
|---|---|---|---|---|---|
| R | **指标绕过 indicators** | `indicators` crate 外定义 `fn (atr\|ema\|rsi\|bollinger)_at` / `compute_rsi` | 6(ATR)+4(EMA)+N | 低 | 低(文件级 ratchet) |
| S | **round_price 精度魔数** | 非测试代码出现 `* 10_000.0).round()` | 9 | 低 | 低(文件级 ratchet) |
| U | **candle 结构体泛滥** | `struct \w*Candle\w* {` 定义计数 | 8 | 中(DB 实体合法) | 低(计数 WARN) |
| Q | **手写重试退避** | 研究/CLI 层 `sleep(` + `attempt` 局部循环 | 9+ | 中(合法轮询) | 中(先 WARN 不 ratchet) |

重试(Q)误报中先只 WARN;R/S 文件级 ratchet 冻结当前份数、只许减。回测循环去重(D1)、信号合一(D8)、f64→Decimal(W)需 AST/类型判定,见下表。

## 二、暂时做不到(需 AST / 类型信息 / 语义分析,regex 会大量误报)

老实登记,不假装能查:

| 坏习惯 | 为什么静态查不准 | 未来手段 |
|---|---|---|
| **f64 做金额**(类别 2-A) | 需类型信息判断某 f64 字段/变量是否表示金额;纯名字匹配(price/amount)漏报误报都高 | `syn` AST + 领域类型标注,或引入 `Money` newtype 后查 f64 |
| **零字段 Service**(类别 1) | 需解析 struct 定义体判断是否零字段 + 是否纯命名空间 | `syn` AST 解析 struct/impl |
| **unwrap_or_default 压平金额**(类别 2-E) | 需判断作用对象是否金额/仓位类型 | AST + 类型 |
| **fire-and-forget spawn**(类别 3-H) | 需数据流分析判断 `tokio::spawn` 返回的 JoinHandle 是否被丢弃 | AST + 简单数据流 |
| **密钥字段 derive(Debug/Serialize)**(类别 3-K) | 需解析 struct 字段名 + derive 属性关联判断 | `syn` AST |
| **跨 await 持锁** | 需控制流分析 lock guard 生命周期是否跨 `.await` | 需 MIR/clippy 级分析 |
| **同名两义配置**(结构 3) | 需跨 crate 类型解析判断同名不同结构 | AST + 符号表 |

这些的正确落地路径是:接入 `syn` 写 AST 级检查,或做 clippy 自定义 lint。属独立较大工程,不在本 regex 检查器范围,另立计划。

## 三、通用强化(不新增规则,提升现有检查器质量)

- 统一"排除测试代码"的判定(`#[cfg(test)]` 块、`tests/` 目录、`mod tests`、文件名含 test),现在各检查各自处理,可抽成共用函数;
- 报告增加 `--baseline-check`:校验 legacy-allowlist 声明计数与实际扫描一致,防基线本身漂移;
- 每类检查输出可定位的 `文件:行`,已部分做到,补齐 legacy 路径类。
