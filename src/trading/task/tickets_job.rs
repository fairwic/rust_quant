use tracing::debug;
use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::market::Market;

pub async fn get_ticket(ins_type: &str) {
    let ticker = Market::get_ticker(&ins_type).await;
    debug!("单个ticket: {:?}", ticker);
    //
    if let Ok(ticker_list) = ticker {
        let res = TicketsModel::new().await;
        let res = res.update(ticker_list.get(0).unwrap()).await;
        debug!("插入数据库结果: {:?}", res);
    }
}

pub async fn init_all_ticker() -> anyhow::Result<()> {
    println!("开始同步ticker...");
    //同步合约产品
    let ins_type = "SWAP";
    let ticker = Market::get_tickers(&ins_type, None, None).await?;
    debug!("全部tickets: {:?}", ticker);

    if ticker.len() > 0 {
        let res = TicketsModel::new().await;
        let res = res.add(ticker).await?;
    };

    //同步币币产品
    let ins_type = "SPOT";
    let ticker = Market::get_tickers(&ins_type, None, None).await?;
    debug!("全部tickets: {:?}", ticker);

    if ticker.len() > 0 {
        let res = TicketsModel::new().await;
        let res = res.add(ticker).await?;
        debug!("插入数据库结果: {:?}", res);
    }
    Ok(())
}

pub async fn sync_ticker() {
    self::get_ticket("BTC-USDT-SWAP").await;
}