use crate::trading::okx::account::Account;
use crate::trading::okx::okx_client;

pub async fn get_account_balance() -> anyhow::Result<()> {
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let balances = Account::get_balances(Some(&ccy)).await?;
    let balances = Account::get_balances(None).await?;
    println!("账户余额:{:#?}", balances);
    Ok(())
}