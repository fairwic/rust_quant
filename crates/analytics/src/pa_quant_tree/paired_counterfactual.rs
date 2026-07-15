use super::{
    historical::{settle_execution, validate_history, Settlement},
    HistoricalCostModel, HistoricalExitReason, HistoricalFundingRatePoint,
};
use rust_quant_common::CandleItem;
use rust_quant_strategies::implementations::pa_quant_tree::{
    build_execution_plan, calculate_pa_features, generate_pa_candidate,
    generate_pa_followthrough_candidate, PaCandidate, PaCandidateKind, PaDirection,
    PaExecutionPlan, PA_MIN_CANDLES,
};
use serde::{Deserialize, Serialize};

/// A、B 或 C 单条路径的执行与结算事实。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaAbcPath {
    /// `symbol + setup_ts + direction` 的严格配对 ID。
    pub pair_id: String,
    /// Core 统一交易对标识。
    pub symbol: String,
    /// 原始趋势 setup 的收盘时间戳。
    pub setup_ts: i64,
    /// setup 时冻结的方向。
    pub direction: PaDirection,
    /// true 仅表示该路径按当时时点可执行，不代表允许真实下单。
    pub tradable: bool,
    /// B 必须为 true，明确它使用入场后才可见的确认信息。
    pub diagnostic_only: bool,
    /// 路径实际使用的入场、结构止损和目标。
    pub execution: PaExecutionPlan,
    /// 确定性退出时间戳。
    pub exit_ts: i64,
    /// 扣成本前 R。
    pub gross_r: f64,
    /// 基础成本后 R。
    pub net_r: f64,
    /// 两倍全部成本压力下 R。
    pub double_cost_net_r: f64,
    /// 确定性退出原因。
    pub exit_reason: HistoricalExitReason,
}

/// 同一已确认 setup 的 B 诊断路径和 C 可执行延迟路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaAbcPair {
    /// 与两条路径一致的严格配对 ID。
    pub pair_id: String,
    /// 复用 A 的 t+1 入场与结算、但不可交易的事后确认筛选路径。
    pub b: PaAbcPath,
    /// t+1 收盘确认后于 t+2 开盘执行的路径。
    pub c: PaAbcPath,
    /// `C - B` 的基础成本后配对延迟增量。
    pub delay_delta_net_r: f64,
    /// `C - B` 的两倍成本后配对延迟增量。
    pub delay_delta_double_cost_net_r: f64,
}

impl PaAbcPair {
    /// 只接受 ID、市场、setup、方向和冻结止损完全一致的 B/C 路径。
    pub fn try_new(b: PaAbcPath, c: PaAbcPath) -> Result<Self, String> {
        if b.pair_id != c.pair_id
            || b.symbol != c.symbol
            || b.setup_ts != c.setup_ts
            || b.direction != c.direction
            || b.execution.direction != c.execution.direction
            || b.execution.stop_price.to_bits() != c.execution.stop_price.to_bits()
            || b.tradable
            || !b.diagnostic_only
            || !c.tradable
            || c.diagnostic_only
        {
            return Err("B/C paths violate strict paired counterfactual identity".to_owned());
        }
        Ok(Self {
            pair_id: b.pair_id.clone(),
            delay_delta_net_r: c.net_r - b.net_r,
            delay_delta_double_cost_net_r: c.double_cost_net_r - b.double_cost_net_r,
            b,
            c,
        })
    }
}

/// A/B/C 扫描中被排除路径的分原因计数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaAbcRejectCounts {
    /// A 在 t+1 开盘无法形成合法风险计划。
    pub a_invalid_risk_plan: usize,
    /// A 在样本末尾仍未结算，因此不能形成 B 诊断路径。
    pub a_unresolved: usize,
    /// t+1 唯一确认棒没有通过冻结的 v5 确认规则。
    pub confirmation_rejected: usize,
    /// 确认候选与原 setup 的方向、时间或结构止损不一致。
    pub confirmation_contract_mismatch: usize,
    /// C 在 t+2 开盘无法形成合法风险计划。
    pub c_invalid_risk_plan: usize,
    /// C 在样本末尾仍未结算。
    pub c_unresolved: usize,
    /// 构造严格配对时发现身份或标志不一致。
    pub strict_pair_rejected: usize,
}

/// 单市场的一次 A/B/C 前向扫描结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaAbcSimulation {
    /// Core 统一交易对标识。
    pub symbol: String,
    /// 进入扫描的确认 K 线数量。
    pub candle_count: usize,
    /// 原始趋势 setup 数量。
    pub setup_count: usize,
    /// 已结算的全部 A 基线路径。
    pub a_paths: Vec<PaAbcPath>,
    /// 已确认且 A 已结算的 B 诊断路径；不要求 C 能够结算。
    pub b_paths: Vec<PaAbcPath>,
    /// 在 t+1 收盘通过确认的 setup ID，不依赖其后结算结果。
    pub confirmed_setup_ids: Vec<String>,
    /// B/C 均合法且已结算的严格配对。
    pub strict_pairs: Vec<PaAbcPair>,
    /// 未进入对应统计的分原因计数。
    pub rejects: PaAbcRejectCounts,
}

/// 对原始 15m trend setup 做一次固定 A/B/C 前向扫描，不训练模型也不调整阈值。
pub fn simulate_pa_abc_counterfactual(
    symbol: &str,
    candles: &[CandleItem],
    cost_model: &HistoricalCostModel,
    funding_rates: &[HistoricalFundingRatePoint],
) -> Result<PaAbcSimulation, String> {
    validate_history(candles, cost_model, funding_rates)?;
    let mut simulation = PaAbcSimulation {
        symbol: symbol.to_owned(),
        candle_count: candles.len(),
        setup_count: 0,
        a_paths: Vec::new(),
        b_paths: Vec::new(),
        confirmed_setup_ids: Vec::new(),
        strict_pairs: Vec::new(),
        rejects: PaAbcRejectCounts::default(),
    };
    if candles.len() < PA_MIN_CANDLES + 2 {
        return Ok(simulation);
    }

    for setup_index in (PA_MIN_CANDLES - 1)..(candles.len() - 2) {
        let setup_window = &candles[setup_index + 1 - PA_MIN_CANDLES..=setup_index];
        let Ok(features) = calculate_pa_features(setup_window) else {
            continue;
        };
        let Ok(setup_candidate) = generate_pa_candidate(setup_window, &features) else {
            continue;
        };
        if setup_candidate.kind != PaCandidateKind::TrendPullback {
            continue;
        }
        simulation.setup_count += 1;
        let setup_ts = candles[setup_index].ts;
        let pair_id = pair_identity(symbol, setup_ts, setup_candidate.direction);

        let Ok(a_execution) = build_execution_plan(&setup_candidate, &candles[setup_index + 1])
        else {
            simulation.rejects.a_invalid_risk_plan += 1;
            continue;
        };
        // 确认资格只读取到 t+1 收盘；必须在任何未来结算分支之前冻结。
        let confirmed_candidate =
            match confirmation_candidate(setup_window, &candles[setup_index + 1]) {
                Ok(candidate) => {
                    simulation.confirmed_setup_ids.push(pair_id.clone());
                    if candidate.setup_ts != Some(setup_ts)
                        || candidate.direction != setup_candidate.direction
                        || candidate.stop_price.to_bits() != setup_candidate.stop_price.to_bits()
                    {
                        simulation.rejects.confirmation_contract_mismatch += 1;
                        None
                    } else {
                        Some(candidate)
                    }
                }
                Err(_) => {
                    simulation.rejects.confirmation_rejected += 1;
                    None
                }
            };
        let Some(a_settlement) = settle_execution(
            &a_execution,
            &candles[setup_index + 1..],
            cost_model,
            funding_rates,
        ) else {
            simulation.rejects.a_unresolved += 1;
            continue;
        };
        let a_path = settled_path(
            symbol,
            pair_id.clone(),
            setup_ts,
            true,
            false,
            a_execution,
            a_settlement,
        );
        simulation.a_paths.push(a_path.clone());
        let Some(confirmed_candidate) = confirmed_candidate else {
            continue;
        };

        let mut b_path = a_path;
        b_path.tradable = false;
        b_path.diagnostic_only = true;
        simulation.b_paths.push(b_path.clone());
        let Ok(c_execution) = build_execution_plan(&confirmed_candidate, &candles[setup_index + 2])
        else {
            simulation.rejects.c_invalid_risk_plan += 1;
            continue;
        };
        let Some(c_settlement) = settle_execution(
            &c_execution,
            &candles[setup_index + 2..],
            cost_model,
            funding_rates,
        ) else {
            simulation.rejects.c_unresolved += 1;
            continue;
        };
        let c_path = settled_path(
            symbol,
            pair_id,
            setup_ts,
            true,
            false,
            c_execution,
            c_settlement,
        );
        match PaAbcPair::try_new(b_path, c_path) {
            Ok(pair) => simulation.strict_pairs.push(pair),
            Err(_) => simulation.rejects.strict_pair_rejected += 1,
        }
    }
    Ok(simulation)
}

/// 只读取 setup 窗口和唯一 t+1 确认棒，返回已确认 setup 的严格 ID。
pub fn confirmed_setup_identity(
    symbol: &str,
    setup_window: &[CandleItem],
    confirmation: &CandleItem,
) -> Result<String, String> {
    let candidate = confirmation_candidate(setup_window, confirmation)?;
    let setup_ts = candidate
        .setup_ts
        .ok_or_else(|| "followthrough candidate is missing setup timestamp".to_owned())?;
    Ok(pair_identity(symbol, setup_ts, candidate.direction))
}

/// 构造只到 t+1 的固定窗口并执行既有 v5 确认函数。
fn confirmation_candidate(
    setup_window: &[CandleItem],
    confirmation: &CandleItem,
) -> Result<PaCandidate, String> {
    if setup_window.len() < PA_MIN_CANDLES {
        return Err("confirmation requires a complete setup window".to_owned());
    }
    let mut window = setup_window[setup_window.len() - PA_MIN_CANDLES..].to_vec();
    window.push(confirmation.clone());
    generate_pa_followthrough_candidate(&window).map_err(|blocker| blocker.code().to_owned())
}

/// 将现有结算器的结果转换为带 A/B/C 审计标志的路径。
fn settled_path(
    symbol: &str,
    pair_id: String,
    setup_ts: i64,
    tradable: bool,
    diagnostic_only: bool,
    execution: PaExecutionPlan,
    settlement: Settlement,
) -> PaAbcPath {
    PaAbcPath {
        pair_id,
        symbol: symbol.to_owned(),
        setup_ts,
        direction: execution.direction,
        tradable,
        diagnostic_only,
        execution,
        exit_ts: settlement.exit_ts,
        gross_r: settlement.gross_r,
        net_r: settlement.net_r,
        double_cost_net_r: settlement.double_cost_net_r,
        exit_reason: settlement.exit_reason,
    }
}

/// 生成不依赖显示格式实现的稳定方向配对 ID。
fn pair_identity(symbol: &str, setup_ts: i64, direction: PaDirection) -> String {
    let direction = match direction {
        PaDirection::Long => "long",
        PaDirection::Short => "short",
    };
    format!("{symbol}:{setup_ts}:{direction}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pa_quant_tree::HistoricalExitReason;
    use rust_quant_common::CandleItem;
    use rust_quant_strategies::implementations::pa_quant_tree::{
        PaCandidateKind, PaDirection, PaExecutionPlan,
    };

    fn trend_setup_candles(count: usize) -> Vec<CandleItem> {
        (0..count)
            .map(|index| {
                let base = 100.0 + index as f64 * 0.1;
                let pullback = if index + 2 >= count { -0.8 } else { 0.0 };
                let open = base + pullback;
                let close = open + 0.25;
                CandleItem {
                    o: open,
                    h: close + 0.35,
                    l: open - 0.35,
                    c: close,
                    v: 10.0,
                    ts: index as i64,
                    confirm: 1,
                }
            })
            .collect()
    }

    fn abc_history() -> Vec<CandleItem> {
        let mut candles = trend_setup_candles(100);
        let setup = candles[99].clone();
        candles.push(CandleItem {
            o: setup.c + 0.05,
            h: setup.h + 0.25,
            l: setup.c,
            c: setup.h + 0.15,
            v: 10.0,
            ts: 100,
            confirm: 1,
        });
        let entry = candles[100].c + 0.05;
        candles.push(CandleItem {
            o: entry,
            h: entry + 10.0,
            l: entry - 0.05,
            c: entry + 5.0,
            v: 10.0,
            ts: 101,
            confirm: 1,
        });
        candles.push(CandleItem {
            o: entry + 5.0,
            h: entry + 6.0,
            l: entry + 4.0,
            c: entry + 5.5,
            v: 10.0,
            ts: 102,
            confirm: 1,
        });
        candles
    }

    fn path(pair_id: &str, direction: PaDirection, stop_price: f64) -> PaAbcPath {
        PaAbcPath {
            pair_id: pair_id.to_owned(),
            symbol: "BTC-USDT-SWAP".to_owned(),
            setup_ts: 99,
            direction,
            tradable: false,
            diagnostic_only: true,
            execution: PaExecutionPlan {
                signal_ts: 100,
                entry_ts: 100,
                direction,
                kind: PaCandidateKind::TrendPullback,
                entry_price: 100.0,
                stop_price,
                target_price: 101.5,
                reward_risk: 1.5,
            },
            exit_ts: 101,
            gross_r: 1.5,
            net_r: 1.4,
            double_cost_net_r: 1.3,
            exit_reason: HistoricalExitReason::TakeProfit,
        }
    }

    #[test]
    fn a_enters_t1_b_reuses_a_and_c_enters_t2() {
        let simulation = simulate_pa_abc_counterfactual(
            "BTC-USDT-SWAP",
            &abc_history(),
            &crate::pa_quant_tree::default_historical_cost_model(),
            &[],
        )
        .unwrap();
        let pair = simulation
            .strict_pairs
            .iter()
            .find(|pair| pair.b.setup_ts == 99)
            .unwrap();
        let a = simulation
            .a_paths
            .iter()
            .find(|path| path.setup_ts == 99)
            .unwrap();

        assert_eq!(a.execution.entry_ts, 100);
        assert_eq!(pair.b.execution, a.execution);
        assert!(!pair.b.tradable && pair.b.diagnostic_only);
        assert_eq!(pair.c.execution.entry_ts, 101);
        assert!(pair.c.tradable && !pair.c.diagnostic_only);
    }

    #[test]
    fn strict_pair_rejects_direction_or_frozen_stop_mismatch() {
        let b = path("BTC:99:long", PaDirection::Long, 99.0);
        let mut wrong_direction = path("BTC:99:long", PaDirection::Short, 99.0);
        wrong_direction.tradable = true;
        wrong_direction.diagnostic_only = false;
        assert!(PaAbcPair::try_new(b.clone(), wrong_direction).is_err());

        let mut wrong_stop = path("BTC:99:long", PaDirection::Long, 98.5);
        wrong_stop.tradable = true;
        wrong_stop.diagnostic_only = false;
        assert!(PaAbcPair::try_new(b, wrong_stop).is_err());
    }

    #[test]
    fn confirmation_identity_uses_only_setup_and_t1_bar() {
        let history = abc_history();
        let first =
            confirmed_setup_identity("BTC-USDT-SWAP", &history[..100], &history[100]).unwrap();
        let mut divergent_future = history.clone();
        divergent_future[101].o = 1.0;
        divergent_future[101].h = 2.0;
        divergent_future[101].l = 0.5;
        divergent_future[101].c = 1.5;
        let second = confirmed_setup_identity(
            "BTC-USDT-SWAP",
            &divergent_future[..100],
            &divergent_future[100],
        )
        .unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn confirmed_setup_is_recorded_even_when_later_a_path_is_unresolved() {
        let mut history = abc_history();
        let flat = history[100].c + 0.05;
        for candle in &mut history[101..] {
            candle.o = flat;
            candle.h = flat + 0.01;
            candle.l = flat - 0.01;
            candle.c = flat;
        }

        let simulation = simulate_pa_abc_counterfactual(
            "BTC-USDT-SWAP",
            &history,
            &crate::pa_quant_tree::default_historical_cost_model(),
            &[],
        )
        .unwrap();

        assert!(simulation
            .confirmed_setup_ids
            .iter()
            .any(|identity| identity == "BTC-USDT-SWAP:99:long"));
        assert!(simulation.strict_pairs.is_empty());
    }
}
