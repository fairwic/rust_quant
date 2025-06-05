// 风险监控任务

use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::SetLeverageRequest;
use okx::dto::asset_dto::{AssetBalance, TransferOkxReqDto};
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::{OkxAccount, OkxAsset};
use std::str::FromStr;
use tracing::{span, Level};

// 常量定义
const DEFAULT_CURRENCY: &str = "USDT";
const BALANCE_RATIO: f64 = 2.0; // 资金账户与交易账户的比例

const BTC_LEVEL: i32 = 20;
const ETH_LEVEL: i32 = 15;
const OTHER_LEVEL: i32 = 10;

/// 风险管理任务，负责在资金账户和交易账户之间平衡资金
pub struct RiskJob {
    /// 默认货币类型
    currency: String,
    /// 资金平衡比例（资金账户:交易账户）
    balance_ratio: f64,
}

impl RiskJob {
    /// 创建新的风险管理任务实例
    pub fn new() -> Self {
        Self {
            currency: DEFAULT_CURRENCY.to_string(),
            balance_ratio: BALANCE_RATIO,
        }
    }

    pub async fn run(&self, inst_ids: &Vec<&str>) -> Result<(), anyhow::Error> {
        // 控制交易资金
        match self.control_trade_amount().await {
            Ok(_) => info!("风险管理任务成功完成"),
            Err(e) => error!("风险管理任务失败: {:?}", e),
        }
        //控制合约杠杆
        match self.run_set_leverage(inst_ids).await {
            Ok(_) => info!("风险管理任务成功完成"),
            Err(e) => error!("风险管理任务失败: {:?}", e),
        }
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
    pub async fn run_set_leverage(&self, inst_ids: &Vec<&str>) -> Result<(), anyhow::Error> {
        let span = span!(Level::DEBUG, "run_set_leverage");
        let _enter = span.enter();

        for inst_id in inst_ids.iter() {
            let mut level = 10;
            if inst_id == &"BTC-USDT-SWAP" {
                level = BTC_LEVEL;
            } else if inst_id == &"ETH-USDT-SWAP" {
                level = ETH_LEVEL;
            } else {
                level = OTHER_LEVEL;
            }

            for post_side in [PositionSide::Long, PositionSide::Short] {
                let params = SetLeverageRequest {
                    inst_id: Some(inst_id.to_string()),
                    ccy: None,
                    mgn_mode: TdModeEnum::ISOLATED.to_string(),
                    lever: level.to_string(),
                    pos_side: Some(post_side.to_string()),
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

        debug!("资金账户余额: {:?}", asset_balance);

        // 获取交易账户资产
        let account = OkxAccount::from_env().context("无法从环境变量创建交易账户客户端")?;

        let trade_asset = account
            .get_balance(Some(&self.currency))
            .await
            .context("获取交易账户余额失败")?;

        let trade_balance = trade_asset
            .first()
            .ok_or_else(|| anyhow!("未找到交易账户中的{}余额", self.currency))?;

        debug!("交易账户余额: {:?}", trade_balance);

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

    #[tokio::test]
    async fn test_risk_job() {
        // 设置日志
        env_logger::init();
        let risk_job = RiskJob::new();
        risk_job.run(&vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP"]).await.unwrap();
    }
}
