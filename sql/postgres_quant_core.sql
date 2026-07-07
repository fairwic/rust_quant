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
    risk_level TEXT,
    description TEXT,
    detail TEXT,
    cover_image TEXT,
    display_total_return_pct NUMERIC(12,4),
    display_sharpe_ratio NUMERIC(12,4),
    display_trade_count INT,
    display_max_drawdown_pct NUMERIC(12,4),
    created_by TEXT,
    updated_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (strategy_key, version, exchange, symbol, timeframe)
);

ALTER TABLE strategy_configs
    ADD COLUMN IF NOT EXISTS legacy_id BIGINT;
ALTER TABLE strategy_configs
    ADD COLUMN IF NOT EXISTS risk_level TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS detail TEXT,
    ADD COLUMN IF NOT EXISTS cover_image TEXT,
    ADD COLUMN IF NOT EXISTS display_total_return_pct NUMERIC(12,4),
    ADD COLUMN IF NOT EXISTS display_sharpe_ratio NUMERIC(12,4),
    ADD COLUMN IF NOT EXISTS display_trade_count INT,
    ADD COLUMN IF NOT EXISTS display_max_drawdown_pct NUMERIC(12,4);

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

CREATE TABLE IF NOT EXISTS exchange_request_rate_limits (
    exchange VARCHAR(64) NOT NULL,
    credential_key VARCHAR(128) NOT NULL,
    endpoint_family VARCHAR(128) NOT NULL,
    window_started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    window_seconds INTEGER NOT NULL DEFAULT 60,
    request_count INTEGER NOT NULL DEFAULT 0,
    max_requests INTEGER NOT NULL DEFAULT 60,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (exchange, credential_key, endpoint_family)
);

CREATE TABLE IF NOT EXISTS exchange_request_circuit_breakers (
    exchange VARCHAR(64) NOT NULL,
    credential_key VARCHAR(128) NOT NULL,
    endpoint_family VARCHAR(128) NOT NULL,
    state VARCHAR(32) NOT NULL DEFAULT 'closed',
    failure_count INTEGER NOT NULL DEFAULT 0,
    opened_until TIMESTAMPTZ,
    last_error TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (exchange, credential_key, endpoint_family)
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
CREATE INDEX IF NOT EXISTS idx_exchange_request_audit_report_replay
    ON exchange_request_audit_logs (endpoint, request_id, request_status, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_request_circuit_opened_until
    ON exchange_request_circuit_breakers (opened_until)
    WHERE opened_until IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_exchange_status
    ON exchange_symbols (exchange, status);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_base_quote
    ON exchange_symbols (base_asset, quote_asset);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_updated_at
    ON exchange_symbols (updated_at DESC);

COMMENT ON TABLE exchange_symbols IS '交易所原始可交易交易对事实表，由 rust_quant 同步维护';
COMMENT ON TABLE exchange_request_rate_limits IS '交易所 API 分布式限频窗口表，由 quant_core live worker 按 exchange、credential、endpoint 维度维护';
COMMENT ON COLUMN exchange_request_rate_limits.exchange IS '交易所标识，如 binance、okx';
COMMENT ON COLUMN exchange_request_rate_limits.credential_key IS '凭证维度键，使用 credential:ID 形式避免存储明文 API Key';
COMMENT ON COLUMN exchange_request_rate_limits.endpoint_family IS '接口族，如 trade.place_order、trade.cancel_order';
COMMENT ON COLUMN exchange_request_rate_limits.window_started_at IS '当前限频窗口开始时间';
COMMENT ON COLUMN exchange_request_rate_limits.window_seconds IS '限频窗口秒数';
COMMENT ON COLUMN exchange_request_rate_limits.request_count IS '当前窗口内已占用请求数';
COMMENT ON COLUMN exchange_request_rate_limits.max_requests IS '当前窗口允许的最大请求数';
COMMENT ON COLUMN exchange_request_rate_limits.updated_at IS '更新时间';
COMMENT ON TABLE exchange_request_circuit_breakers IS '交易所 API 熔断状态表，由 quant_core live worker 按 exchange、credential、endpoint 维度维护';
COMMENT ON COLUMN exchange_request_circuit_breakers.exchange IS '交易所标识，如 binance、okx';
COMMENT ON COLUMN exchange_request_circuit_breakers.credential_key IS '凭证维度键，使用 credential:ID 形式避免存储明文 API Key';
COMMENT ON COLUMN exchange_request_circuit_breakers.endpoint_family IS '接口族，如 trade.place_order、trade.cancel_order';
COMMENT ON COLUMN exchange_request_circuit_breakers.state IS '熔断状态：closed 或 open';
COMMENT ON COLUMN exchange_request_circuit_breakers.failure_count IS '连续失败次数';
COMMENT ON COLUMN exchange_request_circuit_breakers.opened_until IS '熔断开启到期时间；为空表示未打开';
COMMENT ON COLUMN exchange_request_circuit_breakers.last_error IS '最近一次失败摘要，必须是脱敏后的错误';
COMMENT ON COLUMN exchange_request_circuit_breakers.updated_at IS '更新时间';
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
-- Strategy runtime configuration is intentionally single-sourced from
-- quant_core.strategy_configs. The old singular strategy_config table is no
-- longer created by this schema.

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

CREATE TABLE IF NOT EXISTS swap_orders (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    strategy_id INTEGER NOT NULL,
    in_order_id VARCHAR(128) NOT NULL,
    out_order_id VARCHAR(128) NOT NULL DEFAULT '',
    strategy_type VARCHAR(50) NOT NULL,
    period VARCHAR(50) NOT NULL,
    inst_id VARCHAR(64) NOT NULL,
    side VARCHAR(20) NOT NULL,
    pos_size VARCHAR(255) NOT NULL,
    pos_side VARCHAR(20) NOT NULL,
    tag VARCHAR(255) NOT NULL DEFAULT '',
    platform_type VARCHAR(32) NOT NULL,
    detail TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_at TIMESTAMP,
    UNIQUE (in_order_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_swap_orders_out_order_id
    ON swap_orders (out_order_id)
    WHERE out_order_id <> '';
CREATE INDEX IF NOT EXISTS idx_swap_orders_inst
    ON swap_orders (inst_id);
CREATE INDEX IF NOT EXISTS idx_swap_orders_strategy_type
    ON swap_orders (strategy_type);
CREATE INDEX IF NOT EXISTS idx_swap_orders_period
    ON swap_orders (period);
CREATE INDEX IF NOT EXISTS idx_swap_orders_strategy_inst_period_pos_side
    ON swap_orders (strategy_id, inst_id, period, pos_side, created_at DESC);

CREATE TABLE IF NOT EXISTS exchange_apikey_config (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    exchange_name VARCHAR(20) NOT NULL,
    api_key TEXT NOT NULL,
    api_secret TEXT NOT NULL,
    passphrase TEXT,
    is_sandbox SMALLINT NOT NULL DEFAULT 0,
    is_enabled SMALLINT NOT NULL DEFAULT 1,
    description VARCHAR(255) NOT NULL DEFAULT '',
    create_user_id INTEGER NOT NULL DEFAULT 0,
    create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_deleted SMALLINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_exchange_apikey_config_exchange_enabled
    ON exchange_apikey_config (exchange_name, is_enabled, is_deleted);

CREATE TABLE IF NOT EXISTS exchange_apikey_strategy_relation (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    strategy_config_id INTEGER NOT NULL,
    api_config_id INTEGER NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    is_enabled SMALLINT NOT NULL DEFAULT 1,
    is_deleted SMALLINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_exchange_apikey_strategy_relation_strategy
    ON exchange_apikey_strategy_relation (strategy_config_id, priority, is_enabled);
CREATE INDEX IF NOT EXISTS idx_exchange_apikey_strategy_relation_api_config
    ON exchange_apikey_strategy_relation (api_config_id);

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
    strategy_result TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

ALTER TABLE IF EXISTS strategy_job_signal_log
    ALTER COLUMN strategy_result TYPE TEXT;

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
COMMENT ON TABLE back_test_log IS '旧版回测结果日志表';
COMMENT ON TABLE back_test_detail IS '旧版回测交易明细表';
COMMENT ON TABLE back_test_analysis IS '旧版回测延迟收益分析表';
COMMENT ON TABLE filtered_signal_log IS '被过滤策略信号日志表';
COMMENT ON TABLE swap_orders IS '策略实盘合约订单记录表';
COMMENT ON TABLE exchange_apikey_config IS '旧版策略直连交易所API配置表';
COMMENT ON TABLE exchange_apikey_strategy_relation IS '旧版策略与交易所API配置关联表';
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
COMMENT ON COLUMN strategy_configs.risk_level IS '策略商品默认展示风险等级，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.description IS '策略商品默认简介，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.detail IS '策略商品默认详情，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.cover_image IS '策略商品默认展示图路径，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_total_return_pct IS '策略商品默认展示总收益率百分比，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_sharpe_ratio IS '策略商品默认展示夏普比率，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_trade_count IS '策略商品默认展示累计交易笔数，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.display_max_drawdown_pct IS '策略商品默认展示最大回撤百分比，可由 Admin 商品配置覆盖';
COMMENT ON COLUMN strategy_configs.created_by IS '创建者用户名';
COMMENT ON COLUMN strategy_configs.updated_by IS '最后一次编辑者用户名';
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
COMMENT ON COLUMN swap_orders.id IS '主键ID';
COMMENT ON COLUMN swap_orders.strategy_id IS '策略配置ID';
COMMENT ON COLUMN swap_orders.in_order_id IS '内部订单ID，作为策略信号幂等键';
COMMENT ON COLUMN swap_orders.out_order_id IS '交易所订单ID，未产生时为空字符串';
COMMENT ON COLUMN swap_orders.strategy_type IS '策略类型，如 vegas';
COMMENT ON COLUMN swap_orders.period IS '策略周期';
COMMENT ON COLUMN swap_orders.inst_id IS '交易产品ID';
COMMENT ON COLUMN swap_orders.side IS '交易方向，buy 或 sell';
COMMENT ON COLUMN swap_orders.pos_size IS '持仓或下单数量';
COMMENT ON COLUMN swap_orders.pos_side IS '持仓方向，long 或 short';
COMMENT ON COLUMN swap_orders.tag IS '订单标签';
COMMENT ON COLUMN swap_orders.platform_type IS '交易平台类型';
COMMENT ON COLUMN swap_orders.detail IS '下单详情JSON文本';
COMMENT ON COLUMN swap_orders.created_at IS '创建时间';
COMMENT ON COLUMN swap_orders.update_at IS '更新时间';
COMMENT ON COLUMN exchange_apikey_config.id IS '主键ID';
COMMENT ON COLUMN exchange_apikey_config.exchange_name IS '交易所名称';
COMMENT ON COLUMN exchange_apikey_config.api_key IS '交易所API Key';
COMMENT ON COLUMN exchange_apikey_config.api_secret IS '交易所API Secret';
COMMENT ON COLUMN exchange_apikey_config.passphrase IS '交易所API Passphrase';
COMMENT ON COLUMN exchange_apikey_config.is_sandbox IS '是否模拟环境，0否1是';
COMMENT ON COLUMN exchange_apikey_config.is_enabled IS '是否启用，0否1是';
COMMENT ON COLUMN exchange_apikey_config.description IS '配置说明';
COMMENT ON COLUMN exchange_apikey_config.create_user_id IS '创建者用户ID';
COMMENT ON COLUMN exchange_apikey_config.create_time IS '创建时间';
COMMENT ON COLUMN exchange_apikey_config.update_time IS '更新时间';
COMMENT ON COLUMN exchange_apikey_config.is_deleted IS '是否删除，0否1是';
COMMENT ON COLUMN exchange_apikey_strategy_relation.id IS '主键ID';
COMMENT ON COLUMN exchange_apikey_strategy_relation.strategy_config_id IS '策略配置ID';
COMMENT ON COLUMN exchange_apikey_strategy_relation.api_config_id IS 'API配置ID';
COMMENT ON COLUMN exchange_apikey_strategy_relation.priority IS '优先级，数字越小越优先';
COMMENT ON COLUMN exchange_apikey_strategy_relation.is_enabled IS '是否启用，0否1是';
COMMENT ON COLUMN exchange_apikey_strategy_relation.is_deleted IS '是否删除，0否1是';
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

CREATE TABLE IF NOT EXISTS market_anomalies (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(64) NOT NULL UNIQUE,
    current_rank INTEGER NOT NULL,
    rank_15m_ago INTEGER,
    rank_4h_ago INTEGER,
    rank_24h_ago INTEGER,
    delta_15m INTEGER,
    delta_4h INTEGER,
    delta_24h INTEGER,
    volume_24h NUMERIC(30, 8),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(20) NOT NULL DEFAULT 'ACTIVE'
);

CREATE INDEX IF NOT EXISTS idx_market_anomalies_status
    ON market_anomalies (status);
CREATE INDEX IF NOT EXISTS idx_market_anomalies_current_rank
    ON market_anomalies (current_rank);
CREATE INDEX IF NOT EXISTS idx_market_anomalies_updated_at
    ON market_anomalies (updated_at DESC);

COMMENT ON TABLE market_anomalies IS '市场速度雷达当前TopN排名状态表，由 rust_quant 扫描器维护';
COMMENT ON COLUMN market_anomalies.id IS '自增主键';
COMMENT ON COLUMN market_anomalies.symbol IS '交易所交易对标识';
COMMENT ON COLUMN market_anomalies.current_rank IS '当前24小时计价成交额排名';
COMMENT ON COLUMN market_anomalies.rank_15m_ago IS '15分钟前24小时计价成交额排名';
COMMENT ON COLUMN market_anomalies.rank_4h_ago IS '4小时前24小时计价成交额排名';
COMMENT ON COLUMN market_anomalies.rank_24h_ago IS '24小时前24小时计价成交额排名';
COMMENT ON COLUMN market_anomalies.delta_15m IS '15分钟排名变化，正数表示排名上升';
COMMENT ON COLUMN market_anomalies.delta_4h IS '4小时排名变化，正数表示排名上升';
COMMENT ON COLUMN market_anomalies.delta_24h IS '24小时排名变化，正数表示排名上升';
COMMENT ON COLUMN market_anomalies.volume_24h IS '当前24小时计价成交额';
COMMENT ON COLUMN market_anomalies.updated_at IS '最近一次扫描更新时间';
COMMENT ON COLUMN market_anomalies.status IS '当前状态：ACTIVE 或 EXITED';

CREATE TABLE IF NOT EXISTS fund_flow_alerts (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(64) NOT NULL,
    net_inflow NUMERIC(30, 10) NOT NULL,
    total_volume NUMERIC(30, 10) NOT NULL,
    side VARCHAR(16) NOT NULL,
    window_secs INTEGER NOT NULL DEFAULT 60,
    alert_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_fund_flow_alerts_symbol_alert_at
    ON fund_flow_alerts (symbol, alert_at DESC);
CREATE INDEX IF NOT EXISTS idx_fund_flow_alerts_alert_at
    ON fund_flow_alerts (alert_at DESC);

COMMENT ON TABLE fund_flow_alerts IS '市场速度雷达资金流向报警事件表';
COMMENT ON COLUMN fund_flow_alerts.id IS '自增主键';
COMMENT ON COLUMN fund_flow_alerts.symbol IS '交易所交易对标识';
COMMENT ON COLUMN fund_flow_alerts.net_inflow IS '统计窗口内净流入金额';
COMMENT ON COLUMN fund_flow_alerts.total_volume IS '统计窗口内总成交金额';
COMMENT ON COLUMN fund_flow_alerts.side IS '资金流方向：INFLOW 或 OUTFLOW';
COMMENT ON COLUMN fund_flow_alerts.window_secs IS '统计窗口秒数';
COMMENT ON COLUMN fund_flow_alerts.alert_at IS '报警触发时间';

CREATE TABLE IF NOT EXISTS market_rank_events (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(32) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    event_type VARCHAR(32) NOT NULL,
    timeframe VARCHAR(16),
    old_rank INTEGER,
    new_rank INTEGER,
    delta_rank INTEGER,
    volume_24h_quote NUMERIC(30, 8),
    current_price NUMERIC(30, 12),
    previous_price NUMERIC(30, 12),
    price_change_pct NUMERIC(18, 8),
    price_direction VARCHAR(16) NOT NULL DEFAULT 'unknown',
    technical_timeframe VARCHAR(16),
    technical_period INTEGER,
    technical_close_price NUMERIC(30, 12),
    technical_ma_value NUMERIC(30, 12),
    technical_ema_value NUMERIC(30, 12),
    technical_ma_distance_pct NUMERIC(18, 8),
    technical_ema_distance_pct NUMERIC(18, 8),
    technical_ma_state VARCHAR(32),
    technical_ema_state VARCHAR(32),
    technical_candle_count INTEGER,
    technical_snapshot_at TIMESTAMPTZ,
    technical_snapshot_status VARCHAR(32) NOT NULL DEFAULT 'not_requested',
    detected_at TIMESTAMPTZ NOT NULL,
    source VARCHAR(64) NOT NULL,
    notification_state VARCHAR(32) NOT NULL DEFAULT 'pending',
    live_handoff_state VARCHAR(32) NOT NULL DEFAULT 'pending',
    live_handoff_blocker_code VARCHAR(128),
    live_handoff_blocker_detail TEXT,
    live_handoff_last_evaluated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_market_rank_events_event_type
        CHECK (event_type IN ('rank_velocity', 'top_entry', 'top_exit')),
    CONSTRAINT chk_market_rank_events_price_direction
        CHECK (price_direction IN ('up', 'down', 'flat', 'unknown')),
    CONSTRAINT chk_market_rank_events_notification_state
        CHECK (notification_state IN ('pending', 'sent', 'skipped', 'failed')),
    CONSTRAINT chk_market_rank_events_live_handoff_state
        CHECK (live_handoff_state IN ('pending', 'blocked', 'expired', 'created', 'failed'))
);

ALTER TABLE market_rank_events
    ADD COLUMN IF NOT EXISTS live_handoff_state VARCHAR(32) NOT NULL DEFAULT 'pending';
ALTER TABLE market_rank_events
    ADD COLUMN IF NOT EXISTS live_handoff_blocker_code VARCHAR(128);
ALTER TABLE market_rank_events
    ADD COLUMN IF NOT EXISTS live_handoff_blocker_detail TEXT;
ALTER TABLE market_rank_events
    ADD COLUMN IF NOT EXISTS live_handoff_last_evaluated_at TIMESTAMPTZ;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_market_rank_events_live_handoff_state'
          AND conrelid = 'market_rank_events'::regclass
    ) THEN
        ALTER TABLE market_rank_events
            ADD CONSTRAINT chk_market_rank_events_live_handoff_state
            CHECK (live_handoff_state IN ('pending', 'blocked', 'expired', 'created', 'failed'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_market_rank_events_detected_at
    ON market_rank_events (detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_events_symbol_detected_at
    ON market_rank_events (symbol, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_events_type_timeframe
    ON market_rank_events (event_type, timeframe, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_events_radar_exchange_recent
    ON market_rank_events (LOWER(exchange), detected_at DESC, id DESC)
    WHERE new_rank <= 50 OR old_rank <= 50;
CREATE INDEX IF NOT EXISTS idx_market_rank_events_live_handoff_last_evaluated_at
    ON market_rank_events (live_handoff_last_evaluated_at DESC, id DESC)
    WHERE live_handoff_last_evaluated_at IS NOT NULL;

COMMENT ON TABLE market_rank_events IS '市场速度雷达排名事件流水表，用于用户产品时间线、通知和Admin诊断';
COMMENT ON COLUMN market_rank_events.id IS '自增主键';
COMMENT ON COLUMN market_rank_events.exchange IS '事件来源交易所，如 okx、binance';
COMMENT ON COLUMN market_rank_events.symbol IS '交易所交易对标识';
COMMENT ON COLUMN market_rank_events.event_type IS '事件类型：rank_velocity、top_entry、top_exit';
COMMENT ON COLUMN market_rank_events.timeframe IS '对比周期，如 15m、4h、24h；榜单进出事件可为空';
COMMENT ON COLUMN market_rank_events.old_rank IS '对比周期前的排名；新进榜事件可为空';
COMMENT ON COLUMN market_rank_events.new_rank IS '事件发生时的当前排名';
COMMENT ON COLUMN market_rank_events.delta_rank IS '排名变化，正数表示排名上升';
COMMENT ON COLUMN market_rank_events.volume_24h_quote IS '事件发生时24小时计价成交额';
COMMENT ON COLUMN market_rank_events.current_price IS '事件发生时的最新成交价格';
COMMENT ON COLUMN market_rank_events.previous_price IS '对比排名快照对应的历史价格；榜单进出事件可为空';
COMMENT ON COLUMN market_rank_events.price_change_pct IS '当前价格相对历史价格的变化百分比';
COMMENT ON COLUMN market_rank_events.price_direction IS '价格变化方向：up、down、flat、unknown';
COMMENT ON COLUMN market_rank_events.technical_timeframe IS '排名事件发生时同步并计算技术快照的K线周期，如4h';
COMMENT ON COLUMN market_rank_events.technical_period IS '排名事件技术快照使用的均线周期，如20';
COMMENT ON COLUMN market_rank_events.technical_close_price IS '排名事件技术快照对应的最新K线收盘价';
COMMENT ON COLUMN market_rank_events.technical_ma_value IS '排名事件技术快照的简单移动均线值';
COMMENT ON COLUMN market_rank_events.technical_ema_value IS '排名事件技术快照的指数移动均线值';
COMMENT ON COLUMN market_rank_events.technical_ma_distance_pct IS '最新收盘价相对简单移动均线的偏离百分比';
COMMENT ON COLUMN market_rank_events.technical_ema_distance_pct IS '最新收盘价相对指数移动均线的偏离百分比';
COMMENT ON COLUMN market_rank_events.technical_ma_state IS '最新收盘价相对简单移动均线的状态：above、below、touching、breakout_up、breakdown_down';
COMMENT ON COLUMN market_rank_events.technical_ema_state IS '最新收盘价相对指数移动均线的状态：above、below、touching、breakout_up、breakdown_down';
COMMENT ON COLUMN market_rank_events.technical_candle_count IS '用于计算排名事件技术快照的K线数量';
COMMENT ON COLUMN market_rank_events.technical_snapshot_at IS '排名事件技术快照对应的最新K线时间';
COMMENT ON COLUMN market_rank_events.technical_snapshot_status IS '排名事件技术快照状态：not_requested、not_configured、captured、insufficient_kline、fetch_failed';
COMMENT ON COLUMN market_rank_events.detected_at IS '扫描器检测到事件的时间';
COMMENT ON COLUMN market_rank_events.source IS '事件生成来源，如 scanner_service';
COMMENT ON COLUMN market_rank_events.notification_state IS '通知投递状态：pending、sent、skipped、failed';
COMMENT ON COLUMN market_rank_events.live_handoff_state IS '交易 live handoff 最近评估状态：pending、blocked、expired、created、failed；独立于通知投递状态';
COMMENT ON COLUMN market_rank_events.live_handoff_blocker_code IS '交易 live handoff 最近一次阻塞或失败的结构化原因码';
COMMENT ON COLUMN market_rank_events.live_handoff_blocker_detail IS '交易 live handoff 最近一次阻塞或失败的详细说明';
COMMENT ON COLUMN market_rank_events.live_handoff_last_evaluated_at IS '交易 live handoff 最近一次评估时间';
COMMENT ON COLUMN market_rank_events.created_at IS '记录创建时间';

CREATE TABLE IF NOT EXISTS market_velocity_live_handoff_states (
    id BIGSERIAL PRIMARY KEY,
    rank_event_id BIGINT NOT NULL REFERENCES market_rank_events(id) ON DELETE CASCADE,
    strategy_slug VARCHAR(128) NOT NULL,
    strategy_preset VARCHAR(255) NOT NULL,
    entry_rule_version VARCHAR(255) NOT NULL,
    handoff_state VARCHAR(32) NOT NULL DEFAULT 'pending',
    blocker_code VARCHAR(128),
    blocker_detail TEXT,
    last_evaluated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_market_velocity_live_handoff_states_state
        CHECK (handoff_state IN ('pending', 'blocked', 'expired', 'created', 'failed'))
);

CREATE UNIQUE INDEX IF NOT EXISTS uidx_market_velocity_live_handoff_states_contract
    ON market_velocity_live_handoff_states (
        rank_event_id,
        strategy_slug,
        strategy_preset,
        entry_rule_version
    );
CREATE INDEX IF NOT EXISTS idx_market_velocity_live_handoff_states_pending
    ON market_velocity_live_handoff_states (
        strategy_slug,
        strategy_preset,
        entry_rule_version,
        rank_event_id
    )
    WHERE handoff_state = 'pending';
CREATE INDEX IF NOT EXISTS idx_market_velocity_live_handoff_states_last_evaluated
    ON market_velocity_live_handoff_states (last_evaluated_at DESC, id DESC)
    WHERE last_evaluated_at IS NOT NULL;

COMMENT ON TABLE market_velocity_live_handoff_states IS 'Market Velocity live handoff 按策略合同隔离的事件评估状态，避免不同策略调度器抢占同一 rank event';
COMMENT ON COLUMN market_velocity_live_handoff_states.id IS '自增主键';
COMMENT ON COLUMN market_velocity_live_handoff_states.rank_event_id IS '关联的 market_rank_events.id，代表原始动量异动事件';
COMMENT ON COLUMN market_velocity_live_handoff_states.strategy_slug IS '策略标识，如 market_velocity 或 market_velocity_breakdown_short';
COMMENT ON COLUMN market_velocity_live_handoff_states.strategy_preset IS '策略运行 preset，用于区分同一策略 slug 的不同参数版本';
COMMENT ON COLUMN market_velocity_live_handoff_states.entry_rule_version IS '入场规则版本，用于审计不同 handoff 合同的处理状态';
COMMENT ON COLUMN market_velocity_live_handoff_states.handoff_state IS '该策略合同对该事件的 live handoff 状态：pending、blocked、expired、created、failed';
COMMENT ON COLUMN market_velocity_live_handoff_states.blocker_code IS '该策略合同最近一次 live handoff 阻塞或失败的结构化原因码';
COMMENT ON COLUMN market_velocity_live_handoff_states.blocker_detail IS '该策略合同最近一次 live handoff 阻塞或失败的详细说明';
COMMENT ON COLUMN market_velocity_live_handoff_states.last_evaluated_at IS '该策略合同最近一次评估该事件的时间';
COMMENT ON COLUMN market_velocity_live_handoff_states.created_at IS '记录创建时间';
COMMENT ON COLUMN market_velocity_live_handoff_states.updated_at IS '记录更新时间';

CREATE TABLE IF NOT EXISTS market_velocity_episodes (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(32) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    event_type VARCHAR(32) NOT NULL,
    timeframe VARCHAR(16),
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    started_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    first_old_rank INTEGER,
    latest_old_rank INTEGER,
    latest_new_rank INTEGER,
    best_new_rank INTEGER,
    latest_delta_rank INTEGER,
    max_delta_rank INTEGER,
    hit_count INTEGER NOT NULL DEFAULT 1,
    volume_24h_quote NUMERIC(30, 8),
    current_price NUMERIC(30, 12),
    previous_price NUMERIC(30, 12),
    price_change_pct NUMERIC(18, 8),
    price_direction VARCHAR(16) NOT NULL DEFAULT 'unknown',
    technical_snapshot_status VARCHAR(32) NOT NULL DEFAULT 'not_requested',
    last_rank_event_id BIGINT REFERENCES market_rank_events(id) ON DELETE SET NULL,
    last_escalated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_market_velocity_episodes_event_type
        CHECK (event_type IN ('rank_velocity', 'top_entry', 'top_exit')),
    CONSTRAINT chk_market_velocity_episodes_status
        CHECK (status IN ('active', 'closed')),
    CONSTRAINT chk_market_velocity_episodes_price_direction
        CHECK (price_direction IN ('up', 'down', 'flat', 'unknown')),
    CONSTRAINT chk_market_velocity_episodes_hit_count
        CHECK (hit_count > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS uidx_market_velocity_episodes_active_key
    ON market_velocity_episodes (LOWER(exchange), symbol, event_type, COALESCE(timeframe, ''))
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_recent
    ON market_velocity_episodes (last_seen_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_symbol_recent
    ON market_velocity_episodes (symbol, last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_active_stale
    ON market_velocity_episodes (LOWER(exchange), last_seen_at)
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_backtest_active
    ON market_velocity_episodes (
        status,
        event_type,
        COALESCE(max_delta_rank, latest_delta_rank, 0),
        COALESCE(best_new_rank, latest_new_rank),
        started_at
    )
    WHERE status IN ('active', 'closed') AND current_price IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_last_rank_event
    ON market_velocity_episodes (last_rank_event_id)
    WHERE last_rank_event_id IS NOT NULL;

COMMENT ON TABLE market_velocity_episodes IS '市场动能排名异动聚合机会表，用于去重高频排名事件并作为干净回测样本源';
COMMENT ON COLUMN market_velocity_episodes.id IS '自增主键';
COMMENT ON COLUMN market_velocity_episodes.exchange IS '事件来源交易所，如 okx、binance';
COMMENT ON COLUMN market_velocity_episodes.symbol IS '交易所交易对标识';
COMMENT ON COLUMN market_velocity_episodes.event_type IS '事件类型：rank_velocity、top_entry、top_exit';
COMMENT ON COLUMN market_velocity_episodes.timeframe IS '对比周期，如 15m、4h、24h；榜单进出事件可为空';
COMMENT ON COLUMN market_velocity_episodes.status IS '机会状态：active、closed；active 表示同一交易对/周期机会仍在延续';
COMMENT ON COLUMN market_velocity_episodes.started_at IS '该聚合机会首次出现时间';
COMMENT ON COLUMN market_velocity_episodes.last_seen_at IS '该聚合机会最近一次被扫描命中的时间';
COMMENT ON COLUMN market_velocity_episodes.first_old_rank IS '机会首次出现时的旧排名';
COMMENT ON COLUMN market_velocity_episodes.latest_old_rank IS '机会最近一次命中时的旧排名';
COMMENT ON COLUMN market_velocity_episodes.latest_new_rank IS '机会最近一次命中时的新排名';
COMMENT ON COLUMN market_velocity_episodes.best_new_rank IS '该机会生命周期内达到过的最佳排名，数值越小排名越靠前';
COMMENT ON COLUMN market_velocity_episodes.latest_delta_rank IS '机会最近一次命中时的排名跃迁幅度';
COMMENT ON COLUMN market_velocity_episodes.max_delta_rank IS '该机会生命周期内最大的排名跃迁幅度';
COMMENT ON COLUMN market_velocity_episodes.hit_count IS '该机会累计命中次数';
COMMENT ON COLUMN market_velocity_episodes.volume_24h_quote IS '机会最近一次命中时的24小时计价成交额';
COMMENT ON COLUMN market_velocity_episodes.current_price IS '机会最近一次命中时的最新成交价格';
COMMENT ON COLUMN market_velocity_episodes.previous_price IS '机会最近一次命中时对比排名快照对应的历史价格';
COMMENT ON COLUMN market_velocity_episodes.price_change_pct IS '机会最近一次命中时当前价格相对历史价格的变化百分比';
COMMENT ON COLUMN market_velocity_episodes.price_direction IS '价格变化方向：up、down、flat、unknown';
COMMENT ON COLUMN market_velocity_episodes.technical_snapshot_status IS '机会最近一次命中时的技术快照状态';
COMMENT ON COLUMN market_velocity_episodes.last_rank_event_id IS '该机会最近一次关联写入的排名事件流水ID';
COMMENT ON COLUMN market_velocity_episodes.last_escalated_at IS '该机会最近一次因为排名或跃迁幅度改善而升级的时间';
COMMENT ON COLUMN market_velocity_episodes.created_at IS '记录创建时间';
COMMENT ON COLUMN market_velocity_episodes.updated_at IS '记录更新时间';

CREATE TABLE IF NOT EXISTS market_rank_snapshots (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(32) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    rank INTEGER NOT NULL,
    price NUMERIC(30, 12) NOT NULL,
    volume_24h_quote NUMERIC(30, 8) NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uniq_market_rank_snapshots_exchange_symbol_time
        UNIQUE (exchange, symbol, captured_at)
);

CREATE INDEX IF NOT EXISTS idx_market_rank_snapshots_exchange_time
    ON market_rank_snapshots (exchange, captured_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_snapshots_symbol_time
    ON market_rank_snapshots (symbol, captured_at DESC);

COMMENT ON TABLE market_rank_snapshots IS '市场速度雷达排名价格快照表，用于重启后恢复排名历史和价格对比证据';
COMMENT ON COLUMN market_rank_snapshots.id IS '自增主键';
COMMENT ON COLUMN market_rank_snapshots.exchange IS '快照来源交易所，如 okx、binance';
COMMENT ON COLUMN market_rank_snapshots.symbol IS '交易所交易对标识';
COMMENT ON COLUMN market_rank_snapshots.rank IS '快照时刻的24小时计价成交额排名';
COMMENT ON COLUMN market_rank_snapshots.price IS '快照时刻的最新成交价格';
COMMENT ON COLUMN market_rank_snapshots.volume_24h_quote IS '快照时刻的24小时计价成交额';
COMMENT ON COLUMN market_rank_snapshots.captured_at IS '扫描器采集快照时间';
COMMENT ON COLUMN market_rank_snapshots.created_at IS '记录创建时间';
