ALTER TABLE IF EXISTS market_rank_events
    ADD COLUMN IF NOT EXISTS current_price NUMERIC(30, 12),
    ADD COLUMN IF NOT EXISTS previous_price NUMERIC(30, 12),
    ADD COLUMN IF NOT EXISTS price_change_pct NUMERIC(18, 8),
    ADD COLUMN IF NOT EXISTS price_direction VARCHAR(16) NOT NULL DEFAULT 'unknown';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_market_rank_events_price_direction'
    ) THEN
        ALTER TABLE market_rank_events
            ADD CONSTRAINT chk_market_rank_events_price_direction
            CHECK (price_direction IN ('up', 'down', 'flat', 'unknown'));
    END IF;
END $$;

COMMENT ON COLUMN market_rank_events.current_price IS '事件发生时的最新成交价格';
COMMENT ON COLUMN market_rank_events.previous_price IS '对比排名快照对应的历史价格；榜单进出事件可为空';
COMMENT ON COLUMN market_rank_events.price_change_pct IS '当前价格相对历史价格的变化百分比';
COMMENT ON COLUMN market_rank_events.price_direction IS '价格变化方向：up、down、flat、unknown';
