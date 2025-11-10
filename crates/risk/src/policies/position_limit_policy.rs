//! 持仓限额策略

use rust_quant_domain::entities::Position;
use rust_quant_domain::value_objects::Percentage;

/// 持仓限额策略
pub struct PositionLimitPolicy {
    /// 单个持仓最大占比 (占总资金)
    pub max_single_position_percent: Percentage,

    /// 总持仓最大占比
    pub max_total_position_percent: Percentage,

    /// 单个交易对最大持仓数量
    pub max_positions_per_symbol: usize,
}

impl PositionLimitPolicy {
    /// 检查是否可以开新仓
    pub fn can_open_position(
        &self,
        proposed_position_value: f64,
        total_balance: f64,
        current_positions: &[Position],
    ) -> Result<(), String> {
        // 检查单个持仓限额
        let position_percent = (proposed_position_value / total_balance) * 100.0;
        if position_percent > self.max_single_position_percent.value() {
            return Err(format!(
                "单个持仓占比{}%超过限额{}%",
                position_percent, self.max_single_position_percent
            ));
        }

        // 检查总持仓限额
        let total_position_value: f64 = current_positions.iter().map(|p| p.position_value()).sum();
        let total_percent =
            ((total_position_value + proposed_position_value) / total_balance) * 100.0;

        if total_percent > self.max_total_position_percent.value() {
            return Err(format!(
                "总持仓占比{}%超过限额{}%",
                total_percent, self.max_total_position_percent
            ));
        }

        Ok(())
    }
}

impl Default for PositionLimitPolicy {
    fn default() -> Self {
        Self {
            max_single_position_percent: Percentage::new(20.0).unwrap(), // 单个20%
            max_total_position_percent: Percentage::new(80.0).unwrap(),  // 总计80%
            max_positions_per_symbol: 3,
        }
    }
}
