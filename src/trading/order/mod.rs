use log::info;
use crate::trading::okx::trade;
use crate::trading::okx::trade::{AttachAlgoOrd, OrderRequest, OrderResponse, OrderResponseData, OrdType, Side, TdMode};

//下单现货
pub async fn place_order_spot(inst_id: &str, side: Side, px: f64) -> anyhow::Result<Vec<OrderResponseData>> {

    //todo 获取当前可以开仓的数量
    let sz = 1;
    //todo 设置止盈止损
    let px = 3000.00;

    let order_params = OrderRequest {
        inst_id: inst_id.to_string(),
        td_mode: TdMode::CASH.to_string(),
        side: side.to_string(),
        ord_type: OrdType::LIMIT.to_string(),
        sz: sz.to_string(),
        px: Option::from(px.to_string()),
        reduce_only: Some(false),
        stp_mode: Some("cancel_maker".to_string()),
        attach_algo_ords: Some(vec![
            AttachAlgoOrd {
                attach_algo_cl_ord_id: None,
                tp_trigger_px: Some("3500".to_string()),
                tp_ord_px: Some("-1".to_string()),
                tp_ord_kind: None,
                sl_trigger_px: Some("2200".to_string()),
                sl_ord_px: Some("-1".to_string()),
                tp_trigger_px_type: Some("last".to_string()),
                sl_trigger_px_type: Some("last".to_string()),
                sz: None,
                amend_px_on_trigger_type: Some(0),
            }
        ]),

        ban_amend: None,
        tgt_ccy: None,
        pos_side: None,
        ccy: None,
        cl_ord_id: None,
        tag: None,
        px_usd: None,
        px_vol: None,
        quick_mgn_type: None,
        stp_id: None,
    };
    //下单
    let result = trade::Trade::new().order(order_params).await;

    // okx_response: {"code":"1","data":[{"clOrdId":"","ordId":"","sCode":"51094","sMsg":"You can't place TP limit orders in spot, margin, or options trading.","tag":"","ts":"1718339551210"}],"inTime":"1718339551209444","msg":"All operations failed","outTime":"1718339551210787"}
    // okx_response: {"code":"0","data":[{"clOrdId":"","ordId":"1538100941143183360","sCode":"0","sMsg":"Order placed","tag":"","ts":"1718341380112"}],"inTime":"1718341380111025","msg":"","outTime":"1718341380112306"}

    info!("Order result: {:#?}", result);
    result
}

//下单合约
pub async fn order_swap(inst_id: &str, tdModel: &str) -> anyhow::Result<Vec<OrderResponseData>> {
    let order_params = OrderRequest {
        inst_id: inst_id.to_string(),
        td_mode: TdMode::ISOLATED.to_string(),
        ccy: None,
        cl_ord_id: None,
        tag: None,
        side: "buy".to_string(),
        pos_side: Some("long".to_string()),
        // pos_side: None,
        ord_type: "limit".to_string(),
        sz: "1".to_string(),
        px: Some("30000".to_string()),
        px_usd: None,
        px_vol: None,
        reduce_only: Some(false),
        tgt_ccy: Some("quote_ccy".to_string()),
        ban_amend: Some(false),
        quick_mgn_type: None,
        stp_id: None,
        stp_mode: Some("cancel_maker".to_string()),
        attach_algo_ords: Some(vec![
            AttachAlgoOrd {
                attach_algo_cl_ord_id: None,
                tp_trigger_px: Some("35000".to_string()),
                tp_ord_px: Some("34900".to_string()),
                tp_ord_kind: Some("limit".to_string()),
                sl_trigger_px: Some("29000".to_string()),
                sl_ord_px: Some("28900".to_string()),
                tp_trigger_px_type: Some("last".to_string()),
                sl_trigger_px_type: Some("last".to_string()),
                sz: Some("1".to_string()),
                amend_px_on_trigger_type: Some(0),
            }
        ]),
    };
    //下单
    let result = trade::Trade::new().order(order_params).await;
    info!("Order result: {:#?}", result);
    result
}