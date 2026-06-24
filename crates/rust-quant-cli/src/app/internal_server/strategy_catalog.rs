use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StandardStrategyCatalogItem {
    pub strategy_key: &'static str,
    pub product_slug: &'static str,
    pub display_name: &'static str,
    pub category: &'static str,
    pub risk_level: &'static str,
    pub description: &'static str,
    pub detail: &'static str,
    pub cover_image: &'static str,
    pub display_total_return_pct: Option<f64>,
    pub display_sharpe_ratio: Option<f64>,
    pub display_trade_count: Option<i32>,
    pub display_max_drawdown_pct: Option<f64>,
    pub supported_exchanges: &'static [&'static str],
    pub supported_symbols: &'static [&'static str],
    pub timeframes: &'static [&'static str],
}

pub fn standard_strategy_catalog_items() -> Vec<StandardStrategyCatalogItem> {
    vec![
        StandardStrategyCatalogItem {
            strategy_key: "vegas",
            product_slug: "vegas-eth-usdt-swap-4h",
            display_name: "ETH 趋势复利组",
            category: "趋势",
            risk_level: "中高",
            description: "Vegas 4H ETH 趋势复利策略，结合 EMA、RSI、MACD、成交量、布林与 Fib 回撤确认，适合中周期趋势延续行情。",
            detail: "Vegas 4H ETH 趋势复利组使用趋势确认、回撤确认与保护性风控组合判断入场，适合 ETH-USDT-SWAP 的中周期趋势延续场景。",
            cover_image: "/strategy-covers/strategy-trend-breakout.svg",
            display_total_return_pct: Some(335.20),
            display_sharpe_ratio: Some(2.86),
            display_trade_count: Some(312),
            display_max_drawdown_pct: Some(18.20),
            supported_exchanges: &["okx"],
            supported_symbols: &["ETH-USDT-SWAP"],
            timeframes: &["4H"],
        },
        StandardStrategyCatalogItem {
            strategy_key: "market_velocity",
            product_slug: "market-velocity-radar",
            display_name: "市场动能雷达",
            category: "动量",
            risk_level: "中高",
            description: "Market Velocity 市场动能雷达策略，基于全市场成交额跃迁、排名变化、价格结构和短周期确认生成可审计的动能机会。",
            detail: "Market Velocity 面向全市场候选币种，通过成交额跃迁、排名变化、价格结构、流动性与 15m 确认信号筛选动能机会，不绑定 ETH 单一交易对。",
            cover_image: "/strategy-covers/strategy-quant-core.svg",
            display_total_return_pct: Some(118.40),
            display_sharpe_ratio: Some(2.18),
            display_trade_count: Some(204),
            display_max_drawdown_pct: Some(20.30),
            supported_exchanges: &["okx"],
            supported_symbols: &["ALL"],
            timeframes: &["15m"],
        },
    ]
}
