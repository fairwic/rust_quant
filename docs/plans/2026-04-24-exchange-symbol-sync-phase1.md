# Exchange Symbol Sync Phase 1

## Goal

为 `quant_core` 建立交易所原始可交易交易对事实层，并在 `rust_quant` 内提供第一阶段 Binance USD-M 永续合约同步入口。

## Boundary

- `quant_core.exchange_symbols`：保存交易所最新原始可交易 symbol 事实，不混入订阅/售卖规则。
- `rust_quant`：负责抓取、标准化、落库和后续增量扩展其他交易所。
- `quant_web.strategy_supported_symbols`：继续作为产品/策略可售白名单，不作为交易所全量事实源。

## Phase 1 Scope

- 数据源：Binance USD-M `exchangeInfo`
- 范围：`contractType=PERPETUAL` 的交易对
- 标准化：`BTCUSDT -> BTC-USDT-SWAP`
- 落库字段：exchange / market_type / exchange_symbol / normalized_symbol / base_asset / quote_asset / status / filters / precision / raw_payload / sync timestamps
- 入口：
  - `crates/rust-quant-cli/src/bin/sync_exchange_symbols.rs`
  - `scripts/dev/run_exchange_symbol_sync.sh`

## Follow-up

- 增加 OKX / Bitget / Bybit 的 fetcher，实现同一服务接口下的多交易所同步。
- 在 `rust_quan_web` 或 admin 侧做“事实层 symbol -> 策略白名单”的显式映射，而不是直接复用事实表。
- 增加 freshness 监控、下线 symbol 软失效策略，以及按交易所/市场类型的定时任务。
