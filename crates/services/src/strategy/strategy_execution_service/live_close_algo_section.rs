impl StrategyExecutionService {
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn sync_close_algos(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
        targets: &ExitTargets,
        prev_algo_ids: &[String],
    ) -> Result<CloseAlgoSyncResult> {
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;
        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;
        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;
        let pos_side_str = match side {
            TradeSide::Long => "long",
            TradeSide::Short => "short",
        };
        let position = positions.iter().find(|p| {
            p.inst_id == inst_id
                && p.pos_side.eq_ignore_ascii_case(pos_side_str)
                && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
        });
        let Some(position) = position else {
            warn!(
                "⚠️ 未找到可同步的持仓: inst_id={}, pos_side={}",
                inst_id, pos_side_str
            );
            return Ok(CloseAlgoSyncResult::SkippedNoPosition);
        };
        let mgn_mode = position.mgn_mode.clone();
        if !prev_algo_ids.is_empty() {
            okx_service
                .cancel_close_algos(&api_config, inst_id, prev_algo_ids)
                .await?;
        }
        if targets.stop_loss.is_none() && targets.take_profit.is_none() {
            if let Err(e) = self
                .clear_persisted_close_algos(config_id, inst_id, period, pos_side_str)
                .await
            {
                warn!(
                    "⚠️ 清理持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                    inst_id, config_id, e
                );
            }
            return Ok(CloseAlgoSyncResult::Cleared);
        }
        let close_side = match side {
            TradeSide::Long => "sell",
            TradeSide::Short => "buy",
        };
        let algo_cl_ord_id = Self::build_close_algo_cl_ord_id(config_id);
        let tag = Self::build_close_algo_tag(config_id);
        let algo_ids = okx_service
            .place_close_algo(
                &api_config,
                inst_id,
                &mgn_mode,
                close_side,
                pos_side_str,
                targets.take_profit,
                targets.stop_loss,
                Some(algo_cl_ord_id.as_str()),
                Some(tag.as_str()),
            )
            .await?;
        if algo_ids.is_empty() {
            return Err(anyhow!(
                "下达平仓策略委托未返回algoId: inst_id={}, period={}, config_id={}",
                inst_id,
                period,
                config_id
            ));
        }
        if let Err(e) = self
            .persist_close_algos(
                config_id,
                inst_id,
                period,
                pos_side_str,
                &algo_ids,
                &tag,
                targets.stop_loss,
                targets.take_profit,
            )
            .await
        {
            warn!(
                "⚠️ 持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                inst_id, config_id, e
            );
        }
        Ok(CloseAlgoSyncResult::Placed(algo_ids))
    }
    #[allow(clippy::too_many_arguments)]
    /// 持久化 交易执行与风控 结果，保证写入路径和幂等语义集中处理。
    async fn persist_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
        algo_ids: &[String],
        tag: &str,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        let Some(mut order) = self
            .swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await?
        else {
            warn!(
                "⚠️ 未找到订单记录，跳过持久化止盈止损: inst_id={}, period={}, config_id={}, pos_side={}",
                inst_id, period, config_id, pos_side
            );
            return Ok(());
        };
        order.detail =
            Self::upsert_close_algo_detail(&order.detail, algo_ids, tag, stop_loss, take_profit);
        self.swap_order_repository.update(&order).await?;
        Ok(())
    }
    /// 删除或清理 交易执行与风控 的临时数据，避免过期状态继续影响后续流程。
    async fn clear_persisted_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
    ) -> Result<()> {
        let Some(mut order) = self
            .swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await?
        else {
            return Ok(());
        };
        order.detail = Self::remove_close_algo_detail(&order.detail);
        self.swap_order_repository.update(&order).await?;
        Ok(())
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    async fn load_persisted_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
    ) -> Result<Option<SwapOrder>> {
        self.swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await
    }
    /// 判断cancelcached平仓algos，给交易执行流程提供布尔结果。
    async fn cancel_cached_close_algos(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        trade_side: Option<TradeSide>,
        algo_ids: &[String],
    ) -> Result<()> {
        if algo_ids.is_empty() {
            return Ok(());
        }
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;
        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;
        let okx_service = OkxOrderService;
        okx_service
            .cancel_close_algos(&api_config, inst_id, algo_ids)
            .await?;
        let pos_side_str = match trade_side {
            Some(TradeSide::Long) => Some("long"),
            Some(TradeSide::Short) => Some("short"),
            None => None,
        };
        if let Some(pos_side_str) = pos_side_str {
            if let Err(e) = self
                .clear_persisted_close_algos(config_id, inst_id, period, pos_side_str)
                .await
            {
                warn!(
                    "⚠️ 清理持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                    inst_id, config_id, e
                );
            }
        }
        Ok(())
    }
    /// 提供compensate平仓algosonstart的集中实现，避免交易执行调用方重复处理相同细节。
    pub async fn compensate_close_algos_on_start(&self, config: &StrategyConfig) -> Result<()> {
        let inst_id = config.symbol.as_str();
        let period = config.timeframe.as_str();
        let pos_sides = ["long", "short"];
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut persisted_by_side: Vec<(&str, Option<SwapOrder>)> = Vec::with_capacity(2);
        let mut has_compensable_plan = false;
        for pos_side in pos_sides {
            let mut persisted_order = self
                .load_persisted_close_algos(config.id, inst_id, period, pos_side)
                .await?;
            if let Some(order) = persisted_order.as_ref() {
                if Self::close_algo_detail_has_compensation_plan(&order.detail) {
                    if Self::close_algo_order_is_compensable(order, now_ms) {
                        has_compensable_plan = true;
                    } else {
                        let signal_ts = Self::close_algo_order_signal_ts(order);
                        let mut updated = order.clone();
                        updated.detail = Self::mark_close_algo_detail_expired(
                            &updated.detail,
                            "signal_ttl_exceeded",
                            signal_ts,
                            now_ms,
                            config.timeframe,
                        );
                        if let Err(e) = self.swap_order_repository.update(&updated).await {
                            warn!(
                                "⚠️ 启动补偿撤单过期标记失败: inst_id={}, config_id={}, pos_side={}, signal_ts={:?}, err={}",
                                inst_id, config.id, pos_side, signal_ts, e
                            );
                        } else {
                            info!(
                                "启动补偿撤单已过期并清理: inst_id={}, config_id={}, pos_side={}, signal_ts={:?}, ttl_ms={}",
                                inst_id,
                                config.id,
                                pos_side,
                                signal_ts,
                                Self::STARTUP_CLOSE_ALGO_COMPENSATION_TTL_MS
                            );
                        }
                        persisted_order = None;
                    }
                }
            }
            persisted_by_side.push((pos_side, persisted_order));
        }
        if !has_compensable_plan {
            return Ok(());
        }
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;
        let api_service = create_exchange_api_service();
        let api_config = match api_service.get_first_api_config(config.id as i32).await {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(
                    "⚠️ 启动补偿撤单获取API配置失败: inst_id={}, config_id={}, err={}",
                    inst_id, config.id, e
                );
                return Ok(());
            }
        };
        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;
        for (pos_side, persisted_order) in persisted_by_side {
            let position = positions.iter().find(|p| {
                p.inst_id == inst_id
                    && p.pos_side.eq_ignore_ascii_case(pos_side)
                    && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
            });
            if position.is_none() {
                if let Some(order) = persisted_order {
                    let algo_ids = Self::extract_close_algo_ids(&order.detail);
                    if !algo_ids.is_empty() {
                        if let Err(e) = okx_service
                            .cancel_close_algos(&api_config, inst_id, &algo_ids)
                            .await
                        {
                            warn!(
                                "⚠️ 启动补偿撤单失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                inst_id, config.id, pos_side, e
                            );
                        } else {
                            let mut updated = order;
                            updated.detail = Self::remove_close_algo_detail(&updated.detail);
                            if let Err(e) = self.swap_order_repository.update(&updated).await {
                                warn!(
                                    "⚠️ 启动补偿撤单后清理持久化失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                    inst_id, config.id, pos_side, e
                                );
                            }
                        }
                    }
                }
                continue;
            }
            let position = position.unwrap();
            let trade_side = if pos_side.eq_ignore_ascii_case("long") {
                TradeSide::Long
            } else {
                TradeSide::Short
            };
            let mut has_tp_sl = false;
            let mut exchange_algo_ids: Vec<String> = Vec::new();
            let mut exchange_stop_loss: Option<f64> = None;
            let mut exchange_take_profit: Option<f64> = None;
            if let Some(close_algos) = position.close_order_algo.as_ref() {
                for algo in close_algos {
                    let sl = algo
                        .sl_trigger_px
                        .as_ref()
                        .and_then(|v| v.parse::<f64>().ok());
                    let tp = algo
                        .tp_trigger_px
                        .as_ref()
                        .and_then(|v| v.parse::<f64>().ok());
                    if sl.is_some() || tp.is_some() {
                        has_tp_sl = true;
                    }
                    if !algo.algo_id.is_empty() {
                        exchange_algo_ids.push(algo.algo_id.clone());
                    }
                    if exchange_stop_loss.is_none() {
                        exchange_stop_loss = sl;
                    }
                    if exchange_take_profit.is_none() {
                        exchange_take_profit = tp;
                    }
                }
            }
            if has_tp_sl {
                let tag = Self::build_close_algo_tag(config.id);
                if let Some(order) = persisted_order.as_ref() {
                    if !exchange_algo_ids.is_empty() {
                        let mut updated = order.clone();
                        updated.detail = Self::upsert_close_algo_detail(
                            &updated.detail,
                            &exchange_algo_ids,
                            &tag,
                            exchange_stop_loss,
                            exchange_take_profit,
                        );
                        if let Err(e) = self.swap_order_repository.update(&updated).await {
                            warn!(
                                "⚠️ 同步持久化止盈止损失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                inst_id, config.id, pos_side, e
                            );
                        }
                    }
                }
                self.rehydrate_live_state_from_position(
                    config.id,
                    position,
                    trade_side,
                    persisted_order.as_ref().map(|order| order.detail.as_str()),
                    exchange_stop_loss,
                    exchange_take_profit,
                );
                if !exchange_algo_ids.is_empty() {
                    self.live_exit_targets.insert(
                        config.id,
                        LiveExitTargets {
                            stop_loss: exchange_stop_loss,
                            take_profit: exchange_take_profit,
                            algo_ids: exchange_algo_ids,
                            trade_side: Some(trade_side),
                        },
                    );
                }
                continue;
            }
            let Some(order) = persisted_order.as_ref() else {
                self.rehydrate_live_state_from_position(
                    config.id, position, trade_side, None, None, None,
                );
                warn!(
                    "⚠️ 持仓无止盈止损且无持久化记录: inst_id={}, config_id={}, pos_side={}",
                    inst_id, config.id, pos_side
                );
                continue;
            };
            let (stop_loss, take_profit) = Self::extract_close_algo_targets(&order.detail);
            self.rehydrate_live_state_from_position(
                config.id,
                position,
                trade_side,
                Some(order.detail.as_str()),
                stop_loss,
                take_profit,
            );
            if stop_loss.is_none() && take_profit.is_none() {
                warn!(
                    "⚠️ 持仓无止盈止损且无可用目标: inst_id={}, config_id={}, pos_side={}",
                    inst_id, config.id, pos_side
                );
                continue;
            }
            let close_side = if pos_side.eq_ignore_ascii_case("long") {
                "sell"
            } else {
                "buy"
            };
            let algo_cl_ord_id = Self::build_close_algo_cl_ord_id(config.id);
            let tag = Self::build_close_algo_tag(config.id);
            let algo_ids = okx_service
                .place_close_algo(
                    &api_config,
                    inst_id,
                    &position.mgn_mode,
                    close_side,
                    pos_side,
                    take_profit,
                    stop_loss,
                    Some(algo_cl_ord_id.as_str()),
                    Some(tag.as_str()),
                )
                .await?;
            if !algo_ids.is_empty() {
                if let Err(e) = self
                    .persist_close_algos(
                        config.id,
                        inst_id,
                        period,
                        pos_side,
                        &algo_ids,
                        &tag,
                        stop_loss,
                        take_profit,
                    )
                    .await
                {
                    warn!(
                        "⚠️ 启动补偿挂单持久化失败: inst_id={}, config_id={}, pos_side={}, err={}",
                        inst_id, config.id, pos_side, e
                    );
                }
                self.live_exit_targets.insert(
                    config.id,
                    LiveExitTargets {
                        stop_loss,
                        take_profit,
                        algo_ids,
                        trade_side: Some(trade_side),
                    },
                );
            }
        }
        Ok(())
    }
    /// 停止 交易执行与风控 后台流程，确保退出时不留下未释放状态。
    async fn close_position_internal(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        close_side: TradeSide,
    ) -> Result<()> {
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;
        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| {
                error!("获取API配置失败: config_id={}, error={}", config_id, e);
                anyhow!("获取API配置失败: {}", e)
            })?;
        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| {
                error!("获取账户数据失败: {}", e);
                anyhow!("获取账户数据失败: {}", e)
            })?;
        let close_pos_side_str = match close_side {
            TradeSide::Long => "long",
            TradeSide::Short => "short",
        };
        let persisted_order = self
            .load_persisted_close_algos(config_id, inst_id, period, close_pos_side_str)
            .await?;
        let persisted_algo_ids = persisted_order
            .as_ref()
            .map(|order| Self::extract_close_algo_ids(&order.detail))
            .unwrap_or_default();
        if let Some(p) = positions.iter().find(|p| {
            p.inst_id == inst_id
                && p.pos_side.eq_ignore_ascii_case(close_pos_side_str)
                && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
        }) {
            let mgn_mode = p.mgn_mode.clone();
            let close_pos_side = if close_pos_side_str == "long" {
                okx::dto::PositionSide::Long
            } else {
                okx::dto::PositionSide::Short
            };
            warn!(
                "⚠️ 信号平仓: inst_id={}, period={}, close_pos_side={:?}, mgn_mode={}",
                inst_id, period, close_pos_side, mgn_mode
            );
            okx_service
                .close_position(&api_config, inst_id, close_pos_side, &mgn_mode)
                .await
                .map_err(|e| anyhow!("平仓失败: {}", e))?;
        } else {
            warn!(
                "⚠️ 未找到可平仓位: inst_id={}, period={}, close_side={:?}",
                inst_id, period, close_side
            );
        }
        if !persisted_algo_ids.is_empty() {
            if let Err(e) = okx_service
                .cancel_close_algos(&api_config, inst_id, &persisted_algo_ids)
                .await
            {
                warn!(
                    "⚠️ 平仓后撤销持久化保护单失败: inst_id={}, period={}, config_id={}, err={}",
                    inst_id, period, config_id, e
                );
            }
        }
        if let Err(e) = self
            .clear_persisted_close_algos(config_id, inst_id, period, close_pos_side_str)
            .await
        {
            warn!(
                "⚠️ 平仓后清理持久化保护单失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config_id, e
            );
        }
        if let Some(prev_exit) = self.live_exit_targets.get(&config_id) {
            if prev_exit.trade_side == Some(close_side) {
                self.live_exit_targets.remove(&config_id);
            }
        }
        if let Err(e) = self
            .rebalance_trade_bucket_after_close(&api_config, config_id, inst_id)
            .await
        {
            warn!(
                "⚠️ 平仓后交易桶自动划转失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config_id, e
            );
        }
        Ok(())
    }
}
