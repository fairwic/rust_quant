//! 订单创建服务
//!
//! 根据交易信号创建订单，协调风控检查和订单执行

use anyhow::{anyhow, Result};
use tracing::{error, info, warn};

use rust_quant_domain::{
    Order, OrderSide, OrderType, Price, SignalDirection, SignalResult, Volume,
};

/// 订单创建服务
///
/// 职责：
/// 1. 根据信号创建订单
/// 2. 协调风控检查
/// 3. 订单参数计算（止损、止盈）
/// 4. 提交到执行引擎
pub struct OrderCreationService {
    // 依赖注入
    // order_repository: Arc<dyn OrderRepository>,
    // risk_service: Arc<RiskManagementService>,
    // execution_service: Arc<OrderExecutionService>,
}

impl OrderCreationService {
    /// 创建新的订单服务
    pub fn new() -> Self {
        Self {
            // TODO: 注入依赖
        }
    }

    /// 根据信号创建订单
    ///
    /// 流程：
    /// 1. 验证信号有效性
    /// 2. 风控检查
    /// 3. 计算订单参数
    /// 4. 创建订单对象
    /// 5. 保存订单
    /// 6. 提交到执行引擎
    pub async fn create_order_from_signal(
        &self,
        inst_id: &str,
        signal: &SignalResult,
        strategy_id: i64,
    ) -> Result<String> {
        info!(
            "根据信号创建订单: symbol={}, direction={:?}",
            inst_id, signal.direction
        );

        // 1. 验证信号
        if !self.validate_signal(signal) {
            return Err(anyhow!("信号无效"));
        }

        // 2. 风控检查
        // TODO: 调用 RiskManagementService
        // let can_trade = self.risk_service.check_can_open(inst_id, signal).await?;
        // if !can_trade {
        //     warn!("风控检查未通过");
        //     return Err(anyhow!("风控检查未通过"));
        // }

        // 3. 计算订单参数
        let order_params = self.calculate_order_params(inst_id, signal)?;

        info!(
            "订单参数: price={}, size={}, stop_loss={:?}",
            order_params.price, order_params.size, order_params.stop_loss
        );

        // 4. 创建订单对象
        let _order = self.build_order(inst_id, signal, strategy_id, &order_params)?;

        // 5. 保存订单
        // TODO: 通过 OrderRepository 保存
        // let order_id = self.order_repository.save(&order).await?;
        let order_id = format!("ORDER-{}", chrono::Utc::now().timestamp_millis());

        // 6. 提交到执行引擎
        // TODO: 调用 ExecutionService
        // self.execution_service.submit_order(&order).await?;

        info!("订单创建成功: order_id={}", order_id);
        Ok(order_id)
    }

    /// 批量创建订单
    pub async fn create_multiple_orders(
        &self,
        inst_id: &str,
        signals: Vec<SignalResult>,
        strategy_id: i64,
    ) -> Result<Vec<String>> {
        let mut order_ids = Vec::new();

        for signal in signals {
            match self
                .create_order_from_signal(inst_id, &signal, strategy_id)
                .await
            {
                Ok(order_id) => order_ids.push(order_id),
                Err(e) => {
                    error!("订单创建失败: {}", e);
                    // 继续处理其他信号
                }
            }
        }

        Ok(order_ids)
    }

    /// 平仓服务
    pub async fn close_position(
        &self,
        inst_id: &str,
        position_side: OrderSide,
        reason: &str,
    ) -> Result<String> {
        info!(
            "平仓: symbol={}, side={:?}, reason={}",
            inst_id, position_side, reason
        );

        // TODO: 实现平仓逻辑
        // 1. 获取持仓信息
        // 2. 计算平仓数量
        // 3. 创建平仓订单
        // 4. 提交执行

        let order_id = format!("CLOSE-{}", chrono::Utc::now().timestamp_millis());
        Ok(order_id)
    }

    // ========================================================================
    // 内部辅助方法
    // ========================================================================

    /// 验证信号有效性
    fn validate_signal(&self, signal: &SignalResult) -> bool {
        // 检查方向
        if signal.direction == SignalDirection::None {
            return false;
        }

        // 检查开仓标志
        if !signal.can_open {
            return false;
        }

        // 检查价格
        if signal.entry_price.is_none() {
            warn!("信号缺少入场价格");
            return false;
        }

        true
    }

    /// 计算订单参数
    fn calculate_order_params(&self, _inst_id: &str, signal: &SignalResult) -> Result<OrderParams> {
        let price = signal.entry_price.ok_or_else(|| anyhow!("缺少入场价格"))?;

        // 计算下单数量
        // TODO: 根据风控配置计算
        let size = 0.1; // 暂时固定

        // 止损价格
        let stop_loss = signal.stop_loss_price;

        // 止盈价格
        let take_profit = signal.take_profit_price;

        Ok(OrderParams {
            price,
            size,
            stop_loss,
            take_profit,
        })
    }

    /// 构建订单对象
    fn build_order(
        &self,
        inst_id: &str,
        signal: &SignalResult,
        _strategy_id: i64,
        params: &OrderParams,
    ) -> Result<Order> {
        // 转换信号方向为订单方向
        let side = match signal.direction {
            SignalDirection::Long => OrderSide::Buy,
            SignalDirection::Short => OrderSide::Sell,
            _ => return Err(anyhow!("无效的信号方向")),
        };

        // 创建订单
        let order = Order::new(
            format!("ORDER-{}", chrono::Utc::now().timestamp_millis()),
            inst_id.to_string(),
            side,
            OrderType::Limit,
            Price::new(params.price)?,
            Volume::new(params.size)?,
        )?;

        Ok(order)
    }
}

impl Default for OrderCreationService {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 辅助数据结构
// ============================================================================

/// 订单参数
#[allow(dead_code)]
struct OrderParams {
    /// 入场价格
    price: f64,
    /// 下单数量
    size: f64,
    /// 止损价格
    stop_loss: Option<f64>,
    /// 止盈价格
    take_profit: Option<f64>,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = OrderCreationService::new();
        // 验证服务可以创建
    }

    #[test]
    fn test_validate_signal() {
        let service = OrderCreationService::new();

        // 有效信号
        let valid_signal = SignalResult {
            direction: SignalDirection::Long,
            strength: rust_quant_domain::SignalStrength::new(0.8),
            signals: vec![],
            can_open: true,
            should_close: false,
            entry_price: Some(50000.0),
            stop_loss_price: Some(49000.0),
            take_profit_price: Some(52000.0),
            signal_kline_stop_loss_price: None,
            move_stop_open_price_when_touch_price: None,
            position_time: None,
            signal_kline: None,
            ts: None,
            single_value: None,
            single_result: None,
            should_sell: None,
            should_buy: None,
            open_price: None,
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            counter_trend_pullback_take_profit_price: None,
            filter_reasons: vec![],
            stop_loss_source: None,
        };

        assert!(service.validate_signal(&valid_signal));

        // 无效信号（无方向）
        let mut invalid_signal = valid_signal.clone();
        invalid_signal.direction = SignalDirection::None;
        assert!(!service.validate_signal(&invalid_signal));

        // 无效信号（不可开仓）
        let mut invalid_signal = valid_signal.clone();
        invalid_signal.can_open = false;
        assert!(!service.validate_signal(&invalid_signal));
    }
}
