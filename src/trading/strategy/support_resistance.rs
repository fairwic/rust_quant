use chrono::{DateTime, NaiveDateTime, Utc};
use rbatis::rbatis_codegen::ops::AsProxy;
use serde::{Deserialize, Serialize};
use ta::indicators::{BollingerBands, CommodityChannelIndex, ExponentialMovingAverage, Maximum, Minimum};
use ta::{DataItem, Next};
use crate::trading::model::market::candles::CandlesEntity;

pub struct SupportResistance;

impl SupportResistance {
    /// 方法1：分段数据
    /// 将历史数据分段，并计算每段的最高价和最低价
    pub fn segment_data(candles: &Vec<CandlesEntity>, segment_size: usize) -> (f64, f64) {
        let mut resistance_level = f64::MIN;
        let mut support_level = f64::MAX;

        for segment in candles.chunks(segment_size) {
            // 提取每个分段中的最高价和最低价
            let high_prices: Vec<f64> = segment.iter().map(|c| c.h.parse::<f64>().unwrap_or(0.0)).collect();
            let low_prices: Vec<f64> = segment.iter().map(|c| c.l.parse::<f64>().unwrap_or(0.0)).collect();

            // 找出当前分段中的最高价和最低价
            let segment_resistance = high_prices.iter().cloned().fold(f64::NAN, f64::max);
            let segment_support = low_prices.iter().cloned().fold(f64::NAN, f64::min);

            // 更新全局的阻力位和支撑位
            resistance_level = resistance_level.max(segment_resistance);
            support_level = support_level.min(segment_support);
        }

        (resistance_level, support_level)
    }

    /// 方法2：使用布林带
    /// 使用布林带计算上轨和下轨，分别作为阻力位和支撑位
    pub fn bollinger_bands(candles: &[CandlesEntity], period: usize) -> (f64, f64) {
        // 提取收盘价
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let mut bb = BollingerBands::new(period, 2.0).unwrap();

        let mut upper_band = f64::MIN;
        let mut lower_band = f64::MAX;

        // 计算布林带的上下轨
        for price in close_prices {
            let bands = bb.next(price);
            upper_band = upper_band.max(bands.upper);
            lower_band = lower_band.min(bands.lower);
        }

        (upper_band, lower_band)
    }

    /// 方法2：使用计算出布林带
    /// 使用布林带计算最新价格的上轨、中轨和下轨，分别作为阻力位和支撑位
    pub fn bollinger_bands_latest(candles: &[CandlesEntity], period: usize) -> (f64, f64, f64) {
        // 提取收盘价
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let mut bb = BollingerBands::new(period, 2.0).unwrap();

        // 计算布林带的上下轨和中轨
        let bands = close_prices.iter().map(|&price| bb.next(price)).last().unwrap();

        (bands.upper, bands.average, bands.lower)
    }


    /// 方法3：找出波峰和波谷
    /// 找出交易量中的波峰和波谷，并返回相应的时间点
    /// 方法3：找出波峰和波谷
    /// 找出交易量中的波峰和波谷，并返回相应的时间点
    pub fn peaks_and_valleys(candles: &[CandlesEntity]) -> (Vec<(String, f64)>, Vec<(String, f64)>) {
        let mut peaks = Vec::new();
        let mut valleys = Vec::new();

        for i in 1..candles.len() - 1 {
            let prev_vol = candles[i - 1].vol.parse::<f64>().unwrap_or(0.0);
            let curr_vol = candles[i].vol.parse::<f64>().unwrap_or(0.0);
            let next_vol = candles[i + 1].vol.parse::<f64>().unwrap_or(0.0);

            let timestamp = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp_millis(candles[i].ts.u64() as i64).unwrap(),
                Utc,
            ).format("%Y-%m-%d %H:%M:%S")
                .to_string();

            if curr_vol > prev_vol && curr_vol > next_vol {
                peaks.push((timestamp.clone(), curr_vol));
            } else if curr_vol < prev_vol && curr_vol < next_vol {
                valleys.push((timestamp.clone(), curr_vol));
            }
        }

        (peaks, valleys)
    }

    /// 方法4：分形理论
    /// 使用分形理论找出支撑位和阻力位
    pub fn fractal(candles: &[CandlesEntity]) -> (Vec<f64>, Vec<f64>) {
        let mut fractal_highs = Vec::new();
        let mut fractal_lows = Vec::new();

        // 遍历每个价格，找到分形高点和低点
        for i in 2..candles.len() - 2 {
            let high1 = candles[i - 2].h.parse::<f64>().unwrap_or(0.0);
            let high2 = candles[i - 1].h.parse::<f64>().unwrap_or(0.0);
            let high3 = candles[i].h.parse::<f64>().unwrap_or(0.0);
            let high4 = candles[i + 1].h.parse::<f64>().unwrap_or(0.0);
            let high5 = candles[i + 2].h.parse::<f64>().unwrap_or(0.0);

            let low1 = candles[i - 2].l.parse::<f64>().unwrap_or(0.0);
            let low2 = candles[i - 1].l.parse::<f64>().unwrap_or(0.0);
            let low3 = candles[i].l.parse::<f64>().unwrap_or(0.0);
            let low4 = candles[i + 1].l.parse::<f64>().unwrap_or(0.0);
            let low5 = candles[i + 2].l.parse::<f64>().unwrap_or(0.0);

            // 高点高于前后两个高点，视为分形高点
            if high3 > high2 && high3 > high1 && high3 > high4 && high3 > high5 {
                fractal_highs.push(high3);
            }

            // 低点低于前后两个低点，视为分形低点
            if low3 < low2 && low3 < low1 && low3 < low4 && low3 < low5 {
                fractal_lows.push(low3);
            }
        }

        (fractal_highs, fractal_lows)
    }

    /// 方法5：手动标注高点和低点
    /// 手动标注前期显著高点和低点，作为支撑位和阻力位
    pub fn manual_marking(candles: &[CandlesEntity]) -> (f64, f64) {
        // 提取每个蜡烛图的最高价和最低价
        let high_prices: Vec<f64> = candles.iter().map(|c| c.h.parse::<f64>().unwrap_or(0.0)).collect();
        let low_prices: Vec<f64> = candles.iter().map(|c| c.l.parse::<f64>().unwrap_or(0.0)).collect();

        // 找出所有价格中的最高价和最低价
        let resistance_level = high_prices.iter().cloned().fold(f64::NAN, f64::max);
        let support_level = low_prices.iter().cloned().fold(f64::NAN, f64::min);

        (resistance_level, support_level)
    }

    /// 方法6：使用KAMA计算支撑位和阻力位
    /// KAMA 指标全称为考夫曼自适应移动平均线(Kaufman's Adaptive Moving Average),它是一种自适应性指数移动平均线,旨在跟踪价格波动和趋势变化。KAMA值的计算过程如下:
    //
    // 首先需要计算长周期的EMA(指数移动平均线), 计算公式如下:
    //
    // EMA = Price(当前价格) * k + EMA(前一日) * (1-k)
    // 其中k = 2/(N+1), N为设定的时间周期数
    //
    // 再计算短周期的EMA:
    //
    // 短周期EMA = Price(当前价格) * k快 + 短周期EMA(前一日) * (1-k快)
    // 其中k快 = 2/(快期+1), 快期一般取10
    //
    // 计算ER(有效比率):
    //
    // ER = ER(前一日) + SC * (短周期EMA - 长周期EMA)^2
    // SC是平滑常数,一般取2/(30+1)=0.0667
    //
    // 计算ER比率:
    //
    // ER比率 = ER/(ER + ER平方根)
    //
    // 最后计算KAMA值:
    //
    // KAMA = KAMA(前一日) + ER比率 * (Price(当前价格) - KAMA(前一日))
    // 初始KAMA值可取与设定长周期数相同的SMA值。
    // KAMA适应性很强,会根据市场波动自动调整权重。当市场波动较大时,KAMA对当前价格的反应更为敏捷;当市场走平时,KAMA对价格的反应较为迟缓。因此KAMA能较好地跟踪价格趋势变化。
    pub fn kama(candles: &[CandlesEntity], period: usize) -> (f64, f64) {
        // 提取收盘价
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let mut kama = ExponentialMovingAverage::new(period).unwrap();

        let mut resistance_level = f64::MIN;
        let mut support_level = f64::MAX;

        // 计算KAMA值，并找出最高和最低KAMA值
        for price in close_prices {
            let value = kama.next(price);
            resistance_level = resistance_level.max(value);
            support_level = support_level.min(value);
        }

        (resistance_level, support_level)
    }
    /// 方法7：使用CCI计算支撑位和阻力位
    pub fn cci(candles: &[CandlesEntity], period: usize) -> (Vec<f64>, Vec<f64>) {
        // 提取蜡烛图的最高价、最低价和收盘价
        let high_prices: Vec<f64> = candles.iter().map(|c| c.h.parse::<f64>().unwrap_or(0.0)).collect();
        let low_prices: Vec<f64> = candles.iter().map(|c| c.l.parse::<f64>().unwrap_or(0.0)).collect();
        let close_prices: Vec<f64> = candles.iter().map(|c| c.c.parse::<f64>().unwrap_or(0.0)).collect();
        let mut cci = CommodityChannelIndex::new(period).unwrap();

        let mut overbought = Vec::new();
        let mut oversold = Vec::new();

        // 计算每个典型价格的CCI值，并找出超买和超卖区域
        for i in 0..close_prices.len() {
            let data_item = DataItem::builder()
                .high(high_prices[i])
                .low(low_prices[i])
                .close(close_prices[i])
                .open(close_prices[i]) // 需要提供open价格
                .volume(1.0) // 需要提供volume
                .build()
                .unwrap();

            let value = cci.next(&data_item);
            if value > 100.0 {
                overbought.push(value);
            } else if value < -100.0 {
                oversold.push(value);
            }
        }

        (overbought, oversold)
    }
}
