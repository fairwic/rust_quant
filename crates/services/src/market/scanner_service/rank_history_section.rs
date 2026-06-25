/// 构建build市场ranksnapshotsfromscan，集中维护行情数据的载荷和字段组装规则。
fn build_market_rank_snapshots_from_scan(
    current_snapshots: &[TickerSnapshot],
    current_ranks: &HashMap<String, i32>,
    captured_at: DateTime<Utc>,
) -> Vec<MarketRankSnapshot> {
    current_snapshots
        .iter()
        .filter_map(|snapshot| {
            current_ranks
                .get(&snapshot.symbol)
                .map(|rank| MarketRankSnapshot {
                    id: None,
                    exchange: "okx".to_string(),
                    symbol: snapshot.symbol.clone(),
                    rank: *rank,
                    price: snapshot.price,
                    volume_24h_quote: snapshot.volume_24h_quote,
                    captured_at,
                    created_at: captured_at,
                })
        })
        .collect()
}
/// 启动恢复只需要这些 horizon 前最近一帧，而不是 25 小时内每一帧。
fn market_rank_history_restore_targets(now: DateTime<Utc>) -> [DateTime<Utc>; 4] {
    [
        now - Duration::hours(24),
        now - Duration::hours(4),
        now - Duration::minutes(15),
        now,
    ]
}
/// 提供rankhistoryfrompersistedsnapshots的集中实现，避免行情数据调用方重复处理相同细节。
fn rank_history_from_persisted_snapshots(
    snapshots: Vec<MarketRankSnapshot>,
) -> VecDeque<RankSnapshot> {
    let mut grouped: BTreeMap<DateTime<Utc>, RankSnapshot> = BTreeMap::new();
    for snapshot in snapshots {
        let entry = grouped
            .entry(snapshot.captured_at)
            .or_insert_with(|| RankSnapshot {
                timestamp: snapshot.captured_at,
                ranks: HashMap::new(),
                prices: HashMap::new(),
            });
        entry.ranks.insert(snapshot.symbol.clone(), snapshot.rank);
        entry.prices.insert(snapshot.symbol, snapshot.price);
    }
    grouped.into_values().collect()
}
