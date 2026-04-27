-- Target PostgreSQL ownership model for rust_quant.
--
-- Deploy this file into the standalone quant_core database's public schema.
-- The platform shares a Postgres server/container, but quant_core and
-- quant_news are independent databases, not separate schemas in one database.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS strategy_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    legacy_id BIGINT,
    strategy_key VARCHAR(128) NOT NULL,
    strategy_name VARCHAR(255) NOT NULL DEFAULT '',
    version VARCHAR(64) NOT NULL DEFAULT 'default',
    exchange VARCHAR(64) NOT NULL DEFAULT 'all',
    symbol VARCHAR(64) NOT NULL DEFAULT 'all',
    timeframe VARCHAR(32) NOT NULL DEFAULT '',
    enabled BOOLEAN NOT NULL DEFAULT true,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    risk_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (strategy_key, version, exchange, symbol, timeframe)
);

ALTER TABLE strategy_configs
    ADD COLUMN IF NOT EXISTS legacy_id BIGINT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_strategy_configs_legacy_id
    ON strategy_configs(legacy_id)
    WHERE legacy_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS risk_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_key VARCHAR(128) NOT NULL UNIQUE,
    config_name VARCHAR(255) NOT NULL DEFAULT '',
    enabled BOOLEAN NOT NULL DEFAULT true,
    risk_config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS market_candles (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    timeframe VARCHAR(32) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    close_time TIMESTAMPTZ,
    open_price NUMERIC(32, 12) NOT NULL,
    high_price NUMERIC(32, 12) NOT NULL,
    low_price NUMERIC(32, 12) NOT NULL,
    close_price NUMERIC(32, 12) NOT NULL,
    volume NUMERIC(32, 12),
    quote_volume NUMERIC(32, 12),
    source VARCHAR(64) NOT NULL DEFAULT 'crypto_exc_all',
    raw JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (exchange, symbol, timeframe, open_time)
);

CREATE TABLE IF NOT EXISTS market_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    snapshot_type VARCHAR(64) NOT NULL DEFAULT 'ticker',
    snapshot_status VARCHAR(32) NOT NULL DEFAULT 'active',
    last_price NUMERIC(32, 12),
    bid_price NUMERIC(32, 12),
    ask_price NUMERIC(32, 12),
    volume_24h NUMERIC(32, 12),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS indicator_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_key VARCHAR(128) NOT NULL,
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    timeframe VARCHAR(32) NOT NULL,
    indicator_key VARCHAR(128) NOT NULL,
    indicator_value JSONB NOT NULL DEFAULT '{}'::jsonb,
    candle_open_time TIMESTAMPTZ,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS strategy_signals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signal_key VARCHAR(128) NOT NULL UNIQUE,
    strategy_key VARCHAR(128) NOT NULL,
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    timeframe VARCHAR(32) NOT NULL DEFAULT '',
    side VARCHAR(32) NOT NULL,
    signal_status VARCHAR(32) NOT NULL DEFAULT 'generated',
    strength NUMERIC(18, 8),
    confidence NUMERIC(18, 8),
    source VARCHAR(64) NOT NULL DEFAULT 'strategy',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS strategy_run_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_key VARCHAR(128) NOT NULL,
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    timeframe VARCHAR(32) NOT NULL DEFAULT '',
    run_status VARCHAR(32) NOT NULL DEFAULT 'idle',
    last_signal_id UUID,
    state JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_run_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (strategy_key, exchange, symbol, timeframe)
);

CREATE TABLE IF NOT EXISTS backtest_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_name VARCHAR(255) NOT NULL DEFAULT '',
    strategy_key VARCHAR(128) NOT NULL,
    exchange VARCHAR(64) NOT NULL DEFAULT '',
    symbol VARCHAR(64) NOT NULL DEFAULT '',
    timeframe VARCHAR(32) NOT NULL DEFAULT '',
    run_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS backtest_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES backtest_runs(id) ON DELETE CASCADE,
    net_profit NUMERIC(32, 12),
    max_drawdown NUMERIC(18, 8),
    win_rate NUMERIC(18, 8),
    trade_count INTEGER NOT NULL DEFAULT 0,
    metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS backtest_trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES backtest_runs(id) ON DELETE CASCADE,
    exchange VARCHAR(64) NOT NULL DEFAULT '',
    symbol VARCHAR(64) NOT NULL DEFAULT '',
    side VARCHAR(32) NOT NULL,
    entry_time TIMESTAMPTZ,
    exit_time TIMESTAMPTZ,
    entry_price NUMERIC(32, 12),
    exit_price NUMERIC(32, 12),
    quantity NUMERIC(32, 12),
    profit NUMERIC(32, 12),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS execution_worker_checkpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    worker_id VARCHAR(128) NOT NULL UNIQUE,
    worker_kind VARCHAR(64) NOT NULL DEFAULT 'execution',
    worker_status VARCHAR(32) NOT NULL DEFAULT 'idle',
    lease_owner VARCHAR(128) NOT NULL DEFAULT '',
    checkpoint_key VARCHAR(255) NOT NULL DEFAULT '',
    checkpoint_value JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_task_id VARCHAR(128),
    last_heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS exchange_request_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id VARCHAR(128) NOT NULL,
    exchange VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL DEFAULT '',
    endpoint VARCHAR(255) NOT NULL DEFAULT '',
    request_status VARCHAR(32) NOT NULL,
    latency_ms INTEGER,
    request_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    response_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_message TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS exchange_symbols (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(64) NOT NULL,
    market_type VARCHAR(64) NOT NULL,
    exchange_symbol VARCHAR(128) NOT NULL,
    normalized_symbol VARCHAR(128) NOT NULL,
    base_asset VARCHAR(64) NOT NULL,
    quote_asset VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL,
    contract_type VARCHAR(64),
    price_precision INTEGER,
    quantity_precision INTEGER,
    min_qty VARCHAR(64),
    max_qty VARCHAR(64),
    tick_size VARCHAR(64),
    step_size VARCHAR(64),
    min_notional VARCHAR(64),
    raw_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_exchange_symbols_exchange_market_symbol
        UNIQUE (exchange, market_type, exchange_symbol)
);

CREATE INDEX IF NOT EXISTS idx_market_candles_lookup
    ON market_candles (exchange, symbol, timeframe, open_time DESC);
CREATE INDEX IF NOT EXISTS idx_market_snapshots_lookup
    ON market_snapshots (exchange, symbol, captured_at DESC);
CREATE INDEX IF NOT EXISTS idx_indicator_snapshots_lookup
    ON indicator_snapshots (strategy_key, exchange, symbol, timeframe, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_strategy_signals_lookup
    ON strategy_signals (strategy_key, exchange, symbol, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_strategy_signals_status
    ON strategy_signals (signal_status, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_backtest_runs_lookup
    ON backtest_runs (strategy_key, exchange, symbol, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_backtest_results_run_id
    ON backtest_results (run_id);
CREATE INDEX IF NOT EXISTS idx_backtest_trades_run_id
    ON backtest_trades (run_id);
CREATE INDEX IF NOT EXISTS idx_exchange_request_audit_lookup
    ON exchange_request_audit_logs (exchange, symbol, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_exchange_status
    ON exchange_symbols (exchange, status);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_base_quote
    ON exchange_symbols (base_asset, quote_asset);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_updated_at
    ON exchange_symbols (updated_at DESC);

COMMENT ON TABLE exchange_symbols IS '交易所原始可交易交易对事实表，由 rust_quant 同步维护';
COMMENT ON COLUMN exchange_symbols.id IS '自增主键';
COMMENT ON COLUMN exchange_symbols.exchange IS '交易所标识，如 binance、okx';
COMMENT ON COLUMN exchange_symbols.market_type IS '市场类型，如 perpetual、spot';
COMMENT ON COLUMN exchange_symbols.exchange_symbol IS '交易所原始交易对标识，如 BTCUSDT';
COMMENT ON COLUMN exchange_symbols.normalized_symbol IS '系统内部统一交易对标识，如 BTC-USDT-SWAP';
COMMENT ON COLUMN exchange_symbols.base_asset IS '基础币种';
COMMENT ON COLUMN exchange_symbols.quote_asset IS '计价币种';
COMMENT ON COLUMN exchange_symbols.status IS '交易所返回的交易状态，如 TRADING';
COMMENT ON COLUMN exchange_symbols.contract_type IS '合约类型，如 PERPETUAL';
COMMENT ON COLUMN exchange_symbols.price_precision IS '价格精度';
COMMENT ON COLUMN exchange_symbols.quantity_precision IS '数量精度';
COMMENT ON COLUMN exchange_symbols.min_qty IS '最小下单数量';
COMMENT ON COLUMN exchange_symbols.max_qty IS '最大下单数量';
COMMENT ON COLUMN exchange_symbols.tick_size IS '价格步长';
COMMENT ON COLUMN exchange_symbols.step_size IS '数量步长';
COMMENT ON COLUMN exchange_symbols.min_notional IS '最小名义价值限制';
COMMENT ON COLUMN exchange_symbols.raw_payload IS '交易所原始 symbol 元数据';
COMMENT ON COLUMN exchange_symbols.last_synced_at IS '最近一次成功同步时间';
COMMENT ON COLUMN exchange_symbols.created_at IS '创建时间';
COMMENT ON COLUMN exchange_symbols.updated_at IS '更新时间';

CREATE TABLE IF NOT EXISTS exchange_symbol_listing_events (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(64) NOT NULL,
    market_type VARCHAR(64) NOT NULL,
    exchange_symbol VARCHAR(128) NOT NULL,
    normalized_symbol VARCHAR(128) NOT NULL,
    base_asset VARCHAR(64) NOT NULL,
    quote_asset VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source VARCHAR(64) NOT NULL DEFAULT 'exchange_symbol_sync',
    raw_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uk_exchange_symbol_listing_events_exchange_market_symbol
        UNIQUE (exchange, market_type, exchange_symbol)
);

CREATE INDEX IF NOT EXISTS idx_exchange_symbol_listing_events_asset
    ON exchange_symbol_listing_events (base_asset, quote_asset, market_type, first_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_symbol_listing_events_exchange
    ON exchange_symbol_listing_events (exchange, market_type, first_seen_at DESC);

COMMENT ON TABLE exchange_symbol_listing_events IS '交易对在交易所首次被系统发现的历史事实表，用于识别主流交易所首次上线事件';
COMMENT ON COLUMN exchange_symbol_listing_events.id IS '自增主键';
COMMENT ON COLUMN exchange_symbol_listing_events.exchange IS '交易所标识，如 binance、okx、gate';
COMMENT ON COLUMN exchange_symbol_listing_events.market_type IS '市场类型，如 perpetual、spot';
COMMENT ON COLUMN exchange_symbol_listing_events.exchange_symbol IS '交易所原始交易对标识';
COMMENT ON COLUMN exchange_symbol_listing_events.normalized_symbol IS '系统内部统一交易对标识';
COMMENT ON COLUMN exchange_symbol_listing_events.base_asset IS '基础币种';
COMMENT ON COLUMN exchange_symbol_listing_events.quote_asset IS '计价币种';
COMMENT ON COLUMN exchange_symbol_listing_events.status IS '首次发现时的交易状态';
COMMENT ON COLUMN exchange_symbol_listing_events.first_seen_at IS '系统首次发现该交易所交易对的时间';
COMMENT ON COLUMN exchange_symbol_listing_events.source IS '首次发现来源，如 exchange_symbol_sync';
COMMENT ON COLUMN exchange_symbol_listing_events.raw_payload IS '首次发现时的交易所原始 symbol 元数据';
COMMENT ON COLUMN exchange_symbol_listing_events.created_at IS '创建时间';
COMMENT ON COLUMN exchange_symbol_listing_events.updated_at IS '更新时间';

CREATE TABLE IF NOT EXISTS exchange_symbol_sync_runs (
    id BIGSERIAL PRIMARY KEY,
    run_id VARCHAR(96) NOT NULL UNIQUE,
    trigger_source VARCHAR(32) NOT NULL,
    requested_sources TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    run_status VARCHAR(32) NOT NULL DEFAULT 'running',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    duration_ms INTEGER,
    persisted_rows INTEGER NOT NULL DEFAULT 0,
    first_seen_rows INTEGER NOT NULL DEFAULT 0,
    major_listing_signals INTEGER NOT NULL DEFAULT 0,
    error_message TEXT NOT NULL DEFAULT '',
    report_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_exchange_symbol_sync_runs_status_started
    ON exchange_symbol_sync_runs (run_status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_symbol_sync_runs_trigger_source
    ON exchange_symbol_sync_runs (trigger_source, started_at DESC);

COMMENT ON TABLE exchange_symbol_sync_runs IS '多交易所交易对事实表同步任务运行记录';
COMMENT ON COLUMN exchange_symbol_sync_runs.id IS '自增主键';
COMMENT ON COLUMN exchange_symbol_sync_runs.run_id IS '同步任务运行ID';
COMMENT ON COLUMN exchange_symbol_sync_runs.trigger_source IS '触发来源，如 cli、manual、scheduled、internal';
COMMENT ON COLUMN exchange_symbol_sync_runs.requested_sources IS '本次请求同步的交易所来源列表';
COMMENT ON COLUMN exchange_symbol_sync_runs.run_status IS '运行状态：running、success、failed';
COMMENT ON COLUMN exchange_symbol_sync_runs.started_at IS '同步开始时间';
COMMENT ON COLUMN exchange_symbol_sync_runs.finished_at IS '同步结束时间';
COMMENT ON COLUMN exchange_symbol_sync_runs.duration_ms IS '同步耗时毫秒';
COMMENT ON COLUMN exchange_symbol_sync_runs.persisted_rows IS '本次解析并写入的交易对数量';
COMMENT ON COLUMN exchange_symbol_sync_runs.first_seen_rows IS '本次首次发现的交易对数量';
COMMENT ON COLUMN exchange_symbol_sync_runs.major_listing_signals IS '本次识别出的主流交易所上线利好信号数量';
COMMENT ON COLUMN exchange_symbol_sync_runs.error_message IS '失败错误信息';
COMMENT ON COLUMN exchange_symbol_sync_runs.report_json IS '按交易所拆分的同步报告JSON';
COMMENT ON COLUMN exchange_symbol_sync_runs.created_at IS '创建时间';
COMMENT ON COLUMN exchange_symbol_sync_runs.updated_at IS '更新时间';

-- Legacy MySQL compatibility tables.
--
-- These tables keep the original MySQL table names and core column shapes so
-- the remaining legacy backtest/audit repositories can be moved to Postgres
-- without a table rename. Existing quant_core tables above remain in place.

CREATE TABLE IF NOT EXISTS strategy_config (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    strategy_type VARCHAR(50) NOT NULL,
    inst_id VARCHAR(50) NOT NULL,
    value TEXT,
    risk_config VARCHAR(2000) NOT NULL,
    tags VARCHAR(255),
    time VARCHAR(50) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP,
    kline_start_time BIGINT,
    kline_end_time BIGINT,
    final_fund DOUBLE PRECISION NOT NULL,
    is_deleted SMALLINT NOT NULL
);

ALTER TABLE strategy_config ADD COLUMN IF NOT EXISTS tags VARCHAR(255);
CREATE INDEX IF NOT EXISTS idx_strategy_config_inst_period
    ON strategy_config (inst_id, time);

CREATE TABLE IF NOT EXISTS back_test_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    strategy_type VARCHAR(255) NOT NULL,
    inst_type VARCHAR(255) NOT NULL,
    time VARCHAR(255) NOT NULL,
    win_rate VARCHAR(255) NOT NULL,
    open_positions_num INTEGER NOT NULL,
    final_fund DOUBLE PRECISION NOT NULL,
    strategy_detail TEXT NOT NULL,
    risk_config_detail VARCHAR(1000) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    profit DOUBLE PRECISION,
    one_bar_after_win_rate DOUBLE PRECISION,
    two_bar_after_win_rate DOUBLE PRECISION,
    three_bar_after_win_rate DOUBLE PRECISION,
    four_bar_after_win_rate DOUBLE PRECISION,
    five_bar_after_win_rate DOUBLE PRECISION,
    ten_bar_after_win_rate DOUBLE PRECISION,
    kline_start_time BIGINT NOT NULL,
    kline_end_time BIGINT NOT NULL,
    kline_nums INTEGER NOT NULL,
    sharpe_ratio DOUBLE PRECISION,
    annual_return DOUBLE PRECISION,
    total_return DOUBLE PRECISION,
    max_drawdown DOUBLE PRECISION,
    volatility DOUBLE PRECISION
);

CREATE INDEX IF NOT EXISTS idx_back_test_log_final_fund ON back_test_log (final_fund);
CREATE INDEX IF NOT EXISTS idx_back_test_log_inst ON back_test_log (inst_type);
CREATE INDEX IF NOT EXISTS idx_back_test_log_time_fund ON back_test_log (time, final_fund);

CREATE TABLE IF NOT EXISTS back_test_detail (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    back_test_id BIGINT NOT NULL,
    inst_id VARCHAR(20) NOT NULL,
    time VARCHAR(255) NOT NULL,
    strategy_type VARCHAR(255) NOT NULL,
    option_type VARCHAR(255) NOT NULL,
    signal_open_position_time TIMESTAMP,
    open_position_time TIMESTAMP NOT NULL,
    close_position_time TIMESTAMP NOT NULL,
    open_price VARCHAR(255) NOT NULL,
    close_price VARCHAR(255),
    fee VARCHAR(255) NOT NULL DEFAULT '',
    profit_loss VARCHAR(255) NOT NULL,
    quantity VARCHAR(255) NOT NULL,
    full_close VARCHAR(10) NOT NULL,
    close_type VARCHAR(255) NOT NULL,
    signal_status INTEGER NOT NULL,
    signal_value VARCHAR(5000) NOT NULL,
    signal_result VARCHAR(4000),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    win_nums INTEGER NOT NULL,
    loss_nums INTEGER,
    stop_loss_source VARCHAR(255),
    stop_loss_update_history TEXT
);

CREATE INDEX IF NOT EXISTS idx_back_test_detail_back_test_id
    ON back_test_detail (back_test_id);

CREATE TABLE IF NOT EXISTS back_test_analysis (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    back_test_id BIGINT NOT NULL,
    inst_id VARCHAR(255) NOT NULL,
    time VARCHAR(255) NOT NULL,
    option_type VARCHAR(255) NOT NULL,
    open_position_time VARCHAR(255),
    open_price VARCHAR(255) NOT NULL,
    bars_after INTEGER NOT NULL,
    price_after VARCHAR(255) NOT NULL,
    price_change_percent VARCHAR(255) NOT NULL,
    is_profitable SMALLINT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_back_test_analysis_back_test_id
    ON back_test_analysis (back_test_id);

CREATE TABLE IF NOT EXISTS filtered_signal_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    backtest_id BIGINT NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    period VARCHAR(10) NOT NULL,
    signal_time TIMESTAMP NOT NULL,
    direction VARCHAR(10) NOT NULL,
    filter_reasons JSONB NOT NULL,
    signal_price NUMERIC(20, 8) NOT NULL,
    indicator_snapshot JSONB,
    theoretical_profit NUMERIC(20, 8),
    theoretical_loss NUMERIC(20, 8),
    final_pnl NUMERIC(20, 8),
    trade_result VARCHAR(10),
    signal_value JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_filtered_signal_log_backtest
    ON filtered_signal_log (backtest_id);
CREATE INDEX IF NOT EXISTS idx_filtered_signal_log_inst_period
    ON filtered_signal_log (inst_id, period);

CREATE TABLE IF NOT EXISTS dynamic_config_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    backtest_id BIGINT NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    period VARCHAR(10) NOT NULL,
    kline_time TIMESTAMP NOT NULL,
    adjustments JSONB NOT NULL,
    config_snapshot JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_backtest
    ON dynamic_config_log (backtest_id);
CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_inst_period
    ON dynamic_config_log (inst_id, period);
CREATE INDEX IF NOT EXISTS idx_dynamic_config_log_kline_time
    ON dynamic_config_log (kline_time);

CREATE TABLE IF NOT EXISTS strategy_run (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    period VARCHAR(16) NOT NULL,
    start_at TIMESTAMP,
    end_at TIMESTAMP,
    status VARCHAR(16) NOT NULL DEFAULT 'RUNNING',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (run_id)
);

CREATE INDEX IF NOT EXISTS idx_strategy_run_strategy_inst
    ON strategy_run (strategy_id, inst_id);

CREATE TABLE IF NOT EXISTS signal_snapshot_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    filtered SMALLINT NOT NULL DEFAULT 0,
    filter_reasons JSONB,
    signal_json JSONB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_signal_snapshot_log_run_ts
    ON signal_snapshot_log (run_id, kline_ts);

CREATE TABLE IF NOT EXISTS risk_decision_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    decision VARCHAR(16) NOT NULL,
    reason VARCHAR(255),
    risk_json JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_risk_decision_log_run_ts
    ON risk_decision_log (run_id, kline_ts);

CREATE TABLE IF NOT EXISTS order_decision_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    kline_ts BIGINT NOT NULL,
    side VARCHAR(16) NOT NULL,
    size NUMERIC(30, 10) NOT NULL,
    price NUMERIC(30, 10) NOT NULL,
    decision_json JSONB,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_order_decision_log_run_ts
    ON order_decision_log (run_id, kline_ts);

CREATE TABLE IF NOT EXISTS orders (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    side VARCHAR(16) NOT NULL,
    qty NUMERIC(30, 10) NOT NULL,
    price NUMERIC(30, 10) NOT NULL,
    status VARCHAR(16) NOT NULL,
    client_order_id VARCHAR(64),
    exchange_order_id VARCHAR(64),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_orders_run ON orders (run_id);
CREATE INDEX IF NOT EXISTS idx_orders_inst ON orders (inst_id);

CREATE TABLE IF NOT EXISTS order_state_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    order_id BIGINT NOT NULL,
    from_state VARCHAR(16) NOT NULL,
    to_state VARCHAR(16) NOT NULL,
    reason VARCHAR(255),
    ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_order_state_log_order ON order_state_log (order_id);

CREATE TABLE IF NOT EXISTS positions (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    strategy_id VARCHAR(64) NOT NULL,
    inst_id VARCHAR(32) NOT NULL,
    side VARCHAR(16) NOT NULL,
    qty NUMERIC(30, 10) NOT NULL,
    avg_price NUMERIC(30, 10) NOT NULL,
    unrealized_pnl NUMERIC(30, 10) DEFAULT 0,
    realized_pnl NUMERIC(30, 10) DEFAULT 0,
    status VARCHAR(16) NOT NULL DEFAULT 'OPEN',
    updated_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_positions_run_inst ON positions (run_id, inst_id);

CREATE TABLE IF NOT EXISTS portfolio_snapshot_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    run_id VARCHAR(64) NOT NULL,
    total_equity NUMERIC(30, 10) NOT NULL,
    available NUMERIC(30, 10) NOT NULL,
    margin NUMERIC(30, 10) NOT NULL,
    pnl NUMERIC(30, 10) NOT NULL,
    ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_portfolio_snapshot_log_run_ts
    ON portfolio_snapshot_log (run_id, ts);

CREATE TABLE IF NOT EXISTS strategy_job_signal_log (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    inst_id VARCHAR(50) NOT NULL,
    time VARCHAR(10) NOT NULL,
    strategy_type VARCHAR(50) NOT NULL,
    strategy_result VARCHAR(4000) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE TABLE IF NOT EXISTS funding_rates (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    inst_id VARCHAR(32) NOT NULL,
    funding_time BIGINT NOT NULL,
    funding_rate VARCHAR(32) NOT NULL,
    method VARCHAR(20) NOT NULL,
    next_funding_rate VARCHAR(32),
    next_funding_time BIGINT,
    min_funding_rate VARCHAR(32),
    max_funding_rate VARCHAR(32),
    sett_funding_rate VARCHAR(32),
    sett_state VARCHAR(20),
    premium VARCHAR(32),
    ts BIGINT NOT NULL,
    realized_rate VARCHAR(32),
    interest_rate VARCHAR(32),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (inst_id, funding_time)
);

CREATE INDEX IF NOT EXISTS idx_funding_rates_funding_time ON funding_rates (funding_time);
CREATE INDEX IF NOT EXISTS idx_funding_rates_ts ON funding_rates (ts);

CREATE TABLE IF NOT EXISTS tickers_data (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    inst_type VARCHAR(255) NOT NULL,
    inst_id VARCHAR(255) NOT NULL,
    last VARCHAR(255) NOT NULL,
    last_sz VARCHAR(255) NOT NULL,
    ask_px VARCHAR(255) NOT NULL,
    ask_sz VARCHAR(255) NOT NULL,
    bid_px VARCHAR(255) NOT NULL,
    bid_sz VARCHAR(255) NOT NULL,
    open24h VARCHAR(255) NOT NULL,
    high24h VARCHAR(255) NOT NULL,
    low24h VARCHAR(255) NOT NULL,
    vol_ccy24h VARCHAR(255) NOT NULL,
    vol24h VARCHAR(255) NOT NULL,
    sod_utc0 VARCHAR(255) NOT NULL,
    sod_utc8 VARCHAR(255) NOT NULL,
    ts BIGINT NOT NULL,
    UNIQUE (inst_type, inst_id)
);

CREATE INDEX IF NOT EXISTS idx_tickers_data_inst_type ON tickers_data (inst_type);

CREATE TABLE IF NOT EXISTS tickers_volume (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    inst_id VARCHAR(255) NOT NULL,
    period VARCHAR(50) NOT NULL,
    ts BIGINT NOT NULL,
    oi VARCHAR(255) NOT NULL,
    vol VARCHAR(255) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tickers_volume_inst_id ON tickers_volume (inst_id);

CREATE TABLE IF NOT EXISTS external_market_snapshots (
    id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    source VARCHAR(32) NOT NULL,
    symbol VARCHAR(32) NOT NULL,
    metric_type VARCHAR(32) NOT NULL,
    metric_time BIGINT NOT NULL,
    funding_rate VARCHAR(32),
    premium VARCHAR(32),
    open_interest VARCHAR(64),
    oracle_price VARCHAR(64),
    mark_price VARCHAR(64),
    long_short_ratio VARCHAR(32),
    raw_payload JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (source, symbol, metric_type, metric_time)
);

CREATE INDEX IF NOT EXISTS idx_external_market_snapshots_symbol_metric_time
    ON external_market_snapshots (symbol, metric_type, metric_time);
CREATE INDEX IF NOT EXISTS idx_external_market_snapshots_source_metric_time
    ON external_market_snapshots (source, metric_type, metric_time);

-- Table and column comments.
COMMENT ON TABLE strategy_configs IS '量化策略运行配置表';
COMMENT ON TABLE risk_configs IS '风险控制配置表';
COMMENT ON TABLE market_candles IS '统一市场K线数据表';
COMMENT ON TABLE market_snapshots IS '市场行情快照表';
COMMENT ON TABLE indicator_snapshots IS '策略指标快照表';
COMMENT ON TABLE strategy_signals IS '策略信号表';
COMMENT ON TABLE strategy_run_states IS '策略运行状态表';
COMMENT ON TABLE backtest_runs IS '回测任务运行表';
COMMENT ON TABLE backtest_results IS '回测结果汇总表';
COMMENT ON TABLE backtest_trades IS '回测交易明细表';
COMMENT ON TABLE execution_worker_checkpoints IS '执行 worker 检查点表';
COMMENT ON TABLE exchange_request_audit_logs IS '交易所请求审计日志表';
COMMENT ON TABLE strategy_config IS '旧版策略配置表';
COMMENT ON TABLE back_test_log IS '旧版回测结果日志表';
COMMENT ON TABLE back_test_detail IS '旧版回测交易明细表';
COMMENT ON TABLE back_test_analysis IS '旧版回测延迟收益分析表';
COMMENT ON TABLE filtered_signal_log IS '被过滤策略信号日志表';
COMMENT ON TABLE dynamic_config_log IS '动态策略配置调整日志表';
COMMENT ON TABLE strategy_run IS '实盘策略运行实例表';
COMMENT ON TABLE signal_snapshot_log IS '实盘信号快照日志表';
COMMENT ON TABLE risk_decision_log IS '实盘风控决策日志表';
COMMENT ON TABLE order_decision_log IS '实盘下单决策日志表';
COMMENT ON TABLE orders IS '实盘订单表';
COMMENT ON TABLE order_state_log IS '订单状态流转日志表';
COMMENT ON TABLE positions IS '实盘持仓表';
COMMENT ON TABLE portfolio_snapshot_log IS '组合资产快照日志表';
COMMENT ON TABLE strategy_job_signal_log IS '策略任务信号日志表';
COMMENT ON TABLE funding_rates IS '资金费率数据表';
COMMENT ON TABLE tickers_data IS '行情 ticker 数据表';
COMMENT ON TABLE tickers_volume IS 'ticker 成交量/持仓量表';
COMMENT ON TABLE external_market_snapshots IS '外部市场指标快照表';
COMMENT ON COLUMN strategy_configs.id IS '主键ID';
COMMENT ON COLUMN strategy_configs.strategy_key IS '策略键';
COMMENT ON COLUMN strategy_configs.strategy_name IS '策略名称';
COMMENT ON COLUMN strategy_configs.version IS '版本号';
COMMENT ON COLUMN strategy_configs.exchange IS '交易所';
COMMENT ON COLUMN strategy_configs.symbol IS '交易对';
COMMENT ON COLUMN strategy_configs.timeframe IS 'K线周期';
COMMENT ON COLUMN strategy_configs.enabled IS '是否启用';
COMMENT ON COLUMN strategy_configs.config IS '配置内容';
COMMENT ON COLUMN strategy_configs.risk_config IS '风控配置';
COMMENT ON COLUMN strategy_configs.created_at IS '创建时间';
COMMENT ON COLUMN strategy_configs.updated_at IS '更新时间';
COMMENT ON COLUMN strategy_configs.legacy_id IS '旧系统ID';
COMMENT ON COLUMN risk_configs.id IS '主键ID';
COMMENT ON COLUMN risk_configs.config_key IS '配置键';
COMMENT ON COLUMN risk_configs.config_name IS '配置名称';
COMMENT ON COLUMN risk_configs.enabled IS '是否启用';
COMMENT ON COLUMN risk_configs.risk_config IS '风控配置';
COMMENT ON COLUMN risk_configs.created_at IS '创建时间';
COMMENT ON COLUMN risk_configs.updated_at IS '更新时间';
COMMENT ON COLUMN market_candles.id IS '主键ID';
COMMENT ON COLUMN market_candles.exchange IS '交易所';
COMMENT ON COLUMN market_candles.symbol IS '交易对';
COMMENT ON COLUMN market_candles.timeframe IS 'K线周期';
COMMENT ON COLUMN market_candles.open_time IS '开盘时间';
COMMENT ON COLUMN market_candles.close_time IS '收盘时间';
COMMENT ON COLUMN market_candles.open_price IS '开盘价格';
COMMENT ON COLUMN market_candles.high_price IS '最高价格';
COMMENT ON COLUMN market_candles.low_price IS '最低价格';
COMMENT ON COLUMN market_candles.close_price IS '收盘价格';
COMMENT ON COLUMN market_candles.volume IS '成交量';
COMMENT ON COLUMN market_candles.quote_volume IS '计价成交量';
COMMENT ON COLUMN market_candles.source IS '数据来源';
COMMENT ON COLUMN market_candles.raw IS '原始数据';
COMMENT ON COLUMN market_candles.created_at IS '创建时间';
COMMENT ON COLUMN market_snapshots.id IS '主键ID';
COMMENT ON COLUMN market_snapshots.exchange IS '交易所';
COMMENT ON COLUMN market_snapshots.symbol IS '交易对';
COMMENT ON COLUMN market_snapshots.snapshot_type IS 'market_snapshots.snapshot_type 字段';
COMMENT ON COLUMN market_snapshots.snapshot_status IS '快照状态';
COMMENT ON COLUMN market_snapshots.last_price IS '最新价格';
COMMENT ON COLUMN market_snapshots.bid_price IS '买一价';
COMMENT ON COLUMN market_snapshots.ask_price IS '卖一价';
COMMENT ON COLUMN market_snapshots.volume_24h IS '24小时成交量';
COMMENT ON COLUMN market_snapshots.payload IS '业务载荷';
COMMENT ON COLUMN market_snapshots.captured_at IS '采集时间';
COMMENT ON COLUMN market_snapshots.created_at IS '创建时间';
COMMENT ON COLUMN indicator_snapshots.id IS '主键ID';
COMMENT ON COLUMN indicator_snapshots.strategy_key IS '策略键';
COMMENT ON COLUMN indicator_snapshots.exchange IS '交易所';
COMMENT ON COLUMN indicator_snapshots.symbol IS '交易对';
COMMENT ON COLUMN indicator_snapshots.timeframe IS 'K线周期';
COMMENT ON COLUMN indicator_snapshots.indicator_key IS '指标键';
COMMENT ON COLUMN indicator_snapshots.indicator_value IS '指标值';
COMMENT ON COLUMN indicator_snapshots.candle_open_time IS 'K线开盘时间';
COMMENT ON COLUMN indicator_snapshots.generated_at IS '生成时间';
COMMENT ON COLUMN indicator_snapshots.created_at IS '创建时间';
COMMENT ON COLUMN strategy_signals.id IS '主键ID';
COMMENT ON COLUMN strategy_signals.signal_key IS '信号键';
COMMENT ON COLUMN strategy_signals.strategy_key IS '策略键';
COMMENT ON COLUMN strategy_signals.exchange IS '交易所';
COMMENT ON COLUMN strategy_signals.symbol IS '交易对';
COMMENT ON COLUMN strategy_signals.timeframe IS 'K线周期';
COMMENT ON COLUMN strategy_signals.side IS '方向';
COMMENT ON COLUMN strategy_signals.signal_status IS '信号状态';
COMMENT ON COLUMN strategy_signals.strength IS '强度';
COMMENT ON COLUMN strategy_signals.confidence IS '置信度';
COMMENT ON COLUMN strategy_signals.source IS '数据来源';
COMMENT ON COLUMN strategy_signals.payload IS '业务载荷';
COMMENT ON COLUMN strategy_signals.generated_at IS '生成时间';
COMMENT ON COLUMN strategy_signals.created_at IS '创建时间';
COMMENT ON COLUMN strategy_run_states.id IS '主键ID';
COMMENT ON COLUMN strategy_run_states.strategy_key IS '策略键';
COMMENT ON COLUMN strategy_run_states.exchange IS '交易所';
COMMENT ON COLUMN strategy_run_states.symbol IS '交易对';
COMMENT ON COLUMN strategy_run_states.timeframe IS 'K线周期';
COMMENT ON COLUMN strategy_run_states.run_status IS '运行状态';
COMMENT ON COLUMN strategy_run_states.last_signal_id IS '最近信号ID';
COMMENT ON COLUMN strategy_run_states.state IS '状态数据';
COMMENT ON COLUMN strategy_run_states.last_run_at IS '最近运行时间';
COMMENT ON COLUMN strategy_run_states.created_at IS '创建时间';
COMMENT ON COLUMN strategy_run_states.updated_at IS '更新时间';
COMMENT ON COLUMN backtest_runs.id IS '主键ID';
COMMENT ON COLUMN backtest_runs.run_name IS '运行名称';
COMMENT ON COLUMN backtest_runs.strategy_key IS '策略键';
COMMENT ON COLUMN backtest_runs.exchange IS '交易所';
COMMENT ON COLUMN backtest_runs.symbol IS '交易对';
COMMENT ON COLUMN backtest_runs.timeframe IS 'K线周期';
COMMENT ON COLUMN backtest_runs.run_status IS '运行状态';
COMMENT ON COLUMN backtest_runs.config IS '配置内容';
COMMENT ON COLUMN backtest_runs.started_at IS '开始时间';
COMMENT ON COLUMN backtest_runs.completed_at IS '完成时间';
COMMENT ON COLUMN backtest_runs.created_at IS '创建时间';
COMMENT ON COLUMN backtest_results.id IS '主键ID';
COMMENT ON COLUMN backtest_results.run_id IS '运行ID';
COMMENT ON COLUMN backtest_results.net_profit IS '净利润';
COMMENT ON COLUMN backtest_results.max_drawdown IS '最大回撤';
COMMENT ON COLUMN backtest_results.win_rate IS '胜率';
COMMENT ON COLUMN backtest_results.trade_count IS '交易次数';
COMMENT ON COLUMN backtest_results.metrics IS '指标集合';
COMMENT ON COLUMN backtest_results.created_at IS '创建时间';
COMMENT ON COLUMN backtest_trades.id IS '主键ID';
COMMENT ON COLUMN backtest_trades.run_id IS '运行ID';
COMMENT ON COLUMN backtest_trades.exchange IS '交易所';
COMMENT ON COLUMN backtest_trades.symbol IS '交易对';
COMMENT ON COLUMN backtest_trades.side IS '方向';
COMMENT ON COLUMN backtest_trades.entry_time IS '开仓时间';
COMMENT ON COLUMN backtest_trades.exit_time IS '平仓时间';
COMMENT ON COLUMN backtest_trades.entry_price IS '开仓价格';
COMMENT ON COLUMN backtest_trades.exit_price IS '平仓价格';
COMMENT ON COLUMN backtest_trades.quantity IS '数量';
COMMENT ON COLUMN backtest_trades.profit IS '利润';
COMMENT ON COLUMN backtest_trades.payload IS '业务载荷';
COMMENT ON COLUMN backtest_trades.created_at IS '创建时间';
COMMENT ON COLUMN execution_worker_checkpoints.id IS '主键ID';
COMMENT ON COLUMN execution_worker_checkpoints.worker_id IS 'worker ID';
COMMENT ON COLUMN execution_worker_checkpoints.worker_kind IS 'worker 类型';
COMMENT ON COLUMN execution_worker_checkpoints.worker_status IS 'worker 状态';
COMMENT ON COLUMN execution_worker_checkpoints.lease_owner IS '租约持有者';
COMMENT ON COLUMN execution_worker_checkpoints.checkpoint_key IS '检查点键';
COMMENT ON COLUMN execution_worker_checkpoints.checkpoint_value IS '检查点值';
COMMENT ON COLUMN execution_worker_checkpoints.last_task_id IS '最近任务ID';
COMMENT ON COLUMN execution_worker_checkpoints.last_heartbeat_at IS '最近心跳时间';
COMMENT ON COLUMN execution_worker_checkpoints.created_at IS '创建时间';
COMMENT ON COLUMN execution_worker_checkpoints.updated_at IS '更新时间';
COMMENT ON COLUMN exchange_request_audit_logs.id IS '主键ID';
COMMENT ON COLUMN exchange_request_audit_logs.request_id IS '请求ID';
COMMENT ON COLUMN exchange_request_audit_logs.exchange IS '交易所';
COMMENT ON COLUMN exchange_request_audit_logs.symbol IS '交易对';
COMMENT ON COLUMN exchange_request_audit_logs.endpoint IS '接口路径';
COMMENT ON COLUMN exchange_request_audit_logs.request_status IS '请求状态';
COMMENT ON COLUMN exchange_request_audit_logs.latency_ms IS '延迟毫秒数';
COMMENT ON COLUMN exchange_request_audit_logs.request_payload IS '请求载荷';
COMMENT ON COLUMN exchange_request_audit_logs.response_payload IS '响应载荷';
COMMENT ON COLUMN exchange_request_audit_logs.error_message IS '错误信息';
COMMENT ON COLUMN exchange_request_audit_logs.created_at IS '创建时间';
COMMENT ON COLUMN strategy_config.id IS '主键ID';
COMMENT ON COLUMN strategy_config.strategy_type IS '策略类型';
COMMENT ON COLUMN strategy_config.inst_id IS '交易产品ID';
COMMENT ON COLUMN strategy_config.value IS '配置值';
COMMENT ON COLUMN strategy_config.risk_config IS '风控配置';
COMMENT ON COLUMN strategy_config.tags IS '标签';
COMMENT ON COLUMN strategy_config.time IS '周期';
COMMENT ON COLUMN strategy_config.created_at IS '创建时间';
COMMENT ON COLUMN strategy_config.updated_at IS '更新时间';
COMMENT ON COLUMN strategy_config.kline_start_time IS 'K线开始时间';
COMMENT ON COLUMN strategy_config.kline_end_time IS 'K线结束时间';
COMMENT ON COLUMN strategy_config.final_fund IS '最终资金';
COMMENT ON COLUMN strategy_config.is_deleted IS '是否删除';
COMMENT ON COLUMN back_test_log.id IS '主键ID';
COMMENT ON COLUMN back_test_log.strategy_type IS '策略类型';
COMMENT ON COLUMN back_test_log.inst_type IS '产品类型';
COMMENT ON COLUMN back_test_log.time IS '周期';
COMMENT ON COLUMN back_test_log.win_rate IS '胜率';
COMMENT ON COLUMN back_test_log.open_positions_num IS '开仓次数';
COMMENT ON COLUMN back_test_log.final_fund IS '最终资金';
COMMENT ON COLUMN back_test_log.strategy_detail IS '策略详情';
COMMENT ON COLUMN back_test_log.risk_config_detail IS '风控配置详情';
COMMENT ON COLUMN back_test_log.created_at IS '创建时间';
COMMENT ON COLUMN back_test_log.profit IS '利润';
COMMENT ON COLUMN back_test_log.one_bar_after_win_rate IS '后一根K线胜率';
COMMENT ON COLUMN back_test_log.two_bar_after_win_rate IS '后两根K线胜率';
COMMENT ON COLUMN back_test_log.three_bar_after_win_rate IS '后三根K线胜率';
COMMENT ON COLUMN back_test_log.four_bar_after_win_rate IS '后四根K线胜率';
COMMENT ON COLUMN back_test_log.five_bar_after_win_rate IS '后五根K线胜率';
COMMENT ON COLUMN back_test_log.ten_bar_after_win_rate IS '后十根K线胜率';
COMMENT ON COLUMN back_test_log.kline_start_time IS 'K线开始时间';
COMMENT ON COLUMN back_test_log.kline_end_time IS 'K线结束时间';
COMMENT ON COLUMN back_test_log.kline_nums IS 'K线数量';
COMMENT ON COLUMN back_test_log.sharpe_ratio IS '夏普比率';
COMMENT ON COLUMN back_test_log.annual_return IS '年化收益率';
COMMENT ON COLUMN back_test_log.total_return IS '总收益率';
COMMENT ON COLUMN back_test_log.max_drawdown IS '最大回撤';
COMMENT ON COLUMN back_test_log.volatility IS '波动率';
COMMENT ON COLUMN back_test_detail.id IS '主键ID';
COMMENT ON COLUMN back_test_detail.back_test_id IS '回测ID';
COMMENT ON COLUMN back_test_detail.inst_id IS '交易产品ID';
COMMENT ON COLUMN back_test_detail.time IS '周期';
COMMENT ON COLUMN back_test_detail.strategy_type IS '策略类型';
COMMENT ON COLUMN back_test_detail.option_type IS '操作类型';
COMMENT ON COLUMN back_test_detail.signal_open_position_time IS '信号开仓时间';
COMMENT ON COLUMN back_test_detail.open_position_time IS '开仓时间';
COMMENT ON COLUMN back_test_detail.close_position_time IS '平仓时间';
COMMENT ON COLUMN back_test_detail.open_price IS '开盘价格';
COMMENT ON COLUMN back_test_detail.close_price IS '收盘价格';
COMMENT ON COLUMN back_test_detail.fee IS '手续费';
COMMENT ON COLUMN back_test_detail.profit_loss IS '盈亏';
COMMENT ON COLUMN back_test_detail.quantity IS '数量';
COMMENT ON COLUMN back_test_detail.full_close IS '是否完全平仓';
COMMENT ON COLUMN back_test_detail.close_type IS '平仓类型';
COMMENT ON COLUMN back_test_detail.signal_status IS '信号状态';
COMMENT ON COLUMN back_test_detail.signal_value IS '信号值';
COMMENT ON COLUMN back_test_detail.signal_result IS '信号结果';
COMMENT ON COLUMN back_test_detail.created_at IS '创建时间';
COMMENT ON COLUMN back_test_detail.win_nums IS '盈利次数';
COMMENT ON COLUMN back_test_detail.loss_nums IS '亏损次数';
COMMENT ON COLUMN back_test_detail.stop_loss_source IS '止损来源';
COMMENT ON COLUMN back_test_detail.stop_loss_update_history IS '止损更新历史';
COMMENT ON COLUMN back_test_analysis.id IS '主键ID';
COMMENT ON COLUMN back_test_analysis.back_test_id IS '回测ID';
COMMENT ON COLUMN back_test_analysis.inst_id IS '交易产品ID';
COMMENT ON COLUMN back_test_analysis.time IS '周期';
COMMENT ON COLUMN back_test_analysis.option_type IS '操作类型';
COMMENT ON COLUMN back_test_analysis.open_position_time IS '开仓时间';
COMMENT ON COLUMN back_test_analysis.open_price IS '开盘价格';
COMMENT ON COLUMN back_test_analysis.bars_after IS '之后K线数量';
COMMENT ON COLUMN back_test_analysis.price_after IS '之后价格';
COMMENT ON COLUMN back_test_analysis.price_change_percent IS '价格变化百分比';
COMMENT ON COLUMN back_test_analysis.is_profitable IS '是否盈利';
COMMENT ON COLUMN back_test_analysis.created_at IS '创建时间';
COMMENT ON COLUMN filtered_signal_log.id IS '主键ID';
COMMENT ON COLUMN filtered_signal_log.backtest_id IS '回测ID';
COMMENT ON COLUMN filtered_signal_log.inst_id IS '交易产品ID';
COMMENT ON COLUMN filtered_signal_log.period IS '周期';
COMMENT ON COLUMN filtered_signal_log.signal_time IS '信号时间';
COMMENT ON COLUMN filtered_signal_log.direction IS '方向';
COMMENT ON COLUMN filtered_signal_log.filter_reasons IS '过滤原因';
COMMENT ON COLUMN filtered_signal_log.signal_price IS '信号价格';
COMMENT ON COLUMN filtered_signal_log.indicator_snapshot IS '指标快照';
COMMENT ON COLUMN filtered_signal_log.theoretical_profit IS '理论盈利';
COMMENT ON COLUMN filtered_signal_log.theoretical_loss IS '理论亏损';
COMMENT ON COLUMN filtered_signal_log.final_pnl IS '最终盈亏';
COMMENT ON COLUMN filtered_signal_log.trade_result IS '交易结果';
COMMENT ON COLUMN filtered_signal_log.signal_value IS '信号值';
COMMENT ON COLUMN filtered_signal_log.created_at IS '创建时间';
COMMENT ON COLUMN dynamic_config_log.id IS '主键ID';
COMMENT ON COLUMN dynamic_config_log.backtest_id IS '回测ID';
COMMENT ON COLUMN dynamic_config_log.inst_id IS '交易产品ID';
COMMENT ON COLUMN dynamic_config_log.period IS '周期';
COMMENT ON COLUMN dynamic_config_log.kline_time IS 'K线时间';
COMMENT ON COLUMN dynamic_config_log.adjustments IS '调整内容';
COMMENT ON COLUMN dynamic_config_log.config_snapshot IS '配置快照';
COMMENT ON COLUMN dynamic_config_log.created_at IS '创建时间';
COMMENT ON COLUMN strategy_run.id IS '主键ID';
COMMENT ON COLUMN strategy_run.run_id IS '运行ID';
COMMENT ON COLUMN strategy_run.strategy_id IS '策略ID';
COMMENT ON COLUMN strategy_run.inst_id IS '交易产品ID';
COMMENT ON COLUMN strategy_run.period IS '周期';
COMMENT ON COLUMN strategy_run.start_at IS 'strategy_run.start_at 字段';
COMMENT ON COLUMN strategy_run.end_at IS 'strategy_run.end_at 字段';
COMMENT ON COLUMN strategy_run.status IS '状态';
COMMENT ON COLUMN strategy_run.created_at IS '创建时间';
COMMENT ON COLUMN signal_snapshot_log.id IS '主键ID';
COMMENT ON COLUMN signal_snapshot_log.run_id IS '运行ID';
COMMENT ON COLUMN signal_snapshot_log.kline_ts IS 'K线时间戳';
COMMENT ON COLUMN signal_snapshot_log.filtered IS '是否被过滤';
COMMENT ON COLUMN signal_snapshot_log.filter_reasons IS '过滤原因';
COMMENT ON COLUMN signal_snapshot_log.signal_json IS '信号JSON';
COMMENT ON COLUMN signal_snapshot_log.created_at IS '创建时间';
COMMENT ON COLUMN risk_decision_log.id IS '主键ID';
COMMENT ON COLUMN risk_decision_log.run_id IS '运行ID';
COMMENT ON COLUMN risk_decision_log.kline_ts IS 'K线时间戳';
COMMENT ON COLUMN risk_decision_log.decision IS '决策';
COMMENT ON COLUMN risk_decision_log.reason IS '原因';
COMMENT ON COLUMN risk_decision_log.risk_json IS '风控JSON';
COMMENT ON COLUMN risk_decision_log.created_at IS '创建时间';
COMMENT ON COLUMN order_decision_log.id IS '主键ID';
COMMENT ON COLUMN order_decision_log.run_id IS '运行ID';
COMMENT ON COLUMN order_decision_log.kline_ts IS 'K线时间戳';
COMMENT ON COLUMN order_decision_log.side IS '方向';
COMMENT ON COLUMN order_decision_log.size IS '下单数量';
COMMENT ON COLUMN order_decision_log.price IS '价格';
COMMENT ON COLUMN order_decision_log.decision_json IS '决策JSON';
COMMENT ON COLUMN order_decision_log.created_at IS '创建时间';
COMMENT ON COLUMN orders.id IS '主键ID';
COMMENT ON COLUMN orders.run_id IS '运行ID';
COMMENT ON COLUMN orders.strategy_id IS '策略ID';
COMMENT ON COLUMN orders.inst_id IS '交易产品ID';
COMMENT ON COLUMN orders.side IS '方向';
COMMENT ON COLUMN orders.qty IS '数量';
COMMENT ON COLUMN orders.price IS '价格';
COMMENT ON COLUMN orders.status IS '状态';
COMMENT ON COLUMN orders.client_order_id IS '客户端订单ID';
COMMENT ON COLUMN orders.exchange_order_id IS '交易所订单ID';
COMMENT ON COLUMN orders.created_at IS '创建时间';
COMMENT ON COLUMN orders.updated_at IS '更新时间';
COMMENT ON COLUMN order_state_log.id IS '主键ID';
COMMENT ON COLUMN order_state_log.order_id IS '订单ID';
COMMENT ON COLUMN order_state_log.from_state IS '原状态';
COMMENT ON COLUMN order_state_log.to_state IS '目标状态';
COMMENT ON COLUMN order_state_log.reason IS '原因';
COMMENT ON COLUMN order_state_log.ts IS '时间戳';
COMMENT ON COLUMN positions.id IS '主键ID';
COMMENT ON COLUMN positions.run_id IS '运行ID';
COMMENT ON COLUMN positions.strategy_id IS '策略ID';
COMMENT ON COLUMN positions.inst_id IS '交易产品ID';
COMMENT ON COLUMN positions.side IS '方向';
COMMENT ON COLUMN positions.qty IS '数量';
COMMENT ON COLUMN positions.avg_price IS '平均价格';
COMMENT ON COLUMN positions.unrealized_pnl IS '未实现盈亏';
COMMENT ON COLUMN positions.realized_pnl IS '已实现盈亏';
COMMENT ON COLUMN positions.status IS '状态';
COMMENT ON COLUMN positions.updated_at IS '更新时间';
COMMENT ON COLUMN portfolio_snapshot_log.id IS '主键ID';
COMMENT ON COLUMN portfolio_snapshot_log.run_id IS '运行ID';
COMMENT ON COLUMN portfolio_snapshot_log.total_equity IS '总权益';
COMMENT ON COLUMN portfolio_snapshot_log.available IS '可用余额';
COMMENT ON COLUMN portfolio_snapshot_log.margin IS '保证金';
COMMENT ON COLUMN portfolio_snapshot_log.pnl IS '盈亏';
COMMENT ON COLUMN portfolio_snapshot_log.ts IS '时间戳';
COMMENT ON COLUMN strategy_job_signal_log.id IS '主键ID';
COMMENT ON COLUMN strategy_job_signal_log.inst_id IS '交易产品ID';
COMMENT ON COLUMN strategy_job_signal_log.time IS '周期';
COMMENT ON COLUMN strategy_job_signal_log.strategy_type IS '策略类型';
COMMENT ON COLUMN strategy_job_signal_log.strategy_result IS '策略执行结果';
COMMENT ON COLUMN strategy_job_signal_log.created_at IS '创建时间';
COMMENT ON COLUMN strategy_job_signal_log.updated_at IS '更新时间';
COMMENT ON COLUMN funding_rates.id IS '主键ID';
COMMENT ON COLUMN funding_rates.inst_id IS '交易产品ID';
COMMENT ON COLUMN funding_rates.funding_time IS '资金费率时间';
COMMENT ON COLUMN funding_rates.funding_rate IS '资金费率';
COMMENT ON COLUMN funding_rates.method IS '方法';
COMMENT ON COLUMN funding_rates.next_funding_rate IS '下一期资金费率';
COMMENT ON COLUMN funding_rates.next_funding_time IS '下一期资金费率时间';
COMMENT ON COLUMN funding_rates.min_funding_rate IS '最小资金费率';
COMMENT ON COLUMN funding_rates.max_funding_rate IS '最大资金费率';
COMMENT ON COLUMN funding_rates.sett_funding_rate IS '结算资金费率';
COMMENT ON COLUMN funding_rates.sett_state IS '结算状态';
COMMENT ON COLUMN funding_rates.premium IS '溢价';
COMMENT ON COLUMN funding_rates.ts IS '时间戳';
COMMENT ON COLUMN funding_rates.realized_rate IS '已实现费率';
COMMENT ON COLUMN funding_rates.interest_rate IS '利率';
COMMENT ON COLUMN funding_rates.created_at IS '创建时间';
COMMENT ON COLUMN funding_rates.updated_at IS '更新时间';
COMMENT ON COLUMN tickers_data.id IS '主键ID';
COMMENT ON COLUMN tickers_data.inst_type IS '产品类型';
COMMENT ON COLUMN tickers_data.inst_id IS '交易产品ID';
COMMENT ON COLUMN tickers_data.last IS '最新价';
COMMENT ON COLUMN tickers_data.last_sz IS '最新成交数量';
COMMENT ON COLUMN tickers_data.ask_px IS '卖一价';
COMMENT ON COLUMN tickers_data.ask_sz IS '卖一数量';
COMMENT ON COLUMN tickers_data.bid_px IS '买一价';
COMMENT ON COLUMN tickers_data.bid_sz IS '买一数量';
COMMENT ON COLUMN tickers_data.open24h IS '24小时开盘价';
COMMENT ON COLUMN tickers_data.high24h IS '24小时最高价';
COMMENT ON COLUMN tickers_data.low24h IS '24小时最低价';
COMMENT ON COLUMN tickers_data.vol_ccy24h IS '24小时币成交量';
COMMENT ON COLUMN tickers_data.vol24h IS '24小时张成交量';
COMMENT ON COLUMN tickers_data.sod_utc0 IS 'UTC0日开盘价';
COMMENT ON COLUMN tickers_data.sod_utc8 IS 'UTC8日开盘价';
COMMENT ON COLUMN tickers_data.ts IS '时间戳';
COMMENT ON COLUMN tickers_volume.id IS '主键ID';
COMMENT ON COLUMN tickers_volume.inst_id IS '交易产品ID';
COMMENT ON COLUMN tickers_volume.period IS '周期';
COMMENT ON COLUMN tickers_volume.ts IS '时间戳';
COMMENT ON COLUMN tickers_volume.oi IS '持仓量';
COMMENT ON COLUMN tickers_volume.vol IS '成交量';
COMMENT ON COLUMN external_market_snapshots.id IS '主键ID';
COMMENT ON COLUMN external_market_snapshots.source IS '数据来源';
COMMENT ON COLUMN external_market_snapshots.symbol IS '交易对';
COMMENT ON COLUMN external_market_snapshots.metric_type IS '指标类型';
COMMENT ON COLUMN external_market_snapshots.metric_time IS '指标时间';
COMMENT ON COLUMN external_market_snapshots.funding_rate IS '资金费率';
COMMENT ON COLUMN external_market_snapshots.premium IS '溢价';
COMMENT ON COLUMN external_market_snapshots.open_interest IS '未平仓量';
COMMENT ON COLUMN external_market_snapshots.oracle_price IS '预言机价格';
COMMENT ON COLUMN external_market_snapshots.mark_price IS '标记价格';
COMMENT ON COLUMN external_market_snapshots.long_short_ratio IS '多空比';
COMMENT ON COLUMN external_market_snapshots.raw_payload IS '原始载荷';
COMMENT ON COLUMN external_market_snapshots.created_at IS '创建时间';
COMMENT ON COLUMN external_market_snapshots.updated_at IS '更新时间';
