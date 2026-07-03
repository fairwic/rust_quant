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

CREATE INDEX IF NOT EXISTS idx_market_rank_events_live_handoff_last_evaluated_at
    ON market_rank_events (live_handoff_last_evaluated_at DESC, id DESC)
    WHERE live_handoff_last_evaluated_at IS NOT NULL;

COMMENT ON COLUMN market_rank_events.live_handoff_state IS
    '交易 live handoff 最近评估状态：pending、blocked、expired、created、failed；独立于通知投递状态';
COMMENT ON COLUMN market_rank_events.live_handoff_blocker_code IS
    '交易 live handoff 最近一次阻塞或失败的结构化原因码';
COMMENT ON COLUMN market_rank_events.live_handoff_blocker_detail IS
    '交易 live handoff 最近一次阻塞或失败的详细说明';
COMMENT ON COLUMN market_rank_events.live_handoff_last_evaluated_at IS
    '交易 live handoff 最近一次评估时间';
