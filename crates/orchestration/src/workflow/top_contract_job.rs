use rust_quant_market::models::CandlesEntity;
use rust_quant_common::services::big_data::big_data_service::BigDataContractService;
use rust_quant_common::services::big_data::big_data_top_contract_service::BigDataTopContractService;
use rust_quant_common::services::big_data::big_data_top_position_service::BigDataTopPositionService;
use rust_quant_common::services::big_data::{big_data_service, big_data_top_contract_service};
use rust_quant_strategies::profit_stop_loss::ProfitStopLoss;
use rust_quant_strategies::ut_boot_strategy::UtBootStrategy;
use rust_quant_strategies::StrategyType;
use rust_quant_orchestration::workflow::{basic, big_data_job};
use futures_util::future::join_all;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, span, warn, Level};

pub struct TopContractJob {}
impl TopContractJob {
    // pub async fn run_strategy(
    //     inst_id: &str,
    //     time: &str,
    //     key_value: f64,
    //     atr_period: usize,
    //     max_loss_percent: f64,
    //     semaphore: Arc<Semaphore>,
    //     mysql_candles: Arc<Vec<CandlesEntity>>,
    //     fibonacci_level: Arc<Vec<f64>>,
    // ) -> anyhow::Result<(), anyhow::Error> {
    //     // 获取信号量，控制并发
    //     let _permit = semaphore.acquire().await.unwrap();

    //     // 执行策略
    //     let back_test_result =
    //         UtBootStrategy::run_test(
    //             &mysql_candles,
    //             &fibonacci_level,
    //             max_loss_percent,
    //             false, // is_fibonacci_profit
    //             true,  // is_open_long
    //             true,  // is_open_short
    //             UtBootStrategy {
    //                 key_value,
    //                 ema_period:1,
    //                 atr_period,
    //                 heikin_ashi: false,
    //             },
    //             false, // is_judge_trade_time
    //         )
    //         .await;

    //     // // 构造策略详情字符串
    //     // let strategy_detail = Some(format!(
    //     //     "key_value: {:?}, atr_period: {}, max_loss_percent: {}",
    //     //     key_value, atr_period, max_loss_percent
    //     // ));
    //     //
    //     // // 保存测试日志
    //     // let insert_id = match save_test_log(
    //     //     StrategyType::UtBoot,
    //     //     inst_id,
    //     //     time,
    //     //    back_test_result
    //     // )
    //     // .await
    //     // {
    //     //     Ok(id) => id,
    //     //     Err(e) => {
    //     //         error!("Failed to save test log: {:?}", e);
    //     //         return Err(anyhow::anyhow!("Save test log failed").into());
    //     //     }
    //     // };
    //     //
    //     // // 只在交易记录列表不为空时插入记录
    //     // if !trade_record_list.is_empty() {
    //     //     if let Err(e) = save_test_detail(
    //     //         insert_id,
    //     //         StrategyType::UtBoot,
    //     //         inst_id,
    //     //         time,
    //     //         trade_record_list,
    //     //     )
    //     //     .await
    //     //     {
    //     //         error!("Failed to save test detail: {:?}", e);
    //     //     }
    //     // } else {
    //     //     warn!("Empty trade record list, skipping save_test_detail.");
    //     // }
    //     Ok(())
    // }

    // 主函数，执行所有策略测试
    // pub async fn ut_boot_test(inst_id: &str, time: &str) -> anyhow::Result<(), anyhow::Error> {
    //     // 获取数据
    //     let mysql_candles = basic::get_candle_data(inst_id, time, 2200, None).await?;
    //
    //     let mysql_candles_clone = Arc::new(mysql_candles);
    //     let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);
    //     let fibonacci_level_clone = Arc::new(fibonacci_level);
    //
    //     // 创建信号量限制并发数
    //     let semaphore = Arc::new(Semaphore::new(100)); // 控制最大并发数量为 100
    //
    //     // 灵敏度参数
    //     let key_values: Vec<f64> = (2..=80).map(|x| x as f64 * 0.1).collect();
    //     let max_loss_percent: Vec<f64> = (5..6).map(|x| x as f64 * 0.01).collect();
    //
    //     // 创建任务容器
    //     let mut tasks = Vec::new();
    //
    //     // 遍历所有组合并为每个组合生成一个任务
    //     for key_value in key_values {
    //         for atr_period in 1..=15 {
    //             for &max_loss in &max_loss_percent {
    //                 let inst_id_clone = inst_id.to_string();
    //                 let time_clone = time.to_string();
    //                 let mysql_candles_clone = Arc::clone(&mysql_candles_clone);
    //                 let fibonacci_level_clone = Arc::clone(&fibonacci_level_clone);
    //                 let permit = Arc::clone(&semaphore);
    //
    //                 // 创建任务
    //                 tasks.push(tokio::spawn({
    //                     let inst_id_clone = inst_id_clone.clone();
    //                     let time_clone = time_clone.clone();
    //
    //                     async move {
    //                         // 执行策略测试并处理结果
    //                         if let Err(e) = rust_quant_orchestration::workflow::basic::run_test_strategy(
    //                             &inst_id_clone,
    //                             &time_clone,
    //                             key_value,
    //                             atr_period,
    //                             max_loss,
    //                             permit,
    //                             mysql_candles_clone,
    //                             fibonacci_level_clone,
    //                         )
    //                         .await
    //                         {
    //                             error!("Strategy test failed: {:?}", e);
    //                         }
    //                     }
    //                 }));
    //             }
    //         }
    //     }
    //
    //     // 等待所有任务完成
    //     join_all(tasks).await;
    //
    //     Ok(())
    // }
}
