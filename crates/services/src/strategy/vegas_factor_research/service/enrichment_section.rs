impl VegasFactorResearchService {
    /// 提供数据行to快照的集中实现，避免回测策略调用方重复处理相同细节。
    fn row_to_snapshot(row: SnapshotRow) -> ExternalMarketSnapshot {
        ExternalMarketSnapshot {
            id: Some(row.id),
            source: row.source,
            symbol: row.symbol,
            metric_type: row.metric_type,
            metric_time: row.metric_time,
            funding_rate: row.funding_rate,
            premium: row.premium,
            open_interest: row.open_interest,
            oracle_price: row.oracle_price,
            mark_price: row.mark_price,
            long_short_ratio: row.long_short_ratio,
            raw_payload: row.raw_payload.map(|value| value.0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
    /// 提供enrichsamples的集中实现，避免回测策略调用方重复处理相同细节。
    fn enrich_samples(
        &self,
        trades: Vec<ResearchTradeSample>,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Vec<EnrichedTradeSample> {
        let grouped = Self::group_snapshots(snapshots);
        trades
            .into_iter()
            .map(|trade| self.enrich_trade(trade, &grouped))
            .collect()
    }
    /// 提供enrichfiltered信号的集中实现，避免回测策略调用方重复处理相同细节。
    fn enrich_filtered_signals(
        &self,
        filtered_signals: Vec<ResearchFilteredSignalSample>,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Vec<EnrichedFilteredSignalSample> {
        let grouped = Self::group_snapshots(snapshots);
        filtered_signals
            .into_iter()
            .map(|signal| self.enrich_filtered_signal(signal, &grouped))
            .collect()
    }
    /// 提供enrich交易的集中实现，避免回测策略调用方重复处理相同细节。
    fn enrich_trade(
        &self,
        trade: ResearchTradeSample,
        grouped: &HashMap<String, Vec<ExternalMarketSnapshot>>,
    ) -> EnrichedTradeSample {
        let symbol = trade
            .inst_id
            .split('-')
            .next()
            .unwrap_or(&trade.inst_id)
            .to_string();
        let funding_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.funding_rate.is_some() || row.premium.is_some()
            })
        });
        let price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let previous_price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::previous_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let flow_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(trade.open_time_ms, rows, |row| {
                row.raw_payload
                    .as_ref()
                    .and_then(Self::extract_flow_value)
                    .is_some()
            })
        });
        let price_change = Self::pct_change(
            price_oi_snapshot.and_then(Self::snapshot_price),
            previous_price_oi_snapshot.and_then(Self::snapshot_price),
        );
        let oi_change = Self::pct_change(
            price_oi_snapshot.and_then(|row| row.open_interest),
            previous_price_oi_snapshot.and_then(|row| row.open_interest),
        );
        EnrichedTradeSample {
            tier: VolatilityTier::from_symbol(&trade.inst_id),
            funding_bucket: Self::funding_bucket(funding_snapshot),
            price_oi_state: match (price_change, oi_change) {
                (Some(_), Some(_)) => Some(Self::classify_price_oi_state(price_change, oi_change)),
                _ => None,
            },
            flow_bucket: Self::flow_bucket(flow_snapshot),
            trade,
        }
    }
    /// 提供enrichfiltered信号的集中实现，避免回测策略调用方重复处理相同细节。
    fn enrich_filtered_signal(
        &self,
        signal: ResearchFilteredSignalSample,
        grouped: &HashMap<String, Vec<ExternalMarketSnapshot>>,
    ) -> EnrichedFilteredSignalSample {
        let symbol = signal
            .inst_id
            .split('-')
            .next()
            .unwrap_or(&signal.inst_id)
            .to_string();
        let funding_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.funding_rate.is_some() || row.premium.is_some()
            })
        });
        let price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let previous_price_oi_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::previous_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.open_interest.is_some() && Self::snapshot_price(row).is_some()
            })
        });
        let flow_snapshot = grouped.get(&symbol).and_then(|rows| {
            Self::latest_matching_snapshot(signal.signal_time_ms, rows, |row| {
                row.raw_payload
                    .as_ref()
                    .and_then(Self::extract_flow_value)
                    .is_some()
            })
        });
        let price_change = Self::pct_change(
            price_oi_snapshot.and_then(Self::snapshot_price),
            previous_price_oi_snapshot.and_then(Self::snapshot_price),
        );
        let oi_change = Self::pct_change(
            price_oi_snapshot.and_then(|row| row.open_interest),
            previous_price_oi_snapshot.and_then(|row| row.open_interest),
        );
        EnrichedFilteredSignalSample {
            tier: VolatilityTier::from_symbol(&signal.inst_id),
            funding_bucket: Self::funding_bucket(funding_snapshot),
            price_oi_state: match (price_change, oi_change) {
                (Some(_), Some(_)) => Some(Self::classify_price_oi_state(price_change, oi_change)),
                _ => None,
            },
            flow_bucket: Self::flow_bucket(flow_snapshot),
            signal,
        }
    }
    /// 提供groupsnapshots的集中实现，避免回测策略调用方重复处理相同细节。
    fn group_snapshots(
        snapshots: &[ExternalMarketSnapshot],
    ) -> HashMap<String, Vec<ExternalMarketSnapshot>> {
        let mut grouped: HashMap<String, Vec<ExternalMarketSnapshot>> = HashMap::new();
        for snapshot in snapshots {
            grouped
                .entry(snapshot.symbol.clone())
                .or_default()
                .push(snapshot.clone());
        }
        grouped
    }
    /// 提供最新matching快照的集中实现，避免回测策略调用方重复处理相同细节。
    fn latest_matching_snapshot<F>(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
        predicate: F,
    ) -> Option<&ExternalMarketSnapshot>
    where
        F: Fn(&ExternalMarketSnapshot) -> bool,
    {
        snapshots
            .iter()
            .filter(|row| predicate(row))
            .filter(|row| {
                row.metric_time <= event_time && event_time - row.metric_time <= FOUR_HOURS_MS
            })
            .max_by_key(|row| row.metric_time)
    }
    /// 提供previousmatching快照的集中实现，避免回测策略调用方重复处理相同细节。
    fn previous_matching_snapshot<F>(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
        predicate: F,
    ) -> Option<&ExternalMarketSnapshot>
    where
        F: Fn(&ExternalMarketSnapshot) -> bool,
    {
        snapshots
            .iter()
            .filter(|row| predicate(row))
            .filter(|row| {
                row.metric_time < event_time && event_time - row.metric_time <= FOUR_HOURS_MS * 2
            })
            .max_by_key(|row| row.metric_time)
    }
    fn snapshot_price(snapshot: &ExternalMarketSnapshot) -> Option<f64> {
        snapshot.mark_price.or(snapshot.oracle_price)
    }
    /// 提供pctchange的集中实现，避免回测策略调用方重复处理相同细节。
    fn pct_change(current: Option<f64>, previous: Option<f64>) -> Option<f64> {
        match (current, previous) {
            (Some(now), Some(prev)) if prev.abs() > f64::EPSILON => Some((now - prev) / prev),
            _ => None,
        }
    }
    /// 提供fundingbucket的集中实现，避免回测策略调用方重复处理相同细节。
    fn funding_bucket(snapshot: Option<&ExternalMarketSnapshot>) -> Option<String> {
        match snapshot.map(|row| (row.funding_rate, row.premium)) {
            Some((Some(funding), Some(premium))) if funding > 0.0 && premium > 0.0 => {
                Some("long_crowded".to_string())
            }
            Some((Some(funding), Some(premium))) if funding < 0.0 && premium < 0.0 => {
                Some("short_crowded".to_string())
            }
            Some((Some(funding), Some(premium))) if funding < 0.0 && premium > 0.0 => {
                Some("divergent_bull".to_string())
            }
            Some((Some(funding), Some(premium))) if funding > 0.0 && premium < 0.0 => {
                Some("divergent_bear".to_string())
            }
            Some((Some(funding), _)) if funding >= 0.0 => Some("funding_positive".to_string()),
            Some((Some(_), _)) => Some("funding_negative".to_string()),
            _ => None,
        }
    }
    /// 提供flowbucket的集中实现，避免回测策略调用方重复处理相同细节。
    fn flow_bucket(snapshot: Option<&ExternalMarketSnapshot>) -> Option<String> {
        let value = snapshot
            .and_then(|row| row.raw_payload.as_ref())
            .and_then(Self::extract_flow_value);
        match value {
            Some(v) if v > 0.0 => Some("inflow".to_string()),
            Some(v) if v < 0.0 => Some("outflow".to_string()),
            _ => None,
        }
    }
}
