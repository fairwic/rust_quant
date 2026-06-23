use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLeaseRequest {
    /// worker ID。
    pub worker_id: String,
    /// 查询数量上限。
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// 列表数据。
    pub task_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// 列表数据。
    pub task_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// 列表数据。
    pub task_statuses: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLeaseExtendRequest {
    /// worker ID。
    pub worker_id: String,
    /// 续租秒数。
    pub extend_seconds: Option<i64>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLease {
    /// 列表数据。
    pub tasks: Vec<ExecutionTask>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLeaseExtendResponse {
    /// 任务。
    pub task: ExecutionTask,
    /// 续租后的租约到期时间。
    pub lease_until: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskConfirmationLease {
    /// 列表数据。
    pub items: Vec<ExecutionTaskConfirmationLeaseItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskConfirmationLeaseItem {
    /// 任务。
    pub task: ExecutionTask,
    /// 订单结果，用于记录交易或执行状态。
    pub order_result: ExchangeOrderResult,
}
impl<'de> Deserialize<'de> for ExecutionTaskLease {
    /// 提供deserialize的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLease {
            #[serde(default)]
            /// 列表数据。
            tasks: Vec<ExecutionTask>,
            #[serde(default)]
            /// 列表数据。
            items: Vec<RawLeaseItem>,
        }
        #[derive(Deserialize)]
        struct RawLeaseItem {
            /// 任务。
            task: ExecutionTask,
        }
        let raw = RawLease::deserialize(deserializer)?;
        let tasks = if raw.tasks.is_empty() {
            raw.items.into_iter().map(|item| item.task).collect()
        } else {
            raw.tasks
        };
        Ok(Self { tasks })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTask {
    /// 唯一标识。
    pub id: i64,
    #[serde(default)]
    /// newssignal ID；为空时使用默认值或表示不限制。
    pub news_signal_id: Option<i64>,
    #[serde(default)]
    /// 策略signal ID；为空时使用默认值或表示不限制。
    pub strategy_signal_id: Option<i64>,
    /// combo ID。
    pub combo_id: i64,
    /// buyeremail，用于记录交易或执行状态。
    pub buyer_email: String,
    /// 策略slug，用于记录交易或执行状态。
    pub strategy_slug: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub task_type: String,
    /// 状态值。
    pub task_status: String,
    /// priority，用于记录交易或执行状态。
    pub priority: i32,
    /// leaseowner；为空时表示该条件不启用。
    pub lease_owner: Option<String>,
    /// leaseuntil；为空时表示该条件不启用。
    pub lease_until: Option<String>,
    /// 时间字段。
    pub scheduled_at: String,
    #[serde(deserialize_with = "deserialize_json_value_from_string")]
    /// 请求载荷json，用于构建接口请求。
    pub request_payload_json: Value,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeOrderResult {
    /// 唯一标识。
    pub id: i64,
    /// 执行任务 ID。
    pub execution_task_id: i64,
    /// combo ID。
    pub combo_id: i64,
    /// buyeremail，用于记录交易或执行状态。
    pub buyer_email: String,
    /// 交易所名称。
    pub exchange: String,
    /// externalorder ID。
    pub external_order_id: String,
    /// 订单方向，用于记录交易或执行状态。
    pub order_side: String,
    /// 订单状态。
    pub order_status: String,
    /// 数量数值。
    pub filled_qty: Option<f64>,
    /// 已成交计价金额；为空时表示没有成交金额。
    pub filled_quote: Option<f64>,
    /// 金额数值。
    pub fee_amount: Option<f64>,
    /// raw载荷json，用于记录交易或执行状态。
    pub raw_payload_json: String,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportRequest {
    /// 任务 ID。
    pub task_id: i64,
    /// 执行任务状态。
    pub execution_status: String,
    /// 交易所名称。
    pub exchange: String,
    /// externalorder ID。
    pub external_order_id: String,
    /// 订单方向，用于构建接口请求。
    pub order_side: String,
    /// 订单状态。
    pub order_status: String,
    /// 数量数值。
    pub filled_qty: Option<f64>,
    /// 已成交计价金额；为空时表示没有成交金额。
    pub filled_quote: Option<f64>,
    /// 金额数值。
    pub fee_amount: Option<f64>,
    /// profit USDT 金额；为空时使用默认值或表示不限制。
    pub profit_usdt: Option<f64>,
    /// 时间字段。
    pub executed_at: Option<String>,
    /// 错误消息；为空时使用默认值或表示不限制。
    pub error_message: Option<String>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportResponse {
    /// 任务。
    pub task: ExecutionTask,
    /// attempt，用于返回接口响应。
    pub attempt: Value,
    /// 订单结果；为空时使用默认值或表示不限制。
    pub order_result: Option<Value>,
    /// traderecord；为空时表示该条件不启用。
    pub trade_record: Option<Value>,
}
impl ExecutionTaskReportRequest {
    /// 封装成功，减少Web 商业链路调用方重复实现相同细节。
    pub fn success(
        task_id: i64,
        exchange: impl Into<String>,
        external_order_id: impl Into<String>,
        order_side: impl Into<String>,
        order_status: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "completed".to_string(),
            exchange: exchange.into(),
            external_order_id: external_order_id.into(),
            order_side: order_side.into(),
            order_status: order_status.into(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: None,
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }
    /// 封装失败，减少Web 商业链路调用方重复实现相同细节。
    pub fn failed(
        task_id: i64,
        exchange: impl Into<String>,
        order_side: impl Into<String>,
        message: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "failed".to_string(),
            exchange: exchange.into(),
            external_order_id: format!("failed-task-{task_id}"),
            order_side: order_side.into(),
            order_status: "failed".to_string(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: Some(message.into()),
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeReconciliationIssueType {
    ExchangePositionStale,
    ExchangeOpenOrderConflict,
    ExchangePositionFlat,
}
impl ExchangeReconciliationIssueType {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExchangePositionStale => "exchange_position_stale",
            Self::ExchangeOpenOrderConflict => "exchange_open_order_conflict",
            Self::ExchangePositionFlat => "exchange_position_flat",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeReconciliationReportRequest {
    /// combo ID。
    pub combo_id: i64,
    /// buyeremail，用于构建接口请求。
    pub buyer_email: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub issue_type: ExchangeReconciliationIssueType,
    /// 时间字段。
    pub detected_at: Option<String>,
    /// sourceref；为空时表示该条件不启用。
    pub source_ref: Option<String>,
    /// 提示信息。
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeReconciliationReportResponse {
    /// combo ID。
    pub combo_id: i64,
    /// buyeremail，用于返回接口响应。
    pub buyer_email: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// signal ID。
    pub signal_id: String,
    /// 类型标识。
    pub issue_type: String,
    /// 执行任务状态。
    pub api_execution_status: String,
    /// log，用于返回接口响应。
    pub log: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountOrderSnapshotInput {
    /// externalorder ID。
    pub external_order_id: String,
    /// 订单方向。
    pub order_side: String,
    /// 订单状态。
    pub order_status: String,
    /// 价格。
    pub price: Option<f64>,
    /// 数量数值。
    pub filled_qty: Option<f64>,
    /// 已成交计价金额；为空时表示没有成交金额。
    pub filled_quote: Option<f64>,
    /// 金额数值。
    pub fee_amount: Option<f64>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub observed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountTradeSnapshotInput {
    /// externaltrade ID。
    pub external_trade_id: String,
    /// externalorder ID；为空时使用默认值或表示不限制。
    pub external_order_id: Option<String>,
    /// 交易方向。
    pub side: String,
    /// 数量。
    pub quantity: Option<f64>,
    /// 金额数值。
    pub quote_amount: Option<f64>,
    /// 金额数值。
    pub fee_amount: Option<f64>,
    /// 价格。
    pub price: Option<f64>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub executed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountPositionSnapshotInput {
    /// 交易方向。
    pub side: String,
    /// 数量。
    pub quantity: f64,
    /// 金额数值。
    pub quote_amount: Option<f64>,
    /// 杠杆倍数。
    pub leverage: Option<f64>,
    /// 保证金模式；为空时使用交易所默认模式。
    pub margin_mode: Option<String>,
    /// 价格数值。
    pub liquidation_price: Option<f64>,
    /// margin 比例；为空时使用默认值或表示不限制。
    pub margin_ratio: Option<f64>,
    /// unrealized盈亏；为空时表示该条件不启用。
    pub unrealized_pnl: Option<f64>,
    /// 订单状态。
    pub protective_order_status: Option<String>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub snapshot_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountPositionHistorySnapshotInput {
    /// externalposition ID。
    pub external_position_id: String,
    /// 交易方向。
    pub side: Option<String>,
    /// 方向；为空时表示该条件不启用。
    pub direction: Option<String>,
    /// 类型标识。
    pub close_type: Option<String>,
    /// 保证金模式；为空时使用交易所默认模式。
    pub margin_mode: Option<String>,
    /// 杠杆倍数。
    pub leverage: Option<f64>,
    /// 价格数值。
    pub open_avg_price: Option<f64>,
    /// 离场价格。
    pub close_avg_price: Option<f64>,
    /// open最大仓位；为空时表示该条件不启用。
    pub open_max_position: Option<f64>,
    /// close总计仓位；为空时表示该条件不启用。
    pub close_total_position: Option<f64>,
    /// realizedpnl USDT 金额；为空时使用默认值或表示不限制。
    pub realized_pnl_usdt: Option<f64>,
    /// pnl USDT 金额；为空时使用默认值或表示不限制。
    pub pnl_usdt: Option<f64>,
    /// pnl 比例；为空时使用默认值或表示不限制。
    pub pnl_ratio: Option<f64>,
    /// fee USDT 金额；为空时使用默认值或表示不限制。
    pub fee_usdt: Option<f64>,
    /// 资金费率fee USDT 金额；为空时使用默认值或表示不限制。
    pub funding_fee_usdt: Option<f64>,
    /// liquidationpenalty USDT 金额；为空时使用默认值或表示不限制。
    pub liquidation_penalty_usdt: Option<f64>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub opened_at: Option<String>,
    /// 时间字段。
    pub closed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountBalanceSnapshotInput {
    /// 资产。
    pub asset: String,
    /// 金额数值。
    pub wallet_balance: Option<f64>,
    /// 金额数值。
    pub available_balance: Option<f64>,
    /// equity USDT 金额；为空时使用默认值或表示不限制。
    pub equity_usdt: Option<f64>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub snapshot_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountBillSnapshotInput {
    /// externalbill ID。
    pub external_bill_id: String,
    /// 资产。
    pub asset: String,
    /// balancechange；为空时表示该条件不启用。
    pub balance_change: Option<f64>,
    /// 余额change USDT 金额；为空时使用默认值或表示不限制。
    pub balance_change_usdt: Option<f64>,
    /// balance之后；为空时表示该条件不启用。
    pub balance_after: Option<f64>,
    /// 金额数值。
    pub fee_amount: Option<f64>,
    /// fee USDT 金额；为空时使用默认值或表示不限制。
    pub fee_usdt: Option<f64>,
    /// 金额数值。
    pub pnl_amount: Option<f64>,
    /// pnl USDT 金额；为空时使用默认值或表示不限制。
    pub pnl_usdt: Option<f64>,
    /// 类型标识。
    pub bill_type: Option<String>,
    /// 类型标识。
    pub bill_sub_type: Option<String>,
    /// externalorder ID；为空时使用默认值或表示不限制。
    pub external_order_id: Option<String>,
    /// externaltrade ID；为空时使用默认值或表示不限制。
    pub external_trade_id: Option<String>,
    /// rawpayloadJSON；为空时表示该值未提供。
    pub raw_payload_json: Option<String>,
    /// 时间字段。
    pub bill_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountSnapshotReportRequest {
    /// combo ID。
    pub combo_id: i64,
    /// 买家邮箱。
    pub buyer_email: String,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 来源引用标识。
    pub source_ref: String,
    /// 时间字段。
    pub snapshot_at: Option<String>,
    #[serde(default)]
    /// 列表数据。
    pub orders: Vec<ExchangeAccountOrderSnapshotInput>,
    #[serde(default)]
    /// 列表数据。
    pub trades: Vec<ExchangeAccountTradeSnapshotInput>,
    #[serde(default)]
    /// 列表数据。
    pub positions: Vec<ExchangeAccountPositionSnapshotInput>,
    #[serde(default)]
    /// 列表数据。
    pub position_history: Vec<ExchangeAccountPositionHistorySnapshotInput>,
    #[serde(default)]
    /// 列表数据。
    pub balances: Vec<ExchangeAccountBalanceSnapshotInput>,
    #[serde(default)]
    /// 列表数据。
    pub bills: Vec<ExchangeAccountBillSnapshotInput>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeAccountSnapshotReportResponse {
    /// combo ID。
    pub combo_id: i64,
    /// 买家邮箱。
    pub buyer_email: String,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 来源引用标识。
    pub source_ref: String,
    /// 时间字段。
    pub snapshot_at: String,
    /// 已写入或更新的订单数量。
    pub orders_upserted: i64,
    /// 已写入或更新的成交数量。
    pub trades_upserted: i64,
    /// 已写入或更新的仓位数量。
    pub positions_upserted: i64,
    /// 已写入或更新的仓位历史数量。
    pub position_history_upserted: i64,
    /// balances已写入数量。
    pub balances_upserted: i64,
    /// bills已写入数量。
    pub bills_upserted: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeCloseFillWritebackRequest {
    /// 任务 ID。
    pub task_id: i64,
    /// combo ID。
    pub combo_id: i64,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 来源ref，用于构建接口请求。
    pub source_ref: String,
    /// 开盘order ID；为空时使用默认值或表示不限制。
    pub open_order_id: Option<String>,
    /// 开盘trade ID；为空时使用默认值或表示不限制。
    pub open_trade_id: Option<String>,
    /// 收盘order ID。
    pub close_order_id: String,
    /// 收盘trade ID；为空时使用默认值或表示不限制。
    pub close_trade_id: Option<String>,
    /// 收盘方向，用于构建接口请求。
    pub close_side: String,
    /// 数量数值。
    pub close_size: f64,
    /// 离场价格。
    pub close_price: Option<f64>,
    /// closefee；为空时表示该条件不启用。
    pub close_fee: Option<f64>,
    /// 毫秒级时间戳或时长。
    pub close_timestamp_ms: Option<i64>,
    /// positionflatconfirmed，用于构建接口请求。
    pub position_flat_confirmed: bool,
    /// active未平仓订单数量。
    pub active_open_order_count: i64,
    /// quantitymatch，用于构建接口请求。
    pub quantity_match: bool,
    /// writebackauthorized，用于构建接口请求。
    pub writeback_authorized: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExchangeCloseFillWritebackResponse {
    /// 订单结果，用于返回接口响应。
    pub order_result: ExchangeOrderResult,
    /// traderecord，用于返回接口响应。
    pub trade_record: Value,
    /// position快照cleared，用于返回接口响应。
    pub position_snapshot_cleared: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalSubmitRequest {
    /// 数据来源。
    pub source: String,
    /// external ID。
    pub external_id: String,
    /// 策略slug，用于构建接口请求。
    pub strategy_slug: String,
    /// 策略Key，用于构建接口请求。
    pub strategy_key: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub signal_type: String,
    /// direction，用于构建接口请求。
    pub direction: String,
    /// 标题。
    pub title: String,
    /// 摘要；为空时使用默认值或表示不限制。
    pub summary: Option<String>,
    /// 置信度评分。
    pub confidence: Option<f64>,
    /// 载荷json，用于构建接口请求。
    pub payload_json: String,
    /// 时间字段。
    pub generated_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalInbox {
    /// 唯一标识。
    pub id: i64,
    /// 数据来源。
    pub source: String,
    /// external ID。
    pub external_id: String,
    /// 策略slug，用于记录新闻或情报分析结果。
    pub strategy_slug: String,
    /// 策略Key，用于记录新闻或情报分析结果。
    pub strategy_key: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// 类型标识。
    pub signal_type: String,
    /// direction，用于记录新闻或情报分析结果。
    pub direction: String,
    /// 标题。
    pub title: String,
    /// 摘要；为空时使用默认值或表示不限制。
    pub summary: Option<String>,
    /// 置信度评分。
    pub confidence: Option<f64>,
    /// 载荷json，用于记录新闻或情报分析结果。
    pub payload_json: String,
    /// 时间字段。
    pub generated_at: String,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalDispatchResponse {
    /// inbox，用于返回接口响应。
    pub inbox: StrategySignalInbox,
    /// 列表数据。
    pub generated_tasks: Vec<ExecutionTask>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityPaperOutcomeRequest {
    /// rankevent ID。
    pub rank_event_id: i64,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// targetr，用于构建接口请求。
    pub target_r: f64,
    /// 小时级时长。
    pub horizon_hours: i32,
    /// 入场ruleversion，用于构建接口请求。
    pub entry_rule_version: String,
    /// entry触发原因；为空时表示该条件不启用。
    pub entry_trigger: Option<String>,
    /// 入场价格。
    pub entry_price: f64,
    /// 时间字段。
    pub entry_at: String,
    /// 状态值。
    pub outcome_status: String,
    /// 原因说明。
    pub exit_reason: String,
    /// resultR 倍数；为空时表示该条件不启用。
    pub result_r: Option<f64>,
    /// 时间字段。
    pub evaluated_at: String,
    /// evaluation载荷，用于构建接口请求。
    pub evaluation_payload: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityPaperOutcomeResponse {
    /// outcome，用于返回接口响应。
    pub outcome: Value,
    /// generatedexecutiontask数量。
    pub generated_execution_task_count: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewRequest {
    /// rankevent ID；为空时使用默认值或表示不限制。
    pub rank_event_id: Option<i64>,
    /// 买家邮箱；为空时表示未绑定买家邮箱。
    pub buyer_email: Option<String>,
    /// combo ID；为空时使用默认值或表示不限制。
    pub combo_id: Option<i64>,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// targetr，用于构建接口请求。
    pub target_r: f64,
    /// 小时级时长。
    pub horizon_hours: i32,
    /// entryruleversion；为空时表示该条件不启用。
    pub entry_rule_version: Option<String>,
    /// entry触发原因filterversion；为空时表示该条件不启用。
    pub entry_trigger_filter_version: Option<String>,
    /// 风控adjustedwinrateedge；为空时表示该条件不启用。
    pub risk_adjusted_win_rate_edge: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewCheck {
    /// 代码。
    pub code: String,
    /// label，用于记录交易或执行状态。
    pub label: String,
    /// 当前状态。
    pub status: String,
    /// 是否阻塞当前流程。
    pub blocker_code: Option<String>,
    /// 详情。
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskCreationPreviewResponse {
    /// readonly，用于返回接口响应。
    pub read_only: bool,
    /// Dry-runrunonly，用于返回接口响应。
    pub dry_run_only: bool,
    /// 是否允许该操作。
    pub mutation_allowed: bool,
    /// wouldcreate执行任务，用于返回接口响应。
    pub would_create_execution_task: bool,
    /// generatedexecutiontask数量。
    pub generated_execution_task_count: i64,
    /// ownerservice，用于返回接口响应。
    pub owner_service: String,
    /// 当前状态。
    pub status: String,
    /// 交易所名称。
    pub exchange: String,
    /// 交易对或资产符号。
    pub symbol: String,
    /// rankevent ID；为空时使用默认值或表示不限制。
    pub rank_event_id: Option<i64>,
    /// 买家邮箱；为空时表示未绑定买家邮箱。
    pub buyer_email: Option<String>,
    /// combo ID；为空时使用默认值或表示不限制。
    pub combo_id: Option<i64>,
    /// targetr，用于返回接口响应。
    pub target_r: f64,
    /// 小时级时长。
    pub horizon_hours: i32,
    /// 入场ruleversion，用于返回接口响应。
    pub entry_rule_version: String,
    /// entry触发原因filterversion；为空时表示该条件不启用。
    pub entry_trigger_filter_version: Option<String>,
    /// 风控adjustedwinrateedge；为空时表示该条件不启用。
    pub risk_adjusted_win_rate_edge: Option<f64>,
    /// 列表数据。
    pub required_web_checks: Vec<MarketVelocityExecutionTaskCreationPreviewCheck>,
    /// 是否阻塞当前流程。
    pub blocker_codes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskLiveReadinessCheck {
    /// 代码。
    pub code: String,
    /// label，用于记录交易或执行状态。
    pub label: String,
    /// 当前状态。
    pub status: String,
    /// 是否阻塞当前流程。
    pub blocker_code: Option<String>,
    /// 详情。
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MarketVelocityExecutionTaskLiveReadinessResponse {
    /// readonly，用于返回接口响应。
    pub read_only: bool,
    /// 是否允许该操作。
    pub mutation_allowed: bool,
    /// ownerservice，用于返回接口响应。
    pub owner_service: String,
    /// 当前状态。
    pub status: String,
    /// 任务。
    pub task: ExecutionTask,
    /// 列表数据。
    pub checks: Vec<MarketVelocityExecutionTaskLiveReadinessCheck>,
    /// 是否阻塞当前流程。
    pub blocker_codes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UserExchangeConfig {
    /// buyeremail，用于配置运行参数。
    pub buyer_email: String,
    /// 交易所名称。
    pub exchange: String,
    /// API Key。
    pub api_key: String,
    /// APISecret，用于配置运行参数。
    pub api_secret: String,
    /// API passphrase；为空时表示该交易所不需要 passphrase。
    pub passphrase: Option<String>,
    #[serde(default)]
    /// simulated，用于配置运行参数。
    pub simulated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ApiCredentialCheckSummary {
    /// 唯一标识。
    pub id: i64,
    /// 交易所名称。
    pub exchange: String,
    /// APIKeymask。
    pub api_key_mask: String,
    /// permissionscope，用于展示或持久化查询结果。
    pub permission_scope: String,
    /// 当前状态。
    pub status: String,
    /// 凭证envelopeready，用于展示或持久化查询结果。
    pub credential_envelope_ready: bool,
    /// 时间字段。
    pub last_check_at: Option<String>,
    /// lastcheckcode；为空时表示该条件不启用。
    pub last_check_code: Option<String>,
    /// 最近check消息；为空时使用默认值或表示不限制。
    pub last_check_message: Option<String>,
    /// 创建时间。
    pub created_at: String,
    /// 最后更新时间。
    pub updated_at: String,
    /// 准备度状态，用于展示或持久化查询结果。
    pub execution_readiness: ApiCredentialExecutionReadiness,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ApiCredentialExecutionReadiness {
    /// canexecute。
    pub can_execute: bool,
    /// 是否阻塞当前流程。
    pub blocker_code: Option<String>,
    /// 是否阻塞当前流程。
    pub blocker_message: Option<String>,
    /// nextactionlabel；为空时表示该条件不启用。
    pub next_action_label: Option<String>,
    /// nextactionhref；为空时表示该条件不启用。
    pub next_action_href: Option<String>,
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn deserialize_json_value_from_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Value, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(raw) => serde_json::from_str(&raw).map_err(serde::de::Error::custom),
        other => Ok(other),
    }
}
