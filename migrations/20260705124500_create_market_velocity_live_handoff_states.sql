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
