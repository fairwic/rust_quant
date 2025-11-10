//! 回撤控制策略

use rust_quant_domain::value_objects::Percentage;

/// 回撤控制策略
pub struct DrawdownPolicy {
    /// 最大回撤限制
    pub max_drawdown: Percentage,

    /// 警告回撤阈值
    pub warning_drawdown: Percentage,
}

impl DrawdownPolicy {
    /// 检查回撤是否超限
    pub fn is_drawdown_exceeded(&self, current_drawdown: f64) -> bool {
        current_drawdown > self.max_drawdown.value()
    }

    /// 是否达到警告阈值
    pub fn is_warning_level(&self, current_drawdown: f64) -> bool {
        current_drawdown > self.warning_drawdown.value()
    }

    /// 获取建议动作
    pub fn get_action(&self, current_drawdown: f64) -> DrawdownAction {
        if self.is_drawdown_exceeded(current_drawdown) {
            DrawdownAction::StopAllTrading
        } else if self.is_warning_level(current_drawdown) {
            DrawdownAction::ReducePositions
        } else {
            DrawdownAction::Continue
        }
    }
}

/// 回撤控制动作
pub enum DrawdownAction {
    /// 继续交易
    Continue,
    /// 减少持仓
    ReducePositions,
    /// 停止所有交易
    StopAllTrading,
}

impl Default for DrawdownPolicy {
    fn default() -> Self {
        Self {
            max_drawdown: Percentage::new(20.0).unwrap(), // 最大20%回撤
            warning_drawdown: Percentage::new(15.0).unwrap(), // 15%警告
        }
    }
}
