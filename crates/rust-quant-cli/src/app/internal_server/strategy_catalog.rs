use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StandardStrategyCatalogItem {
    pub strategy_key: &'static str,
    pub product_slug: &'static str,
    pub display_name: &'static str,
    pub category: &'static str,
    pub risk_level: &'static str,
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
            supported_exchanges: &["okx"],
            supported_symbols: &["ALL"],
            timeframes: &["15m"],
        },
    ]
}
