/*获取交易账户余额*/
use reqwest::Method;
use serde::{Deserialize, Serialize};
use crate::trading::okx::{okx_client, OkxApiResponse};
use anyhow::{Result, Error, anyhow};
use tracing::{debug, info};
use crate::trading::okx::trade::TdMode;

#[derive(Serialize, Deserialize, Debug)]
pub struct Balance {
    ccy: String,
    bal: String,
    // 其他字段...
}

#[derive(Serialize, Deserialize, Debug)]
struct CandleData {
    ts: String,
    o: String,
    h: String,
    l: String,
    c: String,
    vol: String,
    vol_ccy: String,
    vol_ccy_quote: String,
    confirm: String,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct PositionResponse {
    code: String,
    msg: String,
    data: Vec<Position>,
}


/// 持仓信息结构体
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TradingNumRequestParams {
    pub inst_id: String,          // 产品ID，如 BTC-USDT
    pub td_mode: String,          // 交易模式: cross, isolated, cash, spot_isolated
    pub ccy: Option<String>,      // 保证金币种，仅适用于单币种保证金模式下的全仓杠杆订单
    pub reduce_only: Option<bool>, // 是否为只减仓模式，仅适用于币币杠杆
    pub px: Option<String>,       // 对应平仓价格下的可用数量，默认为市价，仅适用于杠杆只减仓
    pub un_spot_offset: Option<bool>, // true：禁止现货对冲，false：允许现货对冲，默认为false，仅适用于组合保证金模式
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TradingNumResponseData {
    pub inst_id: String,          // 产品ID，如 BTC-USDT
    pub avail_buy: String,   //最大买入可用数量
    pub avail_sell: String,    //最大卖出可用数量
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TradingSwapNumRequestParams {
    pub inst_id: String,          // 产品ID，如 BTC-USDT
    pub td_mode: String,          // 交易模式: cross, isolated, cash, spot_isolated
    pub ccy: Option<String>,      // 保证金币种，仅适用于单币种保证金模式下的全仓杠杆订单
    pub px: Option<String>,       // 委托价格当不填委托价时，交割和永续会取当前限价计算，其他业务线会按当前最新成交价计算当指定多个产品ID查询时，忽略该参数，当未填写处理
    pub leverage: Option<String>, // 开仓杠杆倍数默认为当前杠杆倍数仅适用于币币杠杆/交割/永续
    pub un_spot_offset: Option<bool>, // true：禁止现货对冲，false：允许现货对冲，默认为false，仅适用于组合保证金模式
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TradingSwapNumResponseData {
    pub inst_id: String,          // 产品ID，如 BTC-USDT
    pub ccy: String,   //保证金币种
    pub max_buy: String,   //最大买入可用数量
    pub max_sell: String,    //最大卖出可用数量
}

/// 持仓信息结构体
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// 产品类型
    pub inst_type: String,
    /// 保证金模式 (cross: 全仓, isolated: 逐仓)
    pub mgn_mode: String,
    /// 持仓ID
    pub pos_id: String,
    /// 持仓方向 (long: 开平仓模式开多, short: 开平仓模式开空, net: 买卖模式)
    pub pos_side: String,
    /// 持仓数量
    pub pos: String,
    /// 仓位资产币种，仅适用于币币杠杆仓位
    pub pos_ccy: Option<String>,
    /// 可平仓数量，适用于币币杠杆, 交割/永续（开平仓模式），期权
    pub avail_pos: Option<String>,
    /// 开仓平均价
    pub avg_px: Option<String>,
    /// 未实现收益（以标记价格计算）
    pub upl: Option<String>,
    /// 未实现收益率（以标记价格计算）
    pub upl_ratio: Option<String>,
    /// 以最新成交价格计算的未实现收益
    pub upl_last_px: Option<String>,
    /// 以最新成交价格计算的未实现收益率
    pub upl_ratio_last_px: Option<String>,
    /// 产品ID，如 BTC-USD-180216
    pub inst_id: String,
    /// 杠杆倍数，不适用于期权以及组合保证金模式下的全仓仓位
    pub lever: Option<String>,
    /// 预估强平价，不适用于期权
    pub liq_px: Option<String>,
    /// 最新标记价格
    pub mark_px: Option<String>,
    /// 初始保证金，仅适用于全仓
    pub imr: Option<String>,
    /// 保证金余额，可增减，仅适用于逐仓
    pub margin: Option<String>,
    /// 保证金率
    pub mgn_ratio: Option<String>,
    /// 维持保证金
    pub mmr: Option<String>,
    /// 负债额，仅适用于币币杠杆
    pub liab: Option<String>,
    /// 负债币种，仅适用于币币杠杆
    pub liab_ccy: Option<String>,
    /// 利息，已经生成的未扣利息
    pub interest: Option<String>,
    /// 最新成交ID
    pub trade_id: Option<String>,
    /// 期权市值，仅适用于期权
    pub opt_val: Option<String>,
    /// 逐仓杠杆负债对应平仓挂单的数量
    pub pending_close_ord_liab_val: Option<String>,
    /// 以美金价值为单位的持仓数量
    pub notional_usd: Option<String>,
    /// 信号区，分为5档，从1到5，数字越小代表adl强度越弱
    pub adl: Option<String>,
    /// 占用保证金的币种
    pub ccy: Option<String>,
    /// 最新成交价
    pub last: Option<String>,
    /// 最新指数价格
    pub idx_px: Option<String>,
    /// 美金价格
    pub usd_px: Option<String>,
    /// 盈亏平衡价
    pub be_px: Option<String>,
    /// 美金本位持仓仓位delta，仅适用于期权
    pub delta_bs: Option<String>,
    /// 币本位持仓仓位delta，仅适用于期权
    pub delta_pa: Option<String>,
    /// 美金本位持仓仓位gamma，仅适用于期权
    pub gamma_bs: Option<String>,
    /// 币本位持仓仓位gamma，仅适用于期权
    pub gamma_pa: Option<String>,
    /// 美金本位持仓仓位theta，仅适用于期权
    pub theta_bs: Option<String>,
    /// 币本位持仓仓位theta，仅适用于期权
    pub theta_pa: Option<String>,
    /// 美金本位持仓仓位vega，仅适用于期权
    pub vega_bs: Option<String>,
    /// 币本位持仓仓位vega，仅适用于期权
    pub vega_pa: Option<String>,
    /// 现货对冲占用数量，适用于组合保证金模式
    pub spot_in_use_amt: Option<String>,
    /// 现货对冲占用币种，适用于组合保证金模式
    pub spot_in_use_ccy: Option<String>,
    /// 用户自定义现货占用数量，适用于组合保证金模式
    pub cl_spot_in_use_amt: Option<String>,
    /// 系统计算得到的最大可能现货占用数量，适用于组合保证金模式
    pub max_spot_in_use_amt: Option<String>,
    /// 已实现收益
    pub realized_pnl: Option<String>,
    /// 平仓订单累计收益额
    pub pnl: Option<String>,
    /// 累计手续费金额
    pub fee: Option<String>,
    /// 累计资金费用
    pub funding_fee: Option<String>,
    /// 累计爆仓罚金
    pub liq_penalty: Option<String>,
    /// 平仓策略委托订单
    pub close_order_algo: Option<Vec<CloseOrderAlgo>>,
    /// 持仓创建时间，Unix时间戳的毫秒数格式
    pub c_time: Option<String>,
    /// 最近一次持仓更新时间，Unix时间戳的毫秒数格式
    pub u_time: Option<String>,
    /// 外部业务id，e.g. 体验券id
    pub biz_ref_id: Option<String>,
    /// 外部业务类型
    pub biz_ref_type: Option<String>,
}

/// 平仓策略委托订单结构体
#[derive(Serialize, Deserialize, Debug)]
pub struct CloseOrderAlgo {
    /// 策略委托单ID
    pub algo_id: String,
    /// 止损触发价
    pub sl_trigger_px: Option<String>,
    /// 止损触发价类型
    pub sl_trigger_px_type: Option<String>,
    /// 止盈委托价
    pub tp_trigger_px: Option<String>,
    /// 止盈触发价类型
    pub tp_trigger_px_type: Option<String>,
    /// 策略委托触发时，平仓的百分比。1 代表100%
    pub close_fraction: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetLeverageRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inst_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy: Option<String>,
    pub lever: String,
    pub mgn_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLeverageData {
    pub lever: String,
    pub mgn_mode: String,
    pub inst_id: String,
    pub pos_side: String,
}

pub struct Account {}


impl Account {
    pub fn new() -> Self {
        Account {}
    }
    pub async fn get_balances(ccy: Option<&Vec<String>>) -> anyhow::Result<OkxApiResponse<Balance>> {
        let mut path = "/api/v5/account/balance".to_string();
        if let Some(ccy) = ccy {
            let ccy_param = ccy.join(",");
            path.push_str(&format!("&ccy={}", ccy_param));
        }
        okx_client::get_okx_client().send_request(Method::GET, &path, "").await
    }

    /// 设置杠杆倍数
    pub async fn set_leverage(params: SetLeverageRequest) -> anyhow::Result<SetLeverageData> {
        let mut path = "/api/v5/account/set-leverage".to_string();
        let body = &serde_json::to_string(&params).unwrap();
        info!("send set_leverage okx_request params:{}",body);
        let res: OkxApiResponse<Vec<SetLeverageData>> = okx_client::get_okx_client().send_request(Method::POST, &path, body).await?;
        Ok(res.data[0].clone())
    }

    /// 获取最大可买卖/开仓数量
    pub async fn get_max_size(ins_id: &str, td_mode: TdMode) -> anyhow::Result<TradingSwapNumResponseData> {
        let mut path = "/api/v5/account/max-size?".to_string();
        path.push_str(&format!("instId={}", ins_id));
        path.push_str(&format!("&tdMode={}", td_mode));

        info!("request okx path: {}", path);
        let res: OkxApiResponse<Vec<TradingSwapNumResponseData>> = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;
        println!("res: {:?}", res);
        Ok(res.data[0].clone())
    }

    /// 获取最大可用数量
    pub async fn get_max_avail_size(ins_id: &str, td_mode: TdMode) -> anyhow::Result<TradingNumResponseData> {
        let mut path = "/api/v5/account/max-avail-size?".to_string();
        path.push_str(&format!("instId={}", ins_id));
        path.push_str(&format!("&tdMode={}", td_mode));

        let res: OkxApiResponse<Vec<TradingNumResponseData>> = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;
        println!("res: {:?}", res);
        Ok(res.data[0].clone())
    }

    /**
    获取该账户下拥有实际持仓的信息。账户为买卖模式会显示净持仓（net），账户为开平仓模式下会分别返回开多（long）或开空（short）的仓位。按照仓位创建时间倒序排列。
    instType	String	否	产品类型
    MARGIN：币币杠杆
    SWAP：永续合约
    FUTURES：交割合约
    OPTION：期权
    instType和instId同时传入的时候会校验instId与instType是否一致。
    instId	String	否	交易产品ID，如：BTC-USDT-SWAP
    支持多个instId查询（不超过10个），半角逗号分隔
    posId	String	否	持仓ID
    支持多个posId查询（不超过20个）。
    存在有效期的属性，自最近一次完全平仓算起，满30天 posId 以及整个仓位会被清除。**/
    pub async fn get_account_positions(
        &self,
        inst_type: Option<&str>,
        inst_id: Option<&str>,
        post_id: Option<&str>,
    ) -> anyhow::Result<Vec<Position>> {
        let mut path = format!("/api/v5/account/positions?tests=1");
        if let Some(instType) = inst_type {
            path.push_str(&format!("&instId={}", instType));
        }
        if let Some(instId) = inst_id {
            path.push_str(&format!("&instId={}", instId));
        }

        if let Some(postId) = post_id {
            path.push_str(&format!("&postId={}", postId));
        }
        info!("okx request path: {}", path);
        let res: PositionResponse = okx_client::get_okx_client().send_request(Method::GET, &path, "").await?;
        Ok(res.data)
    }
}


