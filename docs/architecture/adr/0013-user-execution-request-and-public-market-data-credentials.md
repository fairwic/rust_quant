# ADR-0013：用户执行请求与平台公共市场数据凭证边界

- 状态：已接受
- 首次接受：2026-07-28
- 决策者：Rust Quant Core
- 上位文档：[长期目标架构](../target-architecture.md)、[依赖与代码归属规则](../dependency-rules.md)、[生产运行与恢复](../production-runtime.md)

## 背景

产品面向用户订阅的自动交易：用户是否可以执行，取决于 Web 所拥有的会员、`strategy × symbol` combo、用户交易所凭证、产品资格和风险配置。平台**不存在自营账户或 Core 自行发起的交易业务**。

同时，Market 需要使用平台固定的交易所 API Key 或等价认证材料，读取 K 线、instrument、公共成交、公共盘口等公共市场数据。此类材料的存在不能被误解为用户交易凭证，也不能演变为平台账户下单能力。

## 决策

### 1. 只有 Web 可以创建可执行请求

- `ExecutionRequest` 是 Web 商业授权事实，唯一 creator 是 Web；Core 只能生成 `StrategySignal`，并经版本化 owner Contract 提交给 Web。
- 不定义 `Core ExecutionRequest`、系统自营路径、平台账户默认请求或“缺少用户信息时的兜底请求”。Research 使用 `ResearchScenario`/`ResearchRunSpec`，不是 `ExecutionRequest`。
- 只有 Web 已创建的请求才可进入 Core 的**用户 live 风险增加** Account、Portfolio、Risk、Execution 链路。没有 Web 请求时，Core 可以继续产生信号、市场证据或研究结果，但不得创建或持久化 live `OrderIntent`、Risk approval、`ExecutionPlan`/OMS 事实，也不得触发外部 mutation。Research 可以通过公开的纯规划 API 生成绑定 `ResearchScenario`/`ResearchRunSpec` 的 `ExecutionPlanningValue`/`ProtectionPlanningValue` 模拟输出，并只写入 Research 的 `SimulationLedger`/Evidence；不能取得用户账户身份或升级为 live 事实。

`ExecutionRequest` 必须引用 Web owner 的 `ExecutionAccountBindingV1` identity/version；该冻结 binding 唯一拥有稳定 `ExchangeAccountRef`（`exchange × user account/subaccount × product scope`）以及可轮换的 `credential_reference + credential_revision + credential_revocation_generation`，request/claim receipt 不复制第二份可漂移字段。展示名称、email、可变 slug 和 credential 本身均不能作为账户身份。Account 的 session/lease/shard/fence、Portfolio 的账户级批处理、Risk 的保证金归属和 Execution 的 opening slot 全部以 binding 解析出的 `ExchangeAccountRef` 为键，credential revision/revocation generation 只决定某次 Gateway capability/Evidence 是否仍可解析，轮换不得创建第二个物理账户会话或 slot。

`ClaimExecutionRequestV1` 的 receipt 固定为 `ClaimExecutionRequestReceiptV1 { execution_request_id, claim_id, claim_fence, claim_expires_at, execution_account_binding_ref, risk_profile_ref, risk_profile_version, ... }`；稳定 `ExchangeAccountRef`、credential reference/revision/revocation generation 和 binding version 由其引用的 `ExecutionAccountBindingV1` 提供，不在 receipt 里复制成第二份可漂移事实。`claim_fence` 是 Web 为同一 request 分配的单调 claim generation，不再维护第二个同义字段。Renew 必须返回 current fence/expiry；Release 与 Outcome 必须带回相同 current fence，由 Web 在自己的事务中 CAS 接受。Core 的 live Context/batch/source mapping 保存 receipt 的规范引用/hash；任何 risk-increasing permit/capability 都必须再次验证它仍是当前 receipt，且自身 TTL 不得超过 `claim_expires_at`。这不是 Core 与 Web 的跨库事务，也不允许 Core 通过旧 receipt 覆盖后续 claimant。

#### 1.1 已发起交易的 `SafetyObligation` 不等于 Core 请求

一笔已经由 Web 请求触发、且已签发可能发送的 Submit permit、观察到成交/敞口，或仍有保护、Unknown、撤单和对账责任的交易，必须保留从原 `ExecutionRequest`、live Context、batch/source mapping 与 Order/Protection identity 派生的持久 `SafetyObligation`。它不是第二种 `ExecutionRequest`，不增加 Core 自营入口，也不授权新的风险增加。

- Web subscription/会员/claim 到期只会冻结新开仓和加仓；它们不能删除既有 SafetyObligation，不能把已知/未知敞口当作收敛，也不能让 Core 伪造新的用户请求；
- SafetyObligation 只可驱动 Query、Reconciliation、Cancel、Protect、Reduce、Close 等收敛动作，且每次 mutation 仍需原始证据、Gateway capability、permit/fence 与 reduce-only 或等价安全证明；Cancel 只能取消未成交的风险增加量，或在有已验证保护替换时执行，不能移除已有敞口的有效保护；
- obligation 只能在同一当前 Account generation 的 evidence 同时证明“对应 `ManagedExposure` 为零、无受管开放订单、无 Unknown/未终态 attempt/可消费 permit、保护已 Closed 或被验证替换、Account watermark 覆盖最终事实”后关闭；仅订单终态或已有 stop 均不足够；
- Account 保留该账户的私有流/签名查询监测时，只能通过版本化 `SafetyMonitoringV1 { safety_obligation_id, execution_account_ref, exchange_account_ref, operation, monitoring_fence, obligation_generation, managed_order_exposure_summary, required_fact_kinds, minimum_account_session_generation, minimum_projection_watermark, reason, issued_at, idempotency_key, causation_id }` 获取账户引用和 obligation 状态，不直读 Execution 私有表，也不因此获得新风险 admission。Account Inbox 持久化最大 current fence 后回发绑定同一 operation/fence、session generation 与 projection watermark 的 `SafetyMonitoringAckV1`；漏收、重启或顺序间隙通过版本化全量 snapshot/replay 重建，并保守保留会话；
- 用户 credential 被 Web 删除/撤销，或 Gateway 无法再解析其安全所需能力时，系统必须转 `SafetyBlocked` 并回传人工处置/通知证据。默认阻止删除的最终确认并冻结新风险；只有产品明确授权的“保留最小受限安全能力至收敛”或“责任人、通知 receipt、确认截止和人工 SLA 完整的 `ManualSafetyIntervention`”才可继续处理。不得以平台固定公共市场数据材料、缓存 secret、默认账户或其他用户 credential 继续操作。

#### 1.2 观察到的外部仓位不自动获得交易授权

Account 应如实投影交易所观察到的余额、持仓、订单和成交，但它们分为两类：

- `ManagedExposure` 必须可追溯到原 Web request、live Context、Core Order/Plan 与未结/已结 SafetyObligation；只有该类可进入 Core 自动的安全收敛路径；
- `ObservedExternalPosition` 是无此来源链的人工或外部仓位。Account、Web、Admin、Risk 和 Reconciliation 可以读取、告警、诊断与请求人工处理，但不得仅凭用户 API Key 自动下保护、撤单、减仓或平仓；来源不完整时归为 `UnknownOrigin`，按同样禁止自动 mutation 处理；
- `ObservedExternalPosition`/`UnknownOrigin` 不获得自动处置权，却必须进入 `AccountSnapshot`/`RiskValuationSnapshotV1`：至少占用同标的净敞口、保证金/可用权益、杠杆与风险预算，默认阻塞会与其冲突或无法安全估值的新风险；禁止出现“不能接管、又完全不计入风险”的第三种状态；
- 将外部仓位纳入自动管理必须由 Web 创建单独、版本化、可撤销且可审计的账户管理授权 Contract，明确账户、动作范围和有效期。不得通过 Core 生成 request、复用历史 subscription，或由 `MarketDataAccessCredential` 推断该授权。

### 2. 平台固定 API Key 是 Market 公共数据能力

- 平台固定材料由 App/Platform 配置注入 `exchange-gateway` 的公共数据 Adapter；Market 只消费其产生的公共数据和非敏感数据源身份。
- 该材料的语义是可选的 `MarketDataAccessCredential`：只允许公共只读 endpoint，例如 K 线、instrument、tick、公共盘口、公共成交和公共资金费率；不代表用户、交易账户、余额、仓位、私有流或交易权限。
- 交易所支持权限分级时，平台必须为该材料申请最小公共读取权限，禁止赋予交易、提现或私有账户读取权限；无权限分级的交易所也必须由 Gateway endpoint allowlist 拒绝所有私有与 mutation 调用。
- `MarketDataAccessCredential` 不进入 Domain 实体、`ExecutionRequest`、`ExecutionDecisionContextSnapshot`、Risk/Execution Contract、用户凭证表或任何 mutation capability。日志、Evidence 和必要的公共 Market Contract 只可记录不含 secret 的 `MarketDataAccessCredentialRef`、`market_data_source_profile_id` 与数据源版本。
- 它与 Web 的 `credential_reference` 是两个不可替换的身份空间：前者用于公共数据采集，后者仅表示某个用户可被授权的交易所账户。不得把一个转换、复制、缓存或降级为另一个。
- 公共数据的限流使用 `PublicQuotaKey(exchange, endpoint_group, egress_identity, market_data_source_profile_id)`；用户私有 REST/私有流与 mutation 使用独立的 `PrivateQuotaKey(exchange, credential_reference, endpoint_group)`。公共采集不得挤占用户 mutation 预算。
- 是否允许无认证 public endpoint 作为回退，必须由 Market 数据源 profile 显式声明、可审计并受数据质量/readiness 门禁约束；不得静默改用任一用户 credential。

### 3. 凭证解析与网络能力最小化

- 原始用户 credential 只能由受限的 Gateway 凭证解析边界以短生命周期、内存态方式取得；同一规则适用于平台市场数据材料。
- 公共数据 Adapter 不装配 mutation SDK capability；执行 Gateway 不接收 `MarketDataAccessCredential`。任何试图让公共数据材料访问私有账户或 mutation endpoint 的配置都是 fail-closed 错误。
- Gateway capability 必须区分“新风险增加”与“既有 SafetyObligation 的受限收敛”：前者要求有效 Web claim 与 `AccountAdmissionReady`，后者只能引用原 request/context/obligation，且永远不能扩大敞口。两者都不允许接受平台公共数据材料作为用户 credential。
- 新风险 capability issuance 还必须校验当前 `ClaimExecutionRequestReceiptV1` 的 request/claim id、current fence/expiry，以及其 `ExecutionAccountBindingV1` 的 `ExchangeAccountRef`、credential revision/revocation generation、AccountAdmissionReady、MarketDecisionReadiness、Kill Switch generation 与当前 `ExchangeExecutionCapabilityProfileV1`；capability/permit expiry 不得晚于最早 claim expiry。受限安全 capability 不要求商业 claim，但只能引用原 obligation，仍受 fence、reduce-only 与人工升级边界约束。

### 3.1 交易所能力与风险估值是 live 准入合同

`ExchangeExecutionCapabilityProfileV1` 由 Gateway owner 按 `exchange × product × instrument capability` 发布并版本化，至少声明 stable client identity/duplicate rejection、signed query 与 `DefinitivelyAbsent` 证明窗口、私有流 cursor/替代比较器、attached/post-fill protection、reduce-only、position/margin mode、精度/合约乘数、rate limit、time-sync 与错误语义。无法按该 profile 证明 Unknown recovery、保护窗口或 reduce-only 的组合一律 `Unsupported`，不得仅因 SDK endpoint 存在而 live。

`RiskValuationSnapshotV1` 是 Risk 的冻结动态 evidence，至少包含 `ExchangeAccountRef`、线性/反向/现货产品类型、settlement/collateral currency、position/margin mode、contract multiplier、mark/index/FX source 与 freshness、available equity、挂单/外部仓位占用、funding/fee 与 liquidation buffer。Risk 只可对能用该快照确定估值的产品生成新增风险审批；首批若仅支持 spot 或单一永续类型，必须在 profile 中显式限制，而不是让通用字段静默猜测。

## 后果

### 正面影响

- 用户授权、平台公共数据采集与真实执行三条身份链不再混淆；
- 固定 API Key 可满足市场数据配额/认证需求，而不会制造隐性自营交易路径；
- 商业授权停止后既有敞口仍有受限、安全、可审计的收敛尾巴，而不会静默把外部仓位接管为平台交易；
- Research 的身份和输入可独立重放，不必伪造 Web 请求。

### 代价

- 跨仓库 Contract、Market 配额和 Gateway capability 必须显式区分公共与私有路径；
- 所有迁移 Manifest 都必须删除“系统 ExecutionRequest”假设，并为 Research 使用独立 subject identity。

## 验收条件

1. 架构文档、Manifest、Guardrails 与 Skill 均不再出现 Core 自营 `ExecutionRequest` 路径；
2. `ExchangeAccountRef` 是会话、风险、slot 与 fencing 的稳定主键；`credential_reference + credential_revision + credential_revocation_generation` 只在用户授权、账户 readiness 与执行门禁语义中出现；
3. 平台市场数据材料只出现在 Market/App/Gateway 公共读取边界，且静态检查拒绝其进入私有或 mutation Contract；
4. 公共和私有配额、日志字段与 Evidence 身份能够分别审计。
5. claim 到期、重领、Renew、Release、Outcome 与 permit consume 并发时，旧 fence 无法下单或覆盖后续 claimant 的 Web 状态，capability/permit TTL 不超过 claim expiry；
6. 退订、会员/claim 到期冻结新风险增加但保留既有 SafetyObligation；credential 撤销且安全 capability 不可得时按默认阻止删除或经产品授权的人工 SLA 进入 `SafetyBlocked`，不存在平台或默认 credential 旁路；
7. `ObservedExternalPosition`/`UnknownOrigin` 不会被自动 mutation，除非存在独立 Web 账户管理授权 Contract，且默认已纳入 RiskValuationSnapshot 的风险占用；
8. 每个启用 live 的 exchange/product/instrument 组合都有当前 `ExchangeExecutionCapabilityProfileV1` 和 RiskValuationSnapshot fixture，未支持能力 fail-closed。
