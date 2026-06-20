impl VegasFactorResearchService {
    const SUPPORTED_FACTORS: [&'static str; 9] = [
        "exit_environment_context",
        "flow_proxy",
        "funding_direction_context",
        "funding_filter_context",
        "funding_macd_context",
        "funding_premium_divergence",
        "funding_trend_context",
        "funding_volume_context",
        "price_oi_state",
    ];

    fn build_bucket_reports(
        &self,
        traded_samples: &[EnrichedTradeSample],
        filtered_samples: &[EnrichedFilteredSignalSample],
    ) -> Vec<FactorBucketReport> {
        let mut grouped: HashMap<(String, String, VolatilityTier, String), Vec<f64>> =
            HashMap::new();
        for sample in traded_samples {
            let scope_label = Self::bucket_scope_label(&sample.trade.inst_id, sample.tier);
            if let Some(bucket_name) = &sample.funding_bucket {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Traded.label(),
                            "funding_premium_divergence"
                        ),
                        bucket_name.clone(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_signal_contexts(
                sample.funding_bucket.as_deref(),
                &sample.trade.side,
                sample.trade.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            for (factor_name, bucket_name) in Self::classify_internal_exit_contexts(
                &sample.trade.side,
                sample.trade.close_type.as_deref(),
                sample.trade.stop_loss_source.as_deref(),
                sample.trade.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            if let Some(price_oi_state) = sample.price_oi_state {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Traded.label(),
                            "price_oi_state"
                        ),
                        price_oi_state.label().to_string(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
            if let Some(bucket_name) = &sample.flow_bucket {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Traded.label(), "flow_proxy"),
                        bucket_name.clone(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(sample.trade.pnl);
            }
        }
        for sample in filtered_samples {
            let pnl = sample.signal.theoretical_pnl.unwrap_or_default();
            let scope_label = Self::bucket_scope_label(&sample.signal.inst_id, sample.tier);
            if let Some(bucket_name) = &sample.funding_bucket {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Filtered.label(),
                            "funding_premium_divergence"
                        ),
                        bucket_name.clone(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_signal_contexts(
                sample.funding_bucket.as_deref(),
                &sample.signal.direction,
                sample.signal.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            for (factor_name, bucket_name) in Self::classify_funding_filter_contexts(
                sample.funding_bucket.as_deref(),
                &sample.signal.direction,
                sample.signal.filter_reasons.as_deref(),
                sample.signal.signal_value.as_deref(),
            ) {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), factor_name),
                        bucket_name,
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            if let Some(price_oi_state) = sample.price_oi_state {
                grouped
                    .entry((
                        format!(
                            "{}::{}",
                            ResearchSampleKind::Filtered.label(),
                            "price_oi_state"
                        ),
                        price_oi_state.label().to_string(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
            if let Some(bucket_name) = &sample.flow_bucket {
                grouped
                    .entry((
                        format!("{}::{}", ResearchSampleKind::Filtered.label(), "flow_proxy"),
                        bucket_name.clone(),
                        sample.tier,
                        scope_label.clone(),
                    ))
                    .or_default()
                    .push(pnl);
            }
        }

        let mut rows = Vec::new();
        for ((factor_name, bucket_name, tier, scope_label), pnls) in grouped {
            rows.push(Self::build_bucket_row(
                factor_name,
                bucket_name,
                tier,
                scope_label,
                &pnls,
            ));
        }

        let mut by_bucket: HashMap<(String, String, VolatilityTier, String), Vec<usize>> =
            HashMap::new();
        for (idx, row) in rows.iter().enumerate() {
            by_bucket
                .entry((
                    row.factor_name.clone(),
                    row.bucket_name.clone(),
                    row.volatility_tier,
                    row.scope_label.clone(),
                ))
                .or_default()
                .push(idx);
        }
        for indexes in by_bucket.values() {
            let clone_rows: Vec<_> = indexes.iter().map(|idx| rows[*idx].clone()).collect();
            let conclusion = Self::evaluate_factor_conclusion(&clone_rows);
            for idx in indexes {
                rows[*idx].conclusion = conclusion;
            }
        }

        for sample_kind in [ResearchSampleKind::Traded, ResearchSampleKind::Filtered] {
            for factor_name in Self::SUPPORTED_FACTORS {
                if rows
                    .iter()
                    .any(|row| row.factor_name == factor_name && row.sample_kind == sample_kind)
                {
                    continue;
                }
                for tier in [
                    VolatilityTier::Btc,
                    VolatilityTier::Eth,
                    VolatilityTier::Alt,
                ] {
                    rows.push(FactorBucketReport {
                        factor_name: factor_name.to_string(),
                        bucket_name: "no_data".to_string(),
                        sample_kind,
                        volatility_tier: tier,
                        scope_label: tier.label().to_string(),
                        sample_count: 0,
                        win_rate: 0.0,
                        avg_pnl: 0.0,
                        sharpe_proxy: 0.0,
                        avg_mfe: 0.0,
                        avg_mae: 0.0,
                        conclusion: FactorConclusion::Reject,
                    });
                }
            }
        }

        rows.sort_by(|left, right| {
            left.factor_name
                .cmp(&right.factor_name)
                .then(left.bucket_name.cmp(&right.bucket_name))
                .then(
                    left.volatility_tier
                        .label()
                        .cmp(right.volatility_tier.label()),
                )
                .then(left.scope_label.cmp(&right.scope_label))
        });
        rows
    }

    fn build_bucket_row(
        factor_name: String,
        bucket_name: String,
        tier: VolatilityTier,
        scope_label: String,
        pnls: &[f64],
    ) -> FactorBucketReport {
        let (sample_kind, clean_factor_name) =
            if let Some((kind_label, name)) = factor_name.split_once("::") {
                let kind = if kind_label == ResearchSampleKind::Filtered.label() {
                    ResearchSampleKind::Filtered
                } else {
                    ResearchSampleKind::Traded
                };
                (kind, name.to_string())
            } else {
                (ResearchSampleKind::Traded, factor_name)
            };
        let sample_count = pnls.len();
        let avg_pnl = if sample_count == 0 {
            0.0
        } else {
            pnls.iter().sum::<f64>() / sample_count as f64
        };
        let variance = if sample_count <= 1 {
            0.0
        } else {
            pnls.iter().map(|row| (row - avg_pnl).powi(2)).sum::<f64>() / sample_count as f64
        };
        let std_dev = variance.sqrt();
        let sharpe_proxy = if std_dev > 0.0 {
            avg_pnl / std_dev
        } else if avg_pnl > 0.0 {
            avg_pnl
        } else {
            0.0
        };

        FactorBucketReport {
            factor_name: clean_factor_name,
            bucket_name,
            sample_kind,
            volatility_tier: tier,
            scope_label,
            sample_count,
            win_rate: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().filter(|row| **row > 0.0).count() as f64 / sample_count as f64
            },
            avg_pnl,
            sharpe_proxy,
            avg_mfe: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().copied().filter(|row| *row > 0.0).sum::<f64>() / sample_count as f64
            },
            avg_mae: if sample_count == 0 {
                0.0
            } else {
                pnls.iter().copied().filter(|row| *row < 0.0).sum::<f64>() / sample_count as f64
            },
            conclusion: FactorConclusion::Observe,
        }
    }

    fn bucket_scope_label(inst_id: &str, tier: VolatilityTier) -> String {
        match tier {
            VolatilityTier::Alt => inst_id.split('-').next().unwrap_or(inst_id).to_string(),
            _ => tier.label().to_string(),
        }
    }}
