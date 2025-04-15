use crate::trading::indicator::rma::Rma;
use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence};

/// 成交量比率指标
/// 计算当前成交量与历史n根K线的平均值的比值
#[derive(Debug,Clone)]
pub struct VolumeRatioIndicator {
    prev_volumes: Vec<f64>,
    volume_bar_num: usize,
    is_fitler_last_volume: bool,
}

impl VolumeRatioIndicator {
    pub fn new(length: usize, is_fitler_last_volume: bool) -> Self {
        let mut length = length;
        if is_fitler_last_volume {
            length = length + 1;
        }
        Self {
            prev_volumes: vec![],
            volume_bar_num: length,
            is_fitler_last_volume: is_fitler_last_volume,
        }
    }

    pub fn next(&mut self, current_volume: f64) -> f64 {
        //只保留前N根K线的成交量
        if self.prev_volumes.len() > self.volume_bar_num {
            self.prev_volumes.remove(0);
        }
        let volume_ratio = current_volume / self.avg_volume();
        self.prev_volumes.push(current_volume);
        volume_ratio
    }
    pub fn avg_volume(&self) -> f64 {
        if self.is_fitler_last_volume && self.prev_volumes.len() > 1 {
            //去除最后一根k线
            self.prev_volumes
                .iter()
                .take(self.prev_volumes.len() - 1)
                .sum::<f64>()
                / (self.prev_volumes.len() - 1) as f64
        } else {
            self.prev_volumes.iter().sum::<f64>() / self.prev_volumes.len() as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_ratio_indicator() {
        let mut indicator = VolumeRatioIndicator::new(3, false);
        indicator.next(100.0);
        indicator.next(200.0);
        indicator.next(300.0);
        assert_eq!(indicator.avg_volume(), 200.0);
    }
    #[test]
    fn test_volume_ratio_indicator_next() {
        let mut indicator = VolumeRatioIndicator::new(3, false);
        indicator.next(100.0);
        indicator.next(200.0);
        indicator.next(300.0);
        assert_eq!(indicator.next(400.0), 2.0);
    }

    #[test]
    fn test_volume_ratio_indicator_filter_last_volume() {
        let mut indicator = VolumeRatioIndicator::new(3, true);
        indicator.next(100.0);
        indicator.next(200.0);
        indicator.next(300.0);
        indicator.next(400.0);
        assert_eq!(indicator.avg_volume(), 200.0);
    }
    #[test]
    fn test_volume_ratio_indicator_next_filter_last_volume() {
        let mut indicator = VolumeRatioIndicator::new(3, true);
        indicator.next(100.0);
        indicator.next(200.0);
        indicator.next(300.0);
        indicator.next(400.0);

        assert_eq!(indicator.next(400.0), 2.0);
    }
}
