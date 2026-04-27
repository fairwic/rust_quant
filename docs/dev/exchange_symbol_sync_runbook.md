# Exchange Symbol Sync Runbook

## Purpose

`scripts/dev/run_all_exchange_symbol_sync.sh` 提供多交易所交易对事实表的一键同步入口，按既定顺序串行调用现有的单交易所脚本：

1. `binance`
2. `okx`
3. `bitget`
4. `gate`
5. `kucoin`

这样可以复用已有 `sync_exchange_symbols` 业务逻辑，同时给临时 shell 运维一个稳定入口。服务化入口已经在 `rust_quant` internal server 和每分钟 worker 中提供，Admin 应优先走 internal API。

## Manual Run

默认全量顺序执行：

```bash
bash scripts/dev/run_all_exchange_symbol_sync.sh
```

指定数据库或是否提交 listing signal 时，沿用单交易所脚本已有环境变量：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
EXCHANGE_LISTING_SIGNAL_SUBMIT=0 \
bash scripts/dev/run_all_exchange_symbol_sync.sh
```

只重跑部分交易所时，用 `EXCHANGE_SYMBOL_SOURCES` 覆盖，按填写顺序执行：

```bash
EXCHANGE_SYMBOL_SOURCES="okx gate" bash scripts/dev/run_all_exchange_symbol_sync.sh
```

也支持逗号分隔：

```bash
EXCHANGE_SYMBOL_SOURCES="binance,bitget,kucoin" bash scripts/dev/run_all_exchange_symbol_sync.sh
```

## Failure Semantics

- 任一交易所同步失败，脚本立即停止，不继续后续交易所。
- 日志会打印当前批次序号、交易所名和失败退出码，便于 cron 或 Admin 直接定位失败点。
- 重跑时建议只带失败交易所及其后续需要补偿的交易所，避免无意义重复。

## Cron / Admin Entry

- 本机临时 cron 可直接调 `bash scripts/dev/run_all_exchange_symbol_sync.sh`，把 stdout/stderr 收集到任务日志。
- Admin 手动入口应调用 `rust_quant` internal API：

```http
POST /internal/exchange-symbols/sync
Content-Type: application/json

{"sources":["binance","okx","bitget","gate","kucoin"],"triggerSource":"manual"}
```

- 每次运行都会写 `quant_core.exchange_symbol_sync_runs`，字段包括 `run_id`、`requested_sources`、`run_status`、`persisted_rows`、`first_seen_rows`、`major_listing_signals`、`error_message` 和 `report_json`。
- 如果 Admin 需要单交易所重跑，传 `{"sources":["okx"],"triggerSource":"manual"}` 即可。

## Scheduled Worker

长期运行的自动同步由 `rust_quant` 自己负责，不放在 Admin 浏览器定时器里：

```bash
bash scripts/dev/run_exchange_symbol_sync_worker.sh
```

默认行为：

- `EXCHANGE_SYMBOL_SYNC_INTERVAL_SECS=60`，每分钟同步一次。
- `EXCHANGE_SYMBOL_SOURCES="binance okx bitget gate kucoin"`，默认五个交易所。
- `EXCHANGE_LISTING_SIGNAL_SUBMIT=0`，默认只同步事实表和记录同步运行状态，不自动提交上市交易信号。

只跑一轮用于 smoke：

```bash
EXCHANGE_SYMBOL_SYNC_RUN_ONCE=true \
EXCHANGE_SYMBOL_SOURCES="binance" \
bash scripts/dev/run_exchange_symbol_sync_worker.sh
```
