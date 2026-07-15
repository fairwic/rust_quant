use super::types::*;
use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde_json::json;

/// 震荡结束突破下跌策略
pub struct RangeBreakoutDropStrategy;

impl RangeBreakoutDropStrategy {
    /// 运行回测（默认参数）
    pub fn run_test(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> crate::framework::backtest::types::BackTestResult {
        let tuning = RangeBreakoutDropBacktestTuning::default();
        self.run_test_with_tuning(inst_id, candles, risk, tuning)
    }

    /// 运行回测（自定义参数）
    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: RangeBreakoutDropBacktestTuning,
    ) -> crate::framework::backtest::types::BackTestResult {
        use crate::framework::backtest::adapter::run_indicator_strategy_backtest;

        run_indicator_strategy_backtest(
            inst_id,
            RangeBreakoutDropBacktestAdapter::new(inst_id, tuning),
            candles,
            risk,
        )
    }

    /// 评估市场快照，返回交易决策
    pub fn evaluate(
        thresholds: &RangeBreakoutDropThresholds,
        snapshot: &RangeBreakoutDropSignalSnapshot,
    ) -> RangeBreakoutDropDecision {
        let mut reasons = Vec::new();

        // 核心条件：突破确认
        if !snapshot.breakout_confirmed {
            reasons.push("BREAKOUT_NOT_CONFIRMED".to_string());
        }

        // 可选条件：只在参数要求时检查
        // 1. 震荡模式检查 - 只在波动率设置得比较严格时才检查
        if thresholds.max_range_volatility_pct < 5.0 && !snapshot.in_ranging_mode {
            reasons.push("NOT_IN_RANGING_MODE".to_string());
        }

        // 2. 震荡区间波动检查 - 只在设置得比较严格时才检查
        if thresholds.max_range_volatility_pct < 5.0
            && snapshot.range_volatility_pct > thresholds.max_range_volatility_pct
        {
            reasons.push("RANGE_TOO_VOLATILE".to_string());
        }
        if thresholds.min_range_volatility_pct > 0.5
            && snapshot.range_volatility_pct < thresholds.min_range_volatility_pct
        {
            reasons.push("RANGE_TOO_NARROW".to_string());
        }

        // 3. 突破质量检查 - 只在参数要求比较高时才检查
        if thresholds.min_breakout_body_ratio > 0.4
            && snapshot.breakout_body_ratio < thresholds.min_breakout_body_ratio
        {
            reasons.push("BREAKOUT_BODY_TOO_SMALL".to_string());
        }

        if thresholds.min_breakout_move_atr > 0.5
            && snapshot.breakout_move_atr < thresholds.min_breakout_move_atr
        {
            reasons.push("BREAKOUT_MOVE_TOO_SMALL".to_string());
        }

        if thresholds.min_breakout_volume_mult > 1.3
            && snapshot.breakout_volume_mult < thresholds.min_breakout_volume_mult
        {
            reasons.push("BREAKOUT_VOLUME_TOO_LOW".to_string());
        }

        // 4. 趋势过滤
        if thresholds.require_bearish_ema && !snapshot.price_below_ema {
            reasons.push("PRICE_NOT_BELOW_EMA".to_string());
        }

        // 长期趋势过滤：避免在强势上涨市场做空
        if thresholds.require_below_long_term_ema && !snapshot.price_below_long_term_ema {
            reasons.push("PRICE_NOT_BELOW_LONG_TERM_EMA".to_string());
        }

        // 5. RSI过滤 - 只在要求比较高时才检查
        if thresholds.rsi_min_before_drop > 30.0 && snapshot.rsi < thresholds.rsi_min_before_drop {
            reasons.push("RSI_TOO_LOW".to_string());
        }

        // 6. K线方向 - 只在收盘价突破时要求阴线，最低价触及时不要求
        // 原因：最低价触及已经说明下行压力，K线收阳可能是短期反弹
        if snapshot.is_close_breakout && snapshot.candle_direction >= 0 {
            reasons.push("NOT_BEARISH_CANDLE".to_string());
        }

        // 如果所有条件通过，返回做空信号
        if reasons.is_empty() {
            let stop_price = snapshot.range_high + snapshot.atr * thresholds.stop_atr_mult;
            let risk_distance = stop_price - snapshot.price;

            let target_1 = snapshot.price - risk_distance * thresholds.target_r_1;
            let target_2 = snapshot.price - risk_distance * thresholds.target_r_2;
            let target_3 = snapshot.price - risk_distance * thresholds.target_r_3;

            RangeBreakoutDropDecision {
                action: RangeBreakoutDropAction::Short,
                stop_price: Some(stop_price),
                target_prices: vec![target_1, target_2, target_3],
                reasons: vec![
                    "RANGE_BREAKOUT_DROP_CONFIRMED".to_string(),
                    format!("RANGE_HIGH:{:.0}", snapshot.range_high),
                    format!("RANGE_LOW:{:.0}", snapshot.range_low),
                    format!("STOP_PRICE:{:.0}", stop_price),
                    format!("TARGET_1:{:.0}", target_1),
                    format!("TARGET_2:{:.0}", target_2),
                    format!("TARGET_3:{:.0}", target_3),
                ],
            }
        } else {
            RangeBreakoutDropDecision {
                action: RangeBreakoutDropAction::Flat,
                stop_price: None,
                target_prices: vec![],
                reasons,
            }
        }
    }

    /// 快照缺失时返回Flat信号（用于executor）
    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        RangeBreakoutDropDecision {
            action: RangeBreakoutDropAction::Flat,
            stop_price: None,
            target_prices: vec![],
            reasons: vec!["SNAPSHOT_MISSING".to_string()],
        }
        .to_signal(price, ts)
    }
}

/// 回测适配器
pub struct RangeBreakoutDropBacktestAdapter {
    symbol: String,
    tuning: RangeBreakoutDropBacktestTuning,
    cooldown_remaining: usize,
}

impl RangeBreakoutDropBacktestAdapter {
    pub fn new(symbol: &str, tuning: RangeBreakoutDropBacktestTuning) -> Self {
        Self {
            symbol: symbol.to_string(),
            tuning,
            cooldown_remaining: 0,
        }
    }

    /// 构建市场快照
    fn snapshot(&self, candles: &[CandleItem]) -> Option<RangeBreakoutDropSignalSnapshot> {
        if candles.is_empty() {
            return None;
        }

        let last = candles.last()?;
        let lookback = self.tuning.range_lookback_candles;

        if candles.len() < lookback + 2 {
            return None;
        }

        // 计算ATR
        let atr = atr_wilder(candles, self.tuning.atr_period)?;
        if atr <= 0.0 {
            return None;
        }

        // 识别震荡区间
        let range_candles = &candles[candles.len().saturating_sub(lookback + 1)..candles.len() - 1];
        if range_candles.len() < lookback {
            return None;
        }

        let range_high = range_candles
            .iter()
            .map(|c| c.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let range_low = range_candles
            .iter()
            .map(|c| c.l)
            .fold(f64::INFINITY, f64::min);
        let range_mid = (range_high + range_low) / 2.0;

        let range_volatility_pct = if range_mid > 0.0 {
            ((range_high - range_low) / range_mid) * 100.0
        } else {
            0.0
        };

        let in_ranging_mode = range_volatility_pct >= self.tuning.min_range_volatility_pct
            && range_volatility_pct <= self.tuning.max_range_volatility_pct;

        // 检查突破 - 修改：允许两种突破方式
        // 方式1：收盘价突破（严格）
        let close_breakout = last.c < range_low;
        // 方式2：最低价触及 + 阴线确认（宽松）
        let body_size = (last.o - last.c).max(0.0); // 阴线实体
        let candle_range = last.h - last.l;
        let is_bearish = last.c < last.o;
        let low_touched = last.l < range_low;
        let wick_breakout = low_touched && is_bearish && body_size > 0.0;

        // 只要满足任一种突破方式即可
        let breakout_confirmed = close_breakout || wick_breakout;

        // 记录突破类型，用于后续判断
        let is_close_breakout = close_breakout;

        let breakout_body_ratio = if candle_range > 0.0 {
            (last.o - last.c).abs() / candle_range
        } else {
            0.0
        };

        // 突破幅度计算：使用最低价和range_low的距离
        let breakout_move_atr = if atr > 0.0 {
            let move_distance = if close_breakout {
                range_low - last.c // 收盘价突破：用收盘价计算
            } else {
                range_low - last.l // 最低价触及：用最低价计算
            };
            move_distance / atr
        } else {
            0.0
        };

        let avg_volume =
            range_candles.iter().map(|c| c.v).sum::<f64>() / range_candles.len() as f64;
        let breakout_volume_mult = if avg_volume > 0.0 {
            last.v / avg_volume
        } else {
            0.0
        };

        let closes: Vec<f64> = candles.iter().map(|c| c.c).collect();
        let slow_ema = ema_at(&closes, closes.len(), self.tuning.slow_ema_period)?;
        let price_below_ema = last.c < slow_ema;

        // 计算长期EMA（如200EMA）用于市场环境过滤
        let long_term_ema = ema_at(&closes, closes.len(), self.tuning.long_term_ema_period)?;
        let price_below_long_term_ema = last.c < long_term_ema;

        let rsi = calculate_rsi(&closes, self.tuning.rsi_period)?;

        let candle_direction = if last.c > last.o {
            1
        } else if last.c < last.o {
            -1
        } else {
            0
        };

        Some(RangeBreakoutDropSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            range_high,
            range_low,
            range_volatility_pct,
            in_ranging_mode,
            breakout_confirmed,
            is_close_breakout,
            breakout_body_ratio,
            breakout_move_atr,
            breakout_volume_mult,
            slow_ema,
            price_below_ema,
            long_term_ema,
            price_below_long_term_ema,
            atr,
            rsi,
            candle_direction,
        })
    }
}

impl IndicatorStrategyBacktest for RangeBreakoutDropBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        let range_requirement = self.tuning.range_lookback_candles + 2;

        range_requirement
            .max(self.tuning.slow_ema_period)
            .max(self.tuning.long_term_ema_period)
            .max(self.tuning.atr_period + 1)
            .max(self.tuning.rsi_period + 1)
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

        let thresholds = self.tuning.thresholds();
        let mut decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);

        if !self.tuning.allow_short && decision.action == RangeBreakoutDropAction::Short {
            decision = RangeBreakoutDropDecision {
                action: RangeBreakoutDropAction::Flat,
                stop_price: None,
                target_prices: vec![],
                reasons: vec!["SHORT_DISABLED".to_string()],
            };
        }

        let mut signal = decision.to_signal(snapshot.price, last.ts);

        if signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }

        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

fn atr_wilder(candles: &[CandleItem], period: usize) -> Option<f64> {
    if period == 0 || candles.len() < period + 1 {
        return None;
    }

    let mut trs = Vec::new();
    for i in 1..candles.len() {
        let high_low = candles[i].h - candles[i].l;
        let high_close_prev = (candles[i].h - candles[i - 1].c).abs();
        let low_close_prev = (candles[i].l - candles[i - 1].c).abs();
        let tr = high_low.max(high_close_prev).max(low_close_prev);
        trs.push(tr);
    }

    if trs.len() < period {
        return None;
    }

    let atr_sum: f64 = trs[trs.len() - period..].iter().sum();
    Some(atr_sum / period as f64)
}

fn ema_at(data: &[f64], index: usize, period: usize) -> Option<f64> {
    if index < period || period == 0 {
        return None;
    }

    let alpha = 2.0 / (period as f64 + 1.0);
    // 用前 period 个值的 SMA 作为种子
    let seed_end = index - period;
    let sma: f64 = data[..period].iter().sum::<f64>() / period as f64;

    let mut ema = sma;
    // 从 period 开始迭代到 index
    for &value in &data[period..index] {
        ema = alpha * value + (1.0 - alpha) * ema;
    }
    let _ = seed_end;

    Some(ema)
}

fn calculate_rsi(closes: &[f64], period: usize) -> Option<f64> {
    if closes.len() < period + 1 {
        return None;
    }

    let mut gains = Vec::new();
    let mut losses = Vec::new();

    for i in 1..closes.len() {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(-change);
        }
    }

    if gains.len() < period {
        return None;
    }

    let avg_gain: f64 = gains[gains.len() - period..].iter().sum::<f64>() / period as f64;
    let avg_loss: f64 = losses[losses.len() - period..].iter().sum::<f64>() / period as f64;

    if avg_loss == 0.0 {
        return Some(100.0);
    }

    let rs = avg_gain / avg_loss;
    Some(100.0 - (100.0 / (1.0 + rs)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation() {
        let mut candles = Vec::new();
        let mut ts = 1704067200000i64;
        let base_price = 50000.0;

        // 默认配置包含 EMA200，夹具必须提供足够的历史 K 线。
        for i in 0..220 {
            candles.push(CandleItem {
                ts,
                o: base_price,
                h: base_price + 50.0,
                l: base_price - 50.0,
                c: base_price + ((i % 5) as f64 - 2.0) * 10.0,
                v: 1000.0,
                confirm: 1,
            });
            ts += 300_000;
        }

        let tuning = RangeBreakoutDropBacktestTuning::default();
        let adapter = RangeBreakoutDropBacktestAdapter::new("BTC-USDT-SWAP", tuning);

        let snapshot = adapter.snapshot(&candles);
        assert!(snapshot.is_some(), "快照应该能够成功创建");

        if let Some(snap) = snapshot {
            println!("快照创建成功:");
            println!("  价格: {:.2}", snap.price);
            println!("  震荡区间: {:.2} - {:.2}", snap.range_low, snap.range_high);
            println!("  震荡幅度: {:.2}%", snap.range_volatility_pct);
            println!("  震荡状态: {}", snap.in_ranging_mode);
            println!("  ATR: {:.2}", snap.atr);
            println!("  RSI: {:.2}", snap.rsi);
        }
    }

    #[test]
    fn test_evaluate_with_perfect_setup() {
        let snapshot = RangeBreakoutDropSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            price: 49000.0,
            range_high: 50000.0,
            range_low: 49500.0,
            range_volatility_pct: 1.0,
            in_ranging_mode: true,
            breakout_confirmed: true,
            is_close_breakout: true,
            breakout_body_ratio: 0.7,
            breakout_move_atr: 1.2,
            breakout_volume_mult: 2.0,
            slow_ema: 50000.0,
            price_below_ema: true,
            long_term_ema: 50000.0,
            price_below_long_term_ema: true,
            atr: 200.0,
            rsi: 50.0,
            candle_direction: -1,
        };

        let thresholds = RangeBreakoutDropThresholds::default();
        let decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);

        println!("评估结果:");
        println!("  动作: {:?}", decision.action);
        println!("  原因: {:?}", decision.reasons);

        assert!(
            matches!(decision.action, RangeBreakoutDropAction::Short),
            "完美设置应该产生做空信号"
        );
    }

    #[test]
    fn test_evaluate_blocked_by_no_ranging() {
        let snapshot = RangeBreakoutDropSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            price: 49000.0,
            range_high: 50000.0,
            range_low: 49500.0,
            range_volatility_pct: 1.0,
            in_ranging_mode: false,
            breakout_confirmed: true,
            is_close_breakout: true,
            breakout_body_ratio: 0.7,
            breakout_move_atr: 1.2,
            breakout_volume_mult: 2.0,
            slow_ema: 50000.0,
            price_below_ema: true,
            long_term_ema: 50000.0,
            price_below_long_term_ema: true,
            atr: 200.0,
            rsi: 50.0,
            candle_direction: -1,
        };

        let thresholds = RangeBreakoutDropThresholds::default();
        let decision = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot);

        println!("阻塞原因: {:?}", decision.reasons);

        assert!(
            matches!(decision.action, RangeBreakoutDropAction::Flat),
            "非震荡状态应该被过滤"
        );
        assert!(
            decision
                .reasons
                .contains(&"NOT_IN_RANGING_MODE".to_string()),
            "应该包含 NOT_IN_RANGING_MODE 原因"
        );
    }
}
