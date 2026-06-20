#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TradeBucketTransferDirection {
    FundToTrade,
    TradeToFund,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalFlatDecision {
    Skip,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LiveTradeBucketRebalanceConfig {
    target_trade_ratio: f64,
    min_transfer: f64,
    transfer_epsilon: f64,
}

impl StrategyExecutionService {
    fn live_trade_bucket_rebalance_config() -> Option<LiveTradeBucketRebalanceConfig> {
        if !Self::env_enabled("LIVE_ENABLE_TRADE_BUCKET_REBALANCE") {
            return None;
        }

        let target_trade_ratio = std::env::var("LIVE_TARGET_TRADE_RATIO")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0 && *v < 1.0)
            .unwrap_or(0.30);
        let min_transfer =
            Self::env_positive_f64("LIVE_TRADE_BUCKET_MIN_TRANSFER_USDT").unwrap_or(1.0);
        let transfer_epsilon =
            Self::env_positive_f64("LIVE_TRADE_BUCKET_EPSILON_USDT").unwrap_or(0.5);

        Some(LiveTradeBucketRebalanceConfig {
            target_trade_ratio,
            min_transfer,
            transfer_epsilon,
        })
    }

    fn calculate_trade_bucket_transfer(
        trade_balance: f64,
        funding_balance: f64,
        config: LiveTradeBucketRebalanceConfig,
    ) -> Option<(f64, TradeBucketTransferDirection)> {
        if !trade_balance.is_finite()
            || !funding_balance.is_finite()
            || trade_balance < 0.0
            || funding_balance < 0.0
        {
            return None;
        }

        let total_balance = trade_balance + funding_balance;
        if total_balance <= 0.0 {
            return None;
        }

        let target_trade_balance = total_balance * config.target_trade_ratio;
        let diff = target_trade_balance - trade_balance;
        if diff.abs() < config.transfer_epsilon {
            return None;
        }

        if diff > 0.0 {
            let amount = diff;
            if amount >= config.min_transfer {
                return Some((amount, TradeBucketTransferDirection::FundToTrade));
            }
        }

        if diff < 0.0 {
            let amount = -diff;
            if amount >= config.min_transfer {
                return Some((amount, TradeBucketTransferDirection::TradeToFund));
            }
        }

        None
    }

    async fn reconcile_external_flat_close(
        &self,
        config: &StrategyConfig,
        inst_id: &str,
        period: &str,
    ) -> Result<()> {
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let has_local_state = self
            .live_states
            .get(&config.id)
            .map(|state| state.trade_position.is_some())
            .unwrap_or(false);
        let has_live_exit_cache = self
            .live_exit_targets
            .get(&config.id)
            .map(|targets| !targets.algo_ids.is_empty() || targets.trade_side.is_some())
            .unwrap_or(false);

        let persisted_long = self
            .load_persisted_close_algos(config.id, inst_id, period, "long")
            .await?;
        let persisted_short = self
            .load_persisted_close_algos(config.id, inst_id, period, "short")
            .await?;
        let has_persisted_close_algo = persisted_long
            .as_ref()
            .map(|order| !Self::extract_close_algo_ids(&order.detail).is_empty())
            .unwrap_or(false)
            || persisted_short
                .as_ref()
                .map(|order| !Self::extract_close_algo_ids(&order.detail).is_empty())
                .unwrap_or(false);

        if !has_local_state && !has_live_exit_cache && !has_persisted_close_algo {
            return Ok(());
        }

        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config.id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;

        let has_exchange_position = positions
            .iter()
            .any(|p| p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12);

        if has_exchange_position {
            if let Err(e) = self
                .clear_external_flat_probe(config.id, inst_id, period)
                .await
            {
                warn!(
                    "⚠️ 清理外部平仓探测标记失败: inst_id={}, period={}, config_id={}, err={}",
                    inst_id, period, config.id, e
                );
            }
            return Ok(());
        }

        let mut inspection_confirms_close = false;
        for order in [&persisted_long, &persisted_short].into_iter().flatten() {
            if order.out_order_id.trim().is_empty() {
                continue;
            }
            match okx_service
                .inspect_auto_close_by_order(
                    &api_config,
                    inst_id,
                    Some(order.out_order_id.as_str()),
                    None,
                )
                .await
            {
                Ok(inspection) => {
                    inspection_confirms_close |= inspection.position_closed
                        && (inspection.auto_close_likely
                            || !inspection.pending_algo_ids.is_empty()
                            || !inspection.history_algo_ids.is_empty());
                    info!(
                        "🔎 外部平仓 inspection: config_id={}, inst_id={}, period={}, out_order_id={}, inspection={:?}",
                        config.id, inst_id, period, order.out_order_id, inspection
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️ 外部平仓 inspection 失败: config_id={}, inst_id={}, period={}, out_order_id={}, err={}",
                        config.id, inst_id, period, order.out_order_id, e
                    );
                }
            }
        }

        match self
            .confirm_external_flat_close(config.id, inst_id, period, inspection_confirms_close)
            .await?
        {
            ExternalFlatDecision::Skip => return Ok(()),
            ExternalFlatDecision::Confirmed => {}
        }

        info!(
            "🔄 检测到外部平仓完成，清理本地状态并执行交易桶回补: config_id={}, inst_id={}, period={}",
            config.id, inst_id, period
        );

        if let Err(e) = self
            .clear_persisted_close_algos(config.id, inst_id, period, "long")
            .await
        {
            warn!(
                "⚠️ 外部平仓后清理 long 持久化保护单失败: inst_id={}, config_id={}, err={}",
                inst_id, config.id, e
            );
        }
        if let Err(e) = self
            .clear_persisted_close_algos(config.id, inst_id, period, "short")
            .await
        {
            warn!(
                "⚠️ 外部平仓后清理 short 持久化保护单失败: inst_id={}, config_id={}, err={}",
                inst_id, config.id, e
            );
        }

        self.live_exit_targets.remove(&config.id);
        self.live_states.insert(config.id, TradingState::default());

        if let Err(e) = self
            .clear_external_flat_probe(config.id, inst_id, period)
            .await
        {
            warn!(
                "⚠️ 清理外部平仓探测标记失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config.id, e
            );
        }

        if let Err(e) = self
            .rebalance_trade_bucket_after_close(&api_config, config.id, inst_id)
            .await
        {
            warn!(
                "⚠️ 外部平仓后交易桶自动划转失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config.id, e
            );
        }

        Ok(())
    }

    fn external_flat_probe_key(config_id: i64, inst_id: &str, period: &str) -> String {
        format!(
            "live_external_flat_probe:{}:{}:{}",
            config_id, inst_id, period
        )
    }

    async fn clear_external_flat_probe(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
    ) -> Result<()> {
        let rkey = Self::external_flat_probe_key(config_id, inst_id, period);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!(
                    "⚠️ 获取Redis连接失败，跳过清理外部平仓探测标记: config_id={}, inst_id={}, period={}, err={}",
                    config_id, inst_id, period, e
                );
                return Ok(());
            }
        };
        if let Err(e) = conn.del::<_, ()>(&rkey).await {
            warn!(
                "⚠️ 删除外部平仓探测标记失败: config_id={}, inst_id={}, period={}, err={}",
                config_id, inst_id, period, e
            );
        }
        Ok(())
    }

    async fn confirm_external_flat_close(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        inspection_confirms_close: bool,
    ) -> Result<ExternalFlatDecision> {
        if inspection_confirms_close {
            return Ok(ExternalFlatDecision::Confirmed);
        }

        let rkey = Self::external_flat_probe_key(config_id, inst_id, period);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!(
                    "⚠️ 获取Redis连接失败，外部平仓缺少确认时保守跳过: config_id={}, inst_id={}, period={}, err={}",
                    config_id, inst_id, period, e
                );
                return Ok(ExternalFlatDecision::Skip);
            }
        };
        let seen_once = conn.get::<_, Option<String>>(&rkey).await?.is_some();

        if !seen_once {
            conn.set_ex::<_, _, ()>(&rkey, "1", Self::EXTERNAL_FLAT_PROBE_TTL_SECS)
                .await?;
            warn!(
                "⚠️ 首次观测到交易所无持仓但缺少自动平仓证据，暂不清理本地状态: config_id={}, inst_id={}, period={}",
                config_id, inst_id, period
            );
            return Ok(ExternalFlatDecision::Skip);
        }

        warn!(
            "⚠️ 二次观测到交易所无持仓，按外部平仓处理: config_id={}, inst_id={}, period={}",
            config_id, inst_id, period
        );
        Ok(ExternalFlatDecision::Confirmed)
    }

    async fn rebalance_trade_bucket_after_close(
        &self,
        api_config: &rust_quant_domain::entities::ExchangeApiConfig,
        config_id: i64,
        inst_id: &str,
    ) -> Result<()> {
        let Some(rebalance_config) = Self::live_trade_bucket_rebalance_config() else {
            return Ok(());
        };

        use crate::exchange::OkxOrderService;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(api_config, Some("SWAP"), None)
            .await
            .map_err(|e| anyhow!("获取持仓失败: {}", e))?;

        let has_open_swap_positions = positions
            .iter()
            .any(|p| p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12);

        if has_open_swap_positions {
            info!(
                "⏭️ 跳过交易桶回补: 当前仍有未平 SWAP 持仓, config_id={}, inst_id={}",
                config_id, inst_id
            );
            return Ok(());
        }

        let currency = std::env::var("LIVE_TRADE_BUCKET_CURRENCY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "USDT".to_string());

        let funding_balance = okx_service
            .get_funding_available_balance(api_config, &currency)
            .await?;
        let trade_balance = okx_service
            .get_trade_available_equity(api_config, &currency)
            .await?;
        let target_trade_balance =
            (trade_balance + funding_balance) * rebalance_config.target_trade_ratio;

        let Some((transfer_amount, direction)) =
            Self::calculate_trade_bucket_transfer(trade_balance, funding_balance, rebalance_config)
        else {
            info!(
                "⏭️ 交易桶余额接近动态目标或低于最小划转金额，无需划转: config_id={}, inst_id={}, trade_balance={:.4}, funding_balance={:.4}, total_balance={:.4}, target={:.4}, ratio={:.4}, min_transfer={:.4}, epsilon={:.4}",
                config_id,
                inst_id,
                trade_balance,
                funding_balance,
                trade_balance + funding_balance,
                target_trade_balance,
                rebalance_config.target_trade_ratio,
                rebalance_config.min_transfer,
                rebalance_config.transfer_epsilon
            );
            return Ok(());
        };

        let (from, to, direction_label) = match direction {
            TradeBucketTransferDirection::FundToTrade => {
                (AccountType::FOUND, AccountType::TRADE, "fund_to_trade")
            }
            TradeBucketTransferDirection::TradeToFund => {
                (AccountType::TRADE, AccountType::FOUND, "trade_to_fund")
            }
        };

        if matches!(direction, TradeBucketTransferDirection::FundToTrade)
            && funding_balance + 1e-9 < transfer_amount
        {
            warn!(
                "⚠️ 交易桶回补跳过: 资金账户余额不足, config_id={}, inst_id={}, need={:.4}, funding_balance={:.4}",
                config_id, inst_id, transfer_amount, funding_balance
            );
            return Ok(());
        }

        okx_service
            .transfer_between_accounts(api_config, &currency, transfer_amount, from, to)
            .await?;

        info!(
            "💸 交易桶自动划转完成: config_id={}, inst_id={}, direction={}, amount={:.4}, currency={}, total_balance={:.4}, target={:.4}, ratio={:.4}",
            config_id,
            inst_id,
            direction_label,
            transfer_amount,
            currency,
            trade_balance + funding_balance,
            target_trade_balance,
            rebalance_config.target_trade_ratio
        );

        Ok(())
    }

}
