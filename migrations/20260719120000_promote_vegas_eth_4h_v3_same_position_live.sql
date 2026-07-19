BEGIN;

-- v2 已在生产产生过信号，补齐仓位/R 合同会改变风险语义，因此创建 v3，不能原地覆盖 v2。
-- v1/v2 记录继续保留；回滚只需重新启用旧版本并恢复 Web production_default 指针。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM strategy_configs
        WHERE strategy_key = 'vegas'
          AND version = 'eth_4h_id102_live_v2'
          AND exchange = 'okx'
          AND symbol = 'ETH-USDT-SWAP'
          AND timeframe = '4H'
    ) THEN
        RAISE EXCEPTION 'Vegas ETH 4H v2 strategy config is missing';
    END IF;
END
$$;

INSERT INTO strategy_configs (
    id,
    legacy_id,
    strategy_key,
    strategy_name,
    version,
    exchange,
    symbol,
    timeframe,
    enabled,
    config,
    risk_config,
    risk_level,
    description,
    detail,
    cover_image,
    display_total_return_pct,
    display_sharpe_ratio,
    display_trade_count,
    display_max_drawdown_pct,
    created_by,
    updated_by,
    created_at,
    updated_at
)
SELECT
    gen_random_uuid(),
    NULL,
    source.strategy_key,
    source.strategy_name,
    'eth_4h_id102_live_v3',
    source.exchange,
    source.symbol,
    source.timeframe,
    FALSE,
    jsonb_set(
        jsonb_set(
            source.config,
            '{entry_rule_version}',
            '"eth_4h_id102_live_v3"'::jsonb,
            TRUE
        ),
        '{_production_release}',
        $release${
          "released_at_utc": "2026-07-19T00:00:00Z",
          "release_mode": "manual_operator_override",
          "signal_rules_cloned_from_version": "eth_4h_id102_live_v2",
          "same_position_leverage": 0.58,
          "cost_stress": {
            "extra_slippage_bps_per_side": 5.0,
            "funding_bps_per_8h": 1.0
          },
          "backtest": {
            "total_return_pct": 593.8279,
            "profit_factor": 1.89042,
            "expectancy_r": 0.24577,
            "sharpe_ratio": 1.73413,
            "recovery_factor": 5.44660,
            "win_rate_pct": 51.9016,
            "trade_count": 447,
            "conservative_max_drawdown_pct": 20.7903
          },
          "automatic_promotion_gates_passed": false,
          "manual_live_override": true,
          "user_accepted_drawdown_exception": true,
          "stale_signal_replay_allowed": false
        }$release$::jsonb,
        TRUE
    ),
    jsonb_set(
        source.risk_config - 'fixed_profit_percent_take_profit',
        '{position_leverage}',
        '0.58'::jsonb,
        TRUE
    ),
    'high',
    source.description,
    source.detail,
    source.cover_image,
    593.8279,
    1.7341,
    447,
    20.7903,
    'codex:user_authorized_live_20260719',
    'codex:user_authorized_live_20260719',
    NOW(),
    NOW()
FROM strategy_configs source
WHERE source.strategy_key = 'vegas'
  AND source.version = 'eth_4h_id102_live_v2'
  AND source.exchange = 'okx'
  AND source.symbol = 'ETH-USDT-SWAP'
  AND source.timeframe = '4H'
ON CONFLICT (strategy_key, version, exchange, symbol, timeframe) DO NOTHING;

UPDATE strategy_configs
SET enabled = FALSE,
    updated_by = 'codex:user_authorized_live_20260719',
    updated_at = NOW()
WHERE strategy_key = 'vegas'
  AND exchange = 'okx'
  AND symbol = 'ETH-USDT-SWAP'
  AND timeframe = '4H'
  AND version <> 'eth_4h_id102_live_v3'
  AND enabled IS DISTINCT FROM FALSE;

UPDATE strategy_configs
SET enabled = TRUE,
    updated_by = 'codex:user_authorized_live_20260719',
    updated_at = NOW()
WHERE strategy_key = 'vegas'
  AND version = 'eth_4h_id102_live_v3'
  AND exchange = 'okx'
  AND symbol = 'ETH-USDT-SWAP'
  AND timeframe = '4H';

DO $$
DECLARE
    enabled_version TEXT;
    enabled_count BIGINT;
    promoted_risk JSONB;
    promoted_config JSONB;
BEGIN
    SELECT COUNT(*), MAX(version)
    INTO enabled_count, enabled_version
    FROM strategy_configs
    WHERE strategy_key = 'vegas'
      AND exchange = 'okx'
      AND symbol = 'ETH-USDT-SWAP'
      AND timeframe = '4H'
      AND enabled = TRUE;

    SELECT risk_config, config
    INTO promoted_risk, promoted_config
    FROM strategy_configs
    WHERE strategy_key = 'vegas'
      AND version = 'eth_4h_id102_live_v3'
      AND exchange = 'okx'
      AND symbol = 'ETH-USDT-SWAP'
      AND timeframe = '4H';

    IF enabled_count <> 1 OR enabled_version <> 'eth_4h_id102_live_v3' THEN
        RAISE EXCEPTION 'Vegas ETH 4H must have exactly one enabled v3 config';
    END IF;
    IF promoted_risk->>'position_leverage' <> '0.58' THEN
        RAISE EXCEPTION 'Vegas ETH 4H v3 position_leverage must be 0.58';
    END IF;
    IF promoted_risk->>'is_used_signal_k_line_stop_loss' <> 'true' THEN
        RAISE EXCEPTION 'Vegas ETH 4H v3 protective stop plan is missing';
    END IF;
    IF promoted_risk ? 'fixed_profit_percent_take_profit' THEN
        RAISE EXCEPTION 'Vegas ETH 4H v3 contains a dead take-profit field';
    END IF;
    IF promoted_config->>'entry_rule_version' <> 'eth_4h_id102_live_v3' THEN
        RAISE EXCEPTION 'Vegas ETH 4H v3 entry_rule_version is missing';
    END IF;
END
$$;

COMMIT;
