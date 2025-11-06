use okx::api::api_trait::OkxApiTrait;
use okx::api::asset::OkxAsset;
use tracing::info;
pub async fn get_balance() -> anyhow::Result<()> {
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let ccy=vec![];
    let ccy = vec!["USDT".to_string()];
    // let balances = Account::get_balances(Some(ccy)).await?;
    let balances = OkxAsset::from_env()?.get_balances(Some(&ccy)).await?;
    info!("资金账户余额:{:#?}", balances);
    //写入数据库

    Ok(())
}
