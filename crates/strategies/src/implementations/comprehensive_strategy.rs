use crate::adapters::candle_adapter;
use rust_quant_common::utils::time;
use rust_quant_market::models::CandlesEntity;
use ta::indicators::KeltnerChannel;
use ta::indicators::{
    BollingerBands, ExponentialMovingAverage, RelativeStrengthIndex, SimpleMovingAverage, TrueRange,
};
use ta::{Close, High, Low, Next};

/// Squeeze 结构体
struct Squeeze {
    bb: BollingerBands,
    kc_high: KeltnerChannel,
    kc_mid: KeltnerChannel,
    kc_low: KeltnerChannel,
}

impl Squeeze {
    /// 创建新的 Squeeze 实例
    pub fn new(
        bb_mult: f64,
        kc_mult_high: f64,
        kc_mult_mid: f64,
        kc_mult_low: f64,
        ttm_length: usize,
    ) -> Self {
        let bb = BollingerBands::new(bb_mult as usize, ttm_length as f64).unwrap();
        let kc_high = KeltnerChannel::new(kc_mult_high as usize, ttm_length as f64).unwrap();
        let kc_mid = KeltnerChannel::new(kc_mult_mid as usize, ttm_length as f64).unwrap();
        let kc_low = KeltnerChannel::new(kc_mult_low as usize, ttm_length as f64).unwrap();

        Squeeze {
            bb,
            kc_high,
            kc_mid,
            kc_low,
        }
    }

    /// 计算 Squeeze 条件
    pub fn calculate(&mut self, price: f64) -> (bool, bool, bool, bool) {
        let bb_value = self.bb.next(price);
        let kc_high_value = self.kc_high.next(price);
        let kc_mid_value = self.kc_mid.next(price);
        let kc_low_value = self.kc_low.next(price);

        let no_squeeze = bb_value.lower < kc_low_value.lower || bb_value.upper > kc_low_value.upper;
        // let no_squeeze = bb_value.lower < kc_low_value.lower || bb_value.upper > kc_low_value.upper;
        let low_squeeze =
            bb_value.lower >= kc_low_value.lower && bb_value.upper <= kc_low_value.upper;
        let mid_squeeze =
            bb_value.lower >= kc_mid_value.lower && bb_value.upper <= kc_mid_value.upper;
        let high_squeeze =
            bb_value.lower >= kc_high_value.lower && bb_value.upper <= kc_high_value.upper;

        (no_squeeze, low_squeeze, mid_squeeze, high_squeeze)
    }
}

/// 综合策略结构体，包含所有必要的参数和数据
pub struct ComprehensiveStrategy {
    /// 5分钟蜡烛图数据
    pub candles_5m: Vec<CandlesEntity>,
    /// ADX（平均趋向指数）周期
    pub adx_period: usize,
    /// ADX（平均趋向指数）平滑周期
    pub adx_smoothing: usize,
    /// Andean Oscillator 的长度
    pub andean_length: usize,
    /// EMA信号长度
    pub sig_length: usize,
    /// 布林带标准差倍数
    pub bb_mult: f64,
    /// Keltner Channel 高倍数
    pub kc_mult_high: f64,
    /// Keltner Channel 中倍数
    pub kc_mult_mid: f64,
    /// Keltner Channel 低倍数
    pub kc_mult_low: f64,
    /// TTM Squeeze 长度
    pub ttm_length: usize,
    /// 止损百分比
    pub stop_loss_percent: f64,
}

// ❌ 移除：违反孤儿规则的trait实现
// 不能为外部类型 CandlesEntity 实现外部 trait (High, Low, Close)
// ✅ 新方案：使用 adapters::candle_adapter 模块的适配器

impl ComprehensiveStrategy {
    /// 综合策略函数，执行交易策略并返回最终资金、胜率和开仓次数
    pub async fn comprehensive_strategy(&self) -> (f64, f64, usize) {
        let initial_funds = 100.0;
        let mut funds = initial_funds;
        let mut position: f64 = 0.0;
        let mut wins = 0;
        let mut losses = 0;
        let mut open_trades = 0;

        // 计算 ADX 值
        println!(
            "计算adx,adx_smoothing:{},adx_period:{}",
            self.adx_smoothing, self.adx_period
        );
        let adx_values = ComprehensiveStrategy::calculate_adx(
            &self.candles_5m,
            self.adx_smoothing,
            self.adx_period,
        );

        // 初始化 Squeeze
        let mut squeeze = Squeeze::new(
            self.bb_mult,
            self.kc_mult_high,
            self.kc_mult_mid,
            self.kc_mult_low,
            self.ttm_length,
        );

        // 初始化 Andean Oscillator 参数
        let mut andean_oscillator = AndeanOscillator::new(self.andean_length, self.sig_length);

        // 计算线性回归值
        let linreg_values =
            ComprehensiveStrategy::calculate_linreg(&self.candles_5m, self.ttm_length);

        let mut prev_mom = 0.0;

        for (i, candle) in self.candles_5m.iter().enumerate() {
            let current_price = candle.c.parse::<f64>().unwrap_or_else(|e| {
                eprintln!("Failed to parse price: {}", e);
                0.0
            });

            let high_price = candle.h.parse::<f64>().unwrap_or(0.0);
            let low_price = candle.l.parse::<f64>().unwrap_or(0.0);
            let open_price = candle.o.parse::<f64>().unwrap_or(0.0);

            // 获取 ADX 值
            let adx_value = adx_values[i];

            // 计算 Andean Oscillator
            let (bull, bear, signal) = andean_oscillator.next(current_price, open_price);
            // let signal = andean_oscillator.ema_signal.value();

            // 计算 TTM Squeeze
            let (no_squeeze, low_squeeze, mid_squeeze, high_squeeze) =
                squeeze.calculate(current_price);

            // 获取动量值
            let mom = linreg_values[i];

            // Squeeze 条件
            let sq_color_green = !no_squeeze && mom > prev_mom;
            let sq_color_non_green = no_squeeze || mom <= prev_mom;

            // 综合策略条件
            let buy_condition_adx = adx_value < 20.0;
            let buy_condition_andean = bull > signal;
            let buy_condition_ttm = sq_color_green;

            let sell_condition_adx = adx_value <= 20.0;
            let sell_condition_andean = signal <= bear;
            let sell_condition_ttm = sq_color_non_green;

            let buy_condition = buy_condition_adx && buy_condition_andean && buy_condition_ttm;
            let sell_condition = sell_condition_adx && sell_condition_andean && sell_condition_ttm;

            // 添加日志记录
            println!("Time: {:?}, ADX: {}, Bull: {}, Bear: {}, Signal: {}, MOM: {}, Low Squeeze: {}, Mid Squeeze: {}, No Squeeze: {}",
                     rust_quant_common::utils::time::mill_time_to_datetime_shanghai(candle.ts), adx_value, bull, bear, signal, mom, low_squeeze, mid_squeeze, no_squeeze);

            if buy_condition && position.abs() < f64::EPSILON {
                // 当满足买入条件时开多仓
                position = funds / current_price;
                funds = 0.0;
                open_trades += 1;
                println!(
                    "Buy at time: {}, price: {}, position: {}",
                    candle.ts, current_price, position
                );
            } else if sell_condition && position > 0.0 {
                // 当满足卖出条件时平多仓
                funds = position * current_price;
                position = 0.0;
                println!(
                    "Sell (close long) at time: {}, price: {}, funds: {}",
                    candle.ts, current_price, funds
                );
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            } else if position > 0.0 && current_price < position * (1.0 - self.stop_loss_percent) {
                // 当价格达到止损线时平多仓
                funds = position * current_price;
                position = 0.0;
                losses += 1;
                println!(
                    "Stop loss at time: {}, price: {}, funds: {}",
                    candle.ts, current_price, funds
                );
            }

            prev_mom = mom;
        }

        if position > 0.0 {
            // 如果最后还有多仓未平仓，按最后一个价格平仓
            if let Some(last_candle) = self.candles_5m.last() {
                let last_price = last_candle.c.parse::<f64>().unwrap_or_else(|e| {
                    eprintln!("Failed to parse price: {}", e);
                    0.0
                });
                funds = position * last_price;
                position = 0.0;
                println!("Final sell at price: {}, funds: {}", last_price, funds);
                if funds > initial_funds {
                    wins += 1;
                } else {
                    losses += 1;
                }
            }
        }

        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            0.0
        };

        println!("Final Win rate: {}", win_rate);
        (funds, win_rate, open_trades)
    }

    fn rma(data: &[f64], length: usize) -> Vec<f64> {
        let mut rma = vec![0.0; data.len()];
        if data.len() > 0 {
            rma[0] = data[0];
            for i in 1..data.len() {
                rma[i] = (data[i] + (length as f64 - 1.0) * rma[i - 1]) / length as f64;
            }
        }
        rma
    }

    fn calculate_adx(candles: &[CandlesEntity], adx_len: usize, di_len: usize) -> Vec<f64> {
        let mut adx_values = vec![0.0; candles.len()];

        let mut prev_high = 0.0;
        let mut prev_low = 0.0;
        let mut plus_dm = vec![0.0; candles.len()];
        let mut minus_dm = vec![0.0; candles.len()];
        let mut tr = vec![0.0; candles.len()];

        for (i, candle) in candles.iter().enumerate() {
            // 使用适配器访问K线数据
            let adapter = candle_adapter::adapt(candle);
            let high = adapter.high();
            let low = adapter.low();
            let close = adapter.close();

            if i > 0 {
                let up = high - prev_high;
                let down = prev_low - low;
                plus_dm[i] = if up > down && up > 0.0 { up } else { 0.0 };
                minus_dm[i] = if down > up && down > 0.0 { down } else { 0.0 };
            }

            tr[i] = TrueRange::new().next(&adapter);
            prev_high = high;
            prev_low = low;
        }

        let smoothed_plus_dm = Self::rma(&plus_dm[di_len..], di_len);
        let smoothed_minus_dm = Self::rma(&minus_dm[di_len..], di_len);
        let smoothed_tr = Self::rma(&tr[di_len..], di_len);

        let mut dx = vec![0.0; candles.len() - di_len];

        for i in 0..dx.len() {
            let plus_di = 100.0 * smoothed_plus_dm[i] / smoothed_tr[i];
            let minus_di = 100.0 * smoothed_minus_dm[i] / smoothed_tr[i];
            let sum = plus_di + minus_di;
            dx[i] = 100.0 * (plus_di - minus_di).abs() / if sum == 0.0 { 1.0 } else { sum };
        }

        adx_values[adx_len + di_len..].copy_from_slice(&Self::rma(&dx[adx_len..], adx_len));

        adx_values
    }

    // 线性回归计算函数
    fn calculate_linreg(candles: &[CandlesEntity], length: usize) -> Vec<f64> {
        let mut linreg_values = vec![0.0; candles.len()];
        let mut sma_close = SimpleMovingAverage::new(length).unwrap();

        for i in length..candles.len() {
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut sum_xy = 0.0;
            let mut sum_xx = 0.0;

            // 计算高低价格的复合平均值
            let high_avg = candles[i - length + 1..=i]
                .iter()
                .map(|c| c.h.parse::<f64>().unwrap_or(0.0))
                .fold(0.0, |a, b| a + b)
                / length as f64;
            let low_avg = candles[i - length + 1..=i]
                .iter()
                .map(|c| c.l.parse::<f64>().unwrap_or(0.0))
                .fold(0.0, |a, b| a + b)
                / length as f64;
            let sma_value = sma_close.next(candles[i].c.parse::<f64>().unwrap_or(0.0));
            let compound_avg = (high_avg + low_avg + sma_value) / 3.0;

            for j in 0..length {
                let x = j as f64;
                let y = candles[i - j].c.parse::<f64>().unwrap_or(0.0) - compound_avg;
                sum_x += x;
                sum_y += y;
                sum_xy += x * y;
                sum_xx += x * x;
            }
            let slope =
                (length as f64 * sum_xy - sum_x * sum_y) / (length as f64 * sum_xx - sum_x * sum_x);
            linreg_values[i] = slope;
        }

        linreg_values
    }
}

/// Andean Oscillator 结构体
struct AndeanOscillator {
    alpha: f64,
    sig_length: usize,
    up1: Option<f64>,
    up2: Option<f64>,
    dn1: Option<f64>,
    dn2: Option<f64>,
    ema_signal: ExponentialMovingAverage,
}

impl AndeanOscillator {
    /// 创建新的 Andean Oscillator 实例
    pub fn new(length: usize, sig_length: usize) -> Self {
        let alpha = 2.0 / (length as f64 + 1.0);
        let ema_signal = ExponentialMovingAverage::new(sig_length).unwrap();
        AndeanOscillator {
            alpha,
            sig_length,
            up1: None,
            up2: None,
            dn1: None,
            dn2: None,
            ema_signal,
        }
    }

    /// Helper function to replace None with a default value
    fn nz(&self, value: Option<f64>, default: f64) -> f64 {
        value.unwrap_or(default)
    }

    /// 计算 Andean Oscillator 的 bull 和 bear 值
    pub fn next(&mut self, close: f64, open: f64) -> (f64, f64, f64) {
        // 使用初始值或者之前的值进行计算
        let prev_up1 = self.nz(self.up1, close);
        let prev_up2 = self.nz(self.up2, close * close);
        let prev_dn1 = self.nz(self.dn1, close);
        let prev_dn2 = self.nz(self.dn2, close * close);

        // 计算新的 up1, up2, dn1, dn2 值
        self.up1 = Some((prev_up1 - (prev_up1 - close) * self.alpha).max(open.max(close)));
        self.up2 = Some((prev_up2 - (prev_up2 - close * close) * self.alpha).max(open * open));
        self.dn1 = Some((prev_dn1 + (close - prev_dn1) * self.alpha).min(open.min(close)));
        self.dn2 = Some((prev_dn2 + (close * close - prev_dn2) * self.alpha).min(open * open));

        // 计算 bull 和 bear 值
        let bull = (self.dn2.unwrap() - self.dn1.unwrap() * self.dn1.unwrap()).sqrt();
        let bear = (self.up2.unwrap() - self.up1.unwrap() * self.up1.unwrap()).sqrt();

        // 计算信号值
        let signal = self.ema_signal.next(bull.max(bear));

        (bull, bear, signal)
    }
}
