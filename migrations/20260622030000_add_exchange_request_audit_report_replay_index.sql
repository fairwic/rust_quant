CREATE INDEX IF NOT EXISTS idx_exchange_request_audit_report_replay
    ON exchange_request_audit_logs (endpoint, request_id, request_status, created_at DESC, id DESC);
