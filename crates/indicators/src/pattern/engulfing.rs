use rust_quant_common::CandleItem;
use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence};

/// 成交量比率指标
/// 计算当前成交量与历史n根K线的平均值的比值
#[derive(Debug, Clone)]
pub struct KlineEngulfingIndicator {
    //吞没形态指标值
    last_kline: Option<CandleItem>,
    //看涨||看跌吞没
    is_bullish: bool,
}

#[derive(Debug, Clone)]
pub struct KlineEngulfingOutput {
    pub is_engulfing: bool,
    pub body_ratio: f64,
}

impl KlineEngulfingIndicator {
    pub fn new() -> Self {
        Self {
            last_kline: None,
            is_bullish: false,
        }
    }
    pub fn next(&mut self, current_kline: &CandleItem) -> KlineEngulfingOutput {
        if self.last_kline.is_none() {
            self.last_kline = Some(current_kline.clone());
            return KlineEngulfingOutput {
                is_engulfing: false,
                body_ratio: 0.0,
            };
        }
        let last_kline = self.last_kline.as_ref().unwrap();

        //看涨吞没 ,当前k线的开盘价小于前一根k线的开盘价，且当前k线的收盘价大于前一根k线的收盘价,且当前k线的收盘价大于前一根k线的最高价
        let is_bullish = (current_kline.o < last_kline.o || current_kline.l < last_kline.l)
            && current_kline.c > last_kline.o
           && (current_kline.c > last_kline.h || current_kline.c>current_kline.h*1.005)
            //要求上一个根k线是阴线
            && last_kline.c < last_kline.o;

        //看跌吞没，当前k线的开盘价大于前一根k线的开盘价，且当前k线的收盘价小于前一根k线的开盘价,且当前k线的收盘价小于前一根k线的最低价
        let is_bearish = (current_kline.o > last_kline.o || current_kline.h > last_kline.h)
            && current_kline.c < last_kline.o
            && (current_kline.c < last_kline.l || current_kline.c < last_kline.l*1.005)
            //要求上一个根k线是阳线
            && last_kline.c > last_kline.o;

        let body_ratio = if is_bullish || is_bearish {
            //计算实体比例,当前k线实体部分与当前k线路的上下影线部分的比例
            let body_size = (current_kline.c - current_kline.o).abs();
            let shadow_size = current_kline.h - current_kline.l;
            body_size / shadow_size
        } else {
            0.0
        };
        self.last_kline = Some(current_kline.clone());
        KlineEngulfingOutput {
            is_engulfing: is_bullish || is_bearish,
            body_ratio,
        }
    }
}

impl Default for KlineEngulfingIndicator {
    fn default() -> Self {
        Self::new()
    }
}

//添加测试单例
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_engulfing_indicator() {
        let mut indicator = KlineEngulfingIndicator::new();
        // 创建一个看涨吞没的例子
        let kline1 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 95.0,
            ts: 0,
            v: 0.00,
            confirm: 0,
        };
        let kline2 = CandleItem {
            o: 90.0,
            h: 105.0,
            l: 85.0,
            c: 100.0,
            ts: 1,
            v: 0.00,
            confirm: 0,
        };

        indicator.next(&kline1);
        let output = indicator.next(&kline2);
        assert!(output.is_engulfing);
        assert!(output.body_ratio > 0.0);

        // 创建一个看跌吞没的例子
        let kline3 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 105.0,
            ts: 2,
            v: 0.00,
            confirm: 0,
        };
        let kline4 = CandleItem {
            o: 110.0,
            h: 115.0,
            l: 95.0,
            c: 100.0,
            ts: 3,
            v: 0.00,
            confirm: 0,
        };

        indicator.next(&kline3);
        let output = indicator.next(&kline4);
        println!("{:?}", output);
        assert!(output.is_engulfing);
        assert!(output.body_ratio > 0.0);

        // 创建一个非吞没的例子
        let kline5 = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 105.0,
            ts: 4,
            v: 0.00,
            confirm: 0,
        };
        let kline6 = CandleItem {
            o: 105.0,
            h: 115.0,
            l: 95.0,
            c: 110.0,
            ts: 5,
            v: 0.00,
            confirm: 0,
        };

        indicator.next(&kline5);
        let output = indicator.next(&kline6);
        assert!(!output.is_engulfing);
        assert_eq!(output.body_ratio, 0.0);
    }
}
