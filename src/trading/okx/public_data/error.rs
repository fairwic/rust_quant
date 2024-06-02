use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("OK")]
    Ok = 0,
    #[error("操作全部失败")]
    OperationFailed = 1,
    #[error("批量操作部分成功")]
    PartialSuccess = 2,
    #[error("POST请求的body不能为空")]
    EmptyBody = 50000,
    #[error("服务暂时不可用，请稍后重试")]
    ServiceUnavailable = 50001,
    #[error("JSON 语法错误")]
    JsonSyntaxError = 50002,
    #[error("接口请求超时（不代表请求成功或者失败，请检查请求结果）")]
    RequestTimeout = 50004,
    #[error("接口已下线或无法使用")]
    InterfaceDeprecated = 50005,
    #[error("无效的 Content-Type，请使用“application/JSON”格式")]
    InvalidContentType = 50006,
    #[error("用户被冻结")]
    UserFrozen = 50007,
    #[error("用户不存在")]
    UserNotFound = 50008,
    #[error("用户处于爆仓冻结")]
    UserMarginFrozen = 50009,
    #[error("用户ID为空")]
    UserIdEmpty = 50010,
    // #[error("用户请求频率过快，超过该接口允许的限额。请参考 API 文档并限制请求")]
    // UserRequestTooFrequent = 50011,
    #[error("请求频率太高")]
    TooManyRequests = 50011,
    #[error("账户状态无效，请检查帐户的状态")]
    InvalidAccountStatus = 50012,
    #[error("当前系统繁忙，请稍后重试")]
    SystemBusy = 50013,
    #[error("必填参数param0不能为空")]
    MissingRequiredParameter = 50014,
    #[error("参数params和params不能同时为空")]
    ParametersCannotBeBothEmpty = 50015,
    #[error("参数params和params不匹配")]
    ParameterMismatch = 50016,
    #[error("当前仓位处于自动减仓 (ADL) 冻结中，无法进行相关操作，请稍后重试")]
    AdlPositionFrozen = 50017,
    #[error("params 处于自动减仓 (ADL) 冻结中，无法进行相关操作，请稍后重试")]
    AdlParamFrozen = 50018,
    #[error("当前账户处于自动减仓 (ADL) 冻结中，无法进行相关操作，请稍后重试")]
    AdlAccountFrozen = 50019,
    #[error("当前仓位处于强平冻结中，无法进行相关操作，请稍后重试")]
    ForcedLiquidationPositionFrozen = 50020,
    #[error("params 处于强平冻结中，无法进行相关操作，请稍后重试")]
    ForcedLiquidationParamFrozen = 50021,
    #[error("当前账户处于强平冻结中，无法进行相关操作，请稍后重试")]
    ForcedLiquidationAccountFrozen = 50022,
    #[error("资金费冻结，无法进行相关操作，请稍后重试")]
    FundingFeeFrozen = 50023,
    #[error("参数params和params不能同时存在")]
    ParametersCannotBothExist = 50024,
    #[error("参数params传值个数超过最大限制params")]
    ParameterValueExceedsLimit = 50025,
    #[error("系统错误，请稍后重试")]
    SystemError = 50026,
    #[error("当前账户已被限制交易，请联系客服处理")]
    AccountRestricted = 50027,
    #[error("账户异常无法下单")]
    AccountException = 50028,
    #[error("你的账户已经触发风控体系，禁止交易该标的，请检查您在欧易注册的电子邮件以便我们的客服联系"
    )]
    RiskControlTriggered = 50029,
    #[error("您没有使用此 API 接口的权限")]
    NoApiPermission = 50030,
    #[error("您的账户已设置禁止该币种交易，请确认后重试")]
    CoinTradeForbidden = 50032,
    #[error("您的账户已设置禁止该业务线交易，请确认后重试")]
    BusinessLineTradeForbidden = 50033,
    #[error("该接口要求APIKey必须绑定IP")]
    ApiKeyIpBindingRequired = 50035,
    #[error("expTime 不能早于当前系统时间，请调整 expTime 后重试")]
    ExpTimeInvalid = 50036,
    #[error("订单已过期")]
    OrderExpired = 50037,
    #[error("模拟交易不支持该功能")]
    SimulatedTradingUnsupported = 50038,
    #[error("时间戳分页时，不支持使用before参数")]
    TimestampPaginationUnsupported = 50039,
    #[error("操作频繁，请稍后重试")]
    OperationTooFrequent = 50040,
    #[error("用户 ID 未被列入白名单列表，请联系客服")]
    UserNotInWhitelist = 50041,
    #[error("必须指定一种broker类型")]
    BrokerTypeRequired = 50044,
    #[error("params 已经交割，对应的K线请使用params查询")]
    ParamAlreadyDelivered = 50047,
    #[error("切换对冲单元可能导致仓位风险水平升高，引起强制平仓。请调整仓位，使保证金处于安全状态。")]
    HedgingUnitSwitchRisk = 50048,
    #[error("无仓位档位信息，该币种不支持杠杆交易")]
    NoPositionTierInfo = 50049,
    #[error("您已开通期权交易服务，请勿重复开通")]
    OptionsTradingAlreadyEnabled = 50050,
    #[error("由于您所在国家或地区的合规限制，您无法使用该功能")]
    RegulatoryRestriction = 50051,
    #[error("根据当地的法律法规，您无法交易您选择的币种")]
    CoinTradeProhibitedByLaw = 50052,
    #[error("该功能只支持模拟盘")]
    FunctionSimulatedOnly = 50053,
    #[error("资产重置失败，超过每日设置5次资产上限")]
    AssetResetFailed = 50055,
    #[error("当前账户有交易挂单或持仓，请完成全部撤单/平仓后进行重置")]
    CompleteAllOrdersBeforeReset = 50056,
    #[error("资产重置失败，请稍后重试")]
    AssetResetRetry = 50057,
    #[error("该币种不支持资产重置")]
    AssetResetUnsupported = 50058,
    #[error("继续下一步之前，请按照当地监管机构的要求完成额外步骤。您可以前往欧易网页端或 App 端了解详情。"
    )]
    RegulatoryStepRequired = 50059,
    #[error("根据当地法律法规，您需要完成身份认证方可继续使用我们的服务。")]
    IdentityVerificationRequired = 50060,
    #[error("订单请求频率过快，超过账户允许的最高限额")]
    OrderRequestTooFrequent = 50061,
    #[error("该功能暂不可用")]
    FunctionUnavailable = 50062,
    #[error("激活失败，您的体验金可能已过期或已激活")]
    ActivationFailed = 50063,
    #[error("体验金不存在")]
    ExperienceFundNotFound = 50064,
    // API 类错误码
    #[error("Api 已被冻结，请联系客服处理")]
    ApiFrozen = 50100,
    #[error("APIKey 与当前环境不匹配")]
    ApiKeyEnvironmentMismatch = 50101,
    #[error("请求时间戳过期")]
    RequestTimestampExpired = 50102,
    #[error("请求头\"OK-ACCESS-KEY\"不能为空")]
    MissingOkAccessKey = 50103,
    #[error("请求头\"OK-ACCESS-PASSPHRASE\"不能为空")]
    MissingOkAccessPassphrase = 50104,
    #[error("请求头\"OK-ACCESS-PASSPHRASE\"错误")]
    InvalidOkAccessPassphrase = 50105,
    #[error("请求头\"OK-ACCESS-SIGN\"不能为空")]
    MissingOkAccessSign = 50106,
    #[error("请求头\"OK-ACCESS-TIMESTAMP\"不能为空")]
    MissingOkAccessTimestamp = 50107,
    #[error("券商ID不存在")]
    BrokerIdNotFound = 50108,
    #[error("券商域名不存在")]
    BrokerDomainNotFound = 50109,
    #[error("您的IPparams不在APIKey绑定IP名单中 (您可以将您的IP加入到APIKey绑定白名单中)")]
    IpNotInApiKeyWhitelist = 50110,
    #[error("无效的OK-ACCESS-KEY")]
    InvalidOkAccessKey = 50111,
    #[error("无效的OK-ACCESS-TIMESTAMP")]
    InvalidOkAccessTimestamp = 50112,
    #[error("无效的签名")]
    InvalidSignature = 50113,
    #[error("无效的授权")]
    InvalidAuthorization = 50114,
    #[error("无效的请求类型")]
    InvalidRequestType = 50115,
    #[error("Fast API 只能创建一个 API key")]
    FastApiSingleKey = 50116,
    #[error("如需将 API key 绑定 App，经纪商需要提供 IP 才能加入白名单")]
    ApiKeyAppBinding = 50118,
    #[error("API key 不存在")]
    ApiKeyNotFound = 50119,
    #[error("API key 权限不足")]
    InsufficientApiKeyPermissions = 50120,
    #[error("您无权通过该 IP 地址 (params) 访问")]
    UnauthorizedIpAccess = 50121,
}

impl ApiError {
    pub fn from_code(code: u32) -> Option<ApiError> {
        match code {
            0 => Some(ApiError::Ok),
            1 => Some(ApiError::OperationFailed),
            2 => Some(ApiError::PartialSuccess),
            50000 => Some(ApiError::EmptyBody),
            50001 => Some(ApiError::ServiceUnavailable),
            50002 => Some(ApiError::JsonSyntaxError),
            50004 => Some(ApiError::RequestTimeout),
            50005 => Some(ApiError::InterfaceDeprecated),
            50006 => Some(ApiError::InvalidContentType),
            50007 => Some(ApiError::UserFrozen),
            50008 => Some(ApiError::UserNotFound),
            50009 => Some(ApiError::UserMarginFrozen),
            50010 => Some(ApiError::UserIdEmpty),
            // 50011 => Some(ApiError::UserRequestTooFrequent),
            50012 => Some(ApiError::InvalidAccountStatus),
            50013 => Some(ApiError::SystemBusy),
            50014 => Some(ApiError::MissingRequiredParameter),
            50015 => Some(ApiError::ParametersCannotBeBothEmpty),
            50016 => Some(ApiError::ParameterMismatch),
            50017 => Some(ApiError::AdlPositionFrozen),
            50018 => Some(ApiError::AdlParamFrozen),
            50019 => Some(ApiError::AdlAccountFrozen),
            50020 => Some(ApiError::ForcedLiquidationPositionFrozen),
            50021 => Some(ApiError::ForcedLiquidationParamFrozen),
            50022 => Some(ApiError::ForcedLiquidationAccountFrozen),
            50023 => Some(ApiError::FundingFeeFrozen),
            50024 => Some(ApiError::ParametersCannotBothExist),
            50025 => Some(ApiError::ParameterValueExceedsLimit),
            50026 => Some(ApiError::SystemError),
            50027 => Some(ApiError::AccountRestricted),
            50028 => Some(ApiError::AccountException),
            50029 => Some(ApiError::RiskControlTriggered),
            50030 => Some(ApiError::NoApiPermission),
            50032 => Some(ApiError::CoinTradeForbidden),
            50033 => Some(ApiError::BusinessLineTradeForbidden),
            50035 => Some(ApiError::ApiKeyIpBindingRequired),
            50036 => Some(ApiError::ExpTimeInvalid),
            50037 => Some(ApiError::OrderExpired),
            50038 => Some(ApiError::SimulatedTradingUnsupported),
            50039 => Some(ApiError::TimestampPaginationUnsupported),
            50040 => Some(ApiError::OperationTooFrequent),
            50041 => Some(ApiError::UserNotInWhitelist),
            50044 => Some(ApiError::BrokerTypeRequired),
            50047 => Some(ApiError::ParamAlreadyDelivered),
            50048 => Some(ApiError::HedgingUnitSwitchRisk),
            50049 => Some(ApiError::NoPositionTierInfo),
            50050 => Some(ApiError::OptionsTradingAlreadyEnabled),
            50051 => Some(ApiError::RegulatoryRestriction),
            50052 => Some(ApiError::CoinTradeProhibitedByLaw),
            50053 => Some(ApiError::FunctionSimulatedOnly),
            50055 => Some(ApiError::AssetResetFailed),
            50056 => Some(ApiError::CompleteAllOrdersBeforeReset),
            50057 => Some(ApiError::AssetResetRetry),
            50058 => Some(ApiError::AssetResetUnsupported),
            50059 => Some(ApiError::RegulatoryStepRequired),
            50060 => Some(ApiError::IdentityVerificationRequired),
            50061 => Some(ApiError::OrderRequestTooFrequent),
            50062 => Some(ApiError::FunctionUnavailable),
            50063 => Some(ApiError::ActivationFailed),
            50064 => Some(ApiError::ExperienceFundNotFound),
            50100 => Some(ApiError::ApiFrozen),
            50101 => Some(ApiError::ApiKeyEnvironmentMismatch),
            50102 => Some(ApiError::RequestTimestampExpired),
            50103 => Some(ApiError::MissingOkAccessKey),
            50104 => Some(ApiError::MissingOkAccessPassphrase),
            50105 => Some(ApiError::InvalidOkAccessPassphrase),
            50106 => Some(ApiError::MissingOkAccessSign),
            50107 => Some(ApiError::MissingOkAccessTimestamp),
            50108 => Some(ApiError::BrokerIdNotFound),
            50109 => Some(ApiError::BrokerDomainNotFound),
            50110 => Some(ApiError::IpNotInApiKeyWhitelist),
            50111 => Some(ApiError::InvalidOkAccessKey),
            50112 => Some(ApiError::InvalidOkAccessTimestamp),
            50113 => Some(ApiError::InvalidSignature),
            50114 => Some(ApiError::InvalidAuthorization),
            50115 => Some(ApiError::InvalidRequestType),
            50116 => Some(ApiError::FastApiSingleKey),
            50118 => Some(ApiError::ApiKeyAppBinding),
            50119 => Some(ApiError::ApiKeyNotFound),
            50120 => Some(ApiError::InsufficientApiKeyPermissions),
            50121 => Some(ApiError::UnauthorizedIpAccess),
            _ => None,
        }
    }
}
