use super::*;

const FAILURE_WAIT_BARS: usize = 8;

/// V4 只接受冻结历史币池位置，不暴露任何策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedReboundResearchArgs {
    pub manifest: PathBuf,
}

/// V4 反弹尝试后的首个失败、过期或数据不完整结果。
enum FailureDecision {
    Failed(usize),
    Expired(usize),
    Incomplete,
}

/// 解析 V4 参数；未知参数直接失败，避免研究命令静默偏离预注册规则。
pub fn parse_failed_rebound_research_args<I>(values: I) -> Result<FailedReboundResearchArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    while let Some(arg) = values.next() {
        match arg.as_str() {
            "--manifest" => {
                manifest = Some(PathBuf::from(
                    values.next().context("--manifest requires a value")?,
                ));
            }
            "--help" | "-h" => bail!(failed_rebound_usage()),
            _ => bail!("unknown argument: {arg}\n{}", failed_rebound_usage()),
        }
    }
    Ok(FailedReboundResearchArgs {
        manifest: manifest.context("--manifest is required")?,
    })
}

/// 返回冻结 V4 的最小命令用法。
fn failed_rebound_usage() -> &'static str {
    "Usage: market_failed_rebound_short_research --manifest PATH"
}

/// 运行冻结 V4 的反弹失败顺势空头研究，不加载外部指标或触发交易执行。
pub async fn run_failed_rebound_research(
    args: &FailedReboundResearchArgs,
    database_url: &str,
) -> Result<FlowFlipResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read V4 universe manifest {}", args.manifest.display()))?,
    )
    .context("decode V4 universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    if schedule.windows.len() != 12 {
        bail!("failed-rebound V4 requires exactly twelve contiguous monthly windows");
    }
    let first = schedule
        .windows
        .first()
        .context("missing first V4 window")?;
    let last = schedule.windows.last().context("missing last V4 window")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for failed-rebound V4")?;
    let mut candles_by_symbol = BTreeMap::<String, Vec<CandleItem>>::new();
    for symbol in schedule.union_symbols() {
        candles_by_symbol.insert(
            symbol.clone(),
            load_symbol_candles(
                &pool,
                &symbol,
                first.from_ms.saturating_sub(32 * DAY_MS),
                last.to_ms.saturating_add(3 * DAY_MS),
            )
            .await?,
        );
    }
    let (price_tail, price_coverage_blocked) =
        build_price_tail_states(&schedule, &candles_by_symbol);
    let (candidate_indices, _, mut stages) =
        build_price_candidates(&schedule, &candles_by_symbol, &price_tail);
    let mut trades = Vec::new();
    for (symbol, indices) in candidate_indices {
        let candles = candles_by_symbol
            .get(&symbol)
            .with_context(|| format!("missing V4 candles for {symbol}"))?;
        trades.extend(scan_failed_rebounds(
            &symbol,
            candles,
            &indices,
            &mut stages,
        ));
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let split_ms = schedule.windows[6].from_ms;
    let discovery = trades
        .iter()
        .filter(|trade| trade.entry_ts < split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let validation = trades
        .iter()
        .filter(|trade| trade.entry_ts >= split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = trades
                .iter()
                .filter(|trade| trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms)
                .cloned()
                .collect::<Vec<_>>();
            (window.from_ms, metrics(&values, 1.0))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_owned())
            .or_default() += 1;
    }
    let report = FlowFlipResearchReport {
        rule_version: "bottom_quintile_failed_rebound_short_v4".to_owned(),
        universe_version: schedule.version.clone(),
        symbols: candles_by_symbol.len(),
        metrics_audit: MetricsAudit::default(),
        price_coverage_blocked,
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall: metrics(&trades, 1.0),
        discovery: metrics(&discovery, 1.0),
        validation: metrics(&validation, 1.0),
        double_cost: metrics(&trades, 2.0),
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        trades,
    };
    print_report(&report);
    Ok(report)
}

/// 回放单个币种的反弹失败空头，并避免持仓区间重叠。
fn scan_failed_rebounds(
    symbol: &str,
    candles: &[CandleItem],
    candidate_indices: &[usize],
    stages: &mut FlowFlipStageCounts,
) -> Vec<FlowFlipTrade> {
    let mut trades = Vec::new();
    let mut locked_until = None::<usize>;
    for setup_index in candidate_indices.iter().copied() {
        if locked_until.is_some_and(|resolved| setup_index <= resolved) {
            continue;
        }
        match failure_decision(candles, setup_index) {
            FailureDecision::Failed(failure_index) => {
                stages.failure_pass += 1;
                match settle_short(symbol, candles, setup_index, failure_index) {
                    Settlement::Trade(trade, exit_index) => {
                        trades.push(trade);
                        locked_until = Some(exit_index);
                    }
                    Settlement::RiskBlocked => {
                        stages.risk_blocked += 1;
                        locked_until = Some(failure_index);
                    }
                    Settlement::Incomplete => stages.incomplete_outcomes += 1,
                }
            }
            FailureDecision::Expired(index) => {
                stages.failure_expired += 1;
                locked_until = Some(index);
            }
            FailureDecision::Incomplete => stages.incomplete_outcomes += 1,
        }
    }
    trades
}

/// 在八根等待窗口内识别首次跌回冻结结构低点的阴线。
fn failure_decision(candles: &[CandleItem], setup_index: usize) -> FailureDecision {
    let required_last = setup_index.saturating_add(FAILURE_WAIT_BARS);
    let Some(last_available) = candles.len().checked_sub(1) else {
        return FailureDecision::Incomplete;
    };
    let last = required_last.min(last_available);
    let structure_low = candles[setup_index + 1 - LOW_MEMORY_BARS..=setup_index]
        .iter()
        .map(|candle| candle.l)
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    for index in setup_index + 1..=last {
        let candle = &candles[index];
        if candle.c < candle.o && candle.c < structure_low {
            return FailureDecision::Failed(index);
        }
    }
    if required_last > last_available {
        FailureDecision::Incomplete
    } else {
        FailureDecision::Expired(last)
    }
}

/// 用反弹高点结构止损、3R 目标和 48h 上限结算空头。
fn settle_short(
    symbol: &str,
    candles: &[CandleItem],
    setup_index: usize,
    failure_index: usize,
) -> Settlement {
    let entry_index = failure_index + 1;
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return Settlement::Incomplete;
    }
    let Some(atr) = atr_at(candles, setup_index) else {
        return Settlement::RiskBlocked;
    };
    let entry = candles[entry_index].o;
    let structure_high = candles[setup_index..=failure_index]
        .iter()
        .map(|candle| candle.h)
        .reduce(f64::max)
        .unwrap_or(f64::NAN);
    let stop = structure_high + atr * STOP_ATR_BUFFER;
    let risk = stop - entry;
    let risk_pct = risk / entry * 100.0;
    if !entry.is_finite()
        || !stop.is_finite()
        || risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return Settlement::RiskBlocked;
    }
    let target = entry - risk * TARGET_R;
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = (entry - exit) / risk;
    let mut exit_reason = "max_holding_timeout";
    for (offset, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let current = entry_index + offset;
        if candle.h >= stop {
            exit_index = current;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "structure_stop";
            break;
        }
        if candle.l <= target {
            exit_index = current;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_3r";
            break;
        }
    }
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / risk;
    Settlement::Trade(
        FlowFlipTrade {
            symbol: symbol.to_owned(),
            direction: "short",
            setup_ts: candles[setup_index].ts.saturating_add(MS_15M),
            decision_ts: candles[failure_index].ts.saturating_add(MS_15M),
            entry_ts: candles[entry_index].ts,
            exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
            oi_change_4h: None,
            prior_taker_median: None,
            current_taker_median: None,
            top_account_ratio: None,
            top_position_ratio: None,
            entry,
            stop,
            target,
            gross_r,
            cost_r,
            net_r: gross_r - cost_r,
            exit_reason,
        },
        exit_index,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> CandleItem {
        CandleItem {
            ts: index as i64 * MS_15M,
            o: open,
            h: high,
            l: low,
            c: close,
            v: 1.0,
            confirm: 1,
        }
    }

    #[test]
    fn failure_requires_first_bearish_close_below_frozen_structure_low() {
        let setup = HISTORY_BARS + LOW_MEMORY_BARS;
        let mut candles = (0..=setup + FAILURE_WAIT_BARS)
            .map(|index| candle(index, 100.0, 100.5, 99.8, 100.1))
            .collect::<Vec<_>>();
        candles[setup - 3].l = 99.0;
        candles[setup + 1] = candle(setup + 1, 100.2, 100.4, 98.8, 99.1);
        candles[setup + 2] = candle(setup + 2, 99.2, 99.3, 98.5, 98.9);

        assert!(matches!(
            failure_decision(&candles, setup),
            FailureDecision::Failed(index) if index == setup + 2
        ));
    }
}
