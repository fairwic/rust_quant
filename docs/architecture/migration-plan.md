# Rust Quant Domain Wave 迁移计划

- 状态：W0 治理基线已完成，W1 等待 L2 语义冻结
- 首次采用：2026-08-02
- legacy 基线：rust_quant@9e8a23dab9b68519641f098be979ed528eca4121
- 目标仓库：rust_quant_alpha
- 目标架构：[Rust Quant 长期目标架构](target-architecture.md)
- 详细目录：[目标目录与代码放置规则](target-directory-layout.md)
- 能力盘点：[Legacy 业务能力全量盘点](legacy-business-capability-inventory.md)
- 执行协议：[AI Domain Wave 迁移执行协议](ai-migration-execution-protocol.md)
- 决策：[ADR-0017](adr/0017-capability-catalog-and-domain-wave-migration.md)

## 1. 目标

迁移不再按孤立文件、小 trait 或细粒度 Manifest 衡量。完整流程固定为：

~~~text
全量业务能力 L1 盘点
  -> 冻结完整目标目录与唯一能力归属
  -> 每个 Domain Wave 开始前完成 L2 语义深挖
  -> 按业务闭环成批迁移
  -> 自动结构检查 + Domain tests + legacy parity
  -> 全部 Wave 完成后统一切换
~~~

目标不是机械复刻 legacy，也不是脱离 legacy 重写。每项旧语义必须明确 preserve、optimize、defer 或 retire；每项新能力必须说明为什么 legacy 不存在以及真实消费者是谁。

## 2. 迁移事实边界

- rust_quant 在统一切换前仍是现有生产行为事实源。
- rust_quant_alpha 是唯一目标实现仓库。
- rust_quan_web 继续拥有用户、会员、strategy x symbol combo、用户凭证和商业 readiness。
- crypto_exc_all 继续拥有交易所协议、签名、DTO、精度字段和错误归一。
- Core 不新增直连 quant_web 数据库。
- 第一版只完成 OKX 与 Binance。
- 没有自营账户；固定 API Key 只用于 Market 公共数据访问。
- K 线使用既有交易对与周期分表，不恢复废弃的 market_candles。
- 数据库优先复用既有生产表；不满足时才提出带 owner、回填和回滚的变更。
- 默认禁止真实下单、撤单、平仓和账户 mutation。
- CI/CD 在迁移完成后统一推进；当前每个 Wave 必须有可重放的本地自动校验。

## 3. 唯一迁移控制面

目标仓库的 architecture/business-capability-catalog.toml 是唯一活跃控制面，登记：

- 124 个 capability；
- 唯一 owner 与 target；
- W1 至 W5 归属；
- planned、implementing、implemented 或 deferred；
- legacy 来源和处置；
- 复用策略；
- 已知消费者；
- 每个 Wave 的自动测试 package。

旧 docs/architecture/migrations、programs/registry.toml、Manifest、Evidence 和 Verdict 保留为历史证据，不继续扩展。历史 migration-check 命令不再决定新 Wave 是否完成。

## 4. W0：冻结治理基线

### 交付

- L1 全量业务能力盘点；
- 124 个 capability 的机器总账；
- 详细到 capability 和文件职责的目标目录；
- ADR-0017；
- 新 AI 执行协议；
- 总账重复、非法 owner、非法路径、错误复用策略和状态检查；
- cargo xtask wave-check --wave W0 自动门禁；
- 旧切片计划和协议只读归档。

### 完成条件

- cargo xtask arch-check 通过；
- cargo test -p xtask 通过；
- cargo xtask wave-check --wave W0 通过；
- README、目标架构、依赖规则、护栏和技能引用新流程；
- 旧流程不再被活跃文档描述为必需门禁。

W0 不创建空 Domain、App、Port 或 Adapter。

## 5. W1：Market Data 与 Instrument Reference 闭环

### 业务范围

- Instrument、Timeframe 与 InstrumentRules；
- 标的发现、上市、变更、退市和来源事实；
- OKX、Binance 公共 K 线、成交、盘口、资金费、标记价和指数价；
- fixed service API Key 的只读 source profile；
- ConfirmedCandle、修正、乱序、重复和缺口；
- K 线历史、增量同步、point-in-time snapshot 和 readiness；
- 既有 K 线分表、schema-tool 和独立数据库角色；
- Market contract、Market worker 和可观测性。

### L2 必查

- 所有 legacy 入口、后台任务、WebSocket 和直接 SDK/SQL 调用；
- 分表清单、DDL owner、主键、索引、权限、数据量和异常表；
- OKX、Binance SDK endpoint 与 DTO 能力；
- Decimal 存储与 f64 计算边界；
- 时间戳单位、K 线边界、finality 和修订策略；
- 断线、限频、重试、游标和幂等；
- Market golden datasets 与首个差异层。

### 完成条件

- W1 非 deferred capability 全部 implemented；
- Market 不存在第二套 K 线事实；
- public-market Adapter 只复用 crypto_exc_all；
- 生产分表 lifecycle 可由 schema-tool 规划和验证；
- Market Domain tests、Adapter contract tests、数据库集成测试和 legacy parity 通过；
- cargo xtask wave-check --wave W1 通过。

W1 未完成前，不继续扩展 Strategy、Research 或 Execution 的迁移。

## 6. W2：Strategy、Research 与 Backtest 闭环

### 业务范围

- 策略定义、版本、catalog、runtime、因果 evaluation 和 signal；
- signal handoff；
- Research experiment、dataset、simulation、evaluation、evidence 和 qualification；
- Quant math、indicators、因果时钟、回放、撮合、费用和分析；
- quant-lab、signal-worker；
- promote、回滚、停用和 PromotionReceipt。

### L2 必查

- 真实策略清单、当前生产/准生产/paper/research 入口；
- Vegas 既有入场、出场、止损、过滤、冲突和状态语义；
- 信号时点可见字段及未来数据禁用；
- Backtest、paper、live 的逐层 snapshot/context parity；
- 统一资金曲线、容量、成本、滑点、资金费和 point-in-time universe；
- L0 至 L3 gate、停止条件和版本身份；
- 旧策略保留、新版本并存和显式 promote。

### 完成条件

- 同一策略版本在回测与生产 evaluator 共享相同因果核心；
- Research 不进入生产 Release Unit；
- Strategy 不负责资金分配、账户准入、风险审批或下单；
- golden signal ledger、Pine/Rust parity、回测/live 首个差异层可定位；
- cargo xtask wave-check --wave W2 通过。

## 7. W3：Portfolio、Account 与 Risk 闭环

### 业务范围

- candidate batch、确定性排序、容量、资金分配、相关簇和 PortfolioTarget；
- 凭证引用、账户会话、signed read-only preflight；
- 余额、仓位、挂单、保证金、暴露和恢复投影；
- 风险政策、估值、pre-trade、RiskApproval、持续风险和风险动作；
- quant_web subscription 与 credential owner API Adapter；
- account-worker 和相关 contracts。

### L2 必查

- strategy x symbol combo 的商业与交易边界；
- verified active 凭证、交易权限和产品类型；
- 私有流来源时间、快照恢复和降级；
- 组合容量、并发风险和相关簇；
- 价格来源、风险金额、名义价值、杠杆和保证金口径；
- 风险拒绝、审批版本和 kill switch 请求。

### 完成条件

- Core 不保存或输出用户明文凭证；
- Portfolio、Account、Risk owner 清晰，彼此不共享数据库 Entity；
- 研究与生产使用同一组合和风险政策语义；
- 所有拒绝都有结构化 blocker 和证据；
- cargo xtask wave-check --wave W3 通过。

## 8. W4：Execution 与 Reconciliation 闭环

### 业务范围

- execution intake、context、planning、intent、OMS 和订单状态机；
- Decimal 量化和 InstrumentRules snapshot；
- 强制止损、保护单、Outbox、dispatch、lease 和 fencing；
- 超时、未知结果、重试、回查和 SafetyObligation；
- 订单、成交、仓位、余额和保护单对账；
- quant_web execution request、claim、outcome 和 result writeback；
- execution-worker 与 reconciliation-worker。

### L2 必查

- 全部执行入口和绕过 Core Gateway 的 mutation；
- 状态机、幂等键、本地事务、Outbox 和外部 I/O 时点；
- claim、lease、fencing token 和过期语义；
- OKX、Binance 订单类型、精度、部分成交和错误；
- 保护单必须性、部分成功、补偿和裸仓风险；
- 内外事实差异、案例 owner、恢复和关闭条件。

### 完成条件

- 没有止损计划时不能下单；
- 未知订单或未保护仓位不会只留在内存或日志；
- App 和 Adapter 不承载风控或订单状态机；
- 真实 mutation 仍保持默认关闭；
- dry-run、paper、shadow、恢复和 reconciliation parity 通过；
- cargo xtask wave-check --wave W4 通过。

## 9. W5：运行入口、Control、全链路 parity 与统一切换

### 业务范围

- control activation、publication 和 kill switch；
- 各 App 唯一入口与 Release Unit；
- 配置、生命周期、观测、安全和消息机制收口；
- Core 与 Web contracts 全链路；
- legacy 与 alpha shadow；
- 切流、回滚和 legacy 删除计划。

### 完成条件

- 每个运行角色有唯一 App 和明确 owner；
- Research 不进入生产工件；
- 所有业务事实只有一个写 owner；
- 全链路 readiness、blocker、时间戳和证据可定位；
- 关键 golden cases、状态机、数据库和运行态 parity 通过；
- cargo xtask wave-check --wave W5 通过；
- 有明确切流授权、回滚窗口和 legacy 退役清单。

W5 完成仍不自动执行生产切换。

## 10. 每个 Wave 的固定实施循环

1. 冻结基线与范围
   - 确认 owning child repo、分支、当前工作树和 legacy commit；
   - 查询能力总账，冻结 Wave capability 和禁止范围。

2. 完成 L2 语义盘点
   - 入口、调用方、状态机、数据、配置、外部 I/O、失败、恢复和消费者；
   - 对每项 legacy 语义决定 preserve、optimize、defer 或 retire。

3. 冻结 Wave 计划
   - 只在 `rust_quant_alpha/architecture/domain-waves/Wx/wave-plan.md` 保存一份计划，包含语义矩阵、依赖、数据库、Contract、parity、回滚和完成条件；
   - 不为每个 capability 创建独立 Manifest。

4. 成批迁移
   - 按目标目录在同一 Domain 闭环内实现；
   - 可以优化或重写，但必须有 legacy 语义结论；
   - 不创建无真实入口的抽象。

5. 自动校验
   - cargo xtask arch-check；
   - focused tests；
   - cargo xtask wave-check --wave Wx；
   - 数据库、SDK、contract、parity 和 recovery 验证。

6. 更新状态
   - 只在真实完成后把 capability 改为 implemented；
   - 每个 Wave 只在 `rust_quant_alpha/architecture/domain-waves/Wx/evidence.md` 保存一份证据；
   - 记录首个差异层、允许差异、阻塞和后续 owner。

## 11. Hash 与证据

不再维护每个源码文件、Manifest 或 Evidence 文档的日常哈希。

继续冻结：

- point-in-time dataset 和 universe；
- 策略、研究、回测与 golden fixture identity；
- contract schema；
- 数据库 migration；
- 构建制品和镜像 revision；
- 安全关键发布输入。

结构检查只证明目录和依赖合规。行为完成必须由测试、数据库、SDK、parity、恢复和运行态证据共同证明。

## 12. 统一切换顺序

统一切换必须单独执行：

~~~text
冻结 legacy 写入与版本
  -> alpha 全量只读 shadow
  -> 数据与 contract parity
  -> paper/dry-run execution parity
  -> operator readiness 审查
  -> 显式切流授权
  -> 分阶段切入口
  -> 观察与对账
  -> 稳定窗口
  -> legacy 删除
~~~

发生以下任一情况立即回滚：

- 订单、保护单或账户事实无法对账；
- 数据新鲜度或 K 线 finality 不一致；
- 策略首个差异层无法解释；
- claim、lease、幂等或状态机出现回退；
- 运行角色、数据库 owner 或 contract 版本不清晰；
- 无法证明真实生产 revision。

## 13. 当前进度口径

| Wave | 当前判断 | 说明 |
|---|---|---|
| W0 | completed | 总账、细化目录、ADR、检查器、协议和技能已建立；W0 自动门禁通过 |
| W1 | 待开始 L2 | Market 目标代码已有部分 `implementing` capability，但 Wave 尚未冻结完整 L2、InstrumentRules、数据质量、SDK 与 parity |
| W2 | planned/局部实验中 | Strategy、Research 有局部实现，不能据此宣称 Wave 开始或完成 |
| W3 | planned | 目标 Domain 尚未形成 |
| W4 | planned | legacy 逻辑丰富，但目标 Execution/Reconciliation 尚未形成 |
| W5 | planned | 不提前切流或部署 |

进度只按 capability 和 Domain 闭环计算，不按迁移目录数量、trait 数量、代码行数或历史 Registry 状态计算。
