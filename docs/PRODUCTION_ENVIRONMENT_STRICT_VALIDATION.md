# 生产环境严格校验标准

## 1. 定位

本文档是后续所有生产变更的通用校验标准，不绑定 Vegas，也不只绑定策略。

适用范围：

- 策略上线、切版本、改参数、改风控。
- Core worker、scheduler、internal server、execution worker 发布。
- Web 产品、订阅、readiness、execution task、release pointer 配置变更。
- 数据库 schema、幂等 ensure、生产配置数据修复。
- SDK、交易所 symbol filter、签名只读 preflight、订单适配变更。
- 生产故障修复、回滚、恢复验证。

核心目标：

1. 证明生产当前实际运行的版本是什么。
2. 证明变更只影响预期范围。
3. 证明关键数据合同没有漂移。
4. 证明实盘路径没有绕过 readiness、风控、止损和 worker lease。
5. 证明用户可见状态和 Core/Web 事实源一致。
6. 证明存在可执行的回滚方案。

## 2. 通用结论分级

所有生产校验报告必须使用下面的结论之一，不允许使用“应该可以”“看起来没问题”这类模糊表述。

| 结论 | 含义 |
|---|---|
| `not_deployed` | 代码、镜像或配置未进入生产 |
| `deployed_not_active` | 生产已部署，但未被运行角色或数据配置启用 |
| `active_waiting_trigger` | 已启用，正在等待调度、事件、K 线或用户触发 |
| `active_blocked` | 已启用，但被 readiness、账户、风控、数据或外部依赖阻断 |
| `active_processing` | 已启用并正在处理任务，需要继续跟踪结果 |
| `completed` | 本次变更已完成，并有生产结果证据 |
| `rolled_back` | 已回滚，并完成回滚后验证 |
| `unsafe_stop` | 发现 No-Go 条件，必须停止或回滚 |

如果涉及交易执行，还必须追加交易状态：

| 交易状态 | 含义 |
|---|---|
| `no_signal` | 没有新信号 |
| `signal_generated` | 信号已产生 |
| `task_created` | execution task 已创建 |
| `task_blocked` | 任务被门禁阻断 |
| `order_submitted` | 交易所已接单 |
| `order_filled` | 交易所已成交 |
| `protected` | 保护单或止损计划已确认 |

## 3. 生产变更校验总流程

每次生产变更按固定顺序执行：

1. 定义变更对象和成功标准。
2. 确认 owner repo 和 owner service。
3. 本地或 CI 验证。
4. 提交、推送、跟踪 CI/CD。
5. 核对生产镜像、容器、revision。
6. 核对生产 env 和运行角色范围。
7. 核对 owner DB / owner API 的事实源状态。
8. 核对跨服务合同和消费者状态。
9. 核对日志、任务、队列、调度器和 worker lease。
10. 如涉及实盘，核对用户 readiness、交易所 preflight、symbol filter、止损计划。
11. 输出结论分级、blocker、回滚方案。

任何步骤缺少证据，都只能报告“未完成验证”，不能补猜结论。

## 4. 变更对象登记

上线前必须先写清楚：

| 字段 | 要求 |
|---|---|
| 变更类型 | code、config、DB、worker、scheduler、strategy、SDK、Web product、hotfix |
| owner repo | `rust_quant`、`rust_quan_web`、`rust_quant_news`、`crypto_exc_all`、`rust_quant_admin` |
| owner service | Core、Web、News、SDK、Admin |
| 生产角色 | internal server、execution worker、strategy worker、scheduler、Web backend、Admin、News worker |
| 影响范围 | exchange、symbol、timeframe、user、product、task type、API route、table |
| 成功标准 | 可验证，不使用“正常工作”这种模糊标准 |
| 回滚目标 | 回滚 commit、配置版本、DB 状态或服务版本 |

示例：

```text
变更类型：strategy live config
owner repo：rust_quant
owner service：Core
生产角色：quant-core-vegas-eth-4h-worker、quant-core-execution-worker
影响范围：okx / ETH-USDT-SWAP / 4H / vegas
成功标准：Core enabled version 唯一、worker 范围收窄、Web release pointer 一致、用户 readiness 可解释、无裸单路径
```

## 5. CI/CD 与版本证据

### 5.1 必查项

| 项目 | 通过标准 |
|---|---|
| git commit | 变更 commit 可定位 |
| CI run | 目标 run conclusion 为 `success` |
| deploy job | 生产 deploy job 成功 |
| image tag | 生产镜像 tag 包含目标 commit 或等价 revision |
| 后续覆盖 | 没有更新的 CI/CD 覆盖当前修复 |

### 5.2 命令模板

```bash
git log -1 --oneline
gh run watch <run_id> --exit-status
gh run view <run_id> --json conclusion,headSha,status,url,jobs
```

生产容器核对：

```bash
ssh "$PROD_SSH" \
  "docker ps --format '{{.Names}}\t{{.Image}}\t{{.Status}}' \
  | grep -E '<service-name>|<worker-name>|<scheduler-name>'"
```

禁止：

- 只用本地测试通过证明生产已上线。
- 只用 GitHub Actions 变绿证明生产容器已切换。
- 在生产服务器 build、compile 或临时改代码。

## 6. 运行角色与容器稳定性

### 6.1 容器状态

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{.Name}} status={{.State.Status}} running={{.State.Running}} restarting={{.State.Restarting}} restart_count={{.RestartCount}} image={{.Config.Image}}' \
  <container-1> <container-2>"
```

通过条件：

- `running=true`。
- `restarting=false`。
- `restart_count=0`，或有明确原因且已稳定。
- image 是目标 revision。

### 6.2 运行角色边界

必须确认容器职责没有混淆：

| 角色 | 不应做的事 |
|---|---|
| strategy worker | 不应直接执行用户订单 |
| execution worker | 不应扫描未授权策略信号 |
| internal server | 不应绕过 owner service 直接写商业事实 |
| scheduler | 不应重复消费同一 lease 或同一任务类型 |
| Web backend | 不应绕过 Core worker 真实下单 |
| Admin | 不应绕过 owner API 直接写业务状态机 |

### 6.3 环境变量核对

只输出非敏感 env：

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' <container> \
  | grep -E '^(APP_ENV|IS_RUN_|LIVE_|MARKET_|DEFAULT_|STRATEGY_|EXECUTION_WORKER_|SCHEDULER_)=' \
  | grep -Ev 'SECRET|TOKEN|PASSWORD|DATABASE|URL|KEY|PASSPHRASE'"
```

禁止把 secret、token、password、database URL、API key 输出到文档或聊天。

## 7. 数据库与 owner service 校验

### 7.1 owner 边界

| 事实 | owner |
|---|---|
| 行情、信号、回测、执行事实 | Core / `quant_core` |
| 用户、会员、订阅、API credential、Web execution task | Web / `quant_web` |
| 新闻、AI 分析、资讯事件 | News / `quant_news` |
| 交易所签名、过滤器、精度、错误归一 | SDK |
| 运营诊断与处置入口 | Admin |

校验时必须先确认事实源，不能因为查库方便就跨 owner 写入。

### 7.2 DB 变更校验

对 schema 或生产数据变更，必须校验：

- 目标数据库名正确。
- SQL 只影响预期行数。
- 有事务或幂等保护。
- 新增表/列有注释。
- 索引与查询条件匹配。
- 回滚 SQL 或恢复步骤明确。
- 变更后 owner API 或消费者能读取新状态。

只读核对模板：

```sql
SELECT current_database();
SELECT COUNT(*) FROM <table> WHERE <expected_scope>;
```

写入后必须立即做只读断言：

```sql
SELECT <key_fields>, <state_fields>
FROM <table>
WHERE <exact_scope>
ORDER BY updated_at DESC;
```

## 8. Web / 产品 / 用户侧校验

适用于 Web 产品、订阅、readiness、执行任务、用户可见状态。

### 8.1 产品发布

必须确认：

- `strategy_products.status` 符合预期。
- 产品 slug 与前端路由/API 一致。
- 核心策略 key/version 指向当前 Core 版本。
- 展示指标有来源，不夸大收益。
- archived 产品不会被当作可购买或可实盘项。

### 8.2 订阅与 readiness

必须按 combo 粒度校验：

```sql
SELECT
  s.buyer_email,
  s.strategy_slug,
  s.symbol,
  s.execution_exchange,
  s.service_mode,
  s.status AS subscription_status,
  c.exchange AS credential_exchange,
  c.status AS credential_status,
  c.last_check_code,
  r.risk_acknowledged,
  r.status AS risk_status
FROM strategy_combo_subscriptions s
LEFT JOIN user_api_credentials c
  ON lower(c.buyer_email) = lower(s.buyer_email)
 AND lower(c.exchange) = lower(COALESCE(NULLIF(s.execution_exchange, ''), '<default_exchange>'))
LEFT JOIN combo_risk_settings r
  ON r.combo_id = s.id
WHERE s.strategy_slug = '<strategy_slug>'
ORDER BY s.buyer_email, s.symbol;
```

通过条件：

- api-trade combo 必须有 active credential。
- signed read-only preflight 必须通过。
- risk setting 必须 active 且 acknowledged。
- signal-only 用户不能被误生成实盘 execution task。

## 9. 策略专项校验

策略上线必须额外满足 `PRODUCTION_STRATEGY_STRICT_VALIDATION.md`。

最低要求：

- 有 backtest id 和样本范围。
- 指标口径明确：胜率、回撤、Sharpe、交易笔数、费用/滑点。
- 适用交易对和周期明确。
- 版本字段可审计。
- 不把单币种/单周期结果外推为通用策略。
- 新旧版本不能在存储或展示上混淆。
- 未达默认目标时必须写例外说明。

## 10. 实盘交易专项校验

任何实盘路径都必须额外校验：

### 10.1 信号合同

live 信号 payload 必须包含：

- strategy key / slug / version。
- exchange / symbol / timeframe。
- side / direction。
- generated_at。
- entry price 或执行价格语义。
- stop-loss plan。
- risk sizing 或预算来源。
- 去重 key。

没有止损计划，禁止生成实盘 execution task。

### 10.2 Execution task

必须确认：

```sql
SELECT task_status, COUNT(*) AS n
FROM execution_tasks
WHERE strategy_slug = '<strategy_slug>'
  AND symbol = '<symbol>'
GROUP BY task_status
ORDER BY task_status;
```

attempt 证据：

```sql
SELECT
  t.id AS task_id,
  t.task_status,
  t.api_credential_id,
  a.attempt_no,
  a.attempt_status,
  left(a.error_message, 500) AS error_message,
  a.created_at
FROM execution_tasks t
LEFT JOIN execution_task_attempts a ON a.execution_task_id = t.id
WHERE t.strategy_slug = '<strategy_slug>'
ORDER BY t.id DESC, a.attempt_no DESC
LIMIT 20;
```

禁止：

- 用 task created 代替 order submitted。
- 用 order submitted 代替 order filled。
- 忽略 failed attempt 的真实原因。
- 重试时绕过 readiness 或止损校验。

### 10.3 订单结果

只有出现 exchange order result，才能宣称交易所接单。

只有出现 filled qty / filled quote，才能宣称成交。

必须确认：

- external_order_id。
- order_side。
- order_status。
- filled_qty。
- raw payload 已脱敏。
- 保护单或止损状态。

## 11. 交易所与 SDK 校验

### 11.1 Symbol filter

```sql
SELECT
  exchange,
  market_type,
  exchange_symbol,
  normalized_symbol,
  status,
  min_qty,
  step_size,
  min_notional,
  raw_payload->>'ctVal' AS ctVal,
  raw_payload->>'minSz' AS minSz,
  raw_payload->>'lotSz' AS lotSz
FROM exchange_symbols
WHERE lower(exchange) = lower('<exchange>')
  AND normalized_symbol = '<symbol>'
LIMIT 5;
```

必须确认：

- symbol 可交易。
- tick size / step size / min qty 可用于量化。
- 量化后数量不低于最小值。
- SDK 没有静默丢弃订单字段。

### 11.2 Account readiness

必须确认：

- API key active。
- signed read-only preflight passed。
- 账户模式符合订单语义，例如单向/双向持仓模式。
- 保证金模式、杠杆、合约单位符合 payload。
- 交易所拒绝账户模式变更时必须阻断，不允许继续裸下单。

## 12. 日志与观测校验

每次生产校验必须包含日志证据。

策略/worker 日志：

```bash
ssh "$PROD_SSH" \
  "docker logs --since 30m <worker> 2>&1 \
  | grep -E '启动|过滤后剩余|订阅|handled=|task|order|block|ERROR|panic|failed|失败|止损|保护' \
  | tail -n 160"
```

判定规则：

- 日志必须和 DB 状态对得上。
- 不能只看日志不查 DB。
- 不能只查 DB 不看 runtime 日志。
- 如果日志显示 legacy direct mutation 被阻断，这是正常安全边界；不得为了成交打开它。

## 13. No-Go 条件

出现任一项，必须停止、阻断或回滚：

1. 生产容器 revision 与目标 commit 不一致。
2. 关键容器重启循环。
3. 运行角色范围超出验证范围。
4. owner DB 状态与 owner API 返回不一致。
5. Web 产品指向旧版本，Core 已切新版本，或反之。
6. 用户 readiness 不完整却生成实盘任务。
7. 信号缺少止损计划。
8. execution task 使用 `api_credential_id=0` 进入实盘。
9. symbol filter 缺失或量化后数量不合法。
10. signed read-only preflight 未通过。
11. 交易所账户模式不符合订单语义。
12. 需要通过 legacy direct mutation 才能执行。
13. 没有回滚方案。

## 14. 回滚校验

回滚不是只执行 rollback 命令。回滚后必须验证：

- 容器 revision 已回到目标版本。
- DB enabled 配置已回到目标状态。
- Web 产品指针已回到目标 manifest。
- scheduler 不再生成新版本任务。
- execution worker 不再处理新版本残留任务，或残留任务已标记 blocked/cancelled。
- 用户可见状态不再显示已回滚版本。
- 历史 signal、task、attempt、order result 保留，不删除审计证据。

## 15. 通用校验报告模板

```markdown
# <变更名称> 生产严格校验报告

## 1. 基本信息
- 校验时间：
- 变更类型：
- owner repo：
- owner service：
- 生产角色：
- 影响范围：
- commit / image：

## 2. 结论
- 结论分级：
- 是否允许实盘：
- 是否有 blocker：
- 下一步动作：

## 3. CI/CD 证据
- run id：
- headSha：
- deploy job：

## 4. Runtime 证据
- 容器状态：
- env 范围：
- 日志摘要：

## 5. 数据事实源
- Core：
- Web：
- News：
- Admin：
- SDK / exchange symbols：

## 6. 用户与产品
- 产品状态：
- release pointer：
- subscriptions：
- credentials：
- risk settings：

## 7. 任务与执行
- signals：
- inbox：
- tasks：
- attempts：
- order results：

## 8. Blocker
- blocker：
- owner：
- 解除条件：
- 是否允许继续：

## 9. 回滚方案
- 回滚目标：
- 回滚步骤：
- 回滚后验证：
```

## 16. 专项文档关系

- 策略专项严格标准：`PRODUCTION_STRATEGY_STRICT_VALIDATION.md`
- Vegas id102 实例化记录：`VEGAS_ID102_PRODUCTION_VALIDATION.md`

专项文档只能补充更具体的校验项，不能降低本文档的通用生产校验要求。
