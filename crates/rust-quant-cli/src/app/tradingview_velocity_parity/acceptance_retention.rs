use super::model::Direction;
use super::strict_visual_breakout::StrictVisualBreakoutSignal;

/// 判断原 V23 确认收盘保留的越界幅度是否低于冻结阈值。
///
/// 分母固定使用突破棒越界距离，多空按同一方向语义镜像；非正分母不能通过门禁。
pub(super) fn is_below_minimum(
    source: &StrictVisualBreakoutSignal,
    confirmation_close: f64,
    minimum: f64,
) -> bool {
    let retained = match source.direction {
        Direction::Long => confirmation_close - source.execution_boundary,
        Direction::Short => source.execution_boundary - confirmation_close,
    };
    source.breakout_excess <= 0.0 || retained / source.breakout_excess + 1e-12 < minimum
}
