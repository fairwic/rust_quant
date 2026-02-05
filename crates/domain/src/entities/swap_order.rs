//! 合约订单实体 (SwapOrder)
//!
//! 对应数据库表 `swap_orders`，记录合约交易的下单记录

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 合约订单实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapOrder {
    /// 自增主键
    pub id: Option<i32>,

    /// 策略配置ID
    pub strategy_id: i32,

    /// 内部订单ID（唯一）
    pub in_order_id: String,

    /// 第三方平台订单ID
    pub out_order_id: String,

    /// 策略类型（如 "vegas", "nwe"）
    pub strategy_type: String,

    /// 策略周期（如 "5m", "1H"）
    pub period: String,

    /// 交易产品ID（如 "BTC-USDT-SWAP"）
    pub inst_id: String,

    /// 交易方向（buy/sell）
    pub side: String,

    /// 持仓数量
    pub pos_size: String,

    /// 持仓方向（long/short）
    pub pos_side: String,

    /// 订单标签
    pub tag: String,

    /// 平台类型（如 "okx"）
    pub platform_type: String,

    /// 下单详情（JSON格式）
    pub detail: String,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub update_at: Option<DateTime<Utc>>,
}

impl SwapOrder {
    /// 创建新的合约订单
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        strategy_id: i32,
        in_order_id: String,
        out_order_id: String,
        strategy_type: String,
        period: String,
        inst_id: String,
        side: String,
        pos_size: String,
        pos_side: String,
        platform_type: String,
        detail: String,
    ) -> Self {
        let now = Utc::now();
        let tag = format!(
            "{}-{}-{}-{}-{}-{}",
            now.format("%Y%m%d%H%M%S"),
            strategy_type,
            inst_id,
            period,
            side,
            pos_side
        );

        Self {
            id: None,
            strategy_id,
            in_order_id,
            out_order_id,
            strategy_type,
            period,
            inst_id,
            side,
            pos_size,
            pos_side,
            tag,
            platform_type,
            detail,
            created_at: now,
            update_at: None,
        }
    }

    /// 从信号结果创建订单
    #[allow(clippy::too_many_arguments)]
    pub fn from_signal(
        strategy_id: i32,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        side: &str,
        pos_side: &str,
        pos_size: &str,
        in_order_id: &str,
        out_order_id: &str,
        platform_type: &str,
        detail: &str,
    ) -> Self {
        Self::new(
            strategy_id,
            in_order_id.to_string(),
            out_order_id.to_string(),
            strategy_type.to_string(),
            period.to_string(),
            inst_id.to_string(),
            side.to_string(),
            pos_size.to_string(),
            pos_side.to_string(),
            platform_type.to_string(),
            detail.to_string(),
        )
    }

    /// 生成内部订单ID
    pub fn generate_in_order_id(inst_id: &str, strategy_type: &str, ts: i64) -> String {
        format!("{}_{}_{}", inst_id, strategy_type, ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_swap_order() {
        let order = SwapOrder::new(
            1,
            "BTC-USDT-SWAP_nwe_1234567890".to_string(),
            "ext_12345".to_string(),
            "nwe".to_string(),
            "5m".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "buy".to_string(),
            "0.1".to_string(),
            "long".to_string(),
            "okx".to_string(),
            r#"{"price": 50000}"#.to_string(),
        );

        assert_eq!(order.strategy_id, 1);
        assert_eq!(order.inst_id, "BTC-USDT-SWAP");
        assert_eq!(order.side, "buy");
        assert_eq!(order.pos_side, "long");
        assert!(order.tag.contains("nwe"));
    }

    #[test]
    fn test_generate_in_order_id() {
        let id = SwapOrder::generate_in_order_id("BTC-USDT-SWAP", "nwe", 1234567890);
        assert_eq!(id, "BTC-USDT-SWAP_nwe_1234567890");
    }
}
