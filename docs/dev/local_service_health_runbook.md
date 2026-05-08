# Local Service Health Runbook

## Scope

`scripts/dev/check_local_service_health.sh` 是 `rust_quant` 本地/预发稳定性巡检入口，默认只做只读检查：

- 子仓库根目录存在性与 Git repo 状态。
- 已有 dev 脚本的 `bash -n` 语法检查。
- `rust_quan_web` 与 `rust_quant_news` 本地 HTTP 可达性检查。
- `quant_core`、`quant_web`、`quant_news` 的只读 `SELECT 1`。
- `quant_core.execution_worker_checkpoints` 最近 worker checkpoint 的只读查询与 heartbeat stale 判断。
- 显式 opt-in 后，`quant_core.execution_worker_checkpoints` lease 状态聚合与 `quant_core.exchange_request_audit_logs` request status 聚合。

脚本保持输出脱敏，只显示数据库 URL、secret 是否 `<set>` / `<unset>`，不打印 API key、secret 或数据库连接串。

## Local Triage Order

1. 先跑无数据库巡检，确认脚本、HTTP 和基础服务边界：

```bash
HEALTH_CHECK_DATABASES=false HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

2. 再跑数据库只读巡检，确认三库可读与 worker checkpoint：

```bash
HEALTH_CHECK_BINANCE=false ./scripts/dev/check_local_service_health.sh
```

3. 需要给自动化或 CI 读取时使用 JSON：

```bash
HEALTH_CHECK_OUTPUT=json HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

4. 预发排障时可调低 worker stale 阈值，例如 5 分钟：

```bash
HEALTH_CHECK_WORKER_STALE_SECS=300 HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

默认 stale 只记 warning；如需让 stale 直接失败：

```bash
HEALTH_CHECK_WORKER_STALE_LEVEL=fail HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

5. 需要查看 execution worker lease 与 exchange request audit 只读聚合时显式 opt-in：

```bash
HEALTH_CHECK_OUTPUT=json \
HEALTH_CHECK_BINANCE=false \
HEALTH_CHECK_EXECUTION_AUDIT=true \
HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS=24 \
./scripts/dev/check_local_service_health.sh
```

该检查只查询 `quant_core`，不会 lease execution task，不会 report result，不会读取或打印 request/response payload。

默认 `HEALTH_CHECK_WORKER_MODE=all` 是保守模式：脚本展示最近 worker checkpoint，但不会因为历史 smoke worker 或一次性 worker 的旧 heartbeat 让常规巡检变成 warning/failure。旧 heartbeat 会以 ignored/informational 形式出现在 human/JSON 输出里。

预发或生产需要配置“应在线 worker”时，显式列出 worker id。设置 `HEALTH_CHECK_EXPECTED_WORKERS` 后会自动进入 expected 模式，也可以显式设置 `HEALTH_CHECK_WORKER_MODE=expected`：

```bash
HEALTH_CHECK_WORKER_MODE=expected \
HEALTH_CHECK_EXPECTED_WORKERS="rust_quant_pref_worker_1,rust_quant_pref_worker_2" \
HEALTH_CHECK_WORKER_STALE_SECS=300 \
HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

在 expected 模式下：

- 列入 `HEALTH_CHECK_EXPECTED_WORKERS` 的 worker 超过 stale 阈值才会按 `HEALTH_CHECK_WORKER_STALE_LEVEL` 记 warning/failure。
- 未列入的历史 smoke worker、一次性 worker、旧审计 worker 只展示为 ignored/informational，不影响常规巡检退出码。
- 生产建议把 `HEALTH_CHECK_EXPECTED_WORKERS` 固定为当前部署编排中确实应在线的 worker id，并让 `HEALTH_CHECK_WORKER_STALE_SECS` 大于正常 heartbeat 间隔的 2-3 倍。

也可以保留 warning 语义，但让任意 warning 变成非零退出：

```bash
HEALTH_CHECK_STRICT=true HEALTH_CHECK_BINANCE=false \
./scripts/dev/check_local_service_health.sh
```

## CI / Preflight

CI 或预发推荐使用 JSON 输出，并显式关闭 Binance 检查：

```bash
HEALTH_CHECK_OUTPUT=json \
HEALTH_CHECK_BINANCE=false \
HEALTH_CHECK_WORKER_MODE=expected \
HEALTH_CHECK_EXPECTED_WORKERS="rust_quant_pref_worker_1,rust_quant_pref_worker_2" \
HEALTH_CHECK_WORKER_STALE_SECS=300 \
HEALTH_CHECK_WORKER_STALE_LEVEL=fail \
./scripts/dev/check_local_service_health.sh
```

需要把 warning 也作为失败时，再加：

```bash
HEALTH_CHECK_STRICT=true
```

exit code 语义：

- `0`: 没有 failure；如果 `HEALTH_CHECK_STRICT=false`，warning 不会导致非零退出。
- `1`: 存在 failure，或 `HEALTH_CHECK_STRICT=true` 且存在 warning。

Admin/CI 解析 JSON 时优先读取：

- `status`: `ok` / `warn` / `fail`。
- `summary.expected_worker_failures`: expected worker stale 且按 fail 计数的数量。
- `summary.expected_worker_warnings`: expected worker stale 且按 warning 计数的数量。
- `summary.ignored_worker_count`: expected 模式下未列入 expected allowlist 的 worker 数量。
- `summary.ignored_stale_worker_count`: all 模式下被展示但不影响巡检状态的历史 stale worker 数量。
- `summary.execution_audit_recent_failures`: opt-in audit 窗口内 `exchange_request_audit_logs` 的非 completed 请求数。
- `summary.execution_audit_stale_leased_workers`: opt-in audit 中 heartbeat 已 stale 的 leased/processing worker 数。
- `alerts`: 适合 Admin/CI 直接消费的结构化告警数组。

## JSON Stability Contract

Admin/CI 可以把 JSON 顶层字段视为稳定契约：`output`、`status`、`warnings`、`failures`、`repo`、`umbrella`、`quant_core_database_url`、`web_database_url`、`news_database_url`、`execution_event_secret`、`database_checks`、`binance_public_check`、`execution_audit_check`、`worker_stale_secs`、`worker_stale_level`、`worker_mode`、`expected_workers`、`summary`、`checks`、`alerts` 不应删除或改名。后续脚本只能追加兼容字段，消费方必须忽略未知字段。

状态与退出码解释：

- `status=ok`: `warnings=0` 且 `failures=0`，可继续本地巡检、预发检查或发布前检查。
- `status=warn`: `warnings>0` 且 `failures=0`。默认 exit code 仍为 `0`，但 Admin/CI 应展示人工复核；若设置 `HEALTH_CHECK_STRICT=true`，warning 会变成非零退出。
- `status=fail`: `failures>0`，exit code 为 `1`，应阻止实盘与阻止发布，直到失败项被解释或修复。

`summary` 字段解释：

- `summary.expected_worker_failures>0`: P0。应在线 worker heartbeat 已 stale 且按 failure 计数，阻止实盘、阻止发布。
- `summary.expected_worker_warnings>0`: P1。应在线 worker heartbeat 已 stale 但当前按 warning 计数；预发/生产建议人工确认，严格 CI 可通过 `HEALTH_CHECK_STRICT=true` 升级为阻断。
- `summary.ignored_worker_count>0`: P1/INFO。expected 模式下发现未列入 allowlist 的 worker，通常代表历史 smoke worker、一次性 worker 或配置漂移；默认不阻断，但需要核对 expected worker 列表。
- `summary.ignored_stale_worker_count>0`: INFO。all 模式下的历史 smoke 噪声或一次性 worker 旧 heartbeat，只展示上下文，不应单独阻断常规巡检、实盘或发布。
- `summary.execution_audit_recent_failures>0`: P1。表示 opt-in 窗口内存在 failed/error exchange request audit，需要排查交易所请求路径，但不单独证明订单状态异常。
- `summary.execution_audit_stale_leased_workers>0`: P1。表示有 leased/processing checkpoint 的 heartbeat 超过 stale 阈值，需要检查 worker 是否卡在 lease 处理中。

`checks[]` 是给人和 Admin 明细页使用的事件列表。消费方可以按 `level=FAIL` 显示阻断项，按 `level=WARN` 显示待复核项，按 `level=INFO` 显示历史/ignored 背景。

`alerts[]` 是给 Admin/CI 告警规则使用的稳定数组。每一项只包含小字段：

- `severity`: `P0` / `P1` / `INFO`。
- `code`: 稳定机器码，例如 `EXPECTED_WORKER_STALE`、`UNEXPECTED_WORKER`、`IGNORED_STALE_WORKER`、`HEALTH_CHECK_FAIL`、`HEALTH_CHECK_WARN`。
- opt-in audit 还会产生 `EXCHANGE_REQUEST_AUDIT_FAILURES`、`WORKER_LEASE_STALE`、`EXECUTION_AUDIT_TABLE_MISSING`。
- `section`: 检查分组，例如 `Databases`。
- `message`: 已脱敏的人类上下文，不包含数据库 URL、secret、API key、Binance signed/account/order/position endpoint 或请求体。

Admin/CI 推荐优先按 `alerts[].severity` 路由：`P0` 阻断，`P1` 进入人工复核或严格 CI 阻断，`INFO` 只展示上下文。`checks[]` 仍保留为完整明细，`alerts[]` 只承载需要告警面板消费的子集。

## P0 / P1 Alerting

P0 表示应阻止实盘或阻止发布：

- `status=fail` 或 `failures>0`。
- `summary.expected_worker_failures>0`。
- `alerts[].severity=P0`，尤其是 `code=EXPECTED_WORKER_STALE`。
- 必需服务 repo root 缺失、脚本 `bash -n` 失败。
- 数据库只读 `SELECT 1` 失败，或 expected/preflight 模式下无法读取 `execution_worker_checkpoints`。
- `HEALTH_CHECK_STRICT=true` 时任意 warning 产生非零退出。

P1 表示需要值班或发布负责人复核：

- `status=warn` 且 `failures=0`。
- `summary.expected_worker_warnings>0`。
- `summary.ignored_worker_count>0`，表示有未列入 expected allowlist 的 worker。
- `alerts[].severity=P1`，例如 `code=EXPECTED_WORKER_STALE` 或 `code=UNEXPECTED_WORKER`。
- `summary.execution_audit_recent_failures>0` 或 `alerts[].code=EXCHANGE_REQUEST_AUDIT_FAILURES`。
- `summary.execution_audit_stale_leased_workers>0` 或 `alerts[].code=WORKER_LEASE_STALE`。
- 本地 Web/News HTTP 不可达，或本地缺少 `curl`/`psql` 等诊断工具。

INFO 表示不应单独阻断：

- `summary.ignored_stale_worker_count>0`。
- `checks[]` 中的 `ignored_stale_worker_id`。
- `alerts[].severity=INFO` 且 `code=IGNORED_STALE_WORKER`。
- `alerts[].code=EXECUTION_AUDIT_TABLE_MISSING`，表示当前库还没有部署 opt-in audit 表，只能降级展示。
- 只属于历史 smoke 噪声、一次性 smoke worker、旧审计 worker 的 stale heartbeat。

## Worker Lease / Retry / Exchange Audit View

脚本当前实现的只读检查：

- `HEALTH_CHECK_EXECUTION_AUDIT=true` 后读取 `quant_core.exchange_request_audit_logs` 的聚合计数，只输出 `recent_total`、`recent_failures`、`max_latency_ms`、`lookback_hours`，不输出 endpoint、symbol、request payload、response payload、API key 或 secret。
- 同一 opt-in 检查读取 `quant_core.execution_worker_checkpoints` 中 `worker_status in ('leased', 'processing')` 的聚合计数，输出 `leased_workers` 与 `stale_leased_workers`。
- 表不存在时降级为 INFO；查询失败或聚合出现 recent failures / stale leased workers 时进入 P1。

Admin/CI 面板建议字段：

- `execution_audit_check`: 是否显式启用了该只读视图。
- `summary.execution_audit_recent_failures`: exchange request audit 非 completed 数量。
- `summary.execution_audit_stale_leased_workers`: stale leased/processing worker 数量。
- `alerts[].code`: `EXCHANGE_REQUEST_AUDIT_FAILURES`、`WORKER_LEASE_STALE`、`EXECUTION_AUDIT_TABLE_MISSING`。

仍属于 runbook/契约层定义、脚本未实现的部分：

- Web-owned `execution_tasks` / `execution_task_attempts` retry 详情、order result 和 trade record 的跨库关联。它们属于 `rust_quan_web` 业务事实，本脚本不调用 Web lease/report endpoint，也不修改任务状态。
- 如需 Admin 做完整链路面板，应由 Admin/CI 以只读 DB 账号读取相应表，并继续遵守字段脱敏：不展示 API key、secret、请求体、响应体或 signed endpoint。

## Cross-Service Read-Only Aggregator Contract

`check_local_service_health.sh` 当前仍是 `rust_quant` 本地 health 入口。全产品只读 aggregator 应作为后续独立入口实现，不能破坏上面的 JSON Stability Contract；可以追加字段或单独输出一个新 JSON。推荐固定：

```text
FULL_PRODUCT_HEALTH_SCHEMA_VERSION=1
```

当前有两个可消费入口。fixture runner 用于固定 Admin/CI schema contract：

```bash
./scripts/dev/check_full_product_health_aggregator_fixture.sh
```

它只做三件事：

- 读取 `docs/dev/full_product_health_aggregator.fixture.json`。
- 对 fixture 做结构校验，确认 `schema_version`、`status`、`summary`、`sections`、`alerts`、`correlation` 仍满足 Admin/CI 约定。
- 对 fixture 和 `check_local_service_health.sh` 做本地只读安全扫描，阻止 Binance signed/account/order/position endpoint、Web lease/report mutation endpoint、DB URL、API key/secret、raw payload 等敏感内容混入该最小入口。

该入口默认且只支持 machine-readable JSON 输出，不读取 `.env`，不连库，不 lease/report/mutate task，不访问真实交易所 signed endpoint。

Phase 36 的最小真实只读 collector runner 是：

```bash
./scripts/dev/check_full_product_health.sh
```

它输出同一个 full-product 顶层 JSON 形状：`schema_version`、`status`、`summary`、`sections`、`alerts`、`correlation`。默认行为仍是只读、本地文件/子进程模式：

- 读取 `docs/dev/full_product_health_aggregator.fixture.json` 只作为 schema key 来源，不回放 fixture 样例事实。
- 若未设置 `FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH`，通过子进程调用 `check_local_service_health.sh` 的 JSON 输出，并强制设置 `HEALTH_CHECK_OUTPUT=json`、`HEALTH_CHECK_DATABASES=false`、`HEALTH_CHECK_BINANCE=false`、`HEALTH_CHECK_EXECUTION_AUDIT=false`。
- 不读取 `.env`，不继承 API key/secret/数据库 URL 给 local health 子进程，不连库，不调用 Binance public/signed/account/order/position endpoint，不 lease/report/mutate task。
- 将 local health 的 worker/audit summary 映射到 `sections.quant_worker_checkpoint_audit`，并把 `EXPECTED_WORKER_STALE` 等本地 alert 映射为 `QUANT_EXPECTED_WORKER_STALE` 等 full-product alert code。

Phase 39 增加的安全一键输入 runner 是：

```bash
./scripts/dev/build_full_product_health_inputs.sh
```

它先按显式提供的只读 URL 分别调用三个 producer：

- `build_full_product_health_web_input.sh`
- `build_full_product_health_news_input.sh`
- `build_full_product_health_admin_input.sh`

然后把生成的三个临时 JSON 路径传给 `check_full_product_health.sh` 合并成完整 full-product 报告。未提供的 section 不会尝试读取配置或 `.env`，而是复用对应 producer 的 skipped JSON：`WEB_INPUT_SKIPPED`、`NEWS_INPUT_SKIPPED`、`ADMIN_INPUT_SKIPPED`。默认临时文件会自动删除；需要排查 producer 输出时可以显式设置 `FULL_PRODUCT_HEALTH_KEEP_INPUTS=true`。

最小本地只读合并，不采集本地 worker 子进程时：

```bash
FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false \
./scripts/dev/build_full_product_health_inputs.sh
```

采集三段真实只读 DB 输入时必须显式传入 URL：

```bash
FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=<readonly-web-postgres-url> \
FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=<readonly-news-postgres-url> \
FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=<readonly-admin-postgres-url> \
./scripts/dev/build_full_product_health_inputs.sh
```

该入口用白名单环境调用 producer 和 aggregator：只传递 `PATH`、`FULL_PRODUCT_HEALTH_*` 的只读采样参数和本轮临时 JSON 路径，不继承 API key、交易所 secret 或 provider secret。它会扫描 producer 输出和最终合并报告，拒绝 `.env`、DB URL、API key/secret、request/response/raw payload、Binance signed/account/order/position endpoint、Web lease/report/order mutation endpoint 和 `LINKUSDT` 等敏感标记。它不访问交易所，不下单，不调用 Web lease/report/order endpoint。

如果 CI 或 Admin 已经有只读 SQL/HTTP 采样结果，可先落到本地 JSON 文件，再通过 env 输入给 runner 合并：

```bash
FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH=/tmp/local-health.json \
FULL_PRODUCT_HEALTH_WEB_JSON_PATH=/tmp/web-task-order-health.json \
FULL_PRODUCT_HEALTH_NEWS_JSON_PATH=/tmp/news-source-ai-health.json \
FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH=/tmp/admin-readiness.json \
./scripts/dev/check_full_product_health.sh
```

这些输入文件必须已经脱敏，且只能包含只读事实。runner 会扫描并移除/拒绝 DB URL、API key/secret、raw payload、Binance signed/account/order/position endpoint、Web lease/report mutation endpoint 等敏感内容；输出永远保持 machine-readable JSON。若输入被拒绝，runner 输出 `status=fail` 与结构化 alert，仍不打印原始敏感内容。

### Web Input Producer

第一片真实 Web 只读输入由独立 producer 生成：

```bash
./scripts/dev/build_full_product_health_web_input.sh
```

默认不读取任何 Web 连接配置，也不读取 `.env`。未显式提供只读 Web 数据库输入时，脚本仍以 exit code `0` 输出 machine-readable JSON，字段为 `status=warn`、`source=skipped`、`skipped=true`，并带 `WEB_INPUT_SKIPPED` INFO alert，表示 `web_task_order_health` 真实业务事实未采集。

需要采集 `quant_web` 业务事实时，必须显式传入只读 PostgreSQL URL：

```bash
FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=<readonly-web-postgres-url> \
FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS=3600 \
FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS=900 \
FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS=900 \
./scripts/dev/build_full_product_health_web_input.sh > /tmp/web-task-order-health.json

FULL_PRODUCT_HEALTH_WEB_JSON_PATH=/tmp/web-task-order-health.json \
./scripts/dev/check_full_product_health.sh
```

该 producer 只执行只读 `SELECT` 聚合，读取：

- `news_signal_inbox`：仅用于关联 `signal_inbox_id`。
- `execution_tasks`：聚合 open / stale / completed-but-missing-result / failed task 数。
- `execution_task_attempts`：聚合最近 attempt 与 retry backlog。
- `exchange_order_results`：聚合最近 order result 数与缺失情况。
- `user_trade_records`：聚合最近 trade record 数与缺失情况。
- `combo_signal_delivery_logs`：聚合 delivery blocker 数。

输出只包含最小可合并 section JSON：`status`、`source`、`read_only_input`、窗口阈值、计数、脱敏 sample、`alerts[]`、`correlation`。它不会输出 Web DB URL、API key/secret、buyer email、symbol、request/response/raw payload，也不会输出 signed/account/order/position endpoint。查询失败、`psql` 不可用或 URL 未提供时不会写库、不会重试 mutation，而是输出 `WEB_INPUT_QUERY_FAILED` 或 `WEB_INPUT_SKIPPED` 的降级 JSON，供 full-product runner 合并展示。

第二片真实 News/AI 只读输入由独立 producer 生成：

```bash
./scripts/dev/build_full_product_health_news_input.sh
```

默认不读取任何 News 连接配置，也不读取 `.env`。未显式提供只读 News 数据库输入时，脚本仍以 exit code `0` 输出 machine-readable JSON，字段为 `status=warn`、`source=skipped`、`skipped=true`，并带 `NEWS_INPUT_SKIPPED` INFO alert，表示 `news_source_ai_health` 真实新闻/AI 事实未采集。

需要采集 `quant_news` 新闻与 AI 事实时，必须显式传入只读 PostgreSQL URL：

```bash
FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=<readonly-news-postgres-url> \
FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS=3600 \
FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS=1800 \
FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS=3600 \
./scripts/dev/build_full_product_health_news_input.sh > /tmp/news-source-ai-health.json

FULL_PRODUCT_HEALTH_NEWS_JSON_PATH=/tmp/news-source-ai-health.json \
./scripts/dev/check_full_product_health.sh
```

该 producer 只执行只读 `SELECT` 聚合，读取：

- `news_source_states` / `news_source_health`：聚合 source 总数、degraded / paused / retryable source、连续失败阈值。
- `news_items` 与 split news tables（如 `news_items_jinse`、`news_items_theblockbeats`、`news_items_coindesk`、`news_items_seekingalpha`）：聚合观察窗口内新闻数与候选信号数。
- `news_ai_analysis_results`：聚合最近 AI analysis 数、actionable signal 数，并提供脱敏 `news_id` / `analysis_result_id` correlation。
- `news_analysis_jobs`：聚合 failed / stale locked analysis job 数。
- `news_provider_call_logs` 与 `ai_prompt_configs`：聚合 provider failure 与 active prompt config 可用性；不输出 endpoint、请求、响应、raw response 或 provider secret。

输出只包含最小可合并 section JSON：`status`、`source`、`read_only_input`、窗口阈值、source/news/AI/job/provider 计数、脱敏 sample、`alerts[]`、`correlation`。它不会输出 News DB URL、API key/secret、provider raw request/response、AI raw response、新闻正文、标题、reason、symbol 或交易 endpoint。查询失败、`psql` 不可用或 URL 未提供时不会写库、不会调用 provider，而是输出 `NEWS_INPUT_QUERY_FAILED` 或 `NEWS_INPUT_SKIPPED` 的降级 JSON，供 full-product runner 合并展示。

第三片真实 Admin 只读输入由独立 producer 生成：

```bash
./scripts/dev/build_full_product_health_admin_input.sh
```

默认不读取任何 Admin 连接配置，也不读取 `.env`。未显式提供只读 Admin 数据库输入时，脚本仍以 exit code `0` 输出 machine-readable JSON，字段为 `status=warn`、`source=skipped`、`skipped=true`，并带 `ADMIN_INPUT_SKIPPED` INFO alert，表示 `admin_readiness` 真实审计事实未采集。

需要采集 `rust_quant_admin` 操作审计事实时，必须显式传入只读 PostgreSQL URL：

```bash
FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=<readonly-admin-postgres-url> \
FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS=7200 \
./scripts/dev/build_full_product_health_admin_input.sh > /tmp/admin-readiness.json

FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH=/tmp/admin-readiness.json \
./scripts/dev/check_full_product_health.sh
```

该 producer 只执行只读 `SELECT` 聚合，读取：

- `admin_operation_logs`：聚合观察窗口内后台操作日志总数、高危操作数、失败操作数、readiness blocker/manual review 数。
- 必需高危审计动作：`risk_review_confirm`、`risk_review_cancel`、`api_key_upsert`、`onchain_provider_control_upsert`、`strategy_config_upsert`、`backtest_run`、`exchange_symbol_sync`、`manual_ai_analysis`。
- action audit 覆盖：按必需动作检查近期是否缺失审计记录，输出 `missing_required_action_count`。

输出只包含最小可合并 `admin_readiness` section JSON：`status`、`source`、`read_only_input`、`lookback_secs`、计数、脱敏 sample、`alerts[]`、`correlation`。它不会输出 Admin DB URL、管理员用户名、target 原值、payload、API key/secret/cipher/passphrase、request/response/raw payload，也不会调用 signed/account/order/position endpoint 或 Web lease/report mutation endpoint。查询失败、`psql` 不可用或 URL 未提供时不会写库、不会重试 mutation，而是输出 `ADMIN_INPUT_QUERY_FAILED` 或 `ADMIN_INPUT_SKIPPED` 的降级 JSON，供 full-product runner 合并展示。

aggregator 只聚合已经落库或已有本地 health 输出的事实，不主动推进业务状态。硬边界：

- 不写库。
- 不 lease task。
- 不 report result。
- 不调用 Web risk-review / lease / report mutation endpoint。
- 不调用 Binance signed/account/order/position endpoint。
- 不调用任何交易所下单、撤单、改杠杆、改仓位模式接口。
- 不读取或打印 `.env`、API key、secret、数据库 URL、请求体或响应体。
- 不触碰 `LINKUSDT`，也不把历史 smoke worker stale 单独升级为 P0。

### Aggregator Inputs

`web_task_order_health` 只读读取 `quant_web` 业务事实：

- `news_signal_inbox`: `id`、`source`、`external_id`、`strategy_slug`、`symbol`、`created_at`。
- `execution_tasks`: `id`、`news_signal_id`、`strategy_signal_id`、`task_type`、`task_status`、`lease_owner`、`lease_until`、`created_at`、`updated_at`。
- `execution_task_attempts`: task attempt 数、最近 attempt 状态、最近错误分类；不输出 raw payload。
- `exchange_order_results`: `id`、`execution_task_id`、`exchange`、`symbol`、`order_status`、`created_at`、`updated_at`。
- `user_trade_records`: `id`、`execution_task_id` 或可关联 order result 的业务主键、`created_at`。

`news_source_ai_health` 只读读取 `quant_news` 新闻和 AI 事实：

- `news_source_states` / `news_source_health`: `source`、`effective_status`、`consecutive_failures`、`last_success_at`、`last_failure_at`、`paused_until`。
- `news_ai_analysis_results`: `id`、`news_id`、`prompt_key`、`prompt_version`、`created_at`、`status` 或可推导状态。
- split news tables: `news_id`、`source`、`status`、`is_signal_candidate`、`deep_analyzed_at`、`error_message` 的脱敏分类。

`quant_worker_checkpoint_audit` 复用 `rust_quant` health 和 `quant_core` 技术事实：

- `execution_worker_checkpoints`: `worker_id`、`worker_kind`、`worker_status`、`last_task_id`、`last_heartbeat_at`、heartbeat age。
- `exchange_request_audit_logs`: 按窗口聚合 `request_status`、失败数、最大延迟；不输出 endpoint、request payload、response payload。
- `check_local_service_health.sh` JSON: `status`、`summary.expected_worker_failures`、`summary.expected_worker_warnings`、`summary.execution_audit_recent_failures`、`alerts`。

`admin_readiness` 是只读的发布/实盘准入视图：

- `admin_operation_logs`: 只聚合 module/action/outcome/readiness 状态、覆盖计数和最近脱敏 log id；不输出管理员用户名、target 原值或 payload。
- 最近一次 health aggregator `status`。
- expected worker 是否在线。
- Web pending / leased / failed task 是否在阈值内。
- News source / AI provider 是否处于 degraded 或 blocked。
- live readiness 是否被明确标记为 blocked / manual_review / ready；该字段只能由只读事实推导，不能执行账户检查或实盘 preflight。

### Aggregator Output

推荐 JSON 顶层字段：

- `schema_version`: 固定为 `FULL_PRODUCT_HEALTH_SCHEMA_VERSION` 当前值。
- `status`: `ok` / `warn` / `fail`。
- `generated_at`: aggregator 生成时间。
- `summary`: 只放计数与布尔值，例如 `p0_count`、`p1_count`、`info_count`、`web_open_task_count`、`news_degraded_source_count`、`quant_expected_worker_failures`。
- `sections`: 包含 `web_task_order_health`、`news_source_ai_health`、`quant_worker_checkpoint_audit`、`admin_readiness` 的脱敏明细。
- `alerts`: 兼容现有 `severity`、`code`、`section`、`message` 形状；消费方必须忽略未知字段。

状态聚合规则：

- 任一 `alerts[].severity=P0` => `status=fail`。
- 没有 P0 但存在 `alerts[].severity=P1` => `status=warn`。
- 只有 `INFO` 或无 alert => `status=ok`。

### Admin / CI Summary Artifact

`summarize_full_product_health.sh` 从现有 full-product JSON 读取，不调用 producer，不读取 `.env`，不访问交易所，也不调用 Web lease/report/order endpoint。它面向 Admin readiness panel、CI 注释和发布 checklist，输出更稳定的小型 artifact：

```bash
./scripts/dev/check_full_product_health.sh > /tmp/full-product-health.json

FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH=/tmp/full-product-health.json \
FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT=5 \
./scripts/dev/summarize_full_product_health.sh
```

也可以通过 stdin 读取已生成 JSON：

```bash
./scripts/dev/build_full_product_health_inputs.sh \
  | ./scripts/dev/summarize_full_product_health.sh
```

Summary artifact 顶层字段固定为：

- `status`: 直接沿用 full-product `status`。
- `summary.overall_status`: 同 `status`，便于只读 dashboard 绑定。
- `summary.p0_count` / `summary.p1_count` / `summary.info_count`: 从 `alerts[]` 重新计算；无 alerts 时回退到源 `summary`。
- `section_statuses`: `web_task_order_health`、`news_source_ai_health`、`quant_worker_checkpoint_audit`、`admin_readiness` 到 `ok` / `warn` / `fail` 的扁平映射。
- `checklist`: 每个 section 一条，包含 `ready`、`action_required`、`p0_count`、`p1_count`、`info_count`，用于 CI checklist 或 Admin 首屏 readiness 列表。
- `top_alerts`: 按 P0、P1、INFO 顺序截断展示，截断数量由 `FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT` 控制。
- `required_operator_actions`: 只从 P0/P1 alert 派生。P0 输出 `block_release_until_resolved`，P1 输出 `manual_review_before_release`。
- `correlation`: 保留脱敏后的 correlation object。
- `correlation_ids`: 将非空 correlation 展平成 `key` / `value` 数组，方便 CI 或 Admin 表格直接渲染。

Summary 脚本只做结构整理和脱敏，不改变原始 aggregator 语义。消费方仍应以 full-product `alerts[]` 和原始 section 明细为最终诊断依据；summary artifact 只用于更稳定的首屏状态、checklist 和 operator action 展示。

### Readable Markdown Artifact

`render_full_product_health_markdown.sh` 从已经生成的 summary JSON 渲染人工可读 Markdown。它不调用 producer，不读取 `.env`，不访问本地服务，不访问交易所，也不调用任何 Web mutation endpoint；输入只能来自 `FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH` 或 stdin。

```bash
FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH=/tmp/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH=/tmp/full-product-health.json \
FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH=/tmp/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_MARKDOWN_PATH=/tmp/full-product-health.md \
./scripts/dev/render_full_product_health_markdown.sh > /tmp/full-product-health.md
```

Markdown artifact 固定包含：

- `Status`: summary `overall_status` 或顶层 `status`。
- `Counts`: `p0_count`、`p1_count`、`info_count`、section 数、阻断/告警 section 数、top alert 数、operator action 数、read-only input 数。
- `Top Alerts`: summary `top_alerts` 的 severity、code、section、message。
- `Checklist`: summary `checklist` 的 section、status、ready、action_required、P0/P1/INFO 计数和 reason。
- `Artifact Paths`: full report JSON、summary JSON、Markdown artifact 的路径。路径只作为展示值，不触发读取。
- `Skipped Sections`: 从 summary checklist 的 `skipped=true` / `reason_code=*_SKIPPED`，或 top alert 的 `*_SKIPPED` code 推导。

Markdown renderer 会对输出做同一套敏感标记扫描，拒绝 `.env`、数据库 URL、API key/secret、raw payload、Binance signed/account/order/position endpoint、Web lease/report/order mutation endpoint 和 `LINKUSDT` 标记。

### Safe CI Wrapper

`run_full_product_health_ci.sh` 是 CI / preflight 推荐入口。它只运行一键 input builder 和 summary，不读取 `.env`，默认不跑 local health 子进程，不访问交易所，不下单，不 lease task，不 report result，也不调用 Web order endpoint。无显式 URL 时也会写出 skipped full report 和 summary artifact。

```bash
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json \
FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
./scripts/dev/run_full_product_health_ci.sh
```

产物约定：

- `FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR`: 默认 artifact 目录；未显式设置文件路径时用于生成默认文件名。
- `FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH`: full-product report JSON 写入路径。
- `FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH`: summary JSON 写入路径；脚本同时把该 JSON 输出到 stdout，方便 CI annotation 直接消费。
- `FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH`: 可选。设置后 wrapper 会从 summary JSON 渲染 Markdown artifact；不设置时不生成 Markdown，保持原有 JSON-only 默认行为。
- `FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false`: 默认值。需要采集 `quant_worker_checkpoint_audit` 时必须显式改为 `true`，且 local health 仍会被 aggregator 强制设置为 `HEALTH_CHECK_DATABASES=false`、`HEALTH_CHECK_BINANCE=false`、`HEALTH_CHECK_EXECUTION_AUDIT=false`。
- `FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS`: 默认 `fail`，即 summary `overall_status=fail` 时退出非零；设为 `warn` 时 `warn/fail` 都阻断；设为 `never` 时只写 artifact，不按 overall status 阻断，例如 `FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never`。

CI wrapper 使用 `env -i` 白名单调用 builder、summary 和可选 Markdown renderer，只传递 `PATH`、`FULL_PRODUCT_HEALTH_*` 的只读输入变量和 artifact 路径。它会扫描 full report、summary 和可选 Markdown artifact，拒绝 `.env`、数据库 URL、API key/secret、raw payload、Binance signed/account/order/position endpoint、Web lease/report/order mutation endpoint 和 `LINKUSDT` 标记。

### Artifact Validation

`validate_full_product_health_artifacts.sh` 是独立 artifact 校验/脱敏扫描器，用于 CI 上传前的最后一层守卫。它只读取已经生成的本地文件，不调用 producer，不读取 `.env`，不访问本地服务，不访问交易所，不 lease/report/mutate task。

```bash
FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json \
FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true \
./scripts/dev/validate_full_product_health_artifacts.sh > /tmp/full-product-health-ci/full-product-health-validation.json
```

默认情况下，validator 会读取 `docs/dev/full_product_health_artifact_schema.json` 作为 machine-readable contract。需要在临时分支或下游 CI 校验候选 schema 时，可以显式设置：

```bash
FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH=/tmp/full_product_health_artifact_schema.candidate.json \
./scripts/dev/validate_full_product_health_artifacts.sh
```

validator 固定输出 machine-readable JSON：

- `status`: `ok` / `warn` / `fail`。schema 缺失、schema 结构异常等非 strict 情况会输出明确 finding，通常表现为 `warn`；P0 artifact 问题仍为 `fail`。
- `schema`: 本轮使用的 schema 路径、存在性、JSON 可解析性和 schema version 摘要。
- `summary.artifact_count`: 本轮配置并参与校验的 artifact 数。
- `summary.missing_artifact_count`: 路径缺失或文件不存在的 artifact 数。
- `summary.json_parse_error_count`: full report / summary JSON 解析失败数。
- `summary.missing_required_field_count`: schema 定义的必需顶层字段和 `summary.*` 必需字段缺失数。
- `summary.sensitive_marker_count`: 敏感/危险标记命中数。
- `artifacts.full_report`、`artifacts.summary`、`artifacts.markdown`: 每个 artifact 的存在性、JSON 可解析性或 Markdown section marker 缺失情况。
- `findings[]`: 只输出安全的 `code` / `marker_code` / `artifact`，不会回显原始敏感片段。

校验规则：

- full report JSON 必须存在、可解析，并满足 schema 中 `artifact_schemas.full_report.required_top_level` 和 `required_summary_fields`。
- summary JSON 必须存在、可解析，并满足 schema 中 `artifact_schemas.summary.required_top_level` 和 `required_summary_fields`。
- `status`、`summary.overall_status`、`alerts[].severity`、`top_alerts[].severity`、`required_operator_actions[].severity` 必须命中 schema 中的 `status_values` / `severity_values`。
- `alert_taxonomy[].code`、`alerts[].code`、`top_alerts[].code` 必须命中 schema 中的 `alert_code_values[section]` 或 `alert_code_values.global`；未知 code 会输出 `INVALID_ALERT_CODE`，用于阻断 producer/schema 漂移。
- `alert_code_metadata[section][code]` 为 Admin playbook 提供安全默认值：`owner`、`default_next_action`、`admin_link_target`。这些值只能是稳定 owner/action/route key，不能包含本地路径、secret、raw payload、交易所 signed/account/order/position endpoint 或 live symbol。
- 设置 `FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH` 后，Markdown artifact 必须存在，并包含 schema 中的 `markdown_required_markers`。
- full report、summary、Markdown 以及 artifact 路径都会扫描 `.env`、数据库 URL、API key/secret、passphrase/cipher、request/response/raw payload、Binance signed/account/order/position endpoint、Web lease/report/order mutation endpoint 和 `LINKUSDT` 标记。
- schema 缺失或不可解析时不会静默降级：未开启 strict 时输出 `SCHEMA_MISSING` / `SCHEMA_INVALID_JSON` / `SCHEMA_FIELD_INVALID` finding；`FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true` 时任何 finding 都会退出非零。

CI wrapper 可以显式 opt-in 调用 validator 并写 validation artifact：

```bash
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true \
FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
./scripts/dev/run_full_product_health_ci.sh
```

`FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS` 默认是 `false`，所以旧 JSON-only CI 行为不变；开启后 wrapper 使用 `env -i` 白名单调用 validator，只传递 artifact 路径和 validation 参数。默认 `FULL_PRODUCT_HEALTH_CI_VALIDATION_STRICT=true`，validation 失败会阻止后续上传。未显式设置 `FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH` 时，路径默认为 `${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR}/full-product-health-validation.json`。

### Stable Artifact Schema And Examples

Admin / CI 不应解析脚本文本来判断字段。稳定契约沉淀在：

- `docs/dev/full_product_health_artifact_schema.json`: machine-readable schema source，定义 full report、summary、validation JSON 的必需字段、可追加路径、`status` 枚举和 alert/action `severity` 枚举。
- `docs/dev/full_product_health_artifact_schema.md`: 人工说明文档，解释兼容边界、consumer 绑定方式和安全边界。
- `docs/dev/full_product_health_examples/`: 脱敏 example artifact set，包含 full report JSON、summary JSON、Markdown、validation JSON。

契约测试会读取 schema JSON，校验 example artifact set 与核心字段、枚举和 Markdown marker 一致，并用 validator strict mode 重新校验 example full report / summary / Markdown。后续 Admin/CI 新增消费字段时，应先扩展 schema 与 example，再实现消费端。

validator 自身也读取同一份 schema JSON；不要再在 shell/Python 脚本里重复维护必需字段、`status` 枚举、`severity` 枚举或 Markdown marker。字段或枚举变化必须先落到 `full_product_health_artifact_schema.json`，再由 validator 和 Admin/CI 消费。

### Storage-Ready Artifact Set Publisher

Phase 51 新增 `publish_full_product_health_artifact_set.sh`，把已经生成好的 full report / summary / Markdown 显式路径转换成可存储的 artifact-set metadata/index JSON。它只读调用方明确传入的三个文件，不读取 `.env`，不扫描目录，不调用 service，不访问交易所，也不访问 Web lease/report/mutate endpoint。

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=/tmp/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=/tmp/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=/tmp/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
./scripts/dev/publish_full_product_health_artifact_set.sh
```

输出是 storage-ready metadata/index JSON，固定包含：

- `artifactSetId`
- `schemaVersion`
- `storedAt`
- `sourceGeneratedAt`
- `summaryHash`
- `validationHash`
- `fullArtifactHash`
- `markdownHash`
- `storageStatus`
- `retentionClass`
- `artifactSlaSeconds`
- `stale`
- `staleReason`
- `validation`
- `redaction`
- `operatorMetadata`

publisher 会在内存里做最小 validation/redaction：

- 缺文件、非 JSON、时间戳无效、`sourceGeneratedAt > storedAt` 会把 `storageStatus` / `retentionClass` 降级成 `rejected`。
- 对 artifact 内容扫描 database URL、API key/secret、passphrase、raw request/response、signed endpoint、Web mutation endpoint、`LINKUSDT` 等 marker。
- validation findings 只返回安全 locator 信息，不回显 source text、raw payload、database URL、API key/secret 或本地文件系统 secret。

默认 `artifactSlaSeconds=900`。`stale` 按 `storedAt`、`sourceGeneratedAt` 与发布时刻的差值计算；缺 source 时间或超出 SLA 时必须带 `staleReason`。如果需要稳定测试时间，可以显式传 `FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW`。

### Aggregator Alert Codes

P0 应阻止发布或阻止实盘：

- `WEB_EXECUTION_TASK_STALE`: `execution_tasks` 处于 `leased` / `processing` 超过阈值，且不是已确认历史 smoke 噪声。
- `WEB_ORDER_RESULT_MISSING`: task 已 completed 或 report 成功后，超过阈值仍缺 `exchange_order_results` / `user_trade_records`。
- `QUANT_EXPECTED_WORKER_STALE`: expected worker heartbeat stale，等价于本地 health 的 `EXPECTED_WORKER_STALE` fail。
- `ADMIN_LIVE_READINESS_BLOCKED`: 任一 live readiness 必需条件缺失，或当前环境只能 manual_review。

P1 需要值班或发布负责人复核：

- `NEWS_SOURCE_DEGRADED`: source 处于 degraded / retryable / paused，或 consecutive failure 超过观察阈值。
- `NEWS_AI_PROVIDER_UNAVAILABLE`: AI provider 缺 key、401、配额错误、模型不支持或 prompt 不存在；不得输出 key 或原始响应。
- `WEB_RETRY_BACKLOG`: `execution_task_attempts` 重试数或 failed task 数超过阈值。
- `QUANT_EXCHANGE_AUDIT_FAILURES`: opt-in audit 窗口内 exchange request 非 completed 数量大于 0。
- `ADMIN_HIGH_RISK_OPERATION_FAILED`: 观察窗口内高危 Admin 操作审计记录 outcome/status 为 failed / error。
- `ADMIN_ACTION_AUDIT_MISSING`: 必需高危 Admin action 在观察窗口内缺少 `admin_operation_logs` 审计记录。
- `ADMIN_READINESS_REVIEW_REQUIRED`: 发布/实盘条件不是 blocked，但仍需要人工确认。

INFO 只展示上下文，不单独阻断：

- `IGNORED_HISTORICAL_WORKER`: 历史 smoke worker 或一次性 worker stale。
- `EXECUTION_AUDIT_TABLE_MISSING`: opt-in audit 表未部署，health 降级展示。
- `NO_RECENT_NEWS_SIGNAL`: 观察窗口内无 actionable news signal。
- `MOCK_DEV_BOUNDARY_ACTIVE`: 当前处于 fixture / dry-run / mock seed 路径。

### Correlation IDs

端到端串链路时，aggregator 不做跨库写入，只在输出中按已有字段建立 `correlation` 对象：

- `news_id`: `rust_quant_news` split table 与 `news_ai_analysis_results` 的新闻主键。
- `analysis_result_id`: `news_ai_analysis_results.id`，同时出现在 news signal payload 中。
- `signal_inbox_id`: `quant_web.news_signal_inbox.id`。
- `external_id`: `source:news_id:symbol` 或事件类稳定外部 ID，用于跨服务兜底关联。
- `execution_task_id`: `quant_web.execution_tasks.id`。
- `execution_attempt_id`: `quant_web.execution_task_attempts.id`。
- `order_result_id`: `quant_web.exchange_order_results.id`。
- `trade_record_id`: `quant_web.user_trade_records.id`。
- `request_id`: `quant_core.exchange_request_audit_logs.request_id`，建议沿用 `task-{execution_task_id}-{client_order_id}` 或 `task-{execution_task_id}`。
- `worker_id`: `quant_core.execution_worker_checkpoints.worker_id`。

推荐串联顺序：

```text
news_id
  -> analysis_result_id
  -> external_id / signal_inbox_id
  -> execution_task_id
  -> execution_attempt_id
  -> request_id
  -> order_result_id
  -> trade_record_id
```

当某一段缺失时，aggregator 应输出最近已知 ID 和缺失段 alert，而不是补写数据或调用 mutation endpoint。

## Read-Only Checks

以下检查是只读：

- repo root 与脚本语法检查。
- 本地 HTTP GET `/` 可达性检查。
- Postgres `SELECT 1`。
- `execution_worker_checkpoints` 的 `SELECT` 查询。
- opt-in 时 `exchange_request_audit_logs` 与 `execution_worker_checkpoints` 的聚合 `SELECT` 查询。

这些检查不会 lease execution task，不会上报 execution result，不会改变 worker checkpoint，也不会发交易请求。

## Explicit Opt-In Checks

`HEALTH_CHECK_BINANCE=false` 是默认值。Binance connectivity 只有在显式 opt-in 时才会运行：

```bash
HEALTH_CHECK_BINANCE=true ./scripts/dev/check_local_service_health.sh
```

该检查必须只用于公开连通性探测。不调用 Binance signed/account/order/position endpoint，不下单，不修改账户状态，不触碰 LINKUSDT。
