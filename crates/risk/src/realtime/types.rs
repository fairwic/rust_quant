use rust_quant_common::CandleItem;
use rust_quant_domain::enums::PositionSide;
use rust_quant_domain::BasicRiskConfig;
use serde::{Deserialize, Serialize};
/// 实时风控输入事件
#[derive(Debug, Clone)]
pub enum RealtimeRiskEvent {
    /// K线更新（推荐使用 confirm=1 的确认K线做触发）
    Candle(MarketCandle),
    /// 持仓快照更新（开仓/加仓/减仓/平仓等）
    Position(PositionSnapshot),
    /// 策略风险配置更新（热更新场景）
    RiskConfig(StrategyRiskConfigSnapshot),
}
/// K线事件（包含交易对信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketCandle {
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// K 线。
    pub candle: CandleItem,
}
impl MarketCandle {
    /// 提供tryfromentity的集中实现，避免风控调用方重复处理相同细节。
    pub fn try_from_entity(
        inst_id: String,
        entity: &rust_quant_market::models::CandlesEntity,
    ) -> anyhow::Result<Self> {
        let o = entity.o.parse::<f64>()?;
        let h = entity.h.parse::<f64>()?;
        let l = entity.l.parse::<f64>()?;
        let c = entity.c.parse::<f64>()?;
        let v = entity.vol_ccy.parse::<f64>()?;
        let confirm = entity.confirm.parse::<i32>()?;
        Ok(Self {
            inst_id,
            candle: CandleItem {
                o,
                h,
                l,
                c,
                v,
                ts: entity.ts,
                confirm,
            },
        })
    }
}
/// 单策略的风险配置快照（用于热更新）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRiskConfigSnapshot {
    /// 策略config ID。
    pub strategy_config_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 风险。
    pub risk: BasicRiskConfig,
}
/// 运行中持仓快照（由上层策略执行/持仓同步模块提供）
///
/// 关键字段：
/// - `entry_price`: 开仓均价
/// - `initial_stop_loss`: 初始止损触发价（用于计算 1R）
/// - `ord_id`: 交易所普通订单ID（用于查询订单详情获取 attachAlgoId）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSnapshot {
    /// 策略config ID。
    pub strategy_config_id: i64,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 持仓方向。
    pub pos_side: PositionSide,
    /// 入场价格。
    pub entry_price: f64,
    /// 数量数值。
    pub size: f64,
    /// 初始止损触发价（若未知，可传 None；风控会退化用 max_loss_percent 推算）
    pub initial_stop_loss: Option<f64>,
    /// 交易所订单ID（ordId）
    pub ord_id: Option<String>,
    /// 是否处于打开状态。
    pub is_open: bool,
}
