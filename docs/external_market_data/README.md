# External Market Data

这套目录用于给 Vegas 策略补充交易所外部特征和链上上下文，当前优先支持两类数据源：

- `Hyperliquid` 公共接口：历史资金费率、当前 funding/premium/open interest/mark/oracle
- `Dune` 查询模板：Ethereum 交易所净流入、Hyperliquid basis、ETH 大额转账

## 当前落地范围

第一阶段只要求：

- 能抓取 Hyperliquid `fundingHistory`
- 能抓取 Hyperliquid `metaAndAssetCtxs`
- 能将结果落表到 `external_market_snapshots`
- 能保留 Dune 查询模板，等待 API key 或人工执行

当前 **不直接接入交易信号**。策略侧后续只消费已经沉淀到库里的特征。

## Dune 执行链

当前仓库已经补上最小执行链：

- Infrastructure:
  - `DuneApiClient`
  - `POST /v1/sql/execute`
  - `GET /v1/execution/{execution_id}/status`
  - `GET /v1/execution/{execution_id}/results`
- Services:
  - `DuneMarketSyncService`
  - 模板变量替换
  - 查询结果转 `ExternalMarketSnapshot`

### 环境变量

- `DUNE_API_KEY`
- `DUNE_API_BASE_URL` 可选，默认 `https://api.dune.com/api/v1`
- `DUNE_SQL_POLL_INTERVAL_MS` 可选，默认 `3000`
- `DUNE_SQL_MAX_POLLS` 可选，默认 `40`

### 当前使用方式

- 优先直接执行本地 SQL 模板，不依赖先在 Dune 后台创建 saved query
- 这样可以直接复用仓库里的：
  - `docs/external_market_data/dune/ethereum_cex_flow.sql`
  - `docs/external_market_data/dune/hyperliquid_funding_basis.sql`
  - `docs/external_market_data/dune/eth_whale_transfer.sql`
- CLI 实跑入口：
  - `cargo run -p rust-quant-cli --example run_dune_external_sync`
- 主程序调度入口：
  - `cargo run --bin rust_quant`
  - 配合 `IS_RUN_SYNC_DATA_JOB=1` 与 `IS_RUN_DUNE_SYNC_JOB=1`

### 主程序调度入口环境变量

单任务模式：

- `IS_RUN_SYNC_DATA_JOB=1`
- `IS_RUN_FUNDING_RATE_JOB=1` 可选，执行交易所资金费率同步
- `IS_RUN_DUNE_SYNC_JOB=1`
- `SYNC_SKIP_MARKET_DATA=1` 可选，只跑外部数据同步，不跑 K 线同步
- `DUNE_METRIC_TYPE`
- `DUNE_SYMBOL`
- `DUNE_TEMPLATE_PATH`
- `DUNE_START_TIME`
- `DUNE_END_TIME`
- `DUNE_PERFORMANCE` 可选，默认 `medium`
- `DUNE_MIN_USD` 可选，默认 `100000`

批量任务模式：

- `IS_RUN_SYNC_DATA_JOB=1`
- `IS_RUN_DUNE_SYNC_JOB=1`
- `DUNE_TEMPLATE_JOBS`

`DUNE_TEMPLATE_JOBS` 格式：

- `metric_type|symbol|template_path|start_time|end_time|performance|[min_usd]`
- 多条任务用 `;` 分隔

示例：

```bash
export IS_RUN_SYNC_DATA_JOB=1
export IS_RUN_DUNE_SYNC_JOB=1
export DUNE_TEMPLATE_JOBS='hyperliquid_basis|ETH|docs/external_market_data/dune/hyperliquid_funding_basis.sql|2026-02-21T20:00:00Z|2026-02-22T00:00:00Z|medium|100000;eth_whale_transfer|ETH|docs/external_market_data/dune/eth_whale_transfer.sql|2026-02-21T20:00:00Z|2026-02-22T00:00:00Z|large|250000'
```

只想跑同步后退出，建议显式关闭其他模式：

```bash
export IS_BACK_TEST=0
export IS_OPEN_SOCKET=0
export IS_RUN_REAL_STRATEGY=0
export EXIT_AFTER_SYNC=1
```

如果连 K 线同步也不想跑，只保留 Dune：

```bash
export SYNC_SKIP_MARKET_DATA=1
export SYNC_SKIP_TICKERS=1
```

如果只想跑资金费率同步，不跑 ticker / K 线 / Dune：

```bash
export IS_RUN_SYNC_DATA_JOB=1
export IS_RUN_FUNDING_RATE_JOB=1
export IS_RUN_DUNE_SYNC_JOB=0
export SYNC_SKIP_MARKET_DATA=1
export SYNC_SKIP_TICKERS=1
export IS_BACK_TEST=0
export IS_OPEN_SOCKET=0
export IS_RUN_REAL_STRATEGY=0
export EXIT_AFTER_SYNC=1
```

### 已验证的 Dune 真实约束

- `hyperliquid_funding_basis.sql` 当前应基于 `hyperliquid.market_data`，不要引用不存在的 `dune_user_generated.*`
- Dune ad-hoc SQL 的 `GET /execution/{id}/results` 响应里可能没有 `query_id`
- Dune 返回的时间格式可能是 `2026-02-21 23:00:00.000 UTC`，服务层需要兼容解析
- 当前 `hyperliquid.market_data` 的 `ETH` 数据上限验证到 `2026-02-21 23:59:00 UTC`
  - 用 `2026-03-30` 这类更晚窗口执行会成功返回 0 行，不是代码错误，而是数据尚未覆盖

## OKX 资金费率同步

当前主程序已经支持在同步入口里直接跑资金费率任务：

- `IS_RUN_SYNC_DATA_JOB=1`
- `IS_RUN_FUNDING_RATE_JOB=1`
- 可配合 `SYNC_SKIP_TICKERS=1`
- 可配合 `SYNC_SKIP_MARKET_DATA=1`
- 支持 `SYNC_ONLY_INST_IDS='ETH-USDT-SWAP,BTC-USDT-SWAP,...'`

2026-04-09 实测：

- `ETH / BTC / SOL / BCH` 都成功写入 `funding_rates`
- 每个交易对当前落库 `273` 条
- 东八区时间范围：`2026-01-08 16:00:00` 到 `2026-04-09 08:00:00`

这说明当前通过 OKX `funding-rate-history` 实际可获得的数据窗口约为近 `91` 天，不是完整一年。对“最近一年资金费率”需求，后续需要改成多源补齐，例如：

1. Hyperliquid `fundingHistory`
2. Binance / OKX 其他历史源
3. Dune 社区表

### 当前入库行为

- `source = dune`
- `metric_type` 由调用方指定，例如 `hyperliquid_basis`
- `metric_time` 优先取 `hour_bucket / block_time / time`
- 已支持自动提取：
  - `funding_rate`
  - `premium` 或 `premium_bps`
  - `open_interest` 或 `open_interest_usd`
  - `long_short_ratio`
- 其余字段保存在 `raw_payload`

## Hyperliquid 对应字段

实现方式：

- 优先使用官方 Rust SDK：`hyperliquid_rust_sdk`
- 当前仓库接的是官方 GitHub 仓库，而不是手写 `reqwest` 直连 REST
- 原因：crates.io `0.6.0` 发布版缺少 `metaAndAssetCtxs`，官方仓库主干已经提供对应类型与 `InfoClient` 方法

底层数据源仍然是：`https://api.hyperliquid.xyz/info`

### fundingHistory

输入：

- `coin`
- `startTime`
- `endTime`

输出映射：

- `coin -> symbol`
- `fundingRate -> funding_rate`
- `premium -> premium`
- `time -> metric_time`
- `metric_type = funding`

### metaAndAssetCtxs

输入：

- `coin`

输出映射：

- `funding -> funding_rate`
- `premium -> premium`
- `openInterest -> open_interest`
- `oraclePx -> oracle_price`
- `markPx -> mark_price`
- `metric_type = meta`

## Dune 模板

### 1. `dune/ethereum_cex_flow.sql`

用途：

- 统计 ETH 在中心化交易所地址集合上的净流入/净流出
- 适合做 “价格反转前是否有大额回流交易所” 的上下文过滤

建议特征：

- `cex_inflow_usd_4h`
- `cex_outflow_usd_4h`
- `netflow_usd_4h`

### 2. `dune/hyperliquid_funding_basis.sql`

用途：

- 将 Hyperliquid perp funding 与价格 basis 统一到一个结果集
- 适合做 “funding + premium 背离” 特征
- 当前实现按小时聚合 `hyperliquid.market_data`

建议特征：

- `funding_rate`
- `premium`
- `basis_bps`
- `open_interest`

### 3. `dune/eth_whale_transfer.sql`

用途：

- 统计 ETH 大额转账和与交易所/桥/已知标签地址交互情况
- 适合识别大户搬砖、链上风险偏好切换

建议特征：

- `whale_transfer_count_4h`
- `whale_transfer_usd_4h`
- `exchange_tagged_transfer_usd_4h`

## 参数约定

SQL 模板统一使用 Dune 命名参数，不硬编码 query id：

- `{{start_time}}`
- `{{end_time}}`
- `{{symbol}}`
- `{{min_usd}}`

## 接入建议顺序

1. 先跑 Hyperliquid 公共接口，验证入库链路
2. 再把 Dune 模板做成查询任务，结果同样落到 `external_market_snapshots`
3. 最后补 OKX/Binance 多空比、持仓量、资金费率历史

## 当前限制

- 这台机器对 Hyperliquid 公共接口连通正常
- OKX/Binance 公共 REST 目前网络不稳定，先保留扩展接口，不阻塞 Hyperliquid/Dune 先落地

## Vegas 因子研究入口

当前仓库已经提供一个最小可用的研究入口，用于把正式基线回测样本和 `external_market_snapshots` 对齐，输出文本研究报告。

运行方式：

```bash
QUANT_CORE_DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/quant_core' \
STRATEGY_CONFIG_SOURCE=quant_core \
CANDLE_SOURCE=quant_core \
cargo run -p rust-quant-cli --example run_vegas_factor_research
```

默认读取正式基线：

- `1428`
- `1429`
- `1430`
- `1431`

可选环境变量：

- `VEGAS_RESEARCH_BASELINE_IDS=1428,1429,1430,1431`
- `VEGAS_RESEARCH_TIMEFRAME=4H`
- `VEGAS_RESEARCH_OUTPUT_PATH=/absolute/path/report.md`

第一版报告会输出：

- 因子概览表
- 分桶统计表
- `BTC / ETH / 其他币种` 三层分组结果
- 结论标签：`可回注 / 仅观察 / 拒绝`
- 同时区分：
  - `已成交样本`
  - `过滤候选`

注意：

- 当前研究系统已同时读取：
  - `back_test_detail` 中的已成交开仓样本
  - `filtered_signal_log` 中的过滤候选样本
- 当前有历史覆盖并能产出有效统计的外部因子家族主要是 `funding_premium_divergence`。
- `price_oi_state`、`flow_proxy` 若历史快照覆盖不足，会显式显示为 `no_data`。
