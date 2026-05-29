ALTER TABLE IF EXISTS market_rank_events
    ADD COLUMN IF NOT EXISTS technical_timeframe VARCHAR(16),
    ADD COLUMN IF NOT EXISTS technical_period INTEGER,
    ADD COLUMN IF NOT EXISTS technical_close_price NUMERIC(30, 12),
    ADD COLUMN IF NOT EXISTS technical_ma_value NUMERIC(30, 12),
    ADD COLUMN IF NOT EXISTS technical_ema_value NUMERIC(30, 12),
    ADD COLUMN IF NOT EXISTS technical_ma_distance_pct NUMERIC(18, 8),
    ADD COLUMN IF NOT EXISTS technical_ema_distance_pct NUMERIC(18, 8),
    ADD COLUMN IF NOT EXISTS technical_ma_state VARCHAR(32),
    ADD COLUMN IF NOT EXISTS technical_ema_state VARCHAR(32),
    ADD COLUMN IF NOT EXISTS technical_candle_count INTEGER,
    ADD COLUMN IF NOT EXISTS technical_snapshot_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS technical_snapshot_status VARCHAR(32) NOT NULL DEFAULT 'not_requested';

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
