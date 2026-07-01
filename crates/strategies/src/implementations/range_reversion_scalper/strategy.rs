use super::types::{
    round_price, RangeReversionAction, RangeReversionBacktestTuning, RangeReversionDecision,
    RangeReversionSignalSnapshot, RangeReversionThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde_json::json;

/// Range Reversion Scalper：BTC/ETH 短周期均值回归剥头皮策略（纯 OHLCV）。
pub struct RangeReversionScalperStrategy;

impl RangeReversionScalperStrategy {
    /// 基于快照评估是否触发回归入场；过滤器不满足时返回 Flat。
    pub fn evaluate(
        thresholds: &RangeReversionThresholds,
        snapshot: &RangeReversionSignalSnapshot,
    ) -> RangeReversionDecision {
        let blockers = Self::blockers(snapshot, thresholds);
        if !blockers.is_empty() {
            return Self::decision(RangeReversionAction::Flat, blockers);
        }
        // 方向：跌破下轨 + RSI 超卖 → 抄底做多；突破上轨 + RSI 超买 → 摸顶做空。
        let lower = snapshot.band_mid - snapshot.band_width;
        let upper = snapshot.band_mid + snapshot.band_width;
        let action = if snapshot.price <= lower && snapshot.rsi <= thresholds.rsi_long_max {
            RangeReversionAction::Long
        } else if snapshot.price >= upper && snapshot.rsi >= thresholds.rsi_short_min {
            RangeReversionAction::Short
        } else {
            return Self::decision(
                RangeReversionAction::Flat,
                vec!["NO_REVERSION_SETUP".to_string()],
            );
        };
        let (stop, target) = Self::stop_and_target(snapshot, thresholds, action);
        let reasons = vec![
            "RANGE_REVERSION_CONFIRMED".to_string(),
            format!("STOP_PRICE:{}", round_price(stop)),
            format!("TARGET_PRICE:{}", round_price(target)),
        ];
        Self::decision(action, reasons)
    }

    /// 缺少有效快照时返回 flat，避免用单根 candle 伪造 live 信号。
    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            RangeReversionAction::Flat,
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
            RangeReversionBacktestTuning::default(),
        )
    }

    /// 指定调参回测入口；复用通用 pipeline，保证持仓/止损/审计同口径。
    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: RangeReversionBacktestTuning,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(
            inst_id,
            RangeReversionBacktestAdapter::new(inst_id, tuning),
            candles,
            risk,
        )
    }

    fn blockers(
        snapshot: &RangeReversionSignalSnapshot,
        t: &RangeReversionThresholds,
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
        Self::push_if(snapshot.band_width <= 0.0, "BAND_NOT_READY", &mut reasons);
        // 趋势过滤：慢速 EMA 斜率过大说明单边趋势，禁止逆势抄底/摸顶。
        Self::push_if(
            snapshot.trend_slope_pct > t.max_trend_slope_pct,
            "TREND_TOO_STRONG_FOR_REVERSION",
            &mut reasons,
        );
        // 插针过滤：单根入场振幅过大时回归失败概率高。
        Self::push_if(
            snapshot.entry_amp_pct > t.max_entry_amp_pct,
            "ENTRY_CANDLE_TOO_VOLATILE",
            &mut reasons,
        );
        reasons
    }

    fn stop_and_target(
        snapshot: &RangeReversionSignalSnapshot,
        t: &RangeReversionThresholds,
        action: RangeReversionAction,
    ) -> (f64, f64) {
        let stop_dist = snapshot.atr * t.stop_atr_mult;
        let target_dist = snapshot.atr * t.target_atr_mult;
        match action {
            RangeReversionAction::Long => {
                (snapshot.price - stop_dist, snapshot.price + target_dist)
            }
            RangeReversionAction::Short => {
                (snapshot.price + stop_dist, snapshot.price - target_dist)
            }
            RangeReversionAction::Flat => (snapshot.price, snapshot.price),
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

    fn decision(action: RangeReversionAction, reasons: Vec<String>) -> RangeReversionDecision {
        RangeReversionDecision { action, reasons }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

/// 回测适配器：从 candle 序列增量维护 SMA/标准差/RSI/ATR/EMA，构造纯 OHLCV 快照。
#[derive(Debug, Clone)]
struct RangeReversionBacktestAdapter {
    symbol: String,
    tuning: RangeReversionBacktestTuning,
    cooldown_remaining: usize,
}

impl RangeReversionBacktestAdapter {
    fn new(inst_id: &str, tuning: RangeReversionBacktestTuning) -> Self {
        Self {
            symbol: inst_id.to_string(),
            tuning,
            cooldown_remaining: 0,
        }
    }

    fn snapshot(&self, candles: &[CandleItem]) -> Option<RangeReversionSignalSnapshot> {
        let last = candles.last()?;
        let band_period = self.tuning.band_period.max(2);
        if candles.len() < band_period {
            return None;
        }
        let closes: Vec<f64> = candles.iter().map(|c| c.c).collect();
        let band_window = &closes[closes.len() - band_period..];
        let mean = band_window.iter().sum::<f64>() / band_period as f64;
        let variance =
            band_window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / band_period as f64;
        let std = variance.sqrt();
        let band_width = std * self.tuning.band_k;

        let rsi = rsi_wilder(&closes, self.tuning.rsi_period)?;
        let atr = atr_wilder(candles, self.tuning.atr_period)?;
        let trend_slope_pct = ema_slope_pct(
            &closes,
            self.tuning.trend_ema_period,
            self.tuning.trend_slope_lookback,
        )?;
        let raw_range = (last.h - last.l).abs();
        let entry_amp_pct = if last.c > 0.0 {
            raw_range / last.c * 100.0
        } else {
            0.0
        };

        Some(RangeReversionSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            band_mid: mean,
            band_width,
            rsi,
            atr,
            trend_slope_pct,
            entry_amp_pct,
        })
    }
}

impl IndicatorStrategyBacktest for RangeReversionBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.tuning
            .band_period
            .max(self.tuning.rsi_period + 1)
            .max(self.tuning.atr_period + 1)
            .max(self.tuning.trend_ema_period + self.tuning.trend_slope_lookback)
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
            // 不做空时把超买门槛抬到不可达，等价于只做多。
            thresholds.rsi_short_min = f64::INFINITY;
        }
        if !self.tuning.allow_long {
            // 不做多时把超卖门槛压到不可达，等价于只做空。
            thresholds.rsi_long_max = f64::NEG_INFINITY;
        }
        let decision = RangeReversionScalperStrategy::evaluate(&thresholds, &snapshot);
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_buy || signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

/// Wilder RSI；数据不足返回 None。
fn rsi_wilder(closes: &[f64], period: usize) -> Option<f64> {
    if period == 0 || closes.len() < period + 1 {
        return None;
    }
    let start = closes.len() - period - 1;
    let mut gain = 0.0;
    let mut loss = 0.0;
    for i in start + 1..closes.len() {
        let diff = closes[i] - closes[i - 1];
        if diff >= 0.0 {
            gain += diff;
        } else {
            loss -= diff;
        }
    }
    let avg_gain = gain / period as f64;
    let avg_loss = loss / period as f64;
    if avg_loss == 0.0 {
        return Some(100.0);
    }
    let rs = avg_gain / avg_loss;
    Some(100.0 - 100.0 / (1.0 + rs))
}

/// Wilder ATR（用简单平均近似首值，足够回测稳定）；数据不足返回 None。
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

/// 慢速 EMA 斜率绝对值（按价格百分比），用于趋势过滤；数据不足返回 None。
fn ema_slope_pct(closes: &[f64], period: usize, lookback: usize) -> Option<f64> {
    if period == 0 || lookback == 0 || closes.len() < period + lookback {
        return None;
    }
    let ema_now = ema_at(closes, closes.len(), period)?;
    let ema_prev = ema_at(closes, closes.len() - lookback, period)?;
    if ema_prev <= 0.0 {
        return Some(0.0);
    }
    Some(((ema_now - ema_prev) / ema_prev).abs() * 100.0)
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
