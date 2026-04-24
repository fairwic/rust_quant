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
