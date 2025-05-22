// 风险监控任务

use anyhow::{anyhow, Error,Result};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::asset_dto::AssetBalance;
use okx::{OkxAccount, OkxAsset};
//控制自己的交易
pub struct RiskJob {}
impl RiskJob {
    pub fn new() -> Self {
        Self {}
    }
    ///保持交易账号的资金,是资金账户的0.5
    pub async fn run(&self) -> Result<()> {
        // 获取当前账户资产
        let asset = OkxAsset::from_env();
        if asset.is_err() {
            return Err(anyhow!("获取当前账户资产失败"));
        }
        let asset = asset.unwrap();
        let position = asset.get_balances(Some(&vec!["USDT".to_string()])).await;
        //获取资金账户的资产
        println!("position: {:?}", position);

        let account = OkxAccount::from_env();
        if account.is_err() {
            return Err(anyhow!("获取当前账户资产失败"));
        }
        let account = account.unwrap();
        let trade_asset = account.get_balance(Some("USDT")).await?;
        // 获取交易账户的资产
        println!("trade_asset: {:?}", trade_asset);



        Ok(())
    }
}

///test
#[tokio::test]
async fn test_risk_job() {
    let risk_job = RiskJob::new();
    let res = risk_job.run().await;
    println!("res: {:?}", res);
}
