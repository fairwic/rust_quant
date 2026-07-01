use super::types::{
    GridAction, GridScalperDecision, GridScalperSignalSnapshot, GridScalperThresholds,
};

/// 网格 Scalper 核心策略逻辑
pub struct GridScalperStrategy;

impl GridScalperStrategy {
    /// 评估当前是否应该开仓（买入或卖出网格）
    pub fn evaluate(
        thresholds: &GridScalperThresholds,
        snapshot: &GridScalperSignalSnapshot,
    ) -> GridScalperDecision {
        let mut reasons = Vec::new();

        // 1. 检查是否在震荡模式（必须先确认震荡）
        if !snapshot.in_ranging_mode {
            reasons.push(format!(
                "NOT_RANGING: recent_range={:.2}% > threshold={:.2}%",
                snapshot.recent_range_pct * 100.0,
                thresholds.ranging_threshold_pct * 100.0
            ));
            return GridScalperDecision {
                action: GridAction::Flat,
                reasons,
            };
        }

        // 2. 检查是否趋势突破（价格偏离中心过大）
        let deviation_pct = snapshot.price_to_center_pct.abs();
        let max_deviation = thresholds.trend_break_atr_mult * snapshot.atr / snapshot.grid_center;
        if deviation_pct > max_deviation {
            reasons.push(format!(
                "TREND_BREAK: deviation={:.2}% > max={:.2}%",
                deviation_pct * 100.0,
                max_deviation * 100.0
            ));
            return GridScalperDecision {
                action: GridAction::EmergencyClose,
                reasons,
            };
        }

        // 3. 判断当前价格在网格中的位置
        let price = snapshot.price;
        let center = snapshot.grid_center;
        let upper = snapshot.grid_upper;
        let lower = snapshot.grid_lower;

        // 网格逻辑：价格在下半区→买入，在上半区→卖出
        // 精细化：根据 grid_levels 计算当前档位
        let grid_range = upper - lower;
        let level_size = grid_range / thresholds.grid_levels as f64;
        let price_in_grid = (price - lower) / level_size;

        if price <= center {
            // 下半区：接近下限时买入（抄底）
            let buy_threshold = thresholds.grid_levels as f64 * 0.3; // 低于30%档位时买
            if price_in_grid < buy_threshold {
                reasons.push(format!(
                    "BUY_GRID: price={:.2} in_level={:.1}/{}, center={:.2}",
                    price, price_in_grid, thresholds.grid_levels, center
                ));
                reasons.push(format!("GRID_RANGE: [{:.2}, {:.2}]", lower, upper));
                return GridScalperDecision {
                    action: GridAction::BuyGrid,
                    reasons,
                };
            }
        } else {
            // 上半区：接近上限时卖出（摸顶）
            let sell_threshold = thresholds.grid_levels as f64 * 0.7; // 高于70%档位时卖
            if price_in_grid > sell_threshold {
                reasons.push(format!(
                    "SELL_GRID: price={:.2} in_level={:.1}/{}, center={:.2}",
                    price, price_in_grid, thresholds.grid_levels, center
                ));
                reasons.push(format!("GRID_RANGE: [{:.2}, {:.2}]", lower, upper));
                return GridScalperDecision {
                    action: GridAction::SellGrid,
                    reasons,
                };
            }
        }

        // 4. 中间区域：不操作
        reasons.push(format!(
            "MID_ZONE: price={:.2} level={:.1}, waiting",
            price, price_in_grid
        ));
        GridScalperDecision {
            action: GridAction::Flat,
            reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_buy_at_lower_bound() {
        let thresholds = GridScalperThresholds {
            grid_levels: 5,
            trend_break_atr_mult: 2.5,
            ..Default::default()
        };
        let snapshot = GridScalperSignalSnapshot {
            price: 100.0,
            atr: 1.0,
            grid_center: 101.0,
            grid_lower: 100.0,
            grid_upper: 102.0,
            in_ranging_mode: true,
            price_to_center_pct: -0.01,
            recent_range_pct: 0.015,
        };
        let decision = GridScalperStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, GridAction::BuyGrid));
    }

    #[test]
    fn grid_sell_at_upper_bound() {
        let thresholds = GridScalperThresholds {
            grid_levels: 5,
            ..Default::default()
        };
        let snapshot = GridScalperSignalSnapshot {
            price: 102.0,
            atr: 1.0,
            grid_center: 101.0,
            grid_lower: 100.0,
            grid_upper: 102.0,
            in_ranging_mode: true,
            price_to_center_pct: 0.01,
            recent_range_pct: 0.015,
        };
        let decision = GridScalperStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, GridAction::SellGrid));
    }

    #[test]
    fn no_action_when_not_ranging() {
        let thresholds = GridScalperThresholds::default();
        let snapshot = GridScalperSignalSnapshot {
            price: 100.0,
            atr: 1.0,
            grid_center: 101.0,
            grid_lower: 100.0,
            grid_upper: 102.0,
            in_ranging_mode: false, // 不在震荡
            price_to_center_pct: -0.01,
            recent_range_pct: 0.03,
        };
        let decision = GridScalperStrategy::evaluate(&thresholds, &snapshot);
        assert!(matches!(decision.action, GridAction::Flat));
    }
}
