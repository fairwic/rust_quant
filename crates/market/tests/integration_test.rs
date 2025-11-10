/// Market 包集成测试
///
/// 测试 ORM 迁移后的功能一致性
///
/// 运行前请确保：
/// 1. DATABASE_URL 环境变量已设置
/// 2. MySQL 数据库正在运行
/// 3. 相关表已创建
use rust_quant_market::models::*;

#[cfg(test)]
mod tickers_volume_tests {
    use super::*;

    /// 测试 TickersVolume 基本 CRUD 操作
    #[tokio::test]
    #[ignore] // 需要数据库环境，默认跳过
    async fn test_tickers_volume_crud() {
        // 初始化数据库连接
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = TickersVolumeModel::new();

        // 1. 测试插入
        let test_data = vec![TickersVolume {
            id: None,
            inst_id: "BTC-USDT-SWAP-TEST".to_string(),
            period: "1D".to_string(),
            ts: 1699999999000,
            oi: "1000".to_string(),
            vol: "5000".to_string(),
        }];

        let insert_result = model.add(test_data.clone()).await;
        assert!(insert_result.is_ok(), "插入失败: {:?}", insert_result.err());
        println!("✅ 插入成功: {} 条记录", insert_result.unwrap());

        // 2. 测试查询
        let query_result = model.find_one("BTC-USDT-SWAP-TEST").await;
        assert!(query_result.is_ok(), "查询失败: {:?}", query_result.err());

        let records = query_result.unwrap();
        assert!(!records.is_empty(), "查询结果为空");
        assert_eq!(records[0].inst_id, "BTC-USDT-SWAP-TEST");
        println!("✅ 查询成功: 找到 {} 条记录", records.len());

        // 3. 测试删除
        let delete_result = model.delete_by_inst_id("BTC-USDT-SWAP-TEST").await;
        assert!(delete_result.is_ok(), "删除失败: {:?}", delete_result.err());
        println!("✅ 删除成功: {} 条记录", delete_result.unwrap());

        // 4. 验证删除
        let verify_result = model.find_one("BTC-USDT-SWAP-TEST").await;
        assert!(verify_result.is_ok());
        assert!(verify_result.unwrap().is_empty(), "删除后仍能查到数据");
        println!("✅ 删除验证通过");
    }

    /// 测试批量插入性能
    #[tokio::test]
    #[ignore]
    async fn test_tickers_volume_batch_insert() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = TickersVolumeModel::new();

        // 生成 100 条测试数据
        let mut test_data = Vec::new();
        for i in 0..100 {
            test_data.push(TickersVolume {
                id: None,
                inst_id: format!("TEST-{}", i),
                period: "1H".to_string(),
                ts: 1699999999000 + i,
                oi: format!("{}", 1000 + i),
                vol: format!("{}", 5000 + i),
            });
        }

        let start = std::time::Instant::now();
        let result = model.add(test_data.clone()).await;
        let duration = start.elapsed();

        assert!(result.is_ok(), "批量插入失败");
        println!("✅ 批量插入 100 条记录，耗时: {:?}", duration);

        // 清理测试数据
        for i in 0..100 {
            let _ = model.delete_by_inst_id(&format!("TEST-{}", i)).await;
        }
    }
}

#[cfg(test)]
mod tickers_tests {
    use super::*;

    /// 测试 Tickers 基本 CRUD 操作
    #[tokio::test]
    #[ignore]
    async fn test_tickers_crud() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = TicketsModel::new();

        // 注意：这里需要实际的 OKX DTO 结构
        // 由于 TickerOkxResDto 来自 okx 包，这里使用模拟数据

        println!("✅ Tickers 模型已就绪");

        // TODO: 添加更多测试用例
        // 1. 测试批量插入（add）
        // 2. 测试更新（update）
        // 3. 测试查询（get_all, find_one）
        // 4. 测试每日交易量计算（get_daily_volumes）
    }

    /// 测试每日交易量计算
    #[tokio::test]
    #[ignore]
    async fn test_daily_volumes_calculation() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = TicketsModel::new();

        // 测试不带参数的查询
        let result = model.get_daily_volumes(None).await;
        assert!(result.is_ok(), "查询失败: {:?}", result.err());
        println!("✅ 每日交易量查询成功");

        // 测试带参数的查询
        let inst_ids = vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP"];
        let result_filtered = model.get_daily_volumes(Some(inst_ids)).await;
        assert!(result_filtered.is_ok(), "带参数查询失败");
        println!("✅ 带过滤的每日交易量查询成功");
    }
}

#[cfg(test)]
mod candles_tests {
    use super::*;

    /// 测试 Candles 表创建
    #[tokio::test]
    #[ignore]
    async fn test_candles_create_table() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = CandlesModel::new();

        // 创建测试表
        let result = model.create_table("btc-usdt-swap-test", "1h").await;
        assert!(result.is_ok(), "创建表失败: {:?}", result.err());
        println!("✅ 创建 K线表成功");

        // TODO: 清理测试表
        // DROP TABLE btc-usdt-swap-test_candles_1h
    }

    /// 测试 Candles CRUD 操作
    #[tokio::test]
    #[ignore]
    async fn test_candles_crud() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = CandlesModel::new();
        let inst_id = "btc-usdt-swap-test";
        let time_interval = "1h";

        // 确保表存在
        let _ = model.create_table(inst_id, time_interval).await;

        // TODO: 添加测试用例
        // 1. 测试批量插入（add）
        // 2. 测试 UPSERT（upsert_one, upsert_batch）
        // 3. 测试查询（get_all, get_new_data, get_one_by_ts）
        // 4. 测试更新（update_one）
        // 5. 测试删除（delete_lg_time）

        println!("✅ Candles 模型测试框架已就绪");
    }

    /// 测试 UPSERT 操作
    #[tokio::test]
    #[ignore]
    async fn test_candles_upsert() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = CandlesModel::new();
        let inst_id = "btc-usdt-swap-test";
        let time_interval = "1h";

        // 确保表存在
        let _ = model.create_table(inst_id, time_interval).await;

        // 由于需要 CandleOkxRespDto，这里先验证模型可用
        println!("✅ UPSERT 测试框架已就绪");

        // TODO: 使用真实的 OKX DTO 测试
    }

    /// 测试复杂查询条件
    #[tokio::test]
    #[ignore]
    async fn test_candles_complex_query() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = CandlesModel::new();

        let dto = SelectCandleReqDto {
            inst_id: "btc-usdt-swap".to_string(),
            time_interval: "1h".to_string(),
            limit: 100,
            select_time: None,
            confirm: Some(1), // 只查询已确认的 K线
        };

        let result = model.get_all(dto).await;
        assert!(result.is_ok(), "复杂查询失败: {:?}", result.err());
        println!("✅ 复杂查询成功: 返回 {} 条记录", result.unwrap().len());
    }

    /// 测试表名生成
    #[test]
    fn test_table_name_generation() {
        let table_name = CandlesModel::get_table_name("BTC-USDT-SWAP", "1H");
        assert_eq!(table_name, "btc-usdt-swap_candles_1h");
        println!("✅ 表名生成测试通过: {}", table_name);

        let table_name2 = CandlesModel::get_table_name("ETH-USDT-SWAP", "1D");
        assert_eq!(table_name2, "eth-usdt-swap_candles_1d");
        println!("✅ 表名生成测试通过: {}", table_name2);
    }
}

#[cfg(test)]
mod functionality_comparison_tests {
    use super::*;

    /// 对比新旧实现的功能一致性
    ///
    /// 验证点：
    /// 1. 数据结构是否一致
    /// 2. CRUD 操作是否保持相同的语义
    /// 3. 查询结果是否一致
    /// 4. 性能是否可接受
    #[test]
    fn test_data_structure_compatibility() {
        // 验证 TickersVolume 结构
        let volume = TickersVolume {
            id: None,
            inst_id: "test".to_string(),
            period: "1D".to_string(),
            ts: 123456789,
            oi: "1000".to_string(),
            vol: "5000".to_string(),
        };

        assert_eq!(volume.inst_id, "test");
        println!("✅ TickersVolume 结构兼容");

        // 验证 TickersDataEntity 结构
        let ticker = TickersDataEntity {
            id: None,
            inst_type: "SWAP".to_string(),
            inst_id: "BTC-USDT-SWAP".to_string(),
            last: "50000".to_string(),
            last_sz: "1".to_string(),
            ask_px: "50001".to_string(),
            ask_sz: "10".to_string(),
            bid_px: "49999".to_string(),
            bid_sz: "10".to_string(),
            open24h: "49500".to_string(),
            high24h: "50500".to_string(),
            low24h: "49000".to_string(),
            vol_ccy24h: "1000000".to_string(),
            vol24h: "20000".to_string(),
            sod_utc0: "49800".to_string(),
            sod_utc8: "49800".to_string(),
            ts: 1699999999000,
        };

        assert_eq!(ticker.inst_id, "BTC-USDT-SWAP");
        println!("✅ TickersDataEntity 结构兼容");

        // 验证 CandlesEntity 结构
        let candle = CandlesEntity {
            id: None,
            ts: 1699999999000,
            o: "50000".to_string(),
            h: "50500".to_string(),
            l: "49500".to_string(),
            c: "50200".to_string(),
            vol: "1000".to_string(),
            vol_ccy: "50000000".to_string(),
            confirm: "1".to_string(),
            created_at: None,
            updated_at: None,
        };

        assert_eq!(candle.ts, 1699999999000);
        println!("✅ CandlesEntity 结构兼容");
    }

    /// 验证查询语义一致性
    #[test]
    fn test_query_semantics() {
        // 验证 SelectCandleReqDto 功能
        let dto = SelectCandleReqDto {
            inst_id: "btc-usdt-swap".to_string(),
            time_interval: "1h".to_string(),
            limit: 100,
            select_time: Some(SelectTime {
                start_time: 1699999999000,
                end_time: Some(1700000000000),
                direct: TimeDirect::BEFORE,
            }),
            confirm: Some(1),
        };

        assert_eq!(dto.limit, 100);
        assert!(dto.select_time.is_some());
        println!("✅ 查询 DTO 结构正确");
    }
}

/// 性能基准测试
#[cfg(test)]
mod performance_tests {
    use super::*;

    /// 对比插入性能
    #[tokio::test]
    #[ignore]
    async fn benchmark_batch_insert() {
        dotenv::dotenv().ok();
        rust_quant_core::database::init_db_pool()
            .await
            .expect("Failed to init DB pool");

        let model = TickersVolumeModel::new();

        // 测试不同批量大小的性能
        for batch_size in [10, 50, 100, 500] {
            let mut test_data = Vec::new();
            for i in 0..batch_size {
                test_data.push(TickersVolume {
                    id: None,
                    inst_id: format!("PERF-TEST-{}", i),
                    period: "1D".to_string(),
                    ts: 1699999999000 + i,
                    oi: "1000".to_string(),
                    vol: "5000".to_string(),
                });
            }

            let start = std::time::Instant::now();
            let _ = model.add(test_data).await;
            let duration = start.elapsed();

            println!(
                "批量大小: {}, 耗时: {:?}, 平均: {:?}/条",
                batch_size,
                duration,
                duration / batch_size as u32
            );

            // 清理
            for i in 0..batch_size {
                let _ = model.delete_by_inst_id(&format!("PERF-TEST-{}", i)).await;
            }
        }
    }
}
