use super::{
    append_filtered_volume_rsi_ema_macd_v10_research_args,
    append_filtered_volume_rsi_ema_macd_v11_research_args,
    append_filtered_volume_rsi_ema_macd_v12_research_args,
    append_filtered_volume_rsi_ema_macd_v13_research_args,
    append_filtered_volume_rsi_ema_macd_v1_research_args,
    append_filtered_volume_rsi_ema_macd_v2_research_args,
    append_filtered_volume_rsi_ema_macd_v3_research_args,
    append_filtered_volume_rsi_ema_macd_v4_research_args,
    append_filtered_volume_rsi_ema_macd_v5_research_args,
    append_filtered_volume_rsi_ema_macd_v9_research_args,
    append_momentum_exhaustion_reversal_v1_research_args,
    append_momentum_exhaustion_reversal_v2_research_args,
    append_momentum_exhaustion_reversal_v3_research_args,
    append_volume_anchor_rsi_divergence_reversal_v1_research_args,
    append_volume_anchor_rsi_divergence_reversal_v2_research_args,
    append_volume_platform_break_trend_v1_research_args,
    append_volume_platform_break_trend_v2_research_args, parse_cli_args_from,
    MarketVelocityEventBacktestArgs, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET, MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET,
    MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET, MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET,
    MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET, MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET,
    MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET,
    MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET, MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET,
};
use anyhow::Result;

/// 复用各版本冻结参数并补回研究 preset，仅用于构造不可变的回测配置。
fn research_args(
    append: fn(&mut Vec<String>),
    preset: &'static str,
) -> Result<MarketVelocityEventBacktestArgs> {
    let mut args = Vec::new();
    append(&mut args);
    let mut parsed = parse_cli_args_from(args)?;
    parsed.paper_strategy_preset = preset.to_string();
    Ok(parsed)
}

/// 返回过滤量比与 RSI/EMA/MACD 三分支 v1 的冻结 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v1_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v1_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V1_PRESET,
    )
}

/// 返回采用已确认枢轴对 MACD 背离语义的 v2 Research-only 参数。
///
/// Z 与 D_min 没有可信默认值，因此该基础版本保持两者为空并关闭 MACD 分支。
pub fn market_filtered_volume_rsi_ema_macd_v2_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v2_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V2_PRESET,
    )
}

/// 返回同币种周 `vol_ccy` P90、形态止损与固定 ATR 距离止盈 v3 的 Research-only 参数。
///
/// Z 与 D_min 继续保持显式实验参数；未提供时仅关闭 MACD 分支。
pub fn market_filtered_volume_rsi_ema_macd_v3_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v3_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_PRESET,
    )
}

/// 返回叠加 BB(12, 2.6) 反向冲突缓冲的 v4 Research-only 参数。
///
/// 布林带不能独立开仓；本版本仍沿用 v3 的周成交量、下一根开盘与 1% 风险契约。
pub fn market_filtered_volume_rsi_ema_macd_v4_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v4_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET,
    )
}

/// 返回仅限制 EMA 延续分支距 EMA144 不超过一个 ATR14 的 v5 Research-only 参数。
///
/// 本版本从 v3 做单变量消融，不叠加 v4 的布林冲突缓冲。
pub fn market_filtered_volume_rsi_ema_macd_v5_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v5_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET,
    )
}

/// 返回双端量比 2.5 与周 P90 放量锚点 RSI 背离 v9 的 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v9_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v9_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V9_PRESET,
    )
}

/// 返回 v9 锚点背离增加紧邻下一根收盘突破确认的 v10 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v10_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v10_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V10_PRESET,
    )
}

/// 返回影线下一开盘、非影线紧邻下一根盘中触价成交的 v11 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v11_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v11_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V11_PRESET,
    )
}

/// 返回 v11 入场加趋势目标和持仓放量阶梯保护的 v12 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v12_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v12_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V12_PRESET,
    )
}

/// 返回 V12 全部规则加普通逆势 1.5 ATR 目标的 V13 Research-only 参数。
pub fn market_filtered_volume_rsi_ema_macd_v13_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_filtered_volume_rsi_ema_macd_v13_research_args,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V13_PRESET,
    )
}

/// 返回只检验历史净移动、异常量和价格拒绝的动量衰竭反转家族参数。
pub fn market_momentum_exhaustion_reversal_v1_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_momentum_exhaustion_reversal_v1_research_args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V1_PRESET,
    )
}

/// 返回方向性影线极值挂单 12 根且按信号量比分档止盈的动量衰竭 V2 参数。
pub fn market_momentum_exhaustion_reversal_v2_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_momentum_exhaustion_reversal_v2_research_args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V2_PRESET,
    )
}

/// 返回 55% 方向影线阈值、其余合同冻结为 V2 的动量衰竭 V3 参数。
pub fn market_momentum_exhaustion_reversal_v3_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_momentum_exhaustion_reversal_v3_research_args,
        MARKET_MOMENTUM_EXHAUSTION_REVERSAL_V3_PRESET,
    )
}

/// 返回只检验 q/p 放量锚点 RSI 背离的独立反转家族参数。
pub fn market_volume_anchor_rsi_divergence_reversal_v1_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_volume_anchor_rsi_divergence_reversal_v1_research_args,
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V1_PRESET,
    )
}

/// 返回增加四根间隔与 60/40 摆动重置门禁的放量锚点 RSI V2 参数。
pub fn market_volume_anchor_rsi_divergence_reversal_v2_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_volume_anchor_rsi_divergence_reversal_v2_research_args,
        MARKET_VOLUME_ANCHOR_RSI_DIVERGENCE_REVERSAL_V2_PRESET,
    )
}

/// 返回只检验放量平台破位、两根接受和长期 EMA 确认的趋势家族参数。
pub fn market_volume_platform_break_trend_v1_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_volume_platform_break_trend_v1_research_args,
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V1_PRESET,
    )
}

/// 返回使用破位前 ATR、水平性与分散触碰定义平台的趋势 V2 参数。
pub fn market_volume_platform_break_trend_v2_research_args(
) -> Result<MarketVelocityEventBacktestArgs> {
    research_args(
        append_volume_platform_break_trend_v2_research_args,
        MARKET_VOLUME_PLATFORM_BREAK_TREND_V2_PRESET,
    )
}
