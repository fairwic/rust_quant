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
