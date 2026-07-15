use super::{PortfolioTradeCandidate, ResearchObservation};
use rust_quant_common::CandleItem;
use rust_quant_strategies::implementations::pa_quant_tree::{
    build_execution_plan, calculate_pa_features, generate_pa_candidate,
    generate_pa_followthrough_candidate, PaDecisionTrace, PaDirection, PaExecutionPlan,
    PaStrategyKey, RuntimeManifest, RuntimeModel, PA_MIN_CANDLES,
};
use serde::{Deserialize, Serialize};

/// 历史研究成本口径；费率和滑点按单边 bps，资金费率按整笔往返 bps。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalCostModel {
    /// 单边手续费，单位为 bps；方案基础值为 5。
    pub fee_bps_per_side: f64,
    /// 单边滑点，单位为 bps；方案基础值为 3。
    pub slippage_bps_per_side: f64,
    /// 持仓期累计资金费率成本，单位为 bps。
    /// None 表示数据源未提供，该报告不得用于晋级。
    pub funding_cost_bps_round_trip: Option<f64>,
}

/// 单个资金费率结算点；研究层只消费已落库的分源历史事实。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistoricalFundingRatePoint {
    /// 资金费率结算时间，Unix 毫秒时间戳。
    pub funding_time: i64,
    /// 单次结算费率，例如 0.0001 表示 1bp。
    pub rate: f64,
}

/// 历史路径的确定性退出原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalExitReason {
    /// 结构止损或跳空止损。
    StopLoss,
    /// 固定 R 或区间中点目标。
    TakeProfit,
}

/// 一个已结算的候选路径，同时提供训练样本与共享组合输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalPaTrade {
    /// 时间点一致、成本后标签化的训练样本。
    pub observation: ResearchObservation,
    /// 共享组合回放所需的入场、退出、止损和净 R。
    pub portfolio_trade: PortfolioTradeCandidate,
    /// 扣除成本前的实际 R。
    pub gross_r: f64,
    /// 两倍全部成本压力下的 R。
    pub double_cost_net_r: f64,
    /// 退出原因。
    pub exit_reason: HistoricalExitReason,
}

/// 单市场、单策略的原始候选历史扫描结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoricalPaSimulation {
    /// 被扫描的交易对。
    pub symbol: String,
    /// v1 策略标识。
    pub strategy_key: PaStrategyKey,
    /// 不可变策略版本。
    pub strategy_version: String,
    /// M0 RuntimeManifest 哈希。
    pub manifest_hash: String,
    /// 进入扫描的已确认 K 线数量。
    pub candle_count: usize,
    /// 结构候选总数，包含无法在样本末尾结算的候选。
    pub candidate_count: usize,
    /// 因下一棒止损/RR 复核失败而拒绝的候选数。
    pub invalid_risk_plan_count: usize,
    /// 截止样本末尾仍未触发止损或目标的候选数。
    pub unresolved_count: usize,
    /// 是否已包含持仓期资金费率；false 时禁止晋级。
    pub funding_cost_included: bool,
    /// 已完成结算的候选路径。
    pub trades: Vec<HistoricalPaTrade>,
}

/// 返回方案约定的基础成本：单边 5bps 手续费与 3bps 滑点，资金费率待数据接入。
pub fn default_historical_cost_model() -> HistoricalCostModel {
    HistoricalCostModel {
        fee_bps_per_side: 5.0,
        slippage_bps_per_side: 3.0,
        funding_cost_bps_round_trip: None,
    }
}

/// 扫描原始 PA 候选并按未来路径结算标签；未来 K 线仅用于已生成候选的退出评估。
pub fn simulate_pa_candidate_history(
    symbol: &str,
    candles: &[CandleItem],
    manifest: &RuntimeManifest,
    cost_model: &HistoricalCostModel,
) -> Result<HistoricalPaSimulation, String> {
    simulate_pa_candidate_history_with_funding(symbol, candles, manifest, cost_model, &[])
}

/// 使用分源历史资金费率扫描 PA 候选；代理费率按绝对值扣减，禁止把跨市场负费率当收益。
pub fn simulate_pa_candidate_history_with_funding(
    symbol: &str,
    candles: &[CandleItem],
    manifest: &RuntimeManifest,
    cost_model: &HistoricalCostModel,
    funding_rates: &[HistoricalFundingRatePoint],
) -> Result<HistoricalPaSimulation, String> {
    manifest.validate()?;
    validate_history(candles, cost_model, funding_rates)?;
    let strategy_key = PaStrategyKey::parse(&manifest.strategy_key)?;
    if strategy_key.is_meta_filter() {
        return Err("Vegas Meta-filter requires paired Vegas candidates".to_owned());
    }
    if !matches!(&manifest.model, RuntimeModel::FixedRules { rules } if rules.is_empty()) {
        return Err("historical candidate universe requires an M0 no-filter manifest".to_owned());
    }
    let manifest_hash = manifest.manifest_hash()?;
    let mut result = HistoricalPaSimulation {
        symbol: symbol.to_owned(),
        strategy_key,
        strategy_version: manifest.version.clone(),
        manifest_hash: manifest_hash.clone(),
        candle_count: candles.len(),
        candidate_count: 0,
        invalid_risk_plan_count: 0,
        unresolved_count: 0,
        funding_cost_included: cost_model.funding_cost_bps_round_trip.is_some()
            || !funding_rates.is_empty(),
        trades: Vec::new(),
    };
    if candles.len() <= PA_MIN_CANDLES {
        return Ok(result);
    }

    let first_signal_index = if strategy_key.uses_followthrough_confirmation() {
        PA_MIN_CANDLES
    } else {
        PA_MIN_CANDLES - 1
    };
    for signal_index in first_signal_index..(candles.len() - 1) {
        let window_size = if strategy_key.uses_followthrough_confirmation() {
            PA_MIN_CANDLES + 1
        } else {
            PA_MIN_CANDLES
        };
        let window = &candles[signal_index + 1 - window_size..=signal_index];
        let Ok(features) = calculate_pa_features(window) else {
            continue;
        };
        let candidate_result = if strategy_key.uses_followthrough_confirmation() {
            generate_pa_followthrough_candidate(window)
        } else {
            generate_pa_candidate(window, &features)
        };
        let Ok(candidate) = candidate_result else {
            continue;
        };
        if !strategy_key.supports_candidate(candidate.kind) {
            continue;
        }
        result.candidate_count += 1;
        let entry_index = signal_index + 1;
        let Ok(execution) = build_execution_plan(&candidate, &candles[entry_index]) else {
            result.invalid_risk_plan_count += 1;
            continue;
        };
        let Some(settlement) = settle_execution(
            &execution,
            &candles[entry_index..],
            cost_model,
            funding_rates,
        ) else {
            result.unresolved_count += 1;
            continue;
        };
        let candidate_id = format!(
            "{}:{}:{}",
            symbol, manifest.strategy_key, candidate.signal_ts
        );
        let trace = PaDecisionTrace {
            signal_ts: candidate.signal_ts,
            manifest_hash: manifest_hash.clone(),
            model_score: Some(1.0),
            features: Some(features),
            candidate: Some(candidate),
            execution: Some(execution.clone()),
            blocker: None,
        };
        let observation = ResearchObservation::from_pa_settlement(
            symbol.to_owned(),
            manifest.strategy_key.clone(),
            manifest.version.clone(),
            candidate_id.clone(),
            &trace,
            settlement.exit_ts,
            settlement.net_r,
            None,
        )?;
        let portfolio_trade = PortfolioTradeCandidate {
            candidate_id,
            symbol: symbol.to_owned(),
            entry_ts: execution.entry_ts,
            exit_ts: settlement.exit_ts,
            entry_price: execution.entry_price,
            stop_price: execution.stop_price,
            net_r: settlement.net_r,
        };
        result.trades.push(HistoricalPaTrade {
            observation,
            portfolio_trade,
            gross_r: settlement.gross_r,
            double_cost_net_r: settlement.double_cost_net_r,
            exit_reason: settlement.exit_reason,
        });
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Settlement {
    pub(crate) exit_ts: i64,
    pub(crate) gross_r: f64,
    pub(crate) net_r: f64,
    pub(crate) double_cost_net_r: f64,
    pub(crate) exit_reason: HistoricalExitReason,
}

pub(crate) fn settle_execution(
    execution: &PaExecutionPlan,
    path: &[CandleItem],
    cost_model: &HistoricalCostModel,
    funding_rates: &[HistoricalFundingRatePoint],
) -> Option<Settlement> {
    for candle in path {
        let stop_gap = match execution.direction {
            PaDirection::Long => candle.o <= execution.stop_price,
            PaDirection::Short => candle.o >= execution.stop_price,
        };
        if stop_gap {
            return Some(settlement(
                execution,
                candle.ts,
                candle.o,
                HistoricalExitReason::StopLoss,
                cost_model,
                funding_rates,
            ));
        }
        let stop_hit = match execution.direction {
            PaDirection::Long => candle.l <= execution.stop_price,
            PaDirection::Short => candle.h >= execution.stop_price,
        };
        let target_hit = match execution.direction {
            PaDirection::Long => candle.h >= execution.target_price,
            PaDirection::Short => candle.l <= execution.target_price,
        };
        // 同棒止损和止盈同时发生时按止损处理，避免使用未知的棒内路径美化结果。
        if stop_hit {
            return Some(settlement(
                execution,
                candle.ts,
                execution.stop_price,
                HistoricalExitReason::StopLoss,
                cost_model,
                funding_rates,
            ));
        }
        if target_hit {
            return Some(settlement(
                execution,
                candle.ts,
                execution.target_price,
                HistoricalExitReason::TakeProfit,
                cost_model,
                funding_rates,
            ));
        }
    }
    None
}

fn settlement(
    execution: &PaExecutionPlan,
    exit_ts: i64,
    exit_price: f64,
    exit_reason: HistoricalExitReason,
    cost_model: &HistoricalCostModel,
    funding_rates: &[HistoricalFundingRatePoint],
) -> Settlement {
    let risk_distance = (execution.entry_price - execution.stop_price).abs();
    let gross_r = match execution.direction {
        PaDirection::Long => (exit_price - execution.entry_price) / risk_distance,
        PaDirection::Short => (execution.entry_price - exit_price) / risk_distance,
    };
    let trading_bps = 2.0 * (cost_model.fee_bps_per_side + cost_model.slippage_bps_per_side);
    let funding_bps = cost_model.funding_cost_bps_round_trip.unwrap_or(0.0)
        + proxy_funding_cost_bps(execution.entry_ts, exit_ts, funding_rates);
    let cost_r = execution.entry_price * (trading_bps + funding_bps) / 10_000.0 / risk_distance;
    let double_cost_r =
        execution.entry_price * 2.0 * (trading_bps + funding_bps) / 10_000.0 / risk_distance;
    Settlement {
        exit_ts,
        gross_r,
        net_r: gross_r - cost_r,
        double_cost_net_r: gross_r - double_cost_r,
        exit_reason,
    }
}

/// 按持仓跨越的小时桶累计代理资金费率绝对值，避免来源差异产生虚假费率收益。
fn proxy_funding_cost_bps(
    entry_ts: i64,
    exit_ts: i64,
    funding_rates: &[HistoricalFundingRatePoint],
) -> f64 {
    const HOUR_MS: i64 = 3_600_000;
    let entry_hour = entry_ts.div_euclid(HOUR_MS);
    let exit_hour = exit_ts.div_euclid(HOUR_MS);
    funding_rates
        .iter()
        .filter(|point| {
            let funding_hour = point.funding_time.div_euclid(HOUR_MS);
            funding_hour >= entry_hour && funding_hour <= exit_hour
        })
        .map(|point| point.rate.abs() * 10_000.0)
        .sum()
}

pub(crate) fn validate_history(
    candles: &[CandleItem],
    cost_model: &HistoricalCostModel,
    funding_rates: &[HistoricalFundingRatePoint],
) -> Result<(), String> {
    if candles.windows(2).any(|pair| pair[0].ts >= pair[1].ts)
        || candles.iter().any(|candle| candle.confirm != 1)
    {
        return Err("historical candles must be strictly ordered and confirmed".to_owned());
    }
    if [
        cost_model.fee_bps_per_side,
        cost_model.slippage_bps_per_side,
    ]
    .iter()
    .any(|value| !value.is_finite() || *value < 0.0)
        || cost_model
            .funding_cost_bps_round_trip
            .is_some_and(|value| !value.is_finite())
    {
        return Err("historical cost model contains invalid values".to_owned());
    }
    if funding_rates
        .windows(2)
        .any(|pair| pair[0].funding_time >= pair[1].funding_time)
        || funding_rates
            .iter()
            .any(|point| !point.rate.is_finite() || point.funding_time <= 0)
    {
        return Err("historical funding rates must be finite and strictly ordered".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_strategies::implementations::pa_quant_tree::{
        generate_pa_followthrough_candidate, PaCandidateKind,
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

    fn execution() -> PaExecutionPlan {
        PaExecutionPlan {
            signal_ts: 0,
            entry_ts: 1,
            direction: PaDirection::Long,
            kind: PaCandidateKind::TrendPullback,
            entry_price: 100.0,
            stop_price: 99.0,
            target_price: 101.5,
            reward_risk: 1.5,
        }
    }

    #[test]
    fn same_bar_stop_and_target_uses_stop_and_applies_round_trip_cost() {
        let candle = CandleItem {
            ts: 1,
            o: 100.0,
            h: 102.0,
            l: 98.0,
            c: 100.0,
            v: 1.0,
            confirm: 1,
        };
        let settled = settle_execution(
            &execution(),
            &[candle],
            &default_historical_cost_model(),
            &[],
        )
        .unwrap();
        assert_eq!(settled.exit_reason, HistoricalExitReason::StopLoss);
        assert!((settled.net_r + 1.16).abs() < 1e-12);
        assert!((settled.double_cost_net_r + 1.32).abs() < 1e-12);
    }

    #[test]
    fn gap_stop_uses_next_available_open() {
        let candle = CandleItem {
            ts: 2,
            o: 98.5,
            h: 99.0,
            l: 98.0,
            c: 98.8,
            v: 1.0,
            confirm: 1,
        };
        let settled = settle_execution(
            &execution(),
            &[candle],
            &HistoricalCostModel {
                fee_bps_per_side: 0.0,
                slippage_bps_per_side: 0.0,
                funding_cost_bps_round_trip: Some(0.0),
            },
            &[],
        )
        .unwrap();
        assert!((settled.gross_r + 1.5).abs() < 1e-12);
    }

    #[test]
    fn funding_proxy_uses_absolute_hourly_rates_as_conservative_cost() {
        let candle = CandleItem {
            ts: 3_600_000,
            o: 100.0,
            h: 102.0,
            l: 100.0,
            c: 101.5,
            v: 1.0,
            confirm: 1,
        };
        let settled = settle_execution(
            &execution(),
            &[candle],
            &HistoricalCostModel {
                fee_bps_per_side: 0.0,
                slippage_bps_per_side: 0.0,
                funding_cost_bps_round_trip: None,
            },
            &[
                HistoricalFundingRatePoint {
                    funding_time: 1,
                    rate: -0.0001,
                },
                HistoricalFundingRatePoint {
                    funding_time: 3_600_001,
                    rate: 0.0001,
                },
            ],
        )
        .unwrap();

        assert!((settled.net_r - 1.48).abs() < 1e-12);
        assert!((settled.double_cost_net_r - 1.46).abs() < 1e-12);
    }

    #[test]
    fn followthrough_setup_confirmation_and_entry_use_three_distinct_bars() {
        let mut candles = trend_setup_candles(100);
        let setup = candles[99].clone();
        candles.push(CandleItem {
            o: setup.c + 0.05,
            h: setup.h + 0.25,
            l: setup.c,
            c: setup.h + 0.15,
            v: 1.0,
            ts: 100,
            confirm: 1,
        });
        let candidate = generate_pa_followthrough_candidate(&candles).unwrap();
        let entry = CandleItem {
            o: candles[100].c + 0.05,
            h: candles[100].c + 0.5,
            l: candles[100].c - 0.1,
            c: candles[100].c + 0.2,
            v: 1.0,
            ts: 101,
            confirm: 1,
        };

        let execution = build_execution_plan(&candidate, &entry).unwrap();

        assert_eq!(candidate.setup_ts, Some(99));
        assert_eq!(candidate.signal_ts, 100);
        assert_eq!(execution.entry_ts, 101);
        assert_eq!(execution.entry_price, entry.o);
    }
}
