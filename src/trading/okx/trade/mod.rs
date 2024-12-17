use std::fmt::{Display, Formatter};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::okx::{okx_client, OkxApiResponse};

use anyhow::{Result, anyhow};
use tracing::error;
use tracing::debug;
use tracing::field::debug;

#[derive(Serialize, Deserialize, Debug)]
pub struct CandleData {
    pub ts: String,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub vol: String,
    pub vol_ccy: String,
    pub vol_ccy_quote: String,
    pub confirm: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Ts {
    ts: String,
}

// 使用类型别名来定义特定的响应类型
pub type CandleResponse = OkxApiResponse<Vec<CandleData>>;
pub type TimeResponse = OkxApiResponse<Vec<Ts>>;

/// 订单响应
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    /// 结果代码，0表示成功
    pub code: String,
    /// 错误信息，代码为0时，该字段为空
    pub msg: Option<String>,
    /// 包含结果的对象数组
    pub data: Vec<OrderResponseData>,
    /// REST网关接收请求时的时间戳，Unix时间戳的微秒数格式，如 1597026383085123
    pub in_time: String,
    /// REST网关发送响应时的时间戳，Unix时间戳的微秒数格式，如 1597026383085123
    pub out_time: String,
}

pub enum Side {
    BUY,
    SELL,
}

impl Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::BUY => write!(f, "buy"),
            Side::SELL => write!(f, "sell"),
        }
    }
}


pub enum MgnMode {}


#[derive(Clone, Copy)]
pub enum PosSide {
    LONG,
    SHORT,
}

impl Display for PosSide {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PosSide::LONG => write!(f, "long"),
            PosSide::SHORT => write!(f, "short"),
        }
    }
}

impl PartialEq for PosSide {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}


pub enum OrdType {
    /// 限价单
    LIMIT,
    /// 市价单
    MARKET,
    /// 只做make单
    PostOnly,
    /// 全部成交或立即取消
    FOK,
    /// 立即成交并取消全部
    Ioc,
    // 市价委托立即成交并取消剩余（仅适用交割、永续）
    OptimalLimitIoc,
}

impl Display for OrdType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OrdType::LIMIT => write!(f, "limit"),
            OrdType::MARKET => write!(f, "market"),
            OrdType::PostOnly => write!(f, "post_only"),
            OrdType::FOK => write!(f, "fok"),
            OrdType::Ioc => write!(f, "ioc"),
            OrdType::OptimalLimitIoc => write!(f, "optimal_limit_ioc"),
        }
    }
}


pub enum TdMode {
    /// 保证金模式：isolated：逐仓
    ISOLATED,
    //保证金模式 ；cross：全仓
    CROSS,
    ///非保证模式，现货
    CASH,
}

// 止盈订单类型
// 默认为condition
pub enum TpOrdKind {
    // : 条件单
    CONDITION,
    // : 限价单
    LIMIT,
}

impl Display for TpOrdKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TpOrdKind::CONDITION => write!(f, "condition"),
            TpOrdKind::LIMIT => write!(f, "limit"),
        }
    }
}

impl Display for TdMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TdMode::ISOLATED => write!(f, "isolated"),
            TdMode::CROSS => write!(f, "cross"),
            TdMode::CASH => write!(f, "cash")
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    /// 产品ID，如 BTC-USDT
    pub inst_id: String,
    /// 交易模式
    /// 保证金模式：isolated：逐仓 ；cross：全仓
    /// 非保证金模式：cash：非保证金
    /// spot_isolated：现货逐仓(仅适用于现货带单) ，现货带单时，tdMode 的值需要指定为spot_isolated
    pub td_mode: String,
    /// 保证金币种，仅适用于单币种保证金模式下的全仓杠杆订单
    pub ccy: Option<String>,
    /// 客户自定义订单ID
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字且长度要在1-32位之间。
    pub cl_ord_id: Option<String>,
    /// 订单标签
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字，且长度在1-16位之间。
    pub tag: Option<String>,
    /// 订单方向
    /// buy：买， sell：卖
    pub side: String,
    /// 持仓方向
    /// 在开平仓模式下必填，且仅可选择 long 或 short。 仅适用交割、永续。
    pub pos_side: Option<String>,
    /// 订单类型
    /// market：市价单
    /// limit：限价单
    /// post_only：只做maker单
    /// fok：全部成交或立即取消
    /// Ioc：立即成交并取消剩余
    /// optimal_limit_ioc：市价委托立即成交并取消剩余（仅适用交割、永续）
    /// mmp：做市商保护(仅适用于组合保证金账户模式下的期权订单)
    /// mmp_and_post_only：做市商保护且只做maker单(仅适用于组合保证金账户模式下的期权订单)
    pub ord_type: String,
    /// 委托数量
    pub sz: String,
    /// 委托价格，仅适用于limit、post_only、fok、Ioc、mmp、mmp_and_post_only类型的订单
    /// 期权下单时，px/pxUsd/pxVol 只能填一个
    pub px: Option<String>,
    /// 以USD价格进行期权下单，仅适用于期权
    /// 期权下单时 px/pxUsd/pxVol 必填一个，且只能填一个
    pub px_usd: Option<String>,
    /// 以隐含波动率进行期权下单，例如 1 代表 100%，仅适用于期权
    /// 期权下单时 px/pxUsd/pxVol 必填一个，且只能填一个
    pub px_vol: Option<String>,
    /// 是否只减仓，true 或 false，默认false
    /// 仅适用于币币杠杆，以及买卖模式下的交割/永续
    /// 仅适用于单币种保证金模式和跨币种保证金模式
    pub reduce_only: Option<bool>,
    /// 市价单委托数量sz的单位，仅适用于币币市价订单
    /// base_ccy: 交易货币 ；quote_ccy：计价货币
    /// 买单默认quote_ccy， 卖单默认base_ccy
    pub tgt_ccy: Option<String>,
    /// 是否禁止币币市价改单，true 或 false，默认false
    /// 为true时，余额不足时，系统不会改单，下单会失败，仅适用于币币市价单
    pub ban_amend: Option<bool>,
    /// 一键借币类型，仅适用于杠杆逐仓的一键借币模式：
    /// manual：手动，auto_borrow：自动借币，auto_repay：自动还币
    /// 默认是manual：手动（已弃用）
    pub quick_mgn_type: Option<String>,
    /// 自成交保护ID。来自同一个母账户配着同一个ID的订单不能自成交
    /// 用户自定义1<=x<=999999999的整数（已弃用）
    pub stp_id: Option<String>,
    /// 自成交保护模式
    /// 默认为 cancel maker
    /// cancel_maker,cancel_taker, cancel_both
    /// Cancel both不支持FOK
    pub stp_mode: Option<String>,
    /// 下单附带止盈止损信息
    pub attach_algo_ords: Option<Vec<AttachAlgoOrd>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AttachAlgoOrd {
    /// 下单附带止盈止损时，客户自定义的策略订单ID
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字且长度要在1-32位之间。
    /// 订单完全成交，下止盈止损委托单时，该值会传给algoClOrdId
    pub attach_algo_cl_ord_id: Option<String>,
    /// 止盈触发价
    /// 对于条件止盈单，如果填写此参数，必须填写 止盈委托价
    pub tp_trigger_px: Option<String>,
    /// 止盈委托价
    /// 对于条件止盈单，如果填写此参数，必须填写 止盈触发价
    /// 对于限价止盈单，需填写此参数，不需要填写止盈触发价
    /// 委托价格为-1时，执行市价止盈
    pub tp_ord_px: Option<String>,
    /// 止盈订单类型
    /// condition: 条件单
    /// limit: 限价单
    /// 默认为condition
    pub tp_ord_kind: Option<String>,
    /// 止损触发价，如果填写此参数，必须填写 止损委托价
    pub sl_trigger_px: Option<String>,
    /// 止损委托价，如果填写此参数，必须填写 止损触发价
    /// 委托价格为-1时，执行市价止损
    pub sl_ord_px: Option<String>,
    /// 止盈触发价类型
    /// last：最新价格
    /// index：指数价格
    /// mark：标记价格
    /// 默认为last
    pub tp_trigger_px_type: Option<String>,
    /// 止损触发价类型
    /// last：最新价格
    /// index：指数价格
    /// mark：标记价格
    /// 默认为last
    pub sl_trigger_px_type: Option<String>,
    /// 数量。仅适用于“多笔止盈”的止盈订单，且对于“多笔止盈”的止盈订单必填
    pub sz: Option<String>,
    /// 是否启用开仓价止损，仅适用于分批止盈的止损订单，第一笔止盈触发时，止损触发价格是否移动到开仓均价止损
    /// 0：不开启，默认值
    /// 1：开启，且止损触发价不能为空
    pub amend_px_on_trigger_type: Option<i32>,
}


/// 订单响应数据
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponseData {
    /// 订单ID
    pub ord_id: String,
    /// 客户自定义订单ID
    pub cl_ord_id: Option<String>,
    /// 订单标签
    pub tag: Option<String>,
    /// 系统完成订单请求处理的时间戳，Unix时间戳的毫秒数格式，如 1597026383085
    pub ts: String,
    /// 事件执行结果的code，0代表成功
    pub s_code: String,
    /// 事件执行失败或成功时的msg
    pub s_msg: Option<String>,
}


/// 市价平仓请求参数结构体
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CloseOrderRequest {
    /// 产品ID
    pub inst_id: String,
    /// 持仓方向（可选）
    /// 买卖模式下：可不填写此参数，默认值net，如果填写，仅可以填写net
    /// 开平仓模式下：必须填写此参数，且仅可以填写 long：平多，short：平空
    pub pos_side: Option<String>,
    /// 保证金模式
    /// cross：全仓；isolated：逐仓
    pub mgn_mode: String,
    /// 保证金币种（可选）
    /// 单币种保证金模式的全仓币币杠杆平仓必填
    pub ccy: Option<String>,
    /// 当市价全平时，平仓单是否需要自动撤销，默认为false
    /// false：不自动撤单；true：自动撤单
    pub auto_cxl: Option<bool>,
    /// 客户自定义ID（可选）
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字且长度要在1-32位之间
    pub cl_ord_id: Option<String>,
    /// 订单标签（可选）
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字，且长度在1-16位之间
    pub tag: Option<String>,
}

/// 市价平仓请求参数结构体
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CloseOrderResponseData {
    /// 产品ID
    pub inst_id: String,
    /// 持仓方向（可选）
    /// 买卖模式下：可不填写此参数，默认值net，如果填写，仅可以填写net
    /// 开平仓模式下：必须填写此参数，且仅可以填写 long：平多，short：平空
    pub pos_side: Option<String>,
    /// 客户自定义ID（可选）
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字且长度要在1-32位之间
    pub cl_ord_id: Option<String>,
    /// 订单标签（可选）
    /// 字母（区分大小写）与数字的组合，可以是纯字母、纯数字，且长度在1-16位之间
    pub tag: Option<String>,
}

type CloseOrderResponse = OkxApiResponse<Vec<CloseOrderResponseData>>;


pub(crate) struct OkxTrade {}

impl OkxTrade {
    pub fn new() -> Self {
        OkxTrade {}
    }
    ///下单
    pub async fn order(&self, params: OrderRequest) -> Result<Vec<OrderResponseData>, anyhow::Error> {
        let path = "/api/v5/trade/order";
        let body = &serde_json::to_string(&params).unwrap();
        debug!("send place order okx_request params:{}",body);
        let res: OrderResponse = okx_client::get_okx_client().send_request(Method::POST, &path, body).await?;
        if res.code != "0" {
            error!("okx请求成功，但是操作失败，code:{},msg:{:?},data:{:?}",res.code,res.msg,res.data)
        }
        //判断返回的okx cod是否是0
        Ok(res.data)
    }

    ///市价仓位全平
    pub async fn close_position(&self, params: CloseOrderRequest) -> Result<Vec<CloseOrderResponseData>, anyhow::Error> {
        let path = "/api/v5/trade/close-position";
        let body = &serde_json::to_string(&params).unwrap();
        debug!("send close_position okx_request params:{}",body);
        let res: Result<CloseOrderResponse> = okx_client::get_okx_client().send_request(Method::POST, &path, body).await;
        Ok(res.unwrap().data)
    }
}

