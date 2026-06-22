CREATE INDEX IF NOT EXISTS idx_market_rank_events_radar_exchange_recent
    ON market_rank_events (LOWER(exchange), detected_at DESC, id DESC)
    WHERE new_rank <= 50 OR old_rank <= 50;
