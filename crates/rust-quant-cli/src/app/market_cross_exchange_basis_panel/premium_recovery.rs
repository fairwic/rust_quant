use super::binance_klines::{load_binance_premium_index, BinanceKlineAudit, BinancePremiumCandle};
use super::*;

const RETURN_6H_BARS: usize = 6 * 4;
const RETURN_24H_BARS: usize = 24 * 4;
const PREMIUM_RECOVERY_BARS: usize = 4;
const PREMIUM_DISCOUNT: f64 = -0.0005;
const CANDIDATES_PER_POINT: usize = 2;
const PREMIUM_RULE_VERSION: &str = "top2_down_impulse_premium_discount_5bps_1h_recovery_4h_v1";

/// 记录价格候选、premium 完整性、确认选择与 outcome 漏斗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PremiumRecoveryStages {
    /// 十二个月内的 UTC 4 小时决策时点数。
    pub decision_points: usize,
    /// 完整 6h/24h 价格因子低于当月成员 80% 的时点数。
    pub price_coverage_blocked: usize,
    /// 未阻塞时点成功计算的完整价格观察数。
    pub price_observations: usize,
    /// 每个时点冻结的最弱两个下跌候选总数。
    pub selected_price_candidates: usize,
    /// 前两个候选中具有连续 1h premium 的观察数。
    pub premium_available: usize,
    /// 至少一个候选通过折价修复后选出的确认时点数。
    pub confirmed_selected: usize,
    /// 无确认候选时选出的 premium 完整对照时点数。
    pub control_selected: usize,
    /// 缺少下一根开盘或完整 4h outcome 的选中项数。
    pub incomplete_outcomes: usize,
}

/// 一个预注册分组的 1h/4h 多头毛收益与命中率。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PremiumRecoverySummary {
    /// 当前分组有效观察数。
    pub observations: usize,
    /// 下一根开盘至 1h 的平均多头收益。
    pub mean_forward_1h: Option<f64>,
    /// 下一根开盘至 4h 的平均多头收益。
    pub mean_forward_4h: Option<f64>,
    /// 1h 多头收益为正的比例，单位百分比。
    pub positive_rate_1h_pct: Option<f64>,
    /// 4h 多头收益为正的比例，单位百分比。
    pub positive_rate_4h_pct: Option<f64>,
}

/// premium 折价修复因子面板的数据、稳定性和事件报告。
#[derive(Debug, Clone, PartialEq)]
pub struct PremiumRecoveryPanelReport {
    /// 冻结因子与选择规则身份。
    pub rule_version: String,
    /// 历史币池版本。
    pub universe_version: String,
    /// OKX 币池唯一成员数。
    pub okx_symbols: usize,
    /// Binance premium 当前映射与官方文件审计。
    pub premium_audit: BinanceKlineAudit,
    /// 价格候选到 outcome 的因果漏斗。
    pub stages: PremiumRecoveryStages,
    /// confirmed 触发时间按 4h 聚类后的有效事件数。
    pub effective_events_4h: usize,
    /// 全窗口折价修复确认组。
    pub confirmed_overall: PremiumRecoverySummary,
    /// 全窗口无确认对照组。
    pub control_overall: PremiumRecoverySummary,
    /// 前六个月确认组。
    pub confirmed_discovery: PremiumRecoverySummary,
    /// 前六个月对照组。
    pub control_discovery: PremiumRecoverySummary,
    /// 后六个月确认组。
    pub confirmed_validation: PremiumRecoverySummary,
    /// 后六个月对照组。
    pub control_validation: PremiumRecoverySummary,
    /// 每个历史币池月份的确认组结果。
    pub monthly_confirmed: Vec<(i64, PremiumRecoverySummary)>,
    /// 是否通过全部预注册边际价值与频率门槛。
    pub factor_gate_passed: bool,
}

/// 信号时点冻结的最弱价格候选。
#[derive(Debug, Clone, PartialEq)]
struct PriceCandidate {
    /// OKX 合约标识。
    symbol: String,
    /// 因子决策时间。
    decision_ts: i64,
    /// 最近 6h 收益，用于冻结排序。
    return_6h: f64,
}

/// 绑定 premium 确认状态与固定期限多头 outcome。
#[derive(Debug, Clone, PartialEq)]
struct PremiumObservation {
    /// OKX 合约标识。
    symbol: String,
    /// 因子决策时间。
    decision_ts: i64,
    /// 是否满足至少 5bps 折价且 1h 修复。
    confirmed: bool,
    /// 决策时 premium close。
    premium_current: f64,
    /// 决策前一小时 premium close。
    premium_one_hour_ago: f64,
    /// 下一根开盘至 1h 的多头收益。
    forward_1h: f64,
    /// 下一根开盘至 4h 的多头收益。
    forward_4h: f64,
}

/// 运行冻结 premium 折价修复面板，不写交易事实或触发执行。
pub async fn run_premium_discount_recovery_panel(
    args: &CrossExchangeBasisPanelArgs,
    database_url: &str,
) -> Result<PremiumRecoveryPanelReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode premium recovery universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let first = schedule
        .windows
        .first()
        .context("missing first universe window")?;
    let last = schedule
        .windows
        .last()
        .context("missing last universe window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for premium recovery panel")?;
    let mut okx = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        okx.insert(
            symbol.clone(),
            load_okx_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(2 * DAY_MS),
                last.to_ms.saturating_add(DAY_MS),
            )
            .await?,
        );
    }
    let (premium, premium_audit) = load_binance_premium_index(args, &schedule).await?;
    let (observations, stages) = build_observations(&schedule, &okx, &premium);
    let report = build_report(&schedule, okx.len(), premium_audit, stages, &observations);
    print_report(&report);
    Ok(report)
}

/// 每个 UTC 4 小时时点冻结最弱两个下跌候选，再绑定 premium 确认。
fn build_observations(
    schedule: &UniverseSchedule,
    okx: &BTreeMap<String, Vec<CandleItem>>,
    premium: &BTreeMap<String, Vec<BinancePremiumCandle>>,
) -> (Vec<PremiumObservation>, PremiumRecoveryStages) {
    let mut stages = PremiumRecoveryStages::default();
    let mut observations = Vec::new();
    let Some(first) = schedule.windows.first() else {
        return (observations, stages);
    };
    let Some(last) = schedule.windows.last() else {
        return (observations, stages);
    };
    let mut decision_ts = first.from_ms;
    while decision_ts < last.to_ms {
        let Some(window) = schedule.window_at(decision_ts) else {
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        };
        stages.decision_points += 1;
        let minimum = (window.members.len() as f64 * MIN_FACTOR_COVERAGE).ceil() as usize;
        let mut complete = 0usize;
        let mut price_candidates = Vec::<PriceCandidate>::new();
        for symbol in &window.members {
            let Some(candles) = okx.get(symbol) else {
                continue;
            };
            let Some((return_6h, return_24h)) =
                price_returns_at(candles, decision_ts.saturating_sub(MS_15M))
            else {
                continue;
            };
            complete += 1;
            if return_6h < 0.0 && return_24h < 0.0 {
                price_candidates.push(PriceCandidate {
                    symbol: symbol.clone(),
                    decision_ts,
                    return_6h,
                });
            }
        }
        if minimum == 0 || complete < minimum {
            stages.price_coverage_blocked += 1;
            decision_ts = decision_ts.saturating_add(MS_4H);
            continue;
        }
        stages.price_observations += complete;
        price_candidates.sort_by(|left, right| {
            left.return_6h
                .total_cmp(&right.return_6h)
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        price_candidates.truncate(CANDIDATES_PER_POINT);
        stages.selected_price_candidates += price_candidates.len();
        let mut premium_candidates = Vec::<(PriceCandidate, f64, f64, bool)>::new();
        for candidate in price_candidates {
            let Some(values) = premium.get(&candidate.symbol) else {
                continue;
            };
            let Some((current, one_hour_ago)) =
                premium_at(values, decision_ts.saturating_sub(MS_15M))
            else {
                continue;
            };
            stages.premium_available += 1;
            let confirmed = current <= PREMIUM_DISCOUNT && current > one_hour_ago;
            premium_candidates.push((candidate, current, one_hour_ago, confirmed));
        }
        let selected = premium_candidates
            .iter()
            .find(|(_, _, _, confirmed)| *confirmed)
            .or_else(|| premium_candidates.first());
        if let Some((candidate, current, one_hour_ago, confirmed)) = selected {
            if *confirmed {
                stages.confirmed_selected += 1;
            } else {
                stages.control_selected += 1;
            }
            if let Some((forward_1h, forward_4h)) =
                long_forward_returns(&okx[&candidate.symbol], candidate.decision_ts)
            {
                observations.push(PremiumObservation {
                    symbol: candidate.symbol.clone(),
                    decision_ts: candidate.decision_ts,
                    confirmed: *confirmed,
                    premium_current: *current,
                    premium_one_hour_ago: *one_hour_ago,
                    forward_1h,
                    forward_4h,
                });
            } else {
                stages.incomplete_outcomes += 1;
            }
        }
        decision_ts = decision_ts.saturating_add(MS_4H);
    }
    (observations, stages)
}

/// 只用连续 24h 前缀返回当前完成 K 线的 6h 与 24h 收益。
fn price_returns_at(candles: &[CandleItem], decision_candle_ts: i64) -> Option<(f64, f64)> {
    let index = candles
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    if index < RETURN_24H_BARS
        || candles[index].ts - candles[index - RETURN_24H_BARS].ts
            != RETURN_24H_BARS as i64 * MS_15M
        || candles[index].ts - candles[index - RETURN_6H_BARS].ts != RETURN_6H_BARS as i64 * MS_15M
    {
        return None;
    }
    let return_6h = candles[index].c / candles[index - RETURN_6H_BARS].c - 1.0;
    let return_24h = candles[index].c / candles[index - RETURN_24H_BARS].c - 1.0;
    (return_6h.is_finite() && return_24h.is_finite()).then_some((return_6h, return_24h))
}

/// 读取连续五个 premium close，并返回当前与一小时前的值。
fn premium_at(candles: &[BinancePremiumCandle], decision_candle_ts: i64) -> Option<(f64, f64)> {
    let index = candles
        .binary_search_by_key(&decision_candle_ts, |candle| candle.ts)
        .ok()?;
    if index < PREMIUM_RECOVERY_BARS {
        return None;
    }
    let start = index - PREMIUM_RECOVERY_BARS;
    if candles[start..=index]
        .windows(2)
        .any(|pair| pair[0].ts.saturating_add(MS_15M) != pair[1].ts)
    {
        return None;
    }
    let current = candles[index].close;
    let one_hour_ago = candles[start].close;
    (current.is_finite() && one_hour_ago.is_finite()).then_some((current, one_hour_ago))
}

/// 从下一根 15m 开盘计算固定 1h 与 4h 多头收益。
fn long_forward_returns(candles: &[CandleItem], decision_ts: i64) -> Option<(f64, f64)> {
    let entry_index = candles
        .binary_search_by_key(&decision_ts, |candle| candle.ts)
        .ok()?;
    let exit_1h_index = entry_index.checked_add(FORWARD_1H_BARS - 1)?;
    let exit_4h_index = entry_index.checked_add(FORWARD_4H_BARS - 1)?;
    let entry = candles.get(entry_index)?.o;
    let exit_1h = candles.get(exit_1h_index)?.c;
    let exit_4h = candles.get(exit_4h_index)?.c;
    if entry <= 0.0
        || exit_1h <= 0.0
        || exit_4h <= 0.0
        || candles[exit_4h_index].ts - candles[entry_index].ts
            != (FORWARD_4H_BARS as i64 - 1) * MS_15M
    {
        return None;
    }
    let forward_1h = exit_1h / entry - 1.0;
    let forward_4h = exit_4h / entry - 1.0;
    (forward_1h.is_finite() && forward_4h.is_finite()).then_some((forward_1h, forward_4h))
}

/// 构造 confirmed/control、时间段、月份和 4h 有效事件报告。
fn build_report(
    schedule: &UniverseSchedule,
    okx_symbols: usize,
    premium_audit: BinanceKlineAudit,
    stages: PremiumRecoveryStages,
    observations: &[PremiumObservation],
) -> PremiumRecoveryPanelReport {
    let split_ms = schedule.windows[6].from_ms;
    let confirmed = observations
        .iter()
        .filter(|value| value.confirmed)
        .collect::<Vec<_>>();
    let control = observations
        .iter()
        .filter(|value| !value.confirmed)
        .collect::<Vec<_>>();
    let confirmed_discovery_values = confirmed
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_discovery_values = control
        .iter()
        .filter(|value| value.decision_ts < split_ms)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_validation_values = confirmed
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let control_validation_values = control
        .iter()
        .filter(|value| value.decision_ts >= split_ms)
        .copied()
        .collect::<Vec<_>>();
    let confirmed_overall = summarize(&confirmed);
    let control_overall = summarize(&control);
    let confirmed_discovery = summarize(&confirmed_discovery_values);
    let control_discovery = summarize(&control_discovery_values);
    let confirmed_validation = summarize(&confirmed_validation_values);
    let control_validation = summarize(&control_validation_values);
    let effective_events_4h = effective_events(&confirmed);
    let monthly_confirmed = schedule
        .windows
        .iter()
        .map(|window| {
            let values = confirmed
                .iter()
                .filter(|value| {
                    value.decision_ts >= window.from_ms && value.decision_ts < window.to_ms
                })
                .copied()
                .collect::<Vec<_>>();
            (window.from_ms, summarize(&values))
        })
        .collect::<Vec<_>>();
    let factor_gate_passed = (600..=1_440).contains(&confirmed_overall.observations)
        && effective_events_4h >= 300
        && confirmed_discovery.observations >= 250
        && confirmed_validation.observations >= 250
        && segment_passed(&confirmed_discovery, &control_discovery)
        && segment_passed(&confirmed_validation, &control_validation)
        && confirmed_overall
            .mean_forward_1h
            .is_some_and(|value| value > 0.0);
    PremiumRecoveryPanelReport {
        rule_version: PREMIUM_RULE_VERSION.to_owned(),
        universe_version: schedule.version.clone(),
        okx_symbols,
        premium_audit,
        stages,
        effective_events_4h,
        confirmed_overall,
        control_overall,
        confirmed_discovery,
        control_discovery,
        confirmed_validation,
        control_validation,
        monthly_confirmed,
        factor_gate_passed,
    }
}

/// 判断一个封存时间段是否达到收益、命中率和对照增量门槛。
fn segment_passed(confirmed: &PremiumRecoverySummary, control: &PremiumRecoverySummary) -> bool {
    confirmed
        .mean_forward_4h
        .zip(control.mean_forward_4h)
        .is_some_and(|(confirmed_mean, control_mean)| {
            confirmed_mean >= 0.0025 && confirmed_mean - control_mean >= 0.0015
        })
        && confirmed
            .positive_rate_4h_pct
            .is_some_and(|value| value >= 55.0)
}

/// 汇总一组 premium 观察的固定期限多头收益与命中率。
fn summarize(values: &[&PremiumObservation]) -> PremiumRecoverySummary {
    if values.is_empty() {
        return PremiumRecoverySummary::default();
    }
    let mean = |selector: fn(&PremiumObservation) -> f64| {
        Some(values.iter().map(|value| selector(value)).sum::<f64>() / values.len() as f64)
    };
    let positive = |selector: fn(&PremiumObservation) -> f64| {
        Some(
            values.iter().filter(|value| selector(value) > 0.0).count() as f64
                / values.len() as f64
                * 100.0,
        )
    };
    PremiumRecoverySummary {
        observations: values.len(),
        mean_forward_1h: mean(|value| value.forward_1h),
        mean_forward_4h: mean(|value| value.forward_4h),
        positive_rate_1h_pct: positive(|value| value.forward_1h),
        positive_rate_4h_pct: positive(|value| value.forward_4h),
    }
}

/// 将相邻不超过 4h 的 confirmed 时点归并为同一市场事件。
fn effective_events(values: &[&PremiumObservation]) -> usize {
    let mut count = 0usize;
    let mut latest = None::<i64>;
    for value in values {
        if latest.is_none_or(|point| value.decision_ts - point > MS_4H) {
            count += 1;
        }
        latest = Some(value.decision_ts);
    }
    count
}

/// 输出 premium 数据审计、候选漏斗和全部预注册分组。
fn print_report(report: &PremiumRecoveryPanelReport) {
    println!(
        "premium_recovery_panel\trule={}\tuniverse={}\tokx_symbols={}\tmapped_symbols={}\tmapping_blocked={}\trequested_files={}\tavailable_files={}\tmissing_files={}\tinvalid_files={}\tparsed_rows={}\tdecision_points={}\tcoverage_blocked={}\tprice_observations={}\tprice_candidates={}\tpremium_available={}\tconfirmed_selected={}\tcontrol_selected={}\tincomplete={}\teffective_events_4h={}\tfactor_gate_passed={}",
        report.rule_version,
        report.universe_version,
        report.okx_symbols,
        report.premium_audit.mapped_symbols,
        report.premium_audit.mapping_blocked_symbols,
        report.premium_audit.requested_files,
        report.premium_audit.available_files,
        report.premium_audit.missing_files,
        report.premium_audit.invalid_files,
        report.premium_audit.parsed_rows,
        report.stages.decision_points,
        report.stages.price_coverage_blocked,
        report.stages.price_observations,
        report.stages.selected_price_candidates,
        report.stages.premium_available,
        report.stages.confirmed_selected,
        report.stages.control_selected,
        report.stages.incomplete_outcomes,
        report.effective_events_4h,
        report.factor_gate_passed,
    );
    for (label, value) in [
        ("confirmed_overall", &report.confirmed_overall),
        ("control_overall", &report.control_overall),
        ("confirmed_discovery", &report.confirmed_discovery),
        ("control_discovery", &report.control_discovery),
        ("confirmed_validation", &report.confirmed_validation),
        ("control_validation", &report.control_validation),
    ] {
        print_summary(label, value);
    }
    for (from_ms, value) in &report.monthly_confirmed {
        print_summary(&format!("month_{from_ms}"), value);
    }
}

/// 输出一个 premium 分组的样本、收益与命中率。
fn print_summary(label: &str, value: &PremiumRecoverySummary) {
    println!(
        "premium_recovery_summary\tgroup={}\tobservations={}\tmean_1h={}\tmean_4h={}\tpositive_1h_pct={}\tpositive_4h_pct={}",
        label,
        value.observations,
        optional(value.mean_forward_1h),
        optional(value.mean_forward_4h),
        optional(value.positive_rate_1h_pct),
        optional(value.positive_rate_4h_pct),
    );
}

/// 将缺失浮点指标稳定格式化为 `NA`。
fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_owned(), |number| number.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 premium 时点测试使用的连续 15m 序列。
    fn premium(values: &[f64]) -> Vec<BinancePremiumCandle> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| BinancePremiumCandle {
                ts: index as i64 * MS_15M,
                close: *value,
            })
            .collect()
    }

    #[test]
    fn premium_recovery_uses_current_and_exact_one_hour_ago_values() {
        let values = premium(&[-0.0010, -0.0009, -0.0008, -0.0007, -0.0006]);
        let (current, one_hour_ago) = premium_at(&values, 4 * MS_15M).unwrap();
        assert_eq!(current, -0.0006);
        assert_eq!(one_hour_ago, -0.0010);
        assert!(current <= PREMIUM_DISCOUNT && current > one_hour_ago);
    }

    #[test]
    fn premium_gap_blocks_factor_instead_of_using_stale_value() {
        let mut values = premium(&[-0.0010, -0.0009, -0.0008, -0.0007, -0.0006]);
        values[2].ts += 1;
        assert!(premium_at(&values, 4 * MS_15M).is_none());
    }
}
