use super::MarketVelocityEventBacktestArgs;
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;

/// 回测时点可见的单月成员集合；`to_ms` 为不包含上界。
#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoricalUniverseWindow {
    from_ms: i64,
    to_ms: i64,
    members: BTreeSet<String>,
}

/// 从版本化 manifest 解析出的 point-in-time 币池日程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HistoricalUniverseSchedule {
    pub(super) version: String,
    windows: Vec<HistoricalUniverseWindow>,
}

impl HistoricalUniverseSchedule {
    /// 未指定 manifest 时返回空；指定后严格校验数据域和请求窗口覆盖。
    pub(super) fn from_args(args: &MarketVelocityEventBacktestArgs) -> Result<Option<Self>> {
        let Some(path) = &args.historical_universe_manifest else {
            return Ok(None);
        };
        let manifest: HistoricalUniverseManifest = serde_json::from_slice(
            &std::fs::read(path)
                .with_context(|| format!("read historical universe {}", path.display()))?,
        )
        .with_context(|| format!("decode historical universe {}", path.display()))?;
        let schedule = Self::from_manifest(manifest)?;
        schedule.validate_requested_window(
            args.event_start_ms
                .context("missing historical event start")?,
            args.event_end_ms.context("missing historical event end")?,
        )?;
        Ok(Some(schedule))
    }

    /// 返回 manifest 中所有月份成员的并集，用于一次性加载所需 K 线表。
    pub(super) fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// 返回事件时点所属窗口索引和成员；窗口外时不允许生成排名或信号。
    pub(super) fn members_at(&self, event_ts: i64) -> Option<(usize, &BTreeSet<String>)> {
        self.windows
            .iter()
            .enumerate()
            .find(|(_, window)| event_ts >= window.from_ms && event_ts < window.to_ms)
            .map(|(index, window)| (index, &window.members))
    }

    /// 判断 symbol 在信号时点是否属于预先冻结的当月币池。
    pub(super) fn allows(&self, symbol: &str, event_ts: i64) -> bool {
        self.members_at(event_ts)
            .is_some_and(|(_, members)| members.contains(symbol))
    }

    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
        {
            bail!("historical universe must be schema v1 OKX USDT perpetual_swap 15m");
        }
        if !manifest
            .selection_rule
            .starts_with("current-live OKX USDT swaps only")
        {
            bail!("historical universe must exclude non-current/delisted instruments");
        }
        if manifest.universe_version.trim().is_empty() || manifest.months.is_empty() {
            bail!("historical universe requires a version and at least one month");
        }
        let mut windows = manifest
            .months
            .into_iter()
            .map(|month| {
                if month.effective_from_ms >= month.effective_to_ms || month.members.is_empty() {
                    bail!("historical universe month has invalid bounds or no members");
                }
                let members = month
                    .members
                    .into_iter()
                    .map(|member| member.symbol.trim().to_ascii_uppercase())
                    .collect::<BTreeSet<_>>();
                if members.is_empty()
                    || members.iter().any(|symbol| {
                        !symbol.ends_with("-USDT-SWAP")
                            || !symbol.bytes().all(|byte| {
                                byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-'
                            })
                    })
                {
                    bail!("historical universe contains an invalid USDT swap symbol");
                }
                Ok(HistoricalUniverseWindow {
                    from_ms: month.effective_from_ms,
                    to_ms: month.effective_to_ms,
                    members,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        windows.sort_by_key(|window| window.from_ms);
        if windows
            .windows(2)
            .any(|pair| pair[1].from_ms < pair[0].to_ms)
        {
            bail!("historical universe months overlap");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    /// 请求窗口必须由 manifest 连续覆盖，不能在缺失月份上回退到当前表集合。
    fn validate_requested_window(&self, start_ms: i64, end_ms: i64) -> Result<()> {
        let mut covered_until = start_ms;
        for window in self
            .windows
            .iter()
            .filter(|window| window.to_ms > start_ms && window.from_ms <= end_ms)
        {
            if window.from_ms > covered_until {
                bail!("historical universe has a gap inside the requested event window");
            }
            covered_until = covered_until.max(window.to_ms);
            if covered_until > end_ms {
                return Ok(());
            }
        }
        bail!("historical universe does not cover the requested event window")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::okx_historical_universe::{
        HistoricalUniverseMember, HistoricalUniverseMonth, HistoricalUniverseSource,
    };

    fn manifest(months: Vec<HistoricalUniverseMonth>) -> HistoricalUniverseManifest {
        HistoricalUniverseManifest {
            schema_version: 1,
            universe_version: "fixture_v1".to_string(),
            generated_at_ms: 0,
            exchange: "okx".to_string(),
            market_type: "perpetual_swap".to_string(),
            quote_currency: "USDT".to_string(),
            timeframe: "15m".to_string(),
            selection_rule: "current-live OKX USDT swaps only; fixture".to_string(),
            source: HistoricalUniverseSource {
                instruments_endpoint: String::new(),
                download_link_endpoint: String::new(),
                candlestick_archive_format: String::new(),
                stock_perpetual_first_live_ms: 0,
                classification_boundary: String::new(),
            },
            months,
        }
    }

    fn month(from_ms: i64, to_ms: i64, symbols: &[&str]) -> HistoricalUniverseMonth {
        HistoricalUniverseMonth {
            effective_from_ms: from_ms,
            effective_to_ms: to_ms,
            ranking_source_month: "fixture".to_string(),
            archive_candidate_families: symbols.len(),
            archive_files_available: symbols.len(),
            complete_candidates: symbols.len(),
            members: symbols
                .iter()
                .map(|symbol| HistoricalUniverseMember {
                    symbol: (*symbol).to_string(),
                    median_daily_quote_volume: 1.0,
                    source_url: String::new(),
                    source_sha256: String::new(),
                    source_rows: 1,
                    source_first_ts: 0,
                    source_last_ts: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn switches_members_only_at_effective_boundary() {
        let schedule = HistoricalUniverseSchedule::from_manifest(manifest(vec![
            month(100, 200, &["BTC-USDT-SWAP", "ETH-USDT-SWAP"]),
            month(200, 300, &["BTC-USDT-SWAP", "SOL-USDT-SWAP"]),
        ]))
        .unwrap();

        assert!(schedule.allows("ETH-USDT-SWAP", 199));
        assert!(!schedule.allows("ETH-USDT-SWAP", 200));
        assert!(schedule.allows("SOL-USDT-SWAP", 200));
        assert_eq!(schedule.union_symbols().len(), 3);
    }

    #[test]
    fn rejects_requested_window_with_manifest_gap() {
        let schedule = HistoricalUniverseSchedule::from_manifest(manifest(vec![
            month(100, 200, &["BTC-USDT-SWAP"]),
            month(250, 300, &["BTC-USDT-SWAP"]),
        ]))
        .unwrap();

        assert!(schedule.validate_requested_window(150, 275).is_err());
    }
}
