use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_services::strategy::{
    FactorBucketReport, FactorConclusion, PriceOiState, ResearchFilteredSignalSample,
    ResearchSampleKind, ResearchTradeSample, VegasFactorResearchService, VolatilityTier,
};

struct SnapshotMetrics {
    funding_rate: Option<f64>,
    premium: Option<f64>,
    open_interest: Option<f64>,
    mark_price: Option<f64>,
}

fn snapshot(
    source: &str,
    symbol: &str,
    metric_type: &str,
    metric_time: i64,
    metrics: SnapshotMetrics,
) -> ExternalMarketSnapshot {
    let mut row = ExternalMarketSnapshot::new(
        source.to_string(),
        symbol.to_string(),
        metric_type.to_string(),
        metric_time,
    );
    row.funding_rate = metrics.funding_rate;
    row.premium = metrics.premium;
    row.open_interest = metrics.open_interest;
    row.mark_price = metrics.mark_price;
    row
}

#[test]
fn maps_symbol_to_expected_volatility_tier() {
    assert_eq!(
        VolatilityTier::from_symbol("BTC-USDT-SWAP"),
        VolatilityTier::Btc
    );
    assert_eq!(
        VolatilityTier::from_symbol("ETH-USDT-SWAP"),
        VolatilityTier::Eth
    );
    assert_eq!(
        VolatilityTier::from_symbol("SOL-USDT-SWAP"),
        VolatilityTier::Alt
    );
}

#[test]
fn aligns_latest_snapshot_within_four_hour_window() {
    let event_time = 1_744_000_000_000_i64;
    let snapshots = vec![
        snapshot(
            "hyperliquid",
            "ETH",
            "meta",
            event_time - 4 * 60 * 60 * 1000 - 1,
            SnapshotMetrics {
                funding_rate: Some(0.0001),
                premium: Some(0.001),
                open_interest: Some(1_000.0),
                mark_price: Some(2_000.0),
            },
        ),
        snapshot(
            "hyperliquid",
            "ETH",
            "meta",
            event_time - 30 * 60 * 1000,
            SnapshotMetrics {
                funding_rate: Some(0.0002),
                premium: Some(0.002),
                open_interest: Some(1_100.0),
                mark_price: Some(2_010.0),
            },
        ),
    ];

    let aligned =
        VegasFactorResearchService::align_latest_snapshot(event_time, &snapshots).expect("aligned");

    assert_eq!(aligned.metric_time, event_time - 30 * 60 * 1000);
    assert_eq!(aligned.open_interest, Some(1_100.0));
}

#[test]
fn classifies_price_and_open_interest_state() {
    assert_eq!(
        VegasFactorResearchService::classify_price_oi_state(Some(0.03), Some(0.05)),
        PriceOiState::LongBuildup
    );
    assert_eq!(
        VegasFactorResearchService::classify_price_oi_state(Some(-0.04), Some(0.07)),
        PriceOiState::ShortBuildup
    );
    assert_eq!(
        VegasFactorResearchService::classify_price_oi_state(Some(0.02), Some(-0.03)),
        PriceOiState::ShortCovering
    );
    assert_eq!(
        VegasFactorResearchService::classify_price_oi_state(Some(-0.01), Some(-0.08)),
        PriceOiState::LongUnwinding
    );
}

#[test]
fn classifies_funding_signal_context_buckets() {
    let signal_value = r#"{
        "ema_values": {"is_long_trend": false, "is_short_trend": true},
        "macd_value": {
            "histogram": -6.3,
            "above_zero": true,
            "histogram_improving": false,
            "histogram_decreasing": true
        },
        "volume_value": {"volume_ratio": 1.62}
    }"#;

    let contexts = VegasFactorResearchService::classify_funding_signal_contexts(
        Some("funding_positive"),
        "LONG",
        Some(signal_value),
    );

    assert!(contexts.contains(&(
        "funding_direction_context",
        "funding_positive_long".to_string()
    )));
    assert!(contexts.contains(&(
        "funding_trend_context",
        "funding_positive_long_short_trend".to_string()
    )));
    assert!(contexts.contains(&(
        "funding_macd_context",
        "funding_positive_long_macd_below_zero_hist_decreasing".to_string()
    )));
    assert!(contexts.contains(&(
        "funding_volume_context",
        "funding_positive_long_volume_expansion".to_string()
    )));
}

#[test]
fn classifies_funding_filter_context_buckets() {
    let signal_value = r#"{
        "ema_distance_filter": {"state": "TooFar"},
        "leg_detection_value": {"is_bullish_leg": true, "is_bearish_leg": false}
    }"#;

    let contexts = VegasFactorResearchService::classify_funding_filter_contexts(
        Some("funding_positive"),
        "LONG",
        Some(r#"["MACD_FALLING_KNIFE_LONG"]"#),
        Some(signal_value),
    );

    assert_eq!(
        contexts,
        vec![(
            "funding_filter_context",
            "funding_positive_long_macd_falling_knife_long_distance_too_far_bullish_leg"
                .to_string()
        )]
    );
}

#[test]
fn classifies_internal_exit_environment_context_buckets() {
    let signal_value = r#"{
        "ema_values": {"is_long_trend": false, "is_short_trend": false},
        "macd_value": {"histogram": 7.49},
        "ema_distance_filter": {"state": "TooFar"},
        "volume_value": {"volume_ratio": 2.05}
    }"#;

    let contexts = VegasFactorResearchService::classify_internal_exit_contexts(
        "short",
        Some("Signal_Kline_Stop_Loss"),
        Some("Engulfing_Volume_Confirmed"),
        Some(signal_value),
    );

    assert_eq!(
        contexts,
        vec![(
            "exit_environment_context",
            "signal_stop_mixed_trend_macd_against_distance_too_far_volume_expansion".to_string()
        )]
    );
}

#[test]
fn evaluates_factor_conclusion_using_eth_first_gate() {
    let eligible_traded = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "funding_positive".to_string(),
        sample_kind: ResearchSampleKind::Traded,
        volatility_tier: VolatilityTier::Eth,
        scope_label: "ETH".to_string(),
        sample_count: 6,
        win_rate: 0.66,
        avg_pnl: 42.0,
        sharpe_proxy: 1.35,
        avg_mfe: 88.0,
        avg_mae: -19.0,
        conclusion: FactorConclusion::Observe,
    };
    let eligible_filtered = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "funding_positive".to_string(),
        sample_kind: ResearchSampleKind::Filtered,
        volatility_tier: VolatilityTier::Eth,
        scope_label: "ETH".to_string(),
        sample_count: 10,
        win_rate: 0.2,
        avg_pnl: -1.0,
        sharpe_proxy: 0.1,
        avg_mfe: 2.0,
        avg_mae: -5.0,
        conclusion: FactorConclusion::Observe,
    };
    assert_eq!(
        VegasFactorResearchService::evaluate_factor_conclusion(&[
            eligible_traded,
            eligible_filtered
        ]),
        FactorConclusion::Candidate
    );

    let rejected_traded = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "divergent_bear".to_string(),
        sample_kind: ResearchSampleKind::Traded,
        volatility_tier: VolatilityTier::Eth,
        scope_label: "ETH".to_string(),
        sample_count: 2,
        win_rate: 0.4,
        avg_pnl: -11.0,
        sharpe_proxy: -0.3,
        avg_mfe: 15.0,
        avg_mae: -32.0,
        conclusion: FactorConclusion::Observe,
    };
    let rejected_filtered = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "divergent_bear".to_string(),
        sample_kind: ResearchSampleKind::Filtered,
        volatility_tier: VolatilityTier::Eth,
        scope_label: "ETH".to_string(),
        sample_count: 9,
        win_rate: 0.5,
        avg_pnl: 1.0,
        sharpe_proxy: 0.2,
        avg_mfe: 3.0,
        avg_mae: -2.0,
        conclusion: FactorConclusion::Observe,
    };
    assert_eq!(
        VegasFactorResearchService::evaluate_factor_conclusion(&[
            rejected_traded,
            rejected_filtered
        ]),
        FactorConclusion::Reject
    );

    let positive_filtered_only = FactorBucketReport {
        factor_name: "funding_filter_context".to_string(),
        bucket_name: "funding_positive_long_macd_falling_knife_long_distance_too_far_bullish_leg"
            .to_string(),
        sample_kind: ResearchSampleKind::Filtered,
        volatility_tier: VolatilityTier::Eth,
        scope_label: "ETH".to_string(),
        sample_count: 4,
        win_rate: 0.75,
        avg_pnl: 0.07,
        sharpe_proxy: 0.86,
        avg_mfe: 0.09,
        avg_mae: -0.02,
        conclusion: FactorConclusion::Observe,
    };
    assert_eq!(
        VegasFactorResearchService::evaluate_factor_conclusion(&[positive_filtered_only]),
        FactorConclusion::Candidate
    );
}

#[test]
fn renders_report_with_required_sections() {
    let trades = vec![ResearchTradeSample {
        backtest_id: 1428,
        inst_id: "ETH-USDT-SWAP".to_string(),
        timeframe: "4H".to_string(),
        side: "long".to_string(),
        open_time_ms: 1_744_000_000_000,
        close_time_ms: Some(1_744_014_400_000),
        pnl: 37.8,
        close_type: Some("Signal_Kline_Stop_Loss".to_string()),
        stop_loss_source: Some("Engulfing_Volume_Confirmed".to_string()),
        signal_value: Some("{}".to_string()),
        signal_result: Some("{}".to_string()),
    }];
    let filtered_signals = vec![ResearchFilteredSignalSample {
        backtest_id: 1428,
        inst_id: "ETH-USDT-SWAP".to_string(),
        timeframe: "4H".to_string(),
        direction: "LONG".to_string(),
        signal_time_ms: 1_744_000_000_000,
        theoretical_pnl: Some(-3.4),
        trade_result: Some("LOSS".to_string()),
        filter_reasons: Some("[\"MACD_FALLING_KNIFE_LONG\"]".to_string()),
        signal_value: Some("{}".to_string()),
    }];
    let buckets = vec![
        FactorBucketReport {
            factor_name: "price_oi_state".to_string(),
            bucket_name: "short_covering".to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Eth,
            scope_label: "ETH".to_string(),
            sample_count: 4,
            win_rate: 0.75,
            avg_pnl: 23.0,
            sharpe_proxy: 1.2,
            avg_mfe: 51.0,
            avg_mae: -12.0,
            conclusion: FactorConclusion::Candidate,
        },
        FactorBucketReport {
            factor_name: "funding_macd_context".to_string(),
            bucket_name: "funding_negative_short_macd_below_zero_hist_decreasing".to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Eth,
            scope_label: "ETH".to_string(),
            sample_count: 3,
            win_rate: 0.33,
            avg_pnl: -66.0,
            sharpe_proxy: -0.4,
            avg_mfe: 20.0,
            avg_mae: -86.0,
            conclusion: FactorConclusion::Reject,
        },
        FactorBucketReport {
            factor_name: "funding_volume_context".to_string(),
            bucket_name: "funding_positive_long_volume_normal".to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Btc,
            scope_label: "BTC".to_string(),
            sample_count: 3,
            win_rate: 0.66,
            avg_pnl: -5.72,
            sharpe_proxy: -0.47,
            avg_mfe: 3.0,
            avg_mae: -8.72,
            conclusion: FactorConclusion::Reject,
        },
        FactorBucketReport {
            factor_name: "funding_trend_context".to_string(),
            bucket_name: "funding_positive_long_mixed_trend".to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Btc,
            scope_label: "BTC".to_string(),
            sample_count: 5,
            win_rate: 0.4,
            avg_pnl: -0.66,
            sharpe_proxy: -0.32,
            avg_mfe: 1.0,
            avg_mae: -1.66,
            conclusion: FactorConclusion::Reject,
        },
        FactorBucketReport {
            factor_name: "funding_macd_context".to_string(),
            bucket_name: "funding_positive_long_macd_below_zero_hist_flat".to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Btc,
            scope_label: "BTC".to_string(),
            sample_count: 6,
            win_rate: 0.66,
            avg_pnl: -2.37,
            sharpe_proxy: -0.25,
            avg_mfe: 2.0,
            avg_mae: -4.37,
            conclusion: FactorConclusion::Reject,
        },
        FactorBucketReport {
            factor_name: "exit_environment_context".to_string(),
            bucket_name: "signal_stop_mixed_trend_macd_against_distance_too_far_volume_expansion"
                .to_string(),
            sample_kind: ResearchSampleKind::Traded,
            volatility_tier: VolatilityTier::Eth,
            scope_label: "ETH".to_string(),
            sample_count: 4,
            win_rate: 0.0,
            avg_pnl: -67.35,
            sharpe_proxy: -0.72,
            avg_mfe: 0.0,
            avg_mae: -67.35,
            conclusion: FactorConclusion::Reject,
        },
    ];

    let report = VegasFactorResearchService::render_report(&trades, &filtered_signals, &buckets);

    assert!(report.contains("因子概览表"));
    assert!(report.contains("分桶统计表"));
    assert!(report.contains("BTC / ETH / 其他币种"));
    assert!(report.contains("可实验"));
    assert!(report.contains("路径影响评估"));
    assert!(!report.contains("可回注"));
    assert!(report.contains("过滤候选样本数"));
    assert!(report.contains("已成交样本"));
    assert!(report.contains("过滤候选"));
    assert!(report.contains("低 Sharpe 开仓环境候选"));
    assert!(report.contains("下一轮未覆盖低 Sharpe 开仓环境候选"));
    assert!(report.contains("出场/止损环境候选"));
    assert!(
        report.contains("signal_stop_mixed_trend_macd_against_distance_too_far_volume_expansion")
    );
    assert!(report.contains("低影响观察候选"));
    assert!(report.contains("已覆盖拒绝候选"));
    assert!(report.contains("covered_by_1450"));
    assert!(report.contains("funding_negative_short_macd_below_zero_hist_decreasing"));
    assert!(report.contains("funding_positive_long_mixed_trend"));
    assert!(report.contains("ETH"));
    assert!(report.contains("BTC"));
    assert!(report.contains("TotalPnL"));
    let open_section = report
        .split("## 下一轮未覆盖低 Sharpe 开仓环境候选")
        .nth(1)
        .and_then(|section| section.split("## 低影响观察候选").next())
        .expect("open candidate section");
    assert!(!open_section.contains("exit_environment_context"));
    assert!(
        report.find("funding_positive_long_macd_below_zero_hist_flat")
            < report.find("funding_positive_long_mixed_trend")
    );
    assert!(report.find("低影响观察候选") < report.find("funding_positive_long_mixed_trend"));
    assert!(!report
        .contains("| funding_volume_context | funding_positive_long_volume_normal | BTC | 3 |"));
}

fn trade_sample(
    backtest_id: i64,
    inst_id: &str,
    side: &str,
    open_time_ms: i64,
    pnl: f64,
) -> ResearchTradeSample {
    ResearchTradeSample {
        backtest_id,
        inst_id: inst_id.to_string(),
        timeframe: "4H".to_string(),
        side: side.to_string(),
        open_time_ms,
        close_time_ms: Some(open_time_ms + 4 * 60 * 60 * 1000),
        pnl,
        close_type: Some("Signal".to_string()),
        stop_loss_source: None,
        signal_value: Some("{}".to_string()),
        signal_result: Some("{}".to_string()),
    }
}

#[test]
fn summarizes_path_impact_for_missing_new_and_common_trades() {
    let baseline = vec![
        trade_sample(1428, "ETH-USDT-SWAP", "long", 1_000, 100.0),
        trade_sample(1428, "ETH-USDT-SWAP", "short", 2_000, -50.0),
    ];
    let experiment = vec![
        trade_sample(1450, "ETH-USDT-SWAP", "short", 2_000, -20.0),
        trade_sample(1450, "ETH-USDT-SWAP", "long", 3_000, -10.0),
    ];

    let summary =
        VegasFactorResearchService::summarize_path_impact(1428, 1450, &baseline, &experiment, 10);

    assert_eq!(summary.missing_count, 1);
    assert_eq!(summary.missing_pnl, 100.0);
    assert_eq!(summary.missing_wins, 1);
    assert_eq!(summary.new_count, 1);
    assert_eq!(summary.new_pnl, -10.0);
    assert_eq!(summary.new_wins, 0);
    assert_eq!(summary.common_count, 1);
    assert_eq!(summary.common_pnl_delta, 30.0);
    assert_eq!(summary.common_improved_count, 1);
    assert_eq!(summary.total_path_delta, -80.0);
    assert_eq!(summary.verdict, "path_degraded");
    assert_eq!(summary.top_changes.len(), 3);
}

#[test]
fn renders_path_impact_report_with_required_sections() {
    let summary = VegasFactorResearchService::summarize_path_impact(
        1428,
        1450,
        &[trade_sample(1428, "ETH-USDT-SWAP", "long", 1_000, 100.0)],
        &[trade_sample(1450, "ETH-USDT-SWAP", "long", 2_000, -10.0)],
        10,
    );

    let report = VegasFactorResearchService::render_path_impact_report(&[summary]);

    assert!(report.contains("路径影响评估表"));
    assert!(report.contains("Top Changed Trades"));
    assert!(report.contains("1428"));
    assert!(report.contains("1450"));
    assert!(report.contains("path_degraded"));
}
