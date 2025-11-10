use okx::api::account::OkxAccount;
use okx::api::api_trait::OkxApiTrait;
pub async fn get_account_balance() -> anyhow::Result<()> {
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let balances = Account::get_balances(Some(&ccy)).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
    let balances = OkxAccount::from_env()?
        .get_balance(None)
        .await
        .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
    println!("账户余额:{:#?}", balances);
    Ok(())
}
