# AI Domain Wave 迁移执行协议

- 状态：Accepted
- 生效日期：2026-08-02
- 最近修订：2026-08-03
- 适用范围：rust_quant legacy 到 rust_quant_alpha 的所有新迁移工作
- 上位决策：[ADR-0017](adr/0017-capability-catalog-and-domain-wave-migration.md)
- 能力总账：rust_quant_alpha/architecture/business-capability-catalog.toml
- 目录规范：[目标目录与代码放置规则](target-directory-layout.md)
- 迁移计划：[Domain Wave 迁移计划](migration-plan.md)

## 1. 核心命令

AI 只能按以下顺序推进：

~~~text
查询总账
  -> 确认 owner 和 Wave
  -> 闭合 legacy 语义
  -> 声明唯一目标位置
  -> 实施 Domain 闭环
  -> 自动校验
  -> 更新总账状态和 Wave 证据
~~~

不得先写代码再寻找 owner，不得用编译通过替代业务盘点，不得以历史 Manifest 或 Registry 状态宣称新迁移完成。

## 2. 执行前判断

### 2.1 仓库与工作树

先确认：

- owning child repo；
- 当前分支和 HEAD；
- tracked 与 untracked 修改；
- 用户改动和本任务改动是否重叠；
- legacy 基线是否仍与能力总账一致。

不得在 umbrella 根目录运行 child repo 的 Git、构建或测试结论。不得覆盖用户的并行改动。

### 2.2 当前 Wave

只允许一个活跃业务 Wave。

- W0：治理；
- W1：Market；
- W2：Strategy、Research、Quant；
- W3：Portfolio、Account、Risk；
- W4：Execution、Reconciliation；
- W5：Control、运行入口与切换。

上一 Wave 未闭合时，后续 Wave 只能做只读分析，不能扩大目标实现。

### 2.3 能力查询

新增文件、类型、trait、enum、SQL、事件或 App 前，必须先查询：

- capability id；
- owner；
- target；
- wave；
- reuse_policy；
- legacy_sources；
- consumers。

如果总账已经存在，必须使用登记位置。不存在时先判断：

1. 是否只是现有 capability 的子职责；
2. 是否改变某个 Domain 事实；
3. 是否只是版本化 contract；
4. 是否只是外部 adapter；
5. 是否真的是无业务 owner 的 Quant 或 Platform primitive；
6. 是否只有装配职责。

无法明确 owner 时停止实现，先补能力分析。禁止把不确定内容放进 common、utils、helpers、support、shared、services 或 App。

## 3. L2 语义闭合

每个 Wave 开始时做一次，不为每个小 capability 重复全仓扫描。

### 3.1 入口闭包

至少覆盖：

- App、CLI、Scheduler、Worker、Consumer 和 internal API；
- 绕过 Service 的直接 SDK、SQL、Redis、HTTP 和环境变量读取；
- 测试、部署命令和真实运行角色；
- producer、consumer 与最终用户可见结果。

搜索结果必须形成闭包。只找到类型定义或单个 Service 不算完成。

### 3.2 语义矩阵

每项 legacy 语义记录：

| 字段 | 内容 |
|---|---|
| ID | Wave 内稳定编号 |
| 业务目的 | 为什么存在 |
| 入口与调用方 | 谁触发、谁消费 |
| 输入事实 | 信号时点或操作时点可见信息 |
| 状态与不变量 | 前置、转换、终态、禁止回退 |
| 时间与精度 | 单位、边界、Decimal、舍入、来源时间 |
| 数据与事务 | 表、键、锁、Outbox、幂等 |
| 外部 I/O | SDK、HTTP、Redis、消息 |
| 失败与恢复 | 重试、未知结果、补偿、人工处置 |
| 处置 | preserve、optimize、defer 或 retire |
| 目标能力 | capability id 与 target |
| 验证 | golden case、test、parity 或运行证据 |

同一业务目的存在冲突实现时，必须选出 canonical 语义，并说明其他实现为何 optimize 或 retire。

### 3.3 允许重写的条件

目标代码可以重分层和重写，但必须先满足：

- legacy 业务目的已解释；
- 调用方和最终副作用已找到；
- 状态机、默认值、时间、精度和失败语义已记录；
- 差异已被明确批准为 optimize 或 retire；
- 新行为有测试；
- 未实施语义有明确的 deferred capability。

“代码旧、写得乱、测试少”不是丢弃业务语义的充分理由。

## 4. 放置声明

修改前在 Wave 计划或工作记录中写一份短声明：

~~~text
Capability: execution.protection
Owner: execution
Wave: W4
Fact changed: 保护单计划与未保护仓位安全义务
Target: crates/domains/execution/src/protection
Kind: domain
Reuse policy: owner_only
Consumers: execution.planning, risk.continuous
Legacy disposition: optimize
Forbidden locations: app, adapter, common, global enums
~~~

该声明用于让代码评审快速验证归属，但不需要创建独立 Manifest。

## 5. Domain 闭环实施

### 5.1 实施单位

一次实施一个 Domain Wave 内可验证的业务闭环。闭环通常包含：

~~~text
真实入口
  -> owner API
  -> capability model/policy/command/query
  -> 使用方定义的窄 Port
  -> production Adapter
  -> 必要 contract
  -> 持久化与恢复
  -> focused tests
~~~

同一 Wave 可以连续迁移多个 capability，不需要为每个 capability建立文档和 hash。但每个 capability 仍必须有唯一 target 和状态。

### 5.2 Port 规则

只在以下条件全部满足时创建 Port：

- Domain 确实需要外部能力；
- 由使用方 Domain 定义；
- 方法只覆盖当前真实调用；
- 有生产调用方；
- 有生产 Adapter 或明确的同 Wave 交付；
- 失败、幂等、事务和恢复责任清楚。

仅为 Fake/Mock 服务的 Port 不得进入生产目录。测试替身只能实现已有真实调用方和同 Wave 生产 Adapter 的窄 Port，或直接作为测试局部类型存在。不得按交易所或数据库全量 API 创建巨大 trait。

### 5.3 Use Case 规则

Use Case 只协调一个明确业务结果。出现以下情况必须拆分：

- 同时改变两个 owner 的事实；
- 同时包含信号、资金分配、风险和下单；
- 同时读取多个数据库 Entity 并直接写回；
- 同时负责正常流程、恢复和运营处置；
- 依赖大量布尔参数决定完全不同的流程。

拆分依据是业务结果和状态机，不是为了减少行数随意切函数。

### 5.4 Domain、Adapter 与 App

- Domain：业务事实、不变量、政策、状态机和业务错误；
- Contract：跨进程必需且版本化的数据；
- Adapter：SDK、数据库、Redis、HTTP、对象存储和通知机制；
- Quant：无策略语义的确定性计算和模拟；
- Platform：配置、生命周期、观测、安全 primitive；
- App：装配、调度、健康检查和进程生命周期。

业务判断不得从 Domain 漂移到 App 或 Adapter。

### 5.5 文件与目录

- mod.rs、lib.rs、api.rs 和 spi.rs 保持薄 façade；
- capability 简单时使用单文件；
- 出现多个变化原因时才拆 model、commands、queries、ports 或 tests；
- 枚举跟随它约束的业务事实；
- 不创建全局 enums、types、models 或 services；
- 不预生成空 Domain、App、Port 或 Adapter；
- 遵守 400/600、100/150、250/500 和 1000 行预算。

## 6. 数据库执行协议

### 6.1 优先复用

任何持久化变更先回答：

- legacy 生产表是什么；
- 当前 owner、写入者、读取者和数据量；
- 目标语义是否能由现有字段和约束表达；
- 是否存在历史数据、手工修复或跨服务依赖；
- 索引和查询性能是否满足。

能满足时复用。不能满足时才提出 schema 变更。

### 6.2 变更条件

新增表或列必须同时具备：

- 唯一 DDL owner；
- 表和列注释；
- 约束和索引性能分析；
- schema-tool plan；
- 向前兼容、回填和回滚；
- 独立数据库角色与最小权限；
- 测试数据库验证。

业务运行代码不得自行 CREATE、ALTER 或补列。

### 6.3 K 线分表

- 继续使用交易对与周期分表；
- market_candles 不得重新出现；
- 新交易对或周期只经 schema-tool；
- 冻结生产分表 inventory 和 schema fingerprint；
- 回填保留来源、时间范围、校验和失败恢复；
- 不在交易热路径扫描所有分表。

## 7. 交易所与凭证

- 第一版只允许 OKX、Binance；
- Adapter 必须复用 crypto_exc_all；
- 不重写签名、endpoint DTO、精度或错误归一；
- 固定 Market API Key 只允许公共只读 endpoint；
- 用户凭证由 quant_web owner，Core 使用 opaque credential reference；
- signed read-only preflight 属于 Account；
- live mutation 只经 Execution 和 SDK mutation Adapter；
- SDK 不决定用户授权、强制止损、风险或 lease。

## 8. 交易与研究安全

### 8.1 数值边界

- 行情、指标和研究内部可使用 f64；
- 金额、价格、数量、手续费、余额和订单参数使用 Decimal；
- Execution 必须依据当时有效 InstrumentRules 重新量化；
- 量化是交易所合法性和可审计性要求，不是性能优化；
- 不允许将 f64 分析结果直接作为 mutation 参数。

### 8.2 实盘边界

默认只允许：

- contract test；
- fixture；
- backtest；
- paper/sim；
- dry-run；
- shadow；
- signed read-only。

真实 mutation 需要用户单独授权，并且仍必须具备 credential readiness、symbol filters、Decimal rounding、RiskApproval、lease、fence、保护单计划、最小数量、回滚和平仓计划。

没有止损计划不得下单。

### 8.3 研究边界

- 严格按信号时点回放；
- 信号后数据不能反向决定是否入场；
- Research 只产生资格，不直接 promote；
- Strategy release 执行显式 promote；
- dataset、universe、策略版本和机器结果继续使用身份指纹。

## 9. 每批自动校验

### 9.1 静态架构

~~~text
cargo xtask arch-check
~~~

检查：

- capability id 和 target 唯一；
- owner、kind、Wave、status 和 reuse_policy 闭集；
- implementing/implemented 目标存在；
- 禁止 common、utils、helpers、support、shared、services 与 `*_helpers`、`*_support` 生产目标；
- `module-boundary-policy.toml` 已登记 canonical 实现必须保持唯一定义和禁止别名；
- package role、依赖方向和 Release Unit；
- façade、文件预算、Port 完整性和可静态判定的源码规则。

### 9.2 Wave readiness 与测试

~~~text
cargo xtask wave-check --wave W1
~~~

命令先执行全量静态检查，再确认该 Wave 所有非 deferred capability 已 implemented，最后一次执行总账冻结的 Cargo package tests。

如果 capability 尚为 planned 或 implementing，命令必须失败且不运行 Wave tests。这能防止局部代码或目录被误报为 Domain 完成。

### 9.3 行为验收

自动结构门禁不能替代：

- legacy golden cases；
- Contract snapshot；
- SDK contract tests；
- PostgreSQL 集成测试；
- 最大业务批次、SQL 往返预算、bind/statement 分块和相同 release 输入的性能证据；
- 状态机和恢复测试；
- Backtest/Paper/Live parity；
- 运行态日志、数据库和真实 revision 证据。

每个 Wave 必须在 `rust_quant_alpha/architecture/domain-waves/Wx/evidence.md` 中记录命令、结果、首个差异层和阻塞；L2 矩阵与实施范围只保存在同目录唯一的 `wave-plan.md`。

## 10. 状态更新

capability 状态只允许：

- planned：已登记，尚未开始；
- implementing：真实目标存在，但闭环或验证未完成；
- implemented：业务闭环、测试和 parity 已完成；
- deferred：明确不属于当前迁移范围，并有理由。

禁止因为以下情况改为 implemented：

- 创建了目录；
- 编译通过；
- trait 和 Fake 已存在；
- 单元测试通过但没有真实 Adapter；
- 历史 Manifest 标记 pass；
- Registry 登记 completed；
- README 声称完成；
- 静态 arch-check 通过。

## 11. Hash 规则

不再为以下内容维护日常 hash：

- 普通源码文件；
- Wave 计划；
- Evidence 文档；
- 每个微小迁移记录。

必须保留 hash 或等价 identity：

- dataset、universe 和 snapshot；
- 策略版本、研究 run 和回测结果；
- contract schema；
- database migration；
- golden fixture；
- build artifact 和 image revision；
- 明确安全关键发布输入。

hash 证明输入和制品身份，不证明业务语义正确。

## 12. 历史流程

以下内容只在 legacy 源仓库或 Git 历史中作为只读证据保留：

- docs/architecture/migrations；
- programs/registry.toml；
- 已删除的 migration-check；
- 已删除的 migration-registry-check；
- 历史 Manifest、Evidence 与 Verdict。

允许用它们寻找 legacy 语义、Contract 或历史决策。禁止：

- 为新 Wave 新增同类记录；
- 为普通源码变化回填 hash；
- 修改旧 Registry 依赖图以推动当前进度；
- 让历史 Verdict 替代 capability 或 Wave 验收。

目标仓库不得继续复制这些目录或保留对应运行代码；活跃门禁只有 `cargo xtask arch-check` 与 `cargo xtask wave-check --wave Wx`。

## 13. AI 输出要求

开始实施前，简要报告：

- 当前 Wave；
- legacy 基线；
- capability 范围；
- owner 与目标目录；
- L2 已知缺口；
- 成功标准和禁止范围。

完成后，必须报告：

- 实际完成的 capability；
- preserve、optimize、defer、retire 结论；
- 关键目标文件；
- 自动检查与行为测试；
- 仍未闭合的业务语义；
- 是否触及数据库、Contract、SDK 或 live 安全；
- 下一批的清晰输入条件。

不得只报告“迁移完成”或“测试通过”。
