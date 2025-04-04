use crate::trading::indicator::volume_indicator::VolumeRatioIndicator;

// 成交量预测指标
pub struct VolumePredictionIndicator {
    volumes_rate_indicator: VolumeRatioIndicator,
    target_volumes: f64,
    target_ratios: f64,
}

impl VolumePredictionIndicator {
    pub fn new(length: usize, target_ratios: f64) -> Self {
        Self {
            volumes_rate_indicator: VolumeRatioIndicator::new(length),
            target_volumes: 0.0,
            target_ratios: target_ratios,
        }
    }
    //分析出成交量到达多少，刚好触发目标倍数
    pub fn analyze_volume_to_target_ratio(&self) -> f64 {
        let target_volume = self.volumes_rate_indicator.avg_volume() * self.target_ratios;
        target_volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_volume_to_target_ratio() {
        let mut indicator = VolumePredictionIndicator::new(3, 2.0);
        indicator.volumes_rate_indicator.next(100.0);
        indicator.volumes_rate_indicator.next(200.0);
        indicator.volumes_rate_indicator.next(300.0);

        let target_volumes = indicator.analyze_volume_to_target_ratio();
        assert_eq!(target_volumes, 400.0);

        let mut indicator = VolumePredictionIndicator::new(3, 3.0);
        indicator.volumes_rate_indicator.next(10.0);
        indicator.volumes_rate_indicator.next(9.0);
        indicator.volumes_rate_indicator.next(7.0);

        let target_volumes = indicator.analyze_volume_to_target_ratio();
        assert_eq!(format!("{:.2}", target_volumes), "26.00");
    }
}
