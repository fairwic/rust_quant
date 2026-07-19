const MIGRATION: &str = include_str!(
    "../../../migrations/20260719120000_promote_vegas_eth_4h_v3_same_position_live.sql"
);

#[test]
/// v2 已产生过生产信号，风险合同变化必须创建 v3，不能覆盖旧版本或删除回滚记录。
fn vegas_v3_release_preserves_prior_versions() {
    assert!(MIGRATION.contains("source.version = 'eth_4h_id102_live_v2'"));
    assert!(MIGRATION.contains("'eth_4h_id102_live_v3'"));
    assert!(!MIGRATION.contains("DELETE FROM strategy_configs"));
    assert!(MIGRATION.contains("version <> 'eth_4h_id102_live_v3'"));
}

#[test]
/// 实盘版本必须固定回测同口径仓位，并保证信号 K 线止损合同仍然启用。
fn vegas_v3_release_fixes_position_and_stop_contract() {
    assert!(MIGRATION.contains("'{position_leverage}'"));
    assert!(MIGRATION.contains("'0.58'::jsonb"));
    assert!(MIGRATION.contains("is_used_signal_k_line_stop_loss"));
    assert!(MIGRATION.contains("protective stop plan is missing"));
    assert!(MIGRATION.contains("risk_config - 'fixed_profit_percent_take_profit'"));
}

#[test]
/// 未通过自动门槛的实盘放行必须保留人工授权、压力指标和禁止过期信号重放的证据。
fn vegas_v3_release_records_manual_override() {
    assert!(MIGRATION.contains("\"automatic_promotion_gates_passed\": false"));
    assert!(MIGRATION.contains("\"manual_live_override\": true"));
    assert!(MIGRATION.contains("\"stale_signal_replay_allowed\": false"));
    assert!(MIGRATION.contains("\"conservative_max_drawdown_pct\": 20.7903"));
    assert!(MIGRATION.contains("\"total_return_pct\": 593.8279"));
}
