use super::super::{BacktestCandle, ConfirmedEvent, MarketVelocityEventBacktestArgs, MS_15M};
use super::{push_feature_report, FrameworkEquityFeatureReport};
use rust_quant_strategies::implementations::{
    CausalMarketStructureFeatures, SmartMoneyConceptsStrategy,
};
use rust_quant_strategies::CandleItem;
use std::collections::HashMap;

/// 把结构特征按 present/absent 分开回放，避免未经验证就把它升级为开仓门禁。
pub(super) fn push_market_structure_feature_reports(
    reports: &mut Vec<FrameworkEquityFeatureReport>,
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    target_r: f64,
    args: &MarketVelocityEventBacktestArgs,
) {
    for (cutoff, features) in [
        (
            "at_setup",
            causal_features_by_event(confirmed, candles_15m, |event| event.event.ts),
        ),
        (
            "before_entry",
            causal_features_by_event(confirmed, candles_15m, |event| event.entry_ts),
        ),
    ] {
        for (feature, select) in [
            (
                "bullish_structure_break_event",
                (|item: &CausalMarketStructureFeatures| item.bullish_structure_break)
                    as fn(&CausalMarketStructureFeatures) -> bool,
            ),
            (
                "bullish_bos_event",
                |item: &CausalMarketStructureFeatures| item.bullish_bos,
            ),
            (
                "bullish_choch_event",
                |item: &CausalMarketStructureFeatures| item.bullish_choch,
            ),
            (
                "bullish_choch_active_3",
                |item: &CausalMarketStructureFeatures| {
                    item.bullish_choch_active
                        && item.bullish_choch_age_bars.is_some_and(|age| age <= 3)
                },
            ),
            ("bullish_fvg_new", |item: &CausalMarketStructureFeatures| {
                item.bullish_fvg
            }),
            (
                "bullish_fvg_active",
                |item: &CausalMarketStructureFeatures| item.active_bullish_fvg_age_bars.is_some(),
            ),
        ] {
            let feature = format!("{feature}_{cutoff}");
            push_feature_report(
                reports,
                confirmed,
                candles_15m,
                target_r,
                args,
                &feature,
                "present",
                |event| features.get(&event.event.id).is_some_and(select),
            );
            push_feature_report(
                reports,
                confirmed,
                candles_15m,
                target_r,
                args,
                &feature,
                "absent",
                |event| {
                    features
                        .get(&event.event.id)
                        .is_some_and(|item| !select(item))
                },
            );
        }
    }
}

/// 为同一批事件构建指定决策时点的因果结构快照。
fn causal_features_by_event<F>(
    confirmed: &[ConfirmedEvent],
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    cutoff: F,
) -> HashMap<i64, CausalMarketStructureFeatures>
where
    F: Fn(&ConfirmedEvent) -> i64,
{
    confirmed
        .iter()
        .filter_map(|event| {
            causal_features_before(event, candles_15m, cutoff(event))
                .map(|features| (event.event.id, features))
        })
        .collect()
}

/// 只读取 cutoff 之前已经完成的 K 线，入场所在 K 线的后续高低价不可见。
fn causal_features_before(
    event: &ConfirmedEvent,
    candles_15m: &HashMap<String, Vec<BacktestCandle>>,
    cutoff_ts: i64,
) -> Option<CausalMarketStructureFeatures> {
    let candles = candles_15m.get(&event.event.symbol)?;
    let visible_count = candles.partition_point(|candle| candle.ts + MS_15M <= cutoff_ts);
    if visible_count < 3 {
        return None;
    }
    let start = visible_count.saturating_sub(192);
    let visible = candles[start..visible_count]
        .iter()
        .map(|candle| CandleItem {
            o: candle.open,
            h: candle.high,
            l: candle.low,
            c: candle.close,
            v: candle.volume,
            ts: candle.ts,
            confirm: 1,
        })
        .collect::<Vec<_>>();
    // 固定沿用 SMC research 默认的 5+5 pivot，不根据本轮收益扫描结构灵敏度。
    Some(SmartMoneyConceptsStrategy::causal_market_structure_features(&visible, 5))
}
