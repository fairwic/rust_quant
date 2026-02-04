/// 成交量比率指标
/// 计算当前成交量与历史n根K线的平均值的比值
#[derive(Debug, Clone)]
pub struct VolumeRatioIndicator {
    // 前N根K线的成交量
    prev_volumes: Vec<f64>,
    // 前N根K线的
    volume_bar_num: usize,
    // 是否过滤最后一根K线的成交量
    is_filter_last_volume: bool,
    // 是否增长
    is_increasing_than_pre: bool,
    // 是否下降
    is_decreasing_than_pre: bool,
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
            is_filter_last_volume: is_fitler_last_volume,
            is_increasing_than_pre: false,
            is_decreasing_than_pre: false,
        }
    }

    pub fn next(&mut self, current_volume: f64) -> f64 {
        //只保留前N根K线的成交量
        if self.prev_volumes.len() > self.volume_bar_num {
            //连续成交量下降,或者连续成交量上身
            let list = self.prev_volumes.clone();
            if current_volume > *list.last().unwrap() && list[list.len() - 2] > list[list.len() - 3]
            {
                self.is_increasing_than_pre = true;
                self.is_decreasing_than_pre = false;
            }
            if current_volume < *list.last().unwrap() && list[list.len() - 2] < list[list.len() - 3]
            {
                self.is_increasing_than_pre = false;
                self.is_decreasing_than_pre = true;
            }

            self.prev_volumes.remove(0);
        }
        let denom = self.avg_volume();
        let volume_ratio = if denom == 0.0 || denom.is_nan() {
            0.0
        } else {
            current_volume / denom
        };
        self.prev_volumes.push(current_volume);

        volume_ratio
    }
    pub fn avg_volume(&self) -> f64 {
        if self.is_filter_last_volume && self.prev_volumes.len() > 1 {
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
    pub fn is_increasing_than_pre(&mut self) -> bool {
        self.is_increasing_than_pre
    }
    pub fn is_decreasing_than_pre(&self) -> bool {
        self.is_decreasing_than_pre
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
