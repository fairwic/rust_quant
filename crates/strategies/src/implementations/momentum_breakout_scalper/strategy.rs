use super::types::{
    round_price, MomentumBreakoutAction, MomentumBreakoutBacktestTuning, MomentumBreakoutDecision,
    MomentumBreakoutSignalSnapshot, MomentumBreakoutThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde_json::json;

/// Momentum Breakout Scalper：BTC/ETH 短周期顺势突破回踩策略（纯 OHLCV）。
pub struct MomentumBreakoutScalperStrategy;

impl MomentumBreakoutScalperStrategy {
    /// 基于快照评估是否触发顺势回踩入场；过滤器不满足时返回 Flat。
    pub fn evaluate(
        thresholds: &MomentumBreakoutThresholds,
        snapshot: &MomentumBreakoutSignalSnapshot,
    ) -> MomentumBreakoutDecision {
        let blockers = Self::blockers(snapshot, thresholds);
        if !blockers.is_empty() {
            return Self::decision(MomentumBreakoutAction::Flat, blockers);
        }
        let trend_strength = if snapshot.price > 0.0 {
            (snapshot.fast_ema - snapshot.slow_ema) / snapshot.price * 100.0
        } else {
            0.0
        };
        // 多：上升趋势(fast>slow) + 回踩到快速EMA附近 + 恢复阳线动量确认。
        // 空：下降趋势(fast<slow) + 反抽到快速EMA附近 + 恢复阴线动量确认。
        let action = if trend_strength >= thresholds.min_trend_strength_pct
            && snapshot.price >= snapshot.fast_ema
            && snapshot.resume_direction > 0
        {
            MomentumBreakoutAction::Long
        } else if trend_strength <= -thresholds.min_trend_strength_pct
            && snapshot.price <= snapshot.fast_ema
            && snapshot.resume_direction < 0
        {
            MomentumBreakoutAction::Short
        } else {
            return Self::decision(
                MomentumBreakoutAction::Flat,
                vec!["NO_MOMENTUM_SETUP".to_string()],
            );
        };
        let (stop, t1, t2, t3) = Self::stop_and_targets(snapshot, thresholds, action);
        let reasons = vec![
            "MOMENTUM_BREAKOUT_CONFIRMED".to_string(),
            format!("STOP_PRICE:{}", round_price(stop)),
            format!("TARGET_1:{}", round_price(t1)),
            format!("TARGET_2:{}", round_price(t2)),
            format!("TARGET_3:{}", round_price(t3)),
        ];
        Self::decision(action, reasons)
    }

    /// 缺少有效快照时返回 flat。
    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            MomentumBreakoutAction::Flat,
            vec!["MISSING_MARKET_SNAPSHOT".to_string()],
        )
        .to_signal(price, ts)
    }

    /// 默认调参回测入口。
    pub fn run_test(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        self.run_test_with_tuning(
            inst_id,
            candles,
            risk,
            MomentumBreakoutBacktestTuning::default(),
        )
    }

    /// 指定调参回测入口；复用通用 pipeline。
    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: MomentumBreakoutBacktestTuning,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(
            inst_id,
            MomentumBreakoutBacktestAdapter::new(inst_id, tuning),
            candles,
            risk,
        )
    }

    fn blockers(
        snapshot: &MomentumBreakoutSignalSnapshot,
        t: &MomentumBreakoutThresholds,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_if(
            !Self::is_allowed_exchange(&snapshot.exchange),
            "EXCHANGE_NOT_LIVE_READY_V1",
            &mut reasons,
        );
        Self::push_if(
            !Self::is_btc_or_eth(&snapshot.symbol),
            "SYMBOL_NOT_BTC_ETH",
            &mut reasons,
        );
        Self::push_if(snapshot.atr <= 0.0, "ATR_NOT_READY", &mut reasons);
        // 回踩深度过滤：价格离快速 EMA 太远说明不是回踩而是追单。
        Self::push_if(
            snapshot.pullback_atr > t.max_pullback_atr,
            "PULLBACK_TOO_DEEP",
            &mut reasons,
        );
        // 动量确认：恢复 K 线实体太小说明动能不足。
        Self::push_if(
            snapshot.resume_body_ratio < t.min_resume_body_ratio,
            "RESUME_MOMENTUM_WEAK",
            &mut reasons,
        );
        // 插针过滤。
        Self::push_if(
            snapshot.entry_amp_pct > t.max_entry_amp_pct,
            "ENTRY_CANDLE_TOO_VOLATILE",
            &mut reasons,
        );
        reasons
    }

    fn stop_and_targets(
        snapshot: &MomentumBreakoutSignalSnapshot,
        t: &MomentumBreakoutThresholds,
        action: MomentumBreakoutAction,
    ) -> (f64, f64, f64, f64) {
        let stop_dist = snapshot.atr * t.stop_atr_mult;
        let d1 = snapshot.atr * t.target_atr_mult_1;
        let d2 = snapshot.atr * t.target_atr_mult_2;
        let d3 = snapshot.atr * t.target_atr_mult_3;
        let p = snapshot.price;
        match action {
            MomentumBreakoutAction::Long => (p - stop_dist, p + d1, p + d2, p + d3),
            MomentumBreakoutAction::Short => (p + stop_dist, p - d1, p - d2, p - d3),
            MomentumBreakoutAction::Flat => (p, p, p, p),
        }
    }

    fn is_allowed_exchange(exchange: &str) -> bool {
        matches!(Self::normalize(exchange).as_str(), "binance" | "okx")
    }

    fn is_btc_or_eth(symbol: &str) -> bool {
        let upper = symbol.to_ascii_uppercase();
        upper.starts_with("BTC") || upper.starts_with("ETH")
    }

    fn normalize(value: &str) -> String {
        value.trim().to_ascii_lowercase()
    }

    fn decision(action: MomentumBreakoutAction, reasons: Vec<String>) -> MomentumBreakoutDecision {
        MomentumBreakoutDecision { action, reasons }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

/// 回测适配器：从 candle 序列增量维护 EMA/ATR，构造纯 OHLCV 顺势快照。
#[derive(Debug, Clone)]
struct MomentumBreakoutBacktestAdapter {
    symbol: String,
    tuning: MomentumBreakoutBacktestTuning,
    cooldown_remaining: usize,
}

impl MomentumBreakoutBacktestAdapter {
    fn new(inst_id: &str, tuning: MomentumBreakoutBacktestTuning) -> Self {
        Self {
            symbol: inst_id.to_string(),
            tuning,
            cooldown_remaining: 0,
        }
    }

    fn snapshot(&self, candles: &[CandleItem]) -> Option<MomentumBreakoutSignalSnapshot> {
        let last = candles.last()?;
        let closes: Vec<f64> = candles.iter().map(|c| c.c).collect();
        let fast_ema = ema_at(&closes, closes.len(), self.tuning.fast_ema_period)?;
        let slow_ema = ema_at(&closes, closes.len(), self.tuning.slow_ema_period)?;
        let atr = atr_wilder(candles, self.tuning.atr_period)?;
        if atr <= 0.0 {
            return None;
        }
        let pullback_atr = (last.c - fast_ema).abs() / atr;
        let raw_range = (last.h - last.l).abs().max(1e-9);
        let body = (last.c - last.o).abs();
        let resume_body_ratio = body / raw_range;
        let resume_direction = if last.c > last.o {
            1
        } else if last.c < last.o {
            -1
        } else {
            0
        };
        let entry_amp_pct = if last.c > 0.0 {
            raw_range / last.c * 100.0
        } else {
            0.0
        };
        Some(MomentumBreakoutSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            fast_ema,
            slow_ema,
            atr,
            pullback_atr,
            resume_body_ratio,
            resume_direction,
            entry_amp_pct,
        })
    }
}

impl IndicatorStrategyBacktest for MomentumBreakoutBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.tuning
            .slow_ema_period
            .max(self.tuning.fast_ema_period)
            .max(self.tuning.atr_period + 1)
            .max(8)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _values: &mut Self::IndicatorValues,
        _risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(last) = candles.last() else {
            return SignalResult::default();
        };
        let Some(snapshot) = self.snapshot(candles) else {
            return SignalResult::default();
        };
        let mut thresholds = self.tuning.thresholds();
        if !self.tuning.allow_short {
            // 不做空时把趋势强度门槛设为极大负不可达，等价于只做多。
            thresholds.min_trend_strength_pct = thresholds.min_trend_strength_pct.max(0.0);
        }
        let mut decision = MomentumBreakoutScalperStrategy::evaluate(&thresholds, &snapshot);
        if !self.tuning.allow_short && decision.action == MomentumBreakoutAction::Short {
            decision = MomentumBreakoutDecision {
                action: MomentumBreakoutAction::Flat,
                reasons: vec!["SHORT_DISABLED".to_string()],
            };
        }
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_buy || signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

/// Wilder ATR（用简单平均近似）；数据不足返回 None。
fn atr_wilder(candles: &[CandleItem], period: usize) -> Option<f64> {
    if period == 0 || candles.len() < period + 1 {
        return None;
    }
    let start = candles.len() - period;
    let mut sum_tr = 0.0;
    for i in start..candles.len() {
        let prev_close = candles[i - 1].c;
        let high = candles[i].h;
        let low = candles[i].l;
        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());
        sum_tr += tr;
    }
    Some(sum_tr / period as f64)
}

/// 计算到 `end`（不含）为止序列的 EMA 值。
fn ema_at(closes: &[f64], end: usize, period: usize) -> Option<f64> {
    if end < period || period == 0 {
        return None;
    }
    let alpha = 2.0 / (period as f64 + 1.0);
    let seed_start = end - period;
    let mut ema = closes[seed_start];
    for &value in &closes[seed_start + 1..end] {
        ema = alpha * value + (1.0 - alpha) * ema;
    }
    Some(ema)
}
