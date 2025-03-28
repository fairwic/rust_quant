use crate::trading::indicator::rma::Rma;
use ta::indicators::{ExponentialMovingAverage, MovingAverageConvergenceDivergence};

/// 成交量比率指标
/// 计算当前成交量与历史n根K线的平均值的比值
#[derive(Debug)]
pub struct VolumeRatioIndicator {
    prev_volumes: Vec<f64>,
    volume_bar_num: usize,
}

impl VolumeRatioIndicator {
    pub fn new(length: usize) -> Self {
        Self {
            prev_volumes: vec![],
            volume_bar_num: length,
        }
    }

    pub fn next(&mut self, current_volume: f64) -> f64 {
        //只保留前N根K线的成交量
        if self.prev_volumes.len() > self.volume_bar_num {
            self.prev_volumes.remove(0);
        }
        // println!("111111111111111111111", );
        // println!("self.prev_volumes: {:?}", self.prev_volumes);
        let avg_volume = self.prev_volumes.iter().sum::<f64>() / self.prev_volumes.len() as f64;
        let volume_ratio = current_volume / avg_volume;
        self.prev_volumes.push(current_volume);

        volume_ratio
    }
}
