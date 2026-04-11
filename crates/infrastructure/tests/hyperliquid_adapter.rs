use hyperliquid_rust_sdk::{AssetContext, AssetMeta, FundingHistoryResponse, Meta};
use rust_quant_infrastructure::exchanges::{
    HyperliquidAssetContextSnapshot, HyperliquidFundingHistoryPoint, HyperliquidPublicAdapter,
};

#[test]
fn convert_sdk_funding_history_response_parses_numeric_strings() {
    let payload = vec![FundingHistoryResponse {
        coin: "ETH".to_string(),
        funding_rate: "0.0000105495".to_string(),
        premium: "-0.0004156042".to_string(),
        time: 1774814400062_u64,
    }];

    let rows = HyperliquidPublicAdapter::from_sdk_funding_history(payload)
        .expect("sdk funding history should map");

    assert_eq!(
        rows,
        vec![HyperliquidFundingHistoryPoint {
            coin: "ETH".to_string(),
            funding_rate: 0.0000105495,
            premium: Some(-0.0004156042),
            time: 1774814400062_i64,
        }]
    );
}

#[test]
fn convert_sdk_meta_and_asset_ctxs_response_extracts_requested_coin() {
    let meta = Meta {
        universe: vec![
            AssetMeta {
                name: "BTC".to_string(),
                sz_decimals: 5,
                max_leverage: 40,
                only_isolated: None,
            },
            AssetMeta {
                name: "ETH".to_string(),
                sz_decimals: 4,
                max_leverage: 25,
                only_isolated: None,
            },
        ],
    };
    let contexts = vec![
        AssetContext {
            day_ntl_vlm: "1000000".to_string(),
            funding: "0.0000125".to_string(),
            impact_pxs: None,
            mark_px: "30010.0".to_string(),
            mid_px: None,
            open_interest: "123456.0".to_string(),
            oracle_px: "30000.0".to_string(),
            premium: Some("0.0001".to_string()),
            prev_day_px: "29900.0".to_string(),
        },
        AssetContext {
            day_ntl_vlm: "500000".to_string(),
            funding: "0.0000105495".to_string(),
            impact_pxs: None,
            mark_px: "1985.11".to_string(),
            mid_px: None,
            open_interest: "98765.5".to_string(),
            oracle_px: "1983.26".to_string(),
            premium: Some("-0.0004156042".to_string()),
            prev_day_px: "1990.0".to_string(),
        },
    ];

    let snapshot = HyperliquidPublicAdapter::from_sdk_meta_and_asset_ctxs(&meta, &contexts, "ETH")
        .expect("sdk meta and asset contexts should map");

    assert_eq!(
        snapshot,
        HyperliquidAssetContextSnapshot {
            coin: "ETH".to_string(),
            funding: Some(0.0000105495),
            open_interest: Some(98765.5),
            premium: Some(-0.0004156042),
            oracle_price: Some(1983.26),
            mark_price: Some(1985.11),
        }
    );
}
