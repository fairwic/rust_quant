use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::market::Market;

pub async fn get_ticket(ins_type: &str) {
    let ticker = Market::get_ticker(&ins_type).await;
    println!("单个ticket: {:?}", ticker);
    //
    if let Ok(ticker_list) = ticker {
        let res = TicketsModel::new().await;
        let res = res.update(ticker_list.get(0).unwrap()).await;
        println!("插入数据库结果: {:?}", res);
    }
}

pub async fn init_all_ticker() {

    //同步合约产品
    let ins_type = "SWAP";
    let ticker = Market::get_tickers(&ins_type, None, None).await;
    println!("全部tickets: {:?}", ticker);

    if let Ok(ticker_list) = ticker {
        let res = TicketsModel::new().await;
        let res = res.add(ticker_list).await;
        println!("插入数据库结果: {:?}", res);
    }

    //同步币币产品
    let ins_type = "SPOT";
    let ticker = Market::get_tickers(&ins_type, None, None).await;
    println!("全部tickets: {:?}", ticker);

    if let Ok(ticker_list) = ticker {
        let res = TicketsModel::new().await;
        let res = res.add(ticker_list).await;
        println!("插入数据库结果: {:?}", res);
    }
}

pub async fn sync_ticker() {
    self::get_ticket("BTC-USDT-SWAP").await;
}