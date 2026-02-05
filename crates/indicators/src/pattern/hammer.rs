use rust_quant_common::CandleItem;

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
        let kline = CandleItem {
            o: 100.0,
            h: 102.0,
            l: 70.0,
            c: 101.0,
            v: 100.0,
            ts: 1749650400000,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        println!("indicator: {:?}", output);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);
    }
    #[test]
    fn test_engulfing_indicator() {
        let mut indicator = KlineHammerIndicator::new(0.7, 0.7);

        // 价格下跌，一个锤子形态（长下影线）
        let kline = CandleItem {
            o: 100.0,
            h: 102.0,
            l: 70.0,
            c: 95.0,
            v: 100.0,
            ts: 1000,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);

        // 价格上涨，一个锤子形态（长下影线）
        let kline = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 80.0,
            c: 100.5,
            v: 100.0,
            ts: 1001,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);

        // 价格上涨，上吊线形态（长上影线）
        let kline = CandleItem {
            o: 100.0,
            h: 130.0,
            l: 98.0,
            c: 99.0,
            v: 100.0,
            ts: 1002,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        assert!(!output.is_hammer);
        assert!(output.is_hanging_man);

        // 上下影线都不够长：非锤子/上吊线
        let kline = CandleItem {
            o: 100.0,
            h: 110.0,
            l: 90.0,
            c: 101.0,
            v: 100.0,
            ts: 1003,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        assert!(!output.is_hammer);
        assert!(!output.is_hanging_man);

        // 平头上涨锤子形态
        let kline = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 50.0,
            c: 101.0,
            v: 100.0,
            ts: 1004,
            confirm: 0,
        };
        let output = indicator.next(&kline);
        assert!(output.is_hammer);
        assert!(!output.is_hanging_man);
    }
}
