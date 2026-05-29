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
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_market_rank_events_event_type
        CHECK (event_type IN ('rank_velocity', 'top_entry', 'top_exit')),
    CONSTRAINT chk_market_rank_events_price_direction
        CHECK (price_direction IN ('up', 'down', 'flat', 'unknown')),
    CONSTRAINT chk_market_rank_events_notification_state
        CHECK (notification_state IN ('pending', 'sent', 'skipped', 'failed'))
);

CREATE INDEX IF NOT EXISTS idx_market_rank_events_detected_at
    ON market_rank_events (detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_events_symbol_detected_at
    ON market_rank_events (symbol, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_market_rank_events_type_timeframe
    ON market_rank_events (event_type, timeframe, detected_at DESC);

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
COMMENT ON COLUMN market_rank_events.created_at IS '记录创建时间';
