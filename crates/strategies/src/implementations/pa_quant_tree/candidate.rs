use super::{
    calculate_pa_features, PaBlocker, PaCandidate, PaCandidateKind, PaDirection, PaExecutionPlan,
    PaFeatureSnapshot, PaMarketRegime,
};
use crate::CandleItem;

/// 使用冻结的 v1 事件定义生成独立 PA 候选。
pub fn generate_pa_candidate(
    candles: &[CandleItem],
    features: &PaFeatureSnapshot,
) -> Result<PaCandidate, PaBlocker> {
    let last = candles.last().ok_or(PaBlocker::DataNotReady)?;
    match features.regime {
        PaMarketRegime::Trend => generate_trend_candidate(candles, last, features),
        PaMarketRegime::Range => generate_range_candidate(last, features),
        PaMarketRegime::Chaos | PaMarketRegime::Unknown => Err(PaBlocker::UnknownRegime),
    }
}

/// 在下一棒开盘价已知后冻结止盈并复核方向、风险和最低 RR。
pub fn build_execution_plan(
    candidate: &PaCandidate,
    next_candle: &CandleItem,
) -> Result<PaExecutionPlan, PaBlocker> {
    let entry = next_candle.o;
    if !entry.is_finite() || entry <= 0.0 {
        return Err(PaBlocker::RiskPlanInvalid);
    }
    let risk = match candidate.direction {
        PaDirection::Long => entry - candidate.stop_price,
        PaDirection::Short => candidate.stop_price - entry,
    };
    if risk <= 0.0 || !risk.is_finite() {
        return Err(PaBlocker::RiskPlanInvalid);
    }
    let target = match candidate.range_target {
        Some(target) => target,
        None => match candidate.direction {
            PaDirection::Long => entry + 1.5 * risk,
            PaDirection::Short => entry - 1.5 * risk,
        },
    };
    let reward = match candidate.direction {
        PaDirection::Long => target - entry,
        PaDirection::Short => entry - target,
    };
    let reward_risk = reward / risk;
    if reward <= 0.0 || !reward_risk.is_finite() || reward_risk < 1.2 {
        return Err(PaBlocker::RiskPlanInvalid);
    }
    Ok(PaExecutionPlan {
        signal_ts: candidate.signal_ts,
        entry_ts: next_candle.ts,
        direction: candidate.direction,
        kind: candidate.kind,
        entry_price: entry,
        stop_price: candidate.stop_price,
        target_price: target,
        reward_risk,
    })
}

/// 用原趋势 setup 后唯一一根已确认 K 线生成独立跟随确认候选。
///
/// 该函数只读取确认棒及以前的数据；返回候选后仍需等待下一棒开盘执行。
pub fn generate_pa_followthrough_candidate(
    candles: &[CandleItem],
) -> Result<PaCandidate, PaBlocker> {
    if candles.len() < super::features::PA_MIN_CANDLES + 1 {
        return Err(PaBlocker::DataNotReady);
    }
    // 先计算确认时点特征以统一校验全部 K 线的确认状态与 OHLC 合法性。
    calculate_pa_features(candles)?;
    let confirmation = candles.last().ok_or(PaBlocker::DataNotReady)?;
    let setup_candles = &candles[..candles.len() - 1];
    let setup = setup_candles.last().ok_or(PaBlocker::DataNotReady)?;
    let setup_features = calculate_pa_features(setup_candles)?;
    // 必须经过 v1 顶层 regime 门禁，不能把 Chaos 中仅有方向的棒补造成趋势 setup。
    let setup_candidate = generate_pa_candidate(setup_candles, &setup_features)?;
    if setup_candidate.kind != PaCandidateKind::TrendPullback {
        return Err(PaBlocker::NoCandidate);
    }

    let range = confirmation.h - confirmation.l;
    let directional_close_strength = match setup_candidate.direction {
        PaDirection::Long => (confirmation.c - confirmation.l) / range,
        PaDirection::Short => (confirmation.h - confirmation.c) / range,
    };
    let direction_confirmed = match setup_candidate.direction {
        PaDirection::Long => {
            confirmation.c > confirmation.o
                && confirmation.c > setup.h
                && confirmation.l > setup_candidate.stop_price
        }
        PaDirection::Short => {
            confirmation.c < confirmation.o
                && confirmation.c < setup.l
                && confirmation.h < setup_candidate.stop_price
        }
    };
    if !direction_confirmed
        || directional_close_strength < 0.65
        || range > 1.5 * setup_features.atr14
    {
        return Err(PaBlocker::ConfirmationRejected);
    }

    Ok(PaCandidate {
        signal_ts: confirmation.ts,
        setup_ts: Some(setup.ts),
        direction: setup_candidate.direction,
        kind: PaCandidateKind::TrendFollowThrough,
        stop_price: setup_candidate.stop_price,
        range_target: None,
    })
}

fn generate_trend_candidate(
    candles: &[CandleItem],
    last: &CandleItem,
    features: &PaFeatureSnapshot,
) -> Result<PaCandidate, PaBlocker> {
    let direction = features.trend_direction.ok_or(PaBlocker::UnknownRegime)?;
    let body_aligned = match direction {
        PaDirection::Long => last.c > last.o && last.c > features.ema20,
        PaDirection::Short => last.c < last.o && last.c < features.ema20,
    };
    if !features.recent_ema_touch || !body_aligned {
        return Err(PaBlocker::NoCandidate);
    }
    let recent3 = &candles[candles.len() - 3..];
    let stop_price = match direction {
        PaDirection::Long => {
            recent3.iter().map(|c| c.l).fold(f64::INFINITY, f64::min) - 0.1 * features.atr14
        }
        PaDirection::Short => {
            recent3
                .iter()
                .map(|c| c.h)
                .fold(f64::NEG_INFINITY, f64::max)
                + 0.1 * features.atr14
        }
    };
    Ok(PaCandidate {
        signal_ts: last.ts,
        setup_ts: None,
        direction,
        kind: PaCandidateKind::TrendPullback,
        stop_price,
        range_target: None,
    })
}

fn generate_range_candidate(
    last: &CandleItem,
    features: &PaFeatureSnapshot,
) -> Result<PaCandidate, PaBlocker> {
    let midpoint = (features.range_high_20 + features.range_low_20) / 2.0;
    let (direction, stop_price) = if features.range_position_20 <= 0.2 && last.c > last.o {
        (
            PaDirection::Long,
            features.range_low_20 - 0.1 * features.atr14,
        )
    } else if features.range_position_20 >= 0.8 && last.c < last.o {
        (
            PaDirection::Short,
            features.range_high_20 + 0.1 * features.atr14,
        )
    } else {
        return Err(PaBlocker::NoCandidate);
    };
    Ok(PaCandidate {
        signal_ts: last.ts,
        setup_ts: None,
        direction,
        kind: PaCandidateKind::RangeBoundary,
        stop_price,
        range_target: Some(midpoint),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementations::pa_quant_tree::calculate_pa_features;
    use crate::implementations::pa_quant_tree::features::tests::trend_candles;

    #[test]
    fn trend_candidate_executes_at_next_open_with_fixed_r() {
        let candles = trend_candles(100);
        let features = calculate_pa_features(&candles).unwrap();
        let candidate = generate_pa_candidate(&candles, &features).unwrap();
        let next = CandleItem {
            o: candles[99].c + 0.1,
            h: candles[99].c + 1.0,
            l: candles[99].c - 1.0,
            c: candles[99].c,
            v: 1.0,
            ts: 100,
            confirm: 1,
        };
        let execution = build_execution_plan(&candidate, &next).unwrap();
        assert_eq!(execution.entry_price, next.o);
        assert!((execution.reward_risk - 1.5).abs() < 1e-12);
    }

    #[test]
    fn followthrough_candidate_requires_breakout_and_preserves_setup_time() {
        let mut candles = trend_candles(100);
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

        assert_eq!(candidate.signal_ts, 100);
        assert_eq!(candidate.setup_ts, Some(99));
        assert_eq!(candidate.kind, PaCandidateKind::TrendFollowThrough);
    }

    #[test]
    fn followthrough_candidate_rejects_weak_confirmation() {
        let mut candles = trend_candles(100);
        let setup = candles[99].clone();
        candles.push(CandleItem {
            o: setup.c,
            h: setup.h + 0.1,
            l: setup.c - 0.05,
            c: setup.c + 0.1,
            v: 1.0,
            ts: 100,
            confirm: 1,
        });

        assert_eq!(
            generate_pa_followthrough_candidate(&candles),
            Err(PaBlocker::ConfirmationRejected)
        );
    }
}
