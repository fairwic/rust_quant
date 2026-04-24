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

CREATE INDEX IF NOT EXISTS idx_exchange_symbols_exchange_status
    ON exchange_symbols (exchange, status);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_base_quote
    ON exchange_symbols (base_asset, quote_asset);
CREATE INDEX IF NOT EXISTS idx_exchange_symbols_updated_at
    ON exchange_symbols (updated_at DESC);
