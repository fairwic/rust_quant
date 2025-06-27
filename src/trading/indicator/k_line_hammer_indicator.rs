use crate::{trading::indicator::rma::Rma, CandleItem};
use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence};

/// 锤子/上吊线形态指标
#[derive(Debug, Clone)]
pub struct KlineHammerIndicator {
    stander_down_shadow_ratio: f64,
    stander_up_shadow_ratio: f64,
}
impl Default for KlineHammerIndicator {
    fn default() -> Self {
        Self::new(0.7, 0.7)
    }
}
/// 锤子/上吊线形态指标
#[derive(Debug, Clone)]
pub struct KlineHammerIndicatorOutput {
    //是否是锤子形态,是指下影线较长,上影线较短的形态
    pub is_hammer: bool,
    //是否是上吊线形态,是指上影线较长,下影线较短的形态
    pub is_hanging_man: bool,
    //下影线比例
    pub down_shadow_ratio: f64,
    //上影线比例
    pub up_shadow_ratio: f64,
    //实体比例
    pub body_ratio: f64,
}

impl KlineHammerIndicator {
    pub fn new(low_shadow_ratio: f64, up_shadow_ratio: f64) -> Self {
        Self {
            stander_down_shadow_ratio: low_shadow_ratio,
            stander_up_shadow_ratio: up_shadow_ratio,
        }
    }
    pub fn next(&mut self, current_kline: &CandleItem) -> KlineHammerIndicatorOutput {
        //计算下影线比例
        let down_shadow_ratio = if current_kline.o > current_kline.c {
            //价格下跌
            (current_kline.c - current_kline.l) / (current_kline.h - current_kline.l)
        } else {
            //价格上涨
            (current_kline.o - current_kline.l) / (current_kline.h - current_kline.l)
        };
        //是否是长下影线
        //   let kline = CandleItem {
        //         o: 100.0,
        //         h: 110.0,
        //         l: 50.0,
        //         c: 90.0,
        //         v: 100.0,
        //         ts: 1000,
        //     };
        //计算上影线比例
        let up_shadow_ratio = if current_kline.o > current_kline.c {
            (current_kline.h - current_kline.o) / (current_kline.h - current_kline.l)
        } else {
            (current_kline.h - current_kline.c) / (current_kline.h - current_kline.l)
        };

        //计算实体比例
        let body_ratio = if current_kline.c > current_kline.o {
            (current_kline.c - current_kline.o) / (current_kline.h - current_kline.l)
        } else {
            (current_kline.o - current_kline.c) / (current_kline.h - current_kline.l)
        };

        //是否是长下影线
        let is_hammer = down_shadow_ratio > self.stander_down_shadow_ratio
            && down_shadow_ratio > up_shadow_ratio;

        //是否是长上影线
        let is_hanging_man =
            up_shadow_ratio > self.stander_up_shadow_ratio && up_shadow_ratio > down_shadow_ratio;

        KlineHammerIndicatorOutput {
            is_hammer,
            is_hanging_man,
            down_shadow_ratio,
            up_shadow_ratio,
            body_ratio,
        }
    }
}

//添加测试单例
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kline_hammer_indicator() {
        let mut indicator = KlineHammerIndicator::new(0.7, 0.7);
        // 109623	110360	109582	109829.5
        let kline = CandleItem {
            o: 103687.2,
            h: 104171.4,
            l: 103571.4,
            c: 103640.2,
            v: 100.0,
            ts: 1749650400000,
        };
        let output = indicator.next(&kline);
        println!("indicator: {:?}", output);
        assert!(output.is_hammer);
    }
    #[test]
    fn test_engulfing_indicator() {
        println!("创建一个锤子形态指标");
        let mut indicator = KlineHammerIndicator::new(0.7, 0.7);
        println!("创建一个价格下跌，一个锤子形态");
        // 创建价格是下跌的，一个锤子形态
        let kline = CandleItem {
            o: 100.0,
            h: 100.1,
            l: 50.0,
            c: 90.0,
            v: 100.0,
            ts: 1000,
        };
        let output = indicator.next(&kline);
        println!("indicator: {:?}", output);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);

        println!("创建一个价格上涨，一个锤子形态");
        //价格上涨的，一个锤子形态
        let kline = CandleItem {
            o: 100.0,
            h: 100.1,
            l: 90.0,
            c: 100.5,
            v: 100.0,
            ts: 1000,
        };
        indicator.next(&kline);
        println!("indicator: {:?}", indicator);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);

        println!("创建一个价格上涨，上吊线形态");
        // 创建一个价格上涨，上吊线形态
        let kline = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 99.9,
            c: 101.0,
            v: 100.0,
            ts: 1000,
        };
        indicator.next(&kline);
        println!("indicator: {:?}", indicator);
        assert!(!output.is_hammer);
        assert!(output.is_hanging_man);

        println!("创建一个价格下跌，上吊线路");
        // 创建一个价格下跌，上吊线路
        let kline = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 99.8,
            c: 99.0,
            v: 100.0,
            ts: 1000,
        };

        indicator.next(&kline);
        println!("indicator: {:?}", output);
        assert!(!output.is_hammer);
        assert!(output.is_hanging_man);

        println!("创建一个价格上涨，长上影线，和下影线长度相等");
        // 创建一个价格上涨，长上影线，和下影线长度相等
        let kline = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 101.0,
            v: 100.0,
            ts: 1000,
        };
        indicator.next(&kline);
        println!("indicator: {:?}", indicator);
        assert!(!output.is_hammer);
        assert!(!output.is_hanging_man);

        // 创建平头上涨锤子形态
        println!("创建平头上涨锤子形态");
        let kline = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 50.0,
            c: 101.0,
            v: 100.0,
            ts: 1000,
        };
        let output = indicator.next(&kline);
        println!("indicator: {:?}", output);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);
    }
}
