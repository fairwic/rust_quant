use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::model::market::tickers_volume::{TickersVolume, TickersVolumeModel};
use crate::trading::okx::market::Market;
use crate::trading::okx::public_data::contracts::Contracts;
use std::sync::Arc;
use tracing::{debug, error};

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

pub async fn init_all_ticker_volume(inst_ids: &str, period: &str) -> anyhow::Result<()> {
    println!("开始同步ticker...");
    //同步合约产品
    let ins_type = "SWAP";
    let inst_id = "BTC";
    let items = Contracts::get_open_interest_volume(Some("BTC"), None, None, Some("1D"))
        .await
        .unwrap();

    let model = TickersVolumeModel::new().await;

    //判断数据库是否有
    let res = model.find_one(&inst_id).await?;
    if res.len() > 0 {
        debug!("已经存在,删除旧的");
        let res = model.delete_by_inst_id(inst_id).await?;
    }
    if items.len() > 0 {
        for ticker in items.iter() {
            //判断是否在inst_ids中
            let list = TickersVolume {
                inst_id: inst_id.clone().parse().unwrap(),
                period: period.parse()?,
                ts: ticker.ts.parse().unwrap(),
                vol: ticker.vol.clone(),
                oi: ticker.oi.clone(),
            };
            debug!("新增新增的数据");
            let res = model.add(vec![list]).await?;
        }
    };

    // //同步币币产品
    // let ins_type = "SPOT";
    // let ticker = Market::get_tickers(&ins_type, None, None).await?;
    // debug!("全部tickets: {:?}", ticker);
    //
    // if ticker.len() > 0 {
    //       let model = TicketsModel::new().await;
    //     for ticker in tickers {
    //         //判断是否在inst_ids中
    //         if inst_ids.contains(&&**&ticker.inst_id) {
    //             //判断数据库是否有
    //             let res = model.find_one(&ticker.inst_id).await?;
    //             if res.len() > 0 {
    //                 println!("已经存在,更新");
    //                 let res = model.update(&ticker).await?;
    //             } else {
    //                 println!("不存在");
    //                 let res = model.add(vec![ticker]).await?;
    //             }
    //         }
    //     }
    // }
    Ok(())
}

pub async fn sync_ticker() {
    self::get_ticket("BTC-USDT-SWAP").await;
}
