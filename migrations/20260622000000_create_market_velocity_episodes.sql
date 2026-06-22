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
CREATE INDEX IF NOT EXISTS idx_market_velocity_episodes_backtest_active
    ON market_velocity_episodes (
        event_type,
        COALESCE(max_delta_rank, latest_delta_rank, 0),
        COALESCE(best_new_rank, latest_new_rank),
        started_at
    )
    WHERE status = 'active' AND current_price IS NOT NULL;
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
