// 风险监控任务
use anyhow::{anyhow, Context, Result};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::SetLeverageRequest;
use okx::dto::asset_dto::TransferOkxReqDto;
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::EnumToStrTrait;
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::{OkxAccount, OkxAsset};
use std::str::FromStr;
use tracing::{error, info, span, Level};
// 常量定义
const DEFAULT_CURRENCY: &str = "USDT";
const BALANCE_RATIO: f64 = 2.0; // 资金账户与交易账户的比例
const BTC_LEVEL: i32 = 8;
const ETH_LEVEL: i32 = 5;
const OTHER_LEVEL: i32 = 3;
const RISK_BALANCE_LIVE_MUTATION_CONFIRM_ENV: &str = "RISK_BALANCE_LIVE_MUTATION_CONFIRM";
const RISK_BALANCE_LIVE_MUTATION_CONFIRM_TOKEN: &str = "I_UNDERSTAND_RISK_BALANCE_LIVE_MUTATIONS";
const LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
const LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS";
/// 风险管理任务，负责在资金账户和交易账户之间平衡资金
pub struct RiskBalanceWithLevelJob {
    /// 默认货币类型
    currency: String,
    /// 资金平衡比例（资金账户:交易账户）
    balance_ratio: f64,
}
impl RiskBalanceWithLevelJob {
    /// 创建新的风险管理任务实例
    pub fn new() -> Self {
        Self {
            currency: DEFAULT_CURRENCY.to_string(),
            balance_ratio: BALANCE_RATIO,
        }
    }
}
impl Default for RiskBalanceWithLevelJob {
    fn default() -> Self {
        Self::new()
    }
}
impl RiskBalanceWithLevelJob {
    /// 封装当前函数，减少风控调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub async fn run(&self, inst_ids: &[String]) -> Result<(), anyhow::Error> {
        Self::ensure_live_mutation_allowed()?;
        //1. 控制交易资金
        // match self.control_trade_amount().await {
        //     Ok(_) => info!("资金账户与交易账户平衡完成!"),
        //     Err(e) => error!("资金账户与交易账户平衡失败: {:?}", e),
        // }
        //2. 控制合约杠杆
        self.run_set_leverage(inst_ids).await?;
        info!("设置最大杠杆完成!");
        Ok(())
    }
    /// 使用自定义参数创建风险管理任务实例
    pub fn with_config(currency: String, balance_ratio: f64) -> Self {
        Self {
            currency,
            balance_ratio,
        }
    }
    /// risk 2 设置杠杆
    pub async fn run_set_leverage(&self, inst_ids: &[String]) -> Result<(), anyhow::Error> {
        Self::ensure_live_mutation_allowed()?;
        let span = span!(Level::DEBUG, "run_set_leverage");
        let _enter = span.enter();
        for inst_id in inst_ids.iter() {
            let level: i32;
            if inst_id == "BTC-USDT-SWAP" {
                level = BTC_LEVEL;
            } else if inst_id == "ETH-USDT-SWAP" {
                level = ETH_LEVEL;
            } else {
                level = OTHER_LEVEL;
            }
            for post_side in [PositionSide::Long, PositionSide::Short] {
                let params = SetLeverageRequest {
                    inst_id: Some(inst_id.to_string()),
                    ccy: None,
                    mgn_mode: TdModeEnum::ISOLATED.as_str().to_owned(),
                    lever: level.to_string(),
                    pos_side: Some(post_side.as_str().to_owned()),
                };
                //延迟100ms
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                OkxAccount::from_env()?.set_leverage(params).await?;
            }
        }
        Ok(())
    }
    /// risk 1 执行风险管理任务，保持交易账户资金为资金账户的一半
    pub async fn control_trade_amount(&self) -> Result<()> {
        // 获取资金账户和交易账户的余额
        let (asset_balance, trade_balance) = self.get_account_balances().await?;
        // 计算目标余额和当前差额
        let (transfer_needed, direction) =
            self.calculate_transfer_needs(&asset_balance, &trade_balance)?;
        // 如果需要转账，执行转账操作
        if transfer_needed > 0.0 {
            self.execute_transfer(transfer_needed, direction).await?;
        } else {
            info!("账户资金平衡，无需转账");
        }
        Ok(())
    }
    /// 获取资金账户和交易账户的余额
    async fn get_account_balances(&self) -> Result<(String, String)> {
        Self::ensure_signed_read_only_allowed()?;
        // 获取资金账户资产
        let asset = OkxAsset::from_env().context("无法从环境变量创建资金账户客户端")?;
        let currencies = vec![self.currency.clone()];
        let asset_value = asset
            .get_balances(Some(&currencies))
            .await
            .context("获取资金账户余额失败")?;
        let asset_balance = asset_value
            .first()
            .ok_or_else(|| anyhow!("未找到资金账户中的{}余额", self.currency))?;
        info!("资金账户余额: {:?}", asset_balance);
        // 获取交易账户资产
        let account = OkxAccount::from_env().context("无法从环境变量创建交易账户客户端")?;
        let trade_asset = account
            .get_balance(Some(&self.currency))
            .await
            .context("获取交易账户余额失败")?;
        let trade_balance = trade_asset
            .first()
            .ok_or_else(|| anyhow!("未找到交易账户中的{}余额", self.currency))?;
        info!("交易账户余额: {:?}", trade_balance);
        Ok((
            asset_balance.avail_bal.clone(),
            trade_balance.avail_eq.clone(),
        ))
    }
    /// 计算需要转账的金额和方向
    fn calculate_transfer_needs(
        &self,
        asset_balance_str: &str,
        trade_balance_str: &str,
    ) -> Result<(f64, TransferDirection)> {
        // 解析字符串为数字
        println!("asset_balance_str: {}", asset_balance_str);
        println!("trade_balance_str: {}", trade_balance_str);
        let asset_balance = f64::from_str(asset_balance_str).unwrap_or_else(|_| {
            error!("无法解析资金账户余额: {}", asset_balance_str);
            0.0
        });
        let trade_balance = f64::from_str(trade_balance_str).unwrap_or_else(|_| {
            error!("无法解析交易账户余额: {}", trade_balance_str);
            0.0
        });
        // 计算目标余额：资金账户的一半
        let target_trade_balance = asset_balance / self.balance_ratio;
        info!(
            "当前状态 - 资金账户: {:.2} {}, 交易账户: {:.2} {}, 目标交易账户余额: {:.2} {}",
            asset_balance,
            self.currency,
            trade_balance,
            self.currency,
            target_trade_balance,
            self.currency
        );
        // 确定转账方向和金额
        //差值过小,小于10u,也不进行转账
        if (trade_balance - target_trade_balance).abs() < 10.00 {
            return Ok((0.0, TransferDirection::FundToTrade));
        }
        if trade_balance < target_trade_balance {
            // 交易账户资金不足，需要从资金账户转入
            let transfer_amount = target_trade_balance - trade_balance;
            Ok((transfer_amount, TransferDirection::FundToTrade))
        } else if trade_balance > target_trade_balance {
            // 交易账户资金过多，需要转回资金账户
            let transfer_amount = trade_balance - target_trade_balance;
            Ok((transfer_amount, TransferDirection::TradeToFund))
        } else {
            // 余额已经平衡，无需转账
            Ok((0.0, TransferDirection::FundToTrade))
        }
    }
    /// 执行转账操作
    async fn execute_transfer(&self, amount: f64, direction: TransferDirection) -> Result<()> {
        Self::ensure_live_mutation_allowed()?;
        let asset = OkxAsset::from_env().context("无法从环境变量创建资金账户客户端")?;
        // 准备转账请求
        let (from, to) = match direction {
            TransferDirection::FundToTrade => (AccountType::FOUND, AccountType::TRADE),
            TransferDirection::TradeToFund => (AccountType::TRADE, AccountType::FOUND),
        };
        info!(
            "执行转账: {:.8} {} 从 {:?} 到 {:?}",
            amount,
            self.currency,
            from.clone(),
            to.clone()
        );
        let transfer_req = TransferOkxReqDto {
            ccy: self.currency.clone(),
            amt: format!("{:.8}", amount), // 使用足够的精度
            transfer_type: Some(format!("{:.8}", amount)),
            from,
            to,
            sub_acct: None,
        };
        // 执行转账
        let res = asset
            .transfer(&transfer_req)
            .await
            .context("执行转账操作失败")?;
        info!("转账结果: {:?}", res);
        Ok(())
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_live_mutation_allowed() -> Result<()> {
        let confirmation = std::env::var(RISK_BALANCE_LIVE_MUTATION_CONFIRM_ENV).ok();
        if confirmation.as_deref().map(str::trim) == Some(RISK_BALANCE_LIVE_MUTATION_CONFIRM_TOKEN)
        {
            return Ok(());
        }
        Err(anyhow!(
            "risk balance live account mutation is blocked; set {}={} after validating OKX credentials, leverage scope, transfer direction, and rollback plan",
            RISK_BALANCE_LIVE_MUTATION_CONFIRM_ENV,
            RISK_BALANCE_LIVE_MUTATION_CONFIRM_TOKEN
        ))
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_signed_read_only_allowed() -> Result<()> {
        let signed_confirmation = std::env::var(LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV).ok();
        let mutation_confirmation = std::env::var(RISK_BALANCE_LIVE_MUTATION_CONFIRM_ENV).ok();
        if signed_confirmation.as_deref().map(str::trim)
            == Some(LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN)
            || mutation_confirmation.as_deref().map(str::trim)
                == Some(RISK_BALANCE_LIVE_MUTATION_CONFIRM_TOKEN)
        {
            return Ok(());
        }
        Err(anyhow!(
            "risk balance signed read-only account query is blocked; set {}={} for account read evidence or {}={} after validating OKX credentials, transfer direction, and rollback plan",
            LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV,
            LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN,
            RISK_BALANCE_LIVE_MUTATION_CONFIRM_ENV,
            RISK_BALANCE_LIVE_MUTATION_CONFIRM_TOKEN
        ))
    }
}
/// 转账方向枚举
enum TransferDirection {
    /// 从资金账户到交易账户
    FundToTrade,
    /// 从交易账户到资金账户
    TradeToFund,
}
/// 测试
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    const TEST_CONFIRM_ENV: &str = "RISK_BALANCE_LIVE_MUTATION_CONFIRM";
    const TEST_SIGNED_READ_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
    /// 封装当前函数，减少风控调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
    /// 提供lock环境变量的集中实现，避免风控调用方重复处理相同细节。
    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    struct EnvSnapshot {
        /// mutation值；为空时表示该条件不启用。
        mutation_value: Option<String>,
        /// signedread值；为空时表示该条件不启用。
        signed_read_value: Option<String>,
    }
    impl EnvSnapshot {
        /// 提供capture的集中实现，避免风控调用方重复处理相同细节。
        fn capture() -> Self {
            Self {
                mutation_value: std::env::var(TEST_CONFIRM_ENV).ok(),
                signed_read_value: std::env::var(TEST_SIGNED_READ_CONFIRM_ENV).ok(),
            }
        }
    }
    impl Drop for EnvSnapshot {
        /// 封装释放，减少风控调用方重复实现相同细节。
        fn drop(&mut self) {
            match &self.mutation_value {
                Some(value) => std::env::set_var(TEST_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_CONFIRM_ENV),
            }
            match &self.signed_read_value {
                Some(value) => std::env::set_var(TEST_SIGNED_READ_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV),
            }
        }
    }
    #[tokio::test]
    async fn run_set_leverage_requires_live_mutation_confirmation_before_okx_client() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV);
        let risk_job = RiskBalanceWithLevelJob::new();
        let error = risk_job
            .run_set_leverage(&["ETH-USDT-SWAP".to_string()])
            .await
            .expect_err("risk leverage mutation must require explicit confirmation");
        assert!(
            error.to_string().contains(TEST_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
    }
    #[tokio::test]
    async fn transfer_requires_live_mutation_confirmation_before_okx_client() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV);
        let risk_job = RiskBalanceWithLevelJob::new();
        let error = risk_job
            .execute_transfer(12.0, TransferDirection::FundToTrade)
            .await
            .expect_err("risk balance transfer must require explicit confirmation");
        assert!(
            error.to_string().contains(TEST_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
    }
    #[tokio::test]
    async fn run_propagates_missing_live_mutation_confirmation() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV);
        let risk_job = RiskBalanceWithLevelJob::new();
        let error = risk_job
            .run(&["ETH-USDT-SWAP".to_string()])
            .await
            .expect_err("risk job run must fail closed when live mutation confirmation is missing");
        assert!(
            error.to_string().contains(TEST_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
    }
    #[test]
    fn live_mutation_confirmation_accepts_exact_token_only() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::set_var(TEST_CONFIRM_ENV, "wrong-token");
        let error = RiskBalanceWithLevelJob::ensure_live_mutation_allowed()
            .expect_err("invalid confirmation token must not allow live account mutation");
        assert!(
            error.to_string().contains(TEST_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
        std::env::set_var(
            TEST_CONFIRM_ENV,
            " I_UNDERSTAND_RISK_BALANCE_LIVE_MUTATIONS ",
        );
        RiskBalanceWithLevelJob::ensure_live_mutation_allowed()
            .expect("exact trimmed confirmation token should allow explicit live mutation");
    }
    #[tokio::test]
    async fn control_trade_amount_requires_signed_read_confirmation_before_okx_client() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV);
        let risk_job = RiskBalanceWithLevelJob::new();
        let error = risk_job
            .control_trade_amount()
            .await
            .expect_err("risk balance account reads must require explicit signed-read scope");
        assert!(
            error.to_string().contains(TEST_SIGNED_READ_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
    }
    #[test]
    fn signed_read_confirmation_accepts_exact_token_or_live_mutation_token() {
        let _guard = lock_env();
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        std::env::set_var(TEST_SIGNED_READ_CONFIRM_ENV, "wrong-token");
        let error = RiskBalanceWithLevelJob::ensure_signed_read_only_allowed()
            .expect_err("invalid signed-read token must not allow account reads");
        assert!(
            error.to_string().contains(TEST_SIGNED_READ_CONFIRM_ENV),
            "unexpected error: {error:#}"
        );
        std::env::set_var(
            TEST_SIGNED_READ_CONFIRM_ENV,
            " I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS ",
        );
        RiskBalanceWithLevelJob::ensure_signed_read_only_allowed()
            .expect("exact trimmed signed-read token should allow account reads");
        std::env::remove_var(TEST_SIGNED_READ_CONFIRM_ENV);
        std::env::set_var(
            TEST_CONFIRM_ENV,
            " I_UNDERSTAND_RISK_BALANCE_LIVE_MUTATIONS ",
        );
        RiskBalanceWithLevelJob::ensure_signed_read_only_allowed()
            .expect("risk mutation token should allow prerequisite account reads");
    }
    #[test]
    fn run_propagates_leverage_errors_instead_of_logging_success() {
        let source = include_str!("risk_balance_job.rs");
        let run_start = source
            .find("pub async fn run(&self, inst_ids: &[String])")
            .expect("risk balance run method must exist");
        let run_end = source[run_start..]
            .find("/// 使用自定义参数创建风险管理任务实例")
            .map(|offset| run_start + offset)
            .expect("risk balance run section end must exist");
        let run_section = &source[run_start..run_end];
        assert!(
            run_section.contains(".run_set_leverage(inst_ids).await?"),
            "risk balance run must propagate set_leverage failures to the scheduler"
        );
        assert!(
            !run_section.contains("Err(e) => error!(\"设置最大杠杆失败"),
            "risk balance run must not swallow set_leverage failures after live mutation authorization"
        );
    }
}
