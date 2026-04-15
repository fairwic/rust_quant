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
fn evaluates_factor_conclusion_using_eth_first_gate() {
    let eligible = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "divergent_bull".to_string(),
        sample_kind: ResearchSampleKind::Traded,
        volatility_tier: VolatilityTier::Eth,
        sample_count: 6,
        win_rate: 0.66,
        avg_pnl: 42.0,
        sharpe_proxy: 1.35,
        avg_mfe: 88.0,
        avg_mae: -19.0,
        conclusion: FactorConclusion::Observe,
    };
    assert_eq!(
        VegasFactorResearchService::evaluate_factor_conclusion(&[eligible]),
        FactorConclusion::Candidate
    );

    let rejected = FactorBucketReport {
        factor_name: "funding_premium_divergence".to_string(),
        bucket_name: "divergent_bear".to_string(),
        sample_kind: ResearchSampleKind::Traded,
        volatility_tier: VolatilityTier::Eth,
        sample_count: 2,
        win_rate: 0.4,
        avg_pnl: -11.0,
        sharpe_proxy: -0.3,
        avg_mfe: 15.0,
        avg_mae: -32.0,
        conclusion: FactorConclusion::Observe,
    };
    assert_eq!(
        VegasFactorResearchService::evaluate_factor_conclusion(&[rejected]),
        FactorConclusion::Reject
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
    let buckets = vec![FactorBucketReport {
        factor_name: "price_oi_state".to_string(),
        bucket_name: "short_covering".to_string(),
        sample_kind: ResearchSampleKind::Traded,
        volatility_tier: VolatilityTier::Eth,
        sample_count: 4,
        win_rate: 0.75,
        avg_pnl: 23.0,
        sharpe_proxy: 1.2,
        avg_mfe: 51.0,
        avg_mae: -12.0,
        conclusion: FactorConclusion::Candidate,
    }];

    let report = VegasFactorResearchService::render_report(&trades, &filtered_signals, &buckets);

    assert!(report.contains("因子概览表"));
    assert!(report.contains("分桶统计表"));
    assert!(report.contains("BTC / ETH / 其他币种"));
    assert!(report.contains("可回注"));
    assert!(report.contains("过滤候选样本数"));
    assert!(report.contains("已成交样本"));
    assert!(report.contains("过滤候选"));
}
