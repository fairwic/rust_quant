use std::sync::Arc;
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

pub async fn init_all_ticker(inst_ids: &Arc<Vec<&str>>) -> anyhow::Result<()> {
    println!("开始同步ticker...");
    //同步合约产品
    let ins_type = "SWAP";
    let tickers = Market::get_tickers(&ins_type, None, None).await?;
    debug!("全部tickets: {:?}", tickers);

    if tickers.len() > 0 {
        let model = TicketsModel::new().await;
        for ticker in tickers {
            //判断是否在inst_ids中
            if inst_ids.contains(&&**&ticker.inst_id) {
                //判断数据库是否有
                let res = model.find_one(&ticker.inst_id).await?;
                if res.len() > 0 {
                    println!("不存在");
                    let res = model.add(vec![ticker]).await?;
                } else {
                    println!("已经存在,更新");
                    let res = model.update(&ticker).await?;
                }
            }
        }
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