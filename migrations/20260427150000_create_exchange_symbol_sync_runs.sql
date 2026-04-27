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
