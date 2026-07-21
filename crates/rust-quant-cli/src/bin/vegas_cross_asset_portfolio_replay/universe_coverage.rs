use super::Args;
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{PgPool, Row};

/// 本地数据是否足以重建当前 live 加密币 Top100 的证据报告。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct UniverseCoverageReport {
    pub(super) universe_rule: &'static str,
    pub(super) current_live_okx_usdt_swaps: usize,
    pub(super) current_eligible_okx_usdt_swaps: usize,
    pub(super) current_eligible_symbols_with_4h_table: usize,
    pub(super) local_usdt_swap_4h_tables: usize,
    pub(super) current_symbol_table_coverage_pct: f64,
    pub(super) backtest_configs: usize,
    pub(super) backtest_configs_with_4h_table: usize,
    pub(super) backtest_configs_with_listing_metadata: usize,
    pub(super) earliest_candidate_open_ts: Option<i64>,
    pub(super) earliest_local_listing_snapshot_ts: Option<i64>,
    pub(super) first_seen_snapshot_rows: usize,
    pub(super) non_live_listing_snapshot_rows: usize,
    pub(super) delisted_symbols_excluded: bool,
    pub(super) universe_limitation: &'static str,
    /// 当前 live-only 合约与 4H 表是否完整；该门禁不证明历史时点成员关系。
    pub(super) current_live_universe_gate_pass: bool,
    /// 是否能在每个历史回测时点重建当时可交易币种池，而非套用今天的 live 列表。
    pub(super) historical_universe_gate_pass: bool,
    pub(super) blockers: Vec<&'static str>,
}

/// 只读核对 K 线表、当前交易能力元数据和 first-seen 快照的覆盖边界。
pub(super) async fn load_universe_coverage(
    pool: &PgPool,
    args: Args,
) -> Result<UniverseCoverageReport> {
    let counts = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(DISTINCT normalized_symbol)
               FROM exchange_symbols
              WHERE exchange = 'okx'
                AND market_type = 'perpetual'
                AND lower(status) IN ('trading', 'live')
                AND normalized_symbol LIKE '%-USDT-SWAP'
                AND raw_payload->>'instCategory' = '1') AS live_symbols,
            (SELECT COUNT(DISTINCT normalized_symbol)
               FROM exchange_symbols
              WHERE exchange = 'okx'
                AND market_type = 'perpetual'
                AND lower(status) IN ('trading', 'live')
                AND normalized_symbol LIKE '%-USDT-SWAP'
                AND raw_payload->>'instCategory' = '1'
                AND NULLIF(raw_payload->>'listTime', '')::bigint
                    <= (EXTRACT(EPOCH FROM now() - interval '150 days') * 1000)::bigint
            ) AS eligible_symbols,
            (SELECT COUNT(DISTINCT s.normalized_symbol)
               FROM exchange_symbols s
               JOIN information_schema.tables t
                 ON t.table_schema = 'public'
                AND t.table_name = lower(s.normalized_symbol) || '_candles_4h'
              WHERE s.exchange = 'okx'
                AND s.market_type = 'perpetual'
                AND lower(s.status) IN ('trading', 'live')
                AND s.normalized_symbol LIKE '%-USDT-SWAP'
                AND s.raw_payload->>'instCategory' = '1'
                AND NULLIF(s.raw_payload->>'listTime', '')::bigint
                    <= (EXTRACT(EPOCH FROM now() - interval '150 days') * 1000)::bigint
            ) AS eligible_symbols_with_table,
            (SELECT COUNT(*)
               FROM information_schema.tables
              WHERE table_schema = 'public'
                AND table_name LIKE '%-usdt-swap_candles_4h') AS candle_tables,
            (SELECT COUNT(*)
               FROM back_test_log
              WHERE id BETWEEN $1 AND $2) AS backtest_configs,
            (SELECT COUNT(*)
               FROM back_test_log b
               JOIN information_schema.tables t
                 ON t.table_schema = 'public'
                AND t.table_name = lower(b.inst_type) || '_candles_4h'
              WHERE b.id BETWEEN $1 AND $2
            ) AS configs_with_table,
            (SELECT COUNT(*)
               FROM back_test_log b
              WHERE b.id BETWEEN $1 AND $2
                AND EXISTS (
                    SELECT 1
                      FROM exchange_symbols s
                     WHERE s.exchange = 'okx'
                       AND s.market_type = 'perpetual'
                       AND s.normalized_symbol = b.inst_type
                       AND NULLIF(s.raw_payload->>'listTime', '') IS NOT NULL
                       AND NULLIF(s.min_qty, '') IS NOT NULL
                       AND NULLIF(s.tick_size, '') IS NOT NULL
                )) AS configs_with_metadata,
            (SELECT MIN((EXTRACT(EPOCH FROM (
                    d.open_position_time AT TIME ZONE 'Asia/Shanghai'
                )) * 1000)::bigint)
               FROM back_test_detail d
              WHERE d.back_test_id BETWEEN $1 AND $2
                AND d.option_type IN ('long', 'short')) AS earliest_open_ts,
            (SELECT (EXTRACT(EPOCH FROM MIN(first_seen_at)) * 1000)::bigint
               FROM exchange_symbol_listing_events
              WHERE exchange = 'okx'
                AND market_type = 'perpetual') AS earliest_snapshot_ts,
            (SELECT COUNT(*)
               FROM exchange_symbol_listing_events
              WHERE exchange = 'okx'
                AND market_type = 'perpetual') AS snapshot_rows,
            (SELECT COUNT(*)
               FROM exchange_symbol_listing_events
              WHERE exchange = 'okx'
                AND market_type = 'perpetual'
                AND status NOT IN ('live', 'preopen')) AS non_live_snapshot_rows
        "#,
    )
    .bind(args.backtest_id_min)
    .bind(args.backtest_id_max)
    .fetch_one(pool)
    .await
    .context("audit historical universe data coverage")?;
    let live_symbols = usize::try_from(counts.try_get::<i64, _>("live_symbols")?)?;
    let eligible_symbols = usize::try_from(counts.try_get::<i64, _>("eligible_symbols")?)?;
    let eligible_symbols_with_table =
        usize::try_from(counts.try_get::<i64, _>("eligible_symbols_with_table")?)?;
    let candle_tables = usize::try_from(counts.try_get::<i64, _>("candle_tables")?)?;
    let backtest_configs = usize::try_from(counts.try_get::<i64, _>("backtest_configs")?)?;
    let configs_with_table = usize::try_from(counts.try_get::<i64, _>("configs_with_table")?)?;
    let configs_with_metadata =
        usize::try_from(counts.try_get::<i64, _>("configs_with_metadata")?)?;
    let earliest_open_ts = counts.try_get::<Option<i64>, _>("earliest_open_ts")?;
    let earliest_snapshot_ts = counts.try_get::<Option<i64>, _>("earliest_snapshot_ts")?;
    let snapshot_rows = usize::try_from(counts.try_get::<i64, _>("snapshot_rows")?)?;
    let non_live_snapshot_rows =
        usize::try_from(counts.try_get::<i64, _>("non_live_snapshot_rows")?)?;
    let complete_current_candles =
        eligible_symbols > 0 && eligible_symbols_with_table == eligible_symbols;
    let backtest_evidence_complete = backtest_configs > 0
        && configs_with_table == backtest_configs
        && configs_with_metadata == backtest_configs;
    let current_live_gate_pass = live_only_gate_pass(
        eligible_symbols,
        eligible_symbols_with_table,
        backtest_configs,
        configs_with_table,
        configs_with_metadata,
    );
    let mut blockers = Vec::new();
    if !complete_current_candles {
        blockers.push(
            "本地 4H K 线未逐币覆盖当前已上市满 150 天的 OKX USDT 永续，不能可靠重建历史 Top100",
        );
    }
    if !backtest_evidence_complete {
        blockers.push("本批回测配置缺少 K 线表、上市时间或交易精度元数据");
    }
    blockers.push("本地上市快照晚于回测起点且显式排除退市币，不能重建 point-in-time 全市场币种池");
    Ok(UniverseCoverageReport {
        universe_rule:
            "okx_current_live_crypto_usdt_swap_monthly_prior30_complete_utc_day_median_quote_volume_top100_age150d_exclude_delisted",
        current_live_okx_usdt_swaps: live_symbols,
        current_eligible_okx_usdt_swaps: eligible_symbols,
        current_eligible_symbols_with_4h_table: eligible_symbols_with_table,
        local_usdt_swap_4h_tables: candle_tables,
        current_symbol_table_coverage_pct: coverage_pct(
            eligible_symbols_with_table,
            eligible_symbols,
        ),
        backtest_configs,
        backtest_configs_with_4h_table: configs_with_table,
        backtest_configs_with_listing_metadata: configs_with_metadata,
        earliest_candidate_open_ts: earliest_open_ts,
        earliest_local_listing_snapshot_ts: earliest_snapshot_ts,
        first_seen_snapshot_rows: snapshot_rows,
        non_live_listing_snapshot_rows: non_live_snapshot_rows,
        delisted_symbols_excluded: true,
        universe_limitation:
            "按研究边界显式排除退市币；结果只代表当前仍 live 的合约，包含幸存者偏差，不得外推为交易所完整历史币池",
        current_live_universe_gate_pass: current_live_gate_pass,
        historical_universe_gate_pass: false,
        blockers,
    })
}

fn coverage_pct(covered: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        covered as f64 / total as f64 * 100.0
    }
}

fn live_only_gate_pass(
    eligible_symbols: usize,
    eligible_symbols_with_table: usize,
    backtest_configs: usize,
    configs_with_table: usize,
    configs_with_metadata: usize,
) -> bool {
    eligible_symbols > 0
        && eligible_symbols_with_table == eligible_symbols
        && backtest_configs > 0
        && configs_with_table == backtest_configs
        && configs_with_metadata == backtest_configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_percentage_does_not_overstate_missing_tables() {
        assert!((coverage_pct(129, 305) - 42.295_081_967_213_115).abs() < 1e-12);
        assert_eq!(coverage_pct(0, 0), 0.0);
    }

    #[test]
    fn live_only_gate_does_not_require_delisted_snapshots() {
        assert!(live_only_gate_pass(251, 251, 251, 251, 251));
        assert!(!live_only_gate_pass(251, 250, 251, 251, 251));
        assert!(!live_only_gate_pass(251, 251, 251, 250, 251));
    }
}
