# 生产策略严格校验标准

本文档是 `PRODUCTION_ENVIRONMENT_STRICT_VALIDATION.md` 的策略专项补充，只覆盖策略类生产变更。所有策略上线仍必须先满足通用生产环境校验标准，再执行本文档的策略专项检查。

## 1. 目标与适用范围

本文档定义所有策略进入生产实盘、生产观察、paper observation、signal-only 发布前后的严格校验标准。

适用对象：

- 新策略首次上线。
- 既有策略切换新版本。
- 既有策略调整参数、过滤器、风控、支持交易对或周期。
- 将 observation / paper 候选升级为 live candidate。
- 生产事故后恢复某个策略版本。

不适用对象：

- 纯研究脚本输出。
- 未进入产品目录、未进入 Core/Web release 链路的临时实验。
- 用户明确只要求本地回测且不涉及生产配置。

核心原则：

1. 证据优先，不凭代码推断生产事实。
2. 生产实盘必须有可审计版本标识。
3. Core、Web、SDK、交易所账户和执行 worker 要逐层校验。
4. 没有止损计划不得放行实盘。
5. 用户 readiness 不完整不得生成可执行任务。
6. CI/CD 成功不等于线上容器已切换，必须核对真实容器 revision。
7. 某个策略在单币种单周期有效，不得默认外推到其他币种或周期。

## 2. 校验分层

生产策略校验必须分成 8 层：

| 层级 | 事实源 | 必须回答的问题 |
|---|---|---|
| L1 代码与镜像 | GitHub Actions、镜像 tag、容器 label | 哪个 commit 正在生产运行 |
| L2 Core 策略配置 | `quant_core.strategy_configs` | 哪个策略版本是唯一 enabled |
| L3 Core runtime | Core worker 容器、日志、env | 生产 worker 实际跑哪些交易所、交易对、周期 |
| L4 Web 产品发布 | `quant_web.strategy_products`、release pointer | 用户侧产品是否指向同一版本 |
| L5 用户 readiness | combo subscription、credential、risk settings | 哪些用户可实盘，哪些用户必须阻断 |
| L6 信号合同 | strategy signal / Web inbox payload | 信号是否带版本、方向、止损、风险计划 |
| L7 执行任务 | execution tasks、attempts、order results | 任务是否通过 lease、精度、余额、风控与保护单 |
| L8 交易所账户 | signed read-only preflight、symbol filters、account mode | 交易所账户是否真的可执行 |

缺任一层证据，不得宣称“生产已可实盘成交”。

## 3. 上线前输入材料

每次上线必须先收集：

| 项目 | 要求 |
|---|---|
| 策略标识 | `strategy_key`、`strategy_slug`、`version`、`entry_rule_version` 或等价 manifest |
| 适用范围 | exchange、symbol、timeframe、market type |
| 回测证据 | backtest id、样本区间、trade count、win rate、max drawdown、Sharpe、费用/滑点口径 |
| 跨样本证据 | 至少说明 BTC/ETH/其他币种、不同周期是否验证过 |
| 风控证据 | 止损、止盈、仓位、杠杆、最大亏损、最小数量处理 |
| 产品口径 | signal-only、api-trade、paper、live 的边界 |
| 用户影响 | 哪些产品、combo、订阅、API credential 会被影响 |
| 回滚方案 | 回滚目标版本、回滚 SQL、回滚后验证项 |

如果胜率、回撤或样本稳定性未达到默认目标，必须写明：

- 哪个指标未达标。
- 为什么仍允许进入生产。
- 是否只允许进入专用候选、shadow、paper 或 observation。
- 谁确认该例外。

## 4. 代码与 CI/CD 校验

### 4.1 本地验证

至少执行：

```bash
cargo fmt --check
cargo test -p <package> <focused_test>
```

共享合同、执行 worker、风控、SDK 或数据库读写发生变化时，必须扩大测试范围。

必须记录：

- 执行命令。
- 退出码。
- 通过/失败数量。
- 是否存在仅 warning。

### 4.2 Commit 与 CI/CD

必须确认：

```bash
git log -1 --oneline
gh run watch <run_id> --exit-status
gh run view <run_id> --json conclusion,headSha,status,url,jobs
```

通过条件：

- CI conclusion 是 `success`。
- headSha 等于要上线的 commit。
- deploy job 成功。
- 没有后续 CI/CD run 覆盖该 revision。

禁止：

- 只看本地测试通过就说生产已上线。
- 只看镜像构建成功就说生产已切换。
- 生产服务器上直接 build 或 compile。

## 5. 生产容器校验

### 5.1 容器 revision

只读命令模板：

```bash
ssh "$PROD_SSH" \
  "docker ps --format '{{.Names}}\t{{.Image}}\t{{.Status}}' \
  | grep -E '<core-service-name>|<execution-worker>|<handoff-scheduler>|<internal-server>'"
```

必须确认：

- 策略 worker 使用目标 commit 的镜像 tag。
- execution worker 使用同一目标 commit 或明确兼容版本。
- internal server、handoff scheduler 如参与链路，也必须在目标 commit 或明确兼容版本。

### 5.2 容器稳定性

只读命令模板：

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{.Name}} status={{.State.Status}} running={{.State.Running}} restarting={{.State.Restarting}} restart_count={{.RestartCount}} image={{.Config.Image}}' \
  <strategy-worker> <execution-worker> <handoff-scheduler> <internal-server>"
```

通过条件：

- `running=true`。
- `restarting=false`。
- `restart_count=0`，或能解释重启原因并已稳定。
- image 指向目标 revision。

## 6. Core 策略配置校验

### 6.1 唯一 enabled 配置

只读 SQL 模板：

```sql
SELECT
  COUNT(*) FILTER (WHERE enabled) AS enabled_count,
  string_agg(version, ',' ORDER BY version) FILTER (WHERE enabled) AS enabled_versions
FROM strategy_configs
WHERE strategy_key = '<strategy_key>'
  AND exchange = '<exchange>'
  AND symbol = '<symbol>'
  AND timeframe = '<timeframe>';
```

通过条件：

- 对 live worker 会消费的同一 `strategy_key x exchange x symbol x timeframe`，通常只能有 1 个 enabled 配置。
- 如果允许多个 enabled 配置，必须有明确的 runtime selection 规则和测试证据。

### 6.2 版本与候选标识

只读 SQL 模板：

```sql
SELECT
  version,
  enabled,
  config->>'candidate_id' AS candidate_id,
  config->>'entry_rule_version' AS entry_rule_version,
  risk_config
FROM strategy_configs
WHERE strategy_key = '<strategy_key>'
  AND exchange = '<exchange>'
  AND symbol = '<symbol>'
  AND timeframe = '<timeframe>'
ORDER BY enabled DESC, version;
```

必须确认：

- `version` 是本次上线版本。
- `candidate_id` 或等价字段可追踪到回测/研究记录。
- `entry_rule_version` 或 manifest 可区分新旧入场语义。
- `risk_config` 与回测/产品口径一致。

### 6.3 止损配置

最低通过条件：

- live strategy 必须启用信号止损或等价保护单计划。
- `risk_config` 中必须能证明最大亏损约束存在。
- 执行 payload 必须在下单前生成可验证 stop-loss plan。

禁止：

- 没有止损计划仍生成 live execution task。
- 只在 UI 文案显示止损，实际 payload 不带止损。
- SDK 静默丢弃 `attached_stop_loss_price` 或等价字段。

## 7. Runtime 范围校验

策略 worker 必须收窄到目标范围。

只读命令模板：

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' <strategy-worker> \
  | grep -E '^(APP_ENV|IS_RUN_REAL_STRATEGY|IS_RUN_EXECUTION_WORKER|LIVE_STRATEGY_ONLY_EXCHANGES|LIVE_STRATEGY_ONLY_INST_IDS|LIVE_STRATEGY_ONLY_PERIODS|MARKET_DATA_EXCHANGE|DEFAULT_EXCHANGE|STRATEGY_SIGNAL_DISPATCH_MODE)='"
```

通过条件：

- `APP_ENV=production`。
- 策略 worker：`IS_RUN_REAL_STRATEGY=true`。
- 策略 worker：`IS_RUN_EXECUTION_WORKER=false`。
- exchange、symbol、period 与上线范围一致。
- signal dispatch mode 与生产链路一致。

不通过条件：

- worker 同时监听未验证交易对。
- worker 同时监听未验证周期。
- worker 同时承担策略扫描和 execution worker 职责。
- 生产 env 覆盖了 compose 默认范围。

## 8. Web 产品与 release pointer 校验

### 8.1 产品指针

只读 SQL 模板：

```sql
SELECT
  slug,
  status,
  core_strategy_key,
  core_strategy_version,
  back_test_log_id,
  display_win_rate_pct,
  display_max_drawdown_pct,
  display_trade_count,
  display_sharpe_ratio
FROM strategy_products
WHERE slug = '<strategy_slug>';
```

通过条件：

- 用户可见产品 `status` 符合预期，例如 `PUBLISHED`。
- `core_strategy_key` 和 `core_strategy_version` 指向 Core live 配置。
- 展示指标来自本次接受的回测口径。
- 如果展示指标未达到默认目标，产品或内部记录必须说明例外。

### 8.2 支持交易对

只读 SQL 模板：

```sql
SELECT p.slug, s.symbol, s.status, s.back_test_log_id
FROM strategy_supported_symbols s
JOIN strategy_products p ON p.id = s.product_id
WHERE p.slug = '<strategy_slug>'
ORDER BY s.symbol;
```

通过条件：

- 只暴露已经验证并允许生产的 symbol。
- 不得把 archived 产品下的 symbol 当成可实盘范围。
- symbol 的 backtest id 或证据链必须能追踪。

### 8.3 Release pointer

只读 SQL 模板：

```sql
SELECT
  p.slug,
  rp.symbol,
  rp.channel,
  rp.manifest_hash,
  rp.status,
  rp.promoted_by,
  rp.promoted_at,
  m.status AS manifest_status,
  m.human_label
FROM strategy_release_pointers rp
JOIN strategy_products p ON p.id = rp.product_id
JOIN strategy_manifests m ON m.manifest_hash = rp.manifest_hash
WHERE p.slug = '<strategy_slug>'
  AND rp.symbol = '<symbol>'
  AND rp.channel = 'live';
```

通过条件：

- live channel 指向本次 manifest。
- manifest status 是 `live`。
- manifest JSON 包含版本、范围、回测 id、指标、风险口径和例外说明。

## 9. 用户 readiness 校验

### 9.1 Combo 订阅与凭证

只读 SQL 模板：

```sql
SELECT
  s.buyer_email,
  s.execution_exchange,
  s.service_mode,
  s.status AS subscription_status,
  c.exchange AS credential_exchange,
  c.status AS credential_status,
  c.last_check_code,
  r.risk_acknowledged,
  r.status AS risk_status,
  r.max_position_usdt,
  r.max_daily_trades,
  r.emergency_stop_enabled
FROM strategy_combo_subscriptions s
LEFT JOIN user_api_credentials c
  ON lower(c.buyer_email) = lower(s.buyer_email)
 AND lower(c.exchange) = lower(COALESCE(NULLIF(s.execution_exchange, ''), '<default_exchange>'))
LEFT JOIN combo_risk_settings r
  ON r.combo_id = s.id
WHERE s.strategy_slug = '<strategy_slug>'
  AND s.symbol = '<symbol>'
ORDER BY s.buyer_email;
```

实盘通过条件：

- `service_mode=api_trade_enabled`。
- `subscription_status=active`。
- credential `status=active`。
- `last_check_code=signed_exchange_preflight_passed` 或等价通过状态。
- `risk_acknowledged=true`。
- `risk_status=active`。
- `max_position_usdt`、`max_daily_trades`、`max_daily_loss_usdt` 不为空且合理。
- `emergency_stop_enabled=true`，除非有明确关闭依据。

阻断条件：

- 没有 credential。
- credential 只读、过期、pending、failed。
- risk 未确认。
- combo 已过期或非 active。
- execution exchange 与 credential exchange 不匹配。

### 9.2 用户范围

必须明确：

- 哪些用户可以实盘。
- 哪些用户只能 signal-only。
- 哪些用户因为 credential 或 risk blocker 被阻断。
- 哪些用户的 blocker 需要人工处理。

不得用一个用户 readiness 代表所有订阅用户 readiness。

## 10. 信号合同校验

### 10.1 Core 信号

只读 SQL 模板：

```sql
SELECT
  signal_key,
  exchange,
  symbol,
  timeframe,
  side,
  signal_status,
  generated_at,
  payload ? 'selected_stop_loss_price' AS has_selected_stop_loss_price,
  payload->>'selected_stop_loss_price' AS selected_stop_loss_price,
  payload->>'entry_rule_version' AS entry_rule_version,
  payload->>'candidate_id' AS candidate_id
FROM strategy_signals
WHERE strategy_key = '<strategy_key>'
  AND exchange = '<exchange>'
  AND symbol = '<symbol>'
  AND timeframe = '<timeframe>'
ORDER BY generated_at DESC
LIMIT 10;
```

通过条件：

- 新信号的 exchange、symbol、timeframe 与上线范围一致。
- payload 带版本标识。
- payload 带方向和入场依据。
- live 信号必须带 stop-loss plan。

当前没有新信号时，不得宣称已经实盘成交；只能说明 worker 已进入等待下一根确认 K 线触发。

### 10.2 Web inbox

只读 SQL 模板：

```sql
SELECT
  id,
  external_id,
  strategy_slug,
  symbol,
  direction,
  generated_at,
  left(payload_json, 400) AS payload_prefix
FROM strategy_signal_inbox
WHERE strategy_slug = '<strategy_slug>'
  AND symbol = '<symbol>'
ORDER BY generated_at DESC
LIMIT 10;
```

通过条件：

- Web inbox 与 Core signal 一致。
- external_id 可去重。
- payload 没有丢失止损计划。
- signal-only 和 api-trade 的分发边界清楚。

## 11. Execution task 校验

### 11.1 任务状态

只读 SQL 模板：

```sql
SELECT task_status, COUNT(*) AS n
FROM execution_tasks
WHERE strategy_slug = '<strategy_slug>'
  AND symbol = '<symbol>'
GROUP BY task_status
ORDER BY task_status;
```

必须分类解释：

- `pending`：是否在 scheduled window 内，是否能被 lease。
- `leased/running`：lease owner 是否正常，是否超时。
- `succeeded`：是否有 exchange order result。
- `failed`：失败原因是配置、账户、交易所、精度、余额还是保护单。
- `blocked` 或等价状态：blocker 是否可人工解除。

### 11.2 Attempt 证据

只读 SQL 模板：

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
  AND t.symbol = '<symbol>'
ORDER BY t.id DESC, a.attempt_no DESC
LIMIT 20;
```

必须确认：

- task 使用了正确 `api_credential_id`。
- failed attempt 的原因已经归类。
- 下单前阻断不能被误报成交易所成交失败。
- 重试不会绕过 readiness、symbol filter 或止损校验。

### 11.3 订单结果

只读 SQL 模板：

```sql
SELECT
  execution_task_id,
  combo_id,
  buyer_email,
  exchange,
  external_order_id,
  order_side,
  order_status,
  filled_qty,
  filled_quote,
  created_at
FROM exchange_order_results
WHERE execution_task_id IN (<task_ids>)
ORDER BY created_at DESC;
```

通过条件：

- 有订单结果才能宣称交易所已接单或成交。
- 没有订单结果只能说明任务未成交或被阻断。
- 不得用 execution task created 代替 order placed。

## 12. 交易所与 SDK 校验

### 12.1 Symbol filter

只读 SQL 模板：

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

通过条件：

- symbol status 是 live 或等价可交易状态。
- min_qty、step_size、tick_size、contract value 可用于执行量化。
- 量化后数量不得低于最小步长。

### 12.2 Account mode 与权限

必须确认：

- signed read-only preflight 通过。
- API key 有读和交易权限。
- 持仓模式、保证金模式、杠杆模式符合执行 payload。
- 交易所拒绝自动切换账户模式时，必须在下单前阻断。

禁止：

- 为了成交绕过账户模式 blocker。
- SDK 静默降级订单字段。
- 手动使用 SDK mutation 绕过 Core worker。

## 13. 日志校验

策略 worker 日志至少确认：

```bash
ssh "$PROD_SSH" \
  "docker logs --since 30m <strategy-worker> 2>&1 \
  | grep -E '实时策略过滤后剩余|启动策略|WebSocket|订阅k线频道成功|legacy direct live exchange mutation is blocked|ERROR|panic|failed|失败' \
  | tail -n 120"
```

通过条件：

- 实时策略过滤后范围正确。
- 策略已预热并等待触发。
- WebSocket 订阅目标正确。
- 没有 panic 或循环错误。
- legacy direct live mutation 被阻断或未启用。

execution worker 日志至少确认：

```bash
ssh "$PROD_SSH" \
  "docker logs --since 30m <execution-worker> 2>&1 \
  | grep -E '执行任务 worker|handled=|execution|task|order|block|stop|ERROR|panic|failed|失败|止损|保护' \
  | tail -n 160"
```

通过条件：

- worker 正常轮询。
- 无循环重启。
- handled 数量与任务表状态能对上。
- 失败原因有 DB attempt 证据。

## 14. 上线结论分级

上线结论必须使用以下分级之一：

| 结论 | 含义 |
|---|---|
| `not_deployed` | 代码或镜像未上线 |
| `deployed_not_enabled` | 容器已上线，但 Core/Web 未切到该版本 |
| `enabled_waiting_signal` | 已启用，worker 等待下一次确认 K 线或触发条件 |
| `signal_generated_not_executable` | 信号已产生，但 readiness 或 payload blocker 阻断 |
| `execution_task_created` | 已生成任务，但未被 worker 成功处理 |
| `order_submitted` | 交易所已接单，有 order result |
| `order_filled` | 交易所已成交，有 filled evidence |
| `blocked` | 已明确 blocker，不允许继续放行 |
| `rolled_back` | 已回滚，需给出回滚后证据 |

禁止使用模糊结论：

- “应该可以实盘”。
- “看起来没问题”。
- “CI 绿了所以线上好了”。
- “任务有了所以已经下单”。
- “策略上线了所以用户都能交易”。

## 15. No-Go 条件

出现以下任一情况，禁止上线或必须立即阻断：

1. Core 同一 live 范围有多个 enabled 配置且没有明确选择规则。
2. 策略 worker 实际监听了未验证 symbol 或 timeframe。
3. 产品 release pointer 与 Core enabled version 不一致。
4. 信号 payload 缺少止损计划。
5. execution task 的 `api_credential_id=0` 或 credential 不匹配。
6. 用户 risk setting 未确认。
7. signed read-only preflight 未通过。
8. symbol filter 缺失或数量量化后低于最小步长。
9. 交易所账户模式不符合订单语义。
10. execution worker 不在目标 revision 或循环重启。
11. live mutation 会走 legacy direct path。
12. 没有回滚方案。

## 16. 回滚标准

必须回滚或停用的情况：

- 新版本生成错误交易对、错误周期或错误方向信号。
- 新版本信号缺少止损却进入 execution task。
- execution worker 因新版本出现系统性失败。
- 用户侧产品展示版本与 Core 实盘版本不一致且会误导用户。
- 交易所反馈订单语义与预期不一致。
- 回撤、失败率或异常任务数超过上线前约定阈值。

回滚后必须复核：

1. Core enabled version 已回到目标旧版本或全部禁用。
2. Web release pointer 已回到旧 manifest 或停用。
3. 策略 worker 已重新加载配置。
4. execution worker 没有继续处理新版本残留任务。
5. 历史 signal、task、attempt、order result 保留审计证据。

## 17. 校验报告模板

每次上线或巡检必须输出以下报告：

```markdown
# <strategy_slug> 生产严格校验报告

## 基本信息
- 校验时间：
- 校验人：
- strategy_key：
- strategy_slug：
- exchange / symbol / timeframe：
- Core version：
- deployed commit：
- image：

## 结论分级
- 当前结论：
- 是否允许实盘：
- 是否有 blocker：

## 回测与例外
- backtest id：
- 样本范围：
- trades：
- win_rate：
- max_drawdown：
- sharpe：
- 例外说明：

## Core 证据
- strategy_configs enabled：
- risk_config：
- stop-loss plan：

## Runtime 证据
- containers：
- env scope：
- logs：

## Web 证据
- product：
- supported symbol：
- release pointer：
- manifest：

## 用户 readiness
- active api-trade combos：
- credential：
- risk settings：
- blocked users：

## 信号与执行
- latest signals：
- execution tasks：
- attempts：
- order results：

## 交易所账户
- signed preflight：
- symbol filter：
- account mode：
- blocker：

## 回滚方案
- 回滚版本：
- 回滚步骤：
- 回滚后验证：
```

## 18. Vegas id102 实例

`docs/VEGAS_ID102_PRODUCTION_VALIDATION.md` 是本文档的一次具体实例化记录。

该实例只证明：

- `vegas / okx / ETH-USDT-SWAP / 4H` 的 `id102` 已按本文档完成生产校验。
- 不证明 Vegas 其他交易对、其他周期或其他策略也满足生产实盘条件。
