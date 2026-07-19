//! 策略数据服务
//!
//! 负责策略数据的初始化（预热）：
//! - 加载历史K线数据
//! - 初始化策略指标缓存
use crate::market::{fetch_latest_candles_for_live_warmup, get_confirmed_candles_for_backtest};
use anyhow::{anyhow, Result};
use chrono::Utc;
use rust_quant_common::CandleItem;
use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};
use rust_quant_market::models::CandlesEntity;
use rust_quant_strategies::framework::strategy_registry::get_strategy_registry;
use std::collections::BTreeMap;
use tracing::{debug, error, info, warn};
/// 策略数据服务
///
/// 职责:
/// - 加载历史K线数据
/// - 初始化策略指标缓存
/// - 批量预热多个策略
pub struct StrategyDataService;
impl StrategyDataService {
    const LIVE_WARMUP_TAIL_MAX: usize = 100;

    /// 提供read环境变量usize的集中实现，避免回测策略调用方重复处理相同细节。
    fn read_env_usize(key: &str) -> Option<usize> {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
    }
    /// 提供determinewarmuplimit的集中实现，避免回测策略调用方重复处理相同细节。
    fn determine_warmup_limit(parameters: &serde_json::Value) -> usize {
        const DEFAULT_WARMUP_LIMIT: usize = 500;
        const DEFAULT_WARMUP_LIMIT_MAX: usize = 10_000;
        let base_limit =
            Self::read_env_usize("STRATEGY_WARMUP_LIMIT").unwrap_or(DEFAULT_WARMUP_LIMIT);
        let max_limit =
            Self::read_env_usize("STRATEGY_WARMUP_LIMIT_MAX").unwrap_or(DEFAULT_WARMUP_LIMIT_MAX);
        let min_k_line_num = parameters
            .get("min_k_line_num")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);
        base_limit.max(min_k_line_num).min(max_limit)
    }
    /// 判断当前进程是否由确认 K 线实时驱动；离线回测和手动任务不读取交易所补尾。
    fn live_socket_enabled() -> bool {
        std::env::var("IS_OPEN_SOCKET").ok().is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            )
        })
    }
    /// 解析实盘行情来源；配置为 all/空时使用进程的行情交易所。
    fn live_market_exchange(config: &StrategyConfig) -> String {
        config
            .exchange
            .as_deref()
            .map(str::trim)
            .filter(|exchange| !exchange.is_empty() && !exchange.eq_ignore_ascii_case("all"))
            .map(str::to_ascii_lowercase)
            .or_else(|| std::env::var("MARKET_DATA_EXCHANGE").ok())
            .or_else(|| std::env::var("DEFAULT_EXCHANGE").ok())
            .unwrap_or_else(|| "okx".to_string())
    }
    /// 计算目标确认 K 线的开盘时间戳，单位为 Unix 毫秒。
    fn expected_latest_confirmed_ts(
        timeframe: Timeframe,
        trigger_ts: Option<i64>,
        now_ms: i64,
    ) -> Result<(i64, i64)> {
        let timeframe_ms = timeframe.to_minutes().saturating_mul(60_000);
        if timeframe_ms <= 0 {
            return Err(anyhow!("实时预热周期必须大于 0: {:?}", timeframe));
        }
        let expected_ts = trigger_ts
            .map(|timestamp| timestamp.saturating_sub(timeframe_ms))
            .unwrap_or_else(|| {
                now_ms
                    .div_euclid(timeframe_ms)
                    .saturating_mul(timeframe_ms)
                    .saturating_sub(timeframe_ms)
            });
        Ok((expected_ts, timeframe_ms))
    }
    /// 合并数据库历史和交易所尾部，并验证从数据库断点到目标 K 线之间没有跳周期。
    fn merge_live_warmup_tail(
        database_candles: Vec<CandlesEntity>,
        exchange_candles: Vec<CandlesEntity>,
        expected_ts: i64,
        timeframe_ms: i64,
    ) -> Result<Vec<CandlesEntity>> {
        let database_latest_ts = database_candles
            .iter()
            .filter(|candle| candle.confirm == "1" && candle.ts <= expected_ts)
            .map(|candle| candle.ts)
            .max()
            .ok_or_else(|| anyhow!("数据库没有可用于实盘预热的确认 K 线"))?;
        let mut merged = BTreeMap::new();
        for candle in database_candles.into_iter().chain(exchange_candles) {
            if candle.confirm == "1" && candle.ts <= expected_ts {
                merged.insert(candle.ts, candle);
            }
        }
        let candles: Vec<CandlesEntity> = merged.into_values().collect();
        let latest_ts = candles.last().map(|candle| candle.ts);
        if latest_ts != Some(expected_ts) {
            return Err(anyhow!(
                "交易所尾部修复后仍缺少目标确认 K 线: database_latest_ts={}, expected_ts={}, actual_latest_ts={:?}",
                database_latest_ts,
                expected_ts,
                latest_ts
            ));
        }
        let bridge_index = candles
            .iter()
            .position(|candle| candle.ts == database_latest_ts)
            .ok_or_else(|| anyhow!("数据库尾部断点未保留在合并结果中"))?;
        for pair in candles[bridge_index..].windows(2) {
            let actual_delta = pair[1].ts.saturating_sub(pair[0].ts);
            if actual_delta != timeframe_ms {
                return Err(anyhow!(
                    "实时预热尾部 K 线不连续: previous_ts={}, next_ts={}, expected_delta_ms={}, actual_delta_ms={}",
                    pair[0].ts,
                    pair[1].ts,
                    timeframe_ms,
                    actual_delta
                ));
            }
        }
        Ok(candles)
    }
    /// 在实盘启动或缺口恢复时补齐交易所尾部；补不齐就失败关闭，禁止用跳周期指标生成信号。
    async fn repair_live_warmup_tail(
        config: &StrategyConfig,
        mut candles: Vec<CandlesEntity>,
        trigger_ts: Option<i64>,
    ) -> Result<Vec<CandlesEntity>> {
        if config.strategy_type != StrategyType::VegasUniversal4h || !Self::live_socket_enabled() {
            return Ok(candles);
        }
        let (expected_ts, timeframe_ms) = Self::expected_latest_confirmed_ts(
            config.timeframe,
            trigger_ts,
            Utc::now().timestamp_millis(),
        )?;
        candles.retain(|candle| candle.confirm == "1" && candle.ts <= expected_ts);
        candles.sort_unstable_by_key(|candle| candle.ts);
        let database_latest_ts = candles
            .last()
            .map(|candle| candle.ts)
            .ok_or_else(|| anyhow!("数据库没有可用于实盘预热的确认 K 线"))?;
        if database_latest_ts == expected_ts {
            return Ok(candles);
        }
        let missing_bars = expected_ts
            .saturating_sub(database_latest_ts)
            .div_euclid(timeframe_ms) as usize;
        if missing_bars >= Self::LIVE_WARMUP_TAIL_MAX {
            return Err(anyhow!(
                "实时预热缺口超过单次安全补齐上限: symbol={}, missing_bars={}, max={}",
                config.symbol,
                missing_bars,
                Self::LIVE_WARMUP_TAIL_MAX - 1
            ));
        }
        let fetch_limit = missing_bars
            .saturating_add(3)
            .clamp(10, Self::LIVE_WARMUP_TAIL_MAX);
        let exchange = Self::live_market_exchange(config);
        warn!(
            "实时策略预热尾部落后，读取交易所确认 K 线补齐: symbol={}, period={}, exchange={}, database_latest_ts={}, expected_ts={}, fetch_limit={}",
            config.symbol,
            config.timeframe.as_str(),
            exchange,
            database_latest_ts,
            expected_ts,
            fetch_limit
        );
        let exchange_candles = fetch_latest_candles_for_live_warmup(
            &exchange,
            &config.symbol,
            config.timeframe.as_str(),
            fetch_limit,
        )
        .await
        .map_err(|error| {
            anyhow!(
                "读取交易所 K 线修复实时预热尾部失败: symbol={}, exchange={}, error={}",
                config.symbol,
                exchange,
                error
            )
        })?;
        Self::merge_live_warmup_tail(candles, exchange_candles, expected_ts, timeframe_ms)
    }
    /// 判断K 线entitytoitem，给回测策略流程提供布尔结果。
    fn candle_entity_to_item(c: &rust_quant_market::models::CandlesEntity) -> Result<CandleItem> {
        let o =
            c.o.parse::<f64>()
                .map_err(|e| anyhow!("解析开盘价失败: {}", e))?;
        let h =
            c.h.parse::<f64>()
                .map_err(|e| anyhow!("解析最高价失败: {}", e))?;
        let l =
            c.l.parse::<f64>()
                .map_err(|e| anyhow!("解析最低价失败: {}", e))?;
        let close =
            c.c.parse::<f64>()
                .map_err(|e| anyhow!("解析收盘价失败: {}", e))?;
        let v = c
            .vol_ccy
            .parse::<f64>()
            .map_err(|e| anyhow!("解析成交量失败: {}", e))?;
        let confirm = c
            .confirm
            .parse::<i32>()
            .map_err(|e| anyhow!("解析 confirm 失败: {}", e))?;
        Ok(CandleItem {
            o,
            h,
            l,
            c: close,
            v,
            ts: c.ts,
            confirm,
        })
    }
    /// 初始化单个策略数据
    /// # 参数
    /// * `config` - 策略配置
    /// # 返回
    /// * `Ok(())` - 初始化成功
    /// * `Err` - 初始化失败
    pub async fn initialize_strategy(config: &StrategyConfig) -> Result<()> {
        Self::initialize_strategy_at(config, None).await
    }
    /// 重建到触发 K 线之前的指标缓存，保证修复后仍由本次确认 K 线产生一次增量计算。
    pub async fn initialize_strategy_before_trigger(
        config: &StrategyConfig,
        trigger_ts: i64,
    ) -> Result<()> {
        Self::initialize_strategy_at(config, Some(trigger_ts)).await
    }
    /// 使用可选触发边界初始化单个策略数据。
    async fn initialize_strategy_at(
        config: &StrategyConfig,
        trigger_ts: Option<i64>,
    ) -> Result<()> {
        let inst_id = &config.symbol;
        let period = config.timeframe.as_str();
        let strategy_type = &config.strategy_type;
        info!(
            "🔥 预热策略数据: inst_id={}, period={}, type={:?}",
            inst_id, period, strategy_type
        );
        // 1. 获取策略执行器
        let registry = get_strategy_registry();
        let executor = registry
            .get(strategy_type.as_str())
            .map_err(|e| anyhow!("获取策略执行器失败: {}", e))?;
        // 2. 加载历史K线数据
        let warmup_limit = Self::determine_warmup_limit(&config.parameters);
        info!(
            "预热K线数量: inst_id={}, period={}, limit={}",
            inst_id, period, warmup_limit
        );
        // 缺口恢复时数据库可能已写入本次触发 K 线，多取一根后再按 trigger_ts 截断，
        // 才能保留与正常启动相同的完整预热窗口。
        let database_limit = warmup_limit.saturating_add(usize::from(trigger_ts.is_some()));
        let candles = get_confirmed_candles_for_backtest(inst_id, period, database_limit, None)
            .await
            .map_err(|e| anyhow!("加载历史K线失败: {}", e))?;
        if candles.is_empty() {
            return Err(anyhow!(
                "历史K线数据为空: inst_id={}, period={}",
                inst_id,
                period
            ));
        }
        let mut candles = Self::repair_live_warmup_tail(config, candles, trigger_ts).await?;
        // 指标初始化要求严格升序；交易所尾部返回通常是倒序。
        candles.sort_unstable_by_key(|a| a.ts);
        let candle_items = candles
            .iter()
            .map(Self::candle_entity_to_item)
            .collect::<Result<Vec<_>>>()?;
        info!(
            "✅ 加载 {} 根历史K线: inst_id={}, period={}",
            candles.len(),
            inst_id,
            period
        );
        // 3. 调用策略执行器初始化数据
        // strategies::StrategyConfig 就是 domain::StrategyConfig 的重导出
        let strategy_config =
            rust_quant_strategies::framework::config::strategy_config::StrategyConfig::new(
                config.id,
                config.strategy_type,
                config.symbol.clone(),
                config.timeframe,
                config.parameters.clone(),
                config.risk_config.clone(),
            );
        let result = executor
            .initialize_data(&strategy_config, inst_id, period, candle_items)
            .await?;
        info!(
            "✅ 策略数据预热完成: hash_key={}, last_ts={}",
            result.hash_key, result.last_timestamp
        );
        Ok(())
    }
    /// 批量初始化多个策略数据
    /// # 参数
    /// * `configs` - 策略配置列表
    /// # 返回
    /// * `Vec<Result<()>>` - 每个策略的初始化结果
    pub async fn initialize_multiple_strategies(configs: &[StrategyConfig]) -> Vec<Result<()>> {
        let mut results = Vec::with_capacity(configs.len());
        for config in configs {
            let result = Self::initialize_strategy(config).await;
            if let Err(ref e) = result {
                error!(
                    "❌ 策略预热失败: id={}, symbol={}, error={}",
                    config.id, config.symbol, e
                );
            } else {
                debug!(
                    "✅ 策略预热成功: id={}, symbol={}",
                    config.id, config.symbol
                );
            }
            results.push(result);
        }
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let fail_count = results.len() - success_count;
        if fail_count > 0 {
            warn!(
                "⚠️  批量预热完成: 成功 {}, 失败 {}",
                success_count, fail_count
            );
        } else {
            info!("✅ 批量预热全部成功: {} 个策略", success_count);
        }
        results
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;
    static ENV_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
    fn candle(ts: i64, confirm: &str) -> CandlesEntity {
        CandlesEntity {
            id: None,
            ts,
            o: "100".to_string(),
            h: "101".to_string(),
            l: "99".to_string(),
            c: "100".to_string(),
            vol: "10".to_string(),
            vol_ccy: "1000".to_string(),
            confirm: confirm.to_string(),
            created_at: None,
            updated_at: None,
        }
    }
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
    fn set_env(key: &str, value: Option<&str>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    #[test]
    fn warmup_limit_defaults_to_max_of_base_and_min_k_line_num() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let old_base = std::env::var("STRATEGY_WARMUP_LIMIT").ok();
        let old_max = std::env::var("STRATEGY_WARMUP_LIMIT_MAX").ok();
        set_env("STRATEGY_WARMUP_LIMIT", None);
        set_env("STRATEGY_WARMUP_LIMIT_MAX", None);
        let params = serde_json::json!({"min_k_line_num": 3600});
        let limit = StrategyDataService::determine_warmup_limit(&params);
        assert_eq!(limit, 3600);
        set_env("STRATEGY_WARMUP_LIMIT", old_base.as_deref());
        set_env("STRATEGY_WARMUP_LIMIT_MAX", old_max.as_deref());
    }
    #[test]
    fn warmup_limit_is_capped_by_max() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let old_base = std::env::var("STRATEGY_WARMUP_LIMIT").ok();
        let old_max = std::env::var("STRATEGY_WARMUP_LIMIT_MAX").ok();
        set_env("STRATEGY_WARMUP_LIMIT", Some("500"));
        set_env("STRATEGY_WARMUP_LIMIT_MAX", Some("2000"));
        let params = serde_json::json!({"min_k_line_num": 3600});
        let limit = StrategyDataService::determine_warmup_limit(&params);
        assert_eq!(limit, 2000);
        set_env("STRATEGY_WARMUP_LIMIT", old_base.as_deref());
        set_env("STRATEGY_WARMUP_LIMIT_MAX", old_max.as_deref());
    }

    #[test]
    fn live_warmup_targets_the_previous_completed_4h_candle() {
        let timeframe_ms = 4 * 60 * 60 * 1000;
        let now_ms = 12 * 60 * 60 * 1000 + 30 * 60 * 1000;

        let (expected_ts, actual_timeframe_ms) =
            StrategyDataService::expected_latest_confirmed_ts(Timeframe::H4, None, now_ms)
                .expect("4H target");

        assert_eq!(actual_timeframe_ms, timeframe_ms);
        assert_eq!(expected_ts, 8 * 60 * 60 * 1000);
    }

    #[test]
    fn live_warmup_merges_a_continuous_exchange_tail() {
        let timeframe_ms = 4 * 60 * 60 * 1000;
        let database = vec![candle(0, "1"), candle(timeframe_ms, "1")];
        let exchange = vec![
            candle(timeframe_ms, "1"),
            candle(timeframe_ms * 2, "1"),
            candle(timeframe_ms * 3, "0"),
        ];

        let merged = StrategyDataService::merge_live_warmup_tail(
            database,
            exchange,
            timeframe_ms * 2,
            timeframe_ms,
        )
        .expect("continuous exchange tail");

        assert_eq!(merged.len(), 3);
        assert_eq!(
            merged.last().map(|candle| candle.ts),
            Some(timeframe_ms * 2)
        );
    }

    #[test]
    fn live_warmup_rejects_an_unbridged_exchange_tail() {
        let timeframe_ms = 4 * 60 * 60 * 1000;
        let database = vec![candle(0, "1")];
        let exchange = vec![candle(timeframe_ms * 2, "1")];

        let error = StrategyDataService::merge_live_warmup_tail(
            database,
            exchange,
            timeframe_ms * 2,
            timeframe_ms,
        )
        .expect_err("gap must fail closed");

        assert!(error.to_string().contains("K 线不连续"));
    }
}
