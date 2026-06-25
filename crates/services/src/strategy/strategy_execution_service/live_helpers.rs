impl StrategyExecutionService {
    const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV: &'static str = "LEGACY_DIRECT_LIVE_ORDER_CONFIRM";
    const LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN: &'static str =
        "I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS";
    /// 判断shoulddispatch策略信号toquantweb，为交易执行流程提供明确的布尔结果。
    fn should_dispatch_strategy_signal_to_quant_web() -> bool {
        Self::should_dispatch_strategy_signal_to_quant_web_from_env(
            std::env::var("STRATEGY_SIGNAL_DISPATCH_MODE")
                .ok()
                .as_deref(),
            std::env::var("RUST_QUAN_WEB_BASE_URL").ok().as_deref(),
            std::env::var("QUANT_WEB_BASE_URL").ok().as_deref(),
        )
    }
    /// 判断 交易执行与风控 条件是否满足，给上层流程提供布尔决策。
    fn should_dispatch_strategy_signal_to_quant_web_from_env(
        mode: Option<&str>,
        rust_quan_web_base_url: Option<&str>,
        quant_web_base_url: Option<&str>,
    ) -> bool {
        let mode = mode.unwrap_or("").trim().to_ascii_lowercase();
        if matches!(
            mode.as_str(),
            "disabled" | "disable" | "false" | "0" | "legacy" | "legacy_local" | "local" | "direct"
        ) {
            return false;
        }
        if matches!(mode.as_str(), "web" | "quant_web" | "execution_tasks") {
            return true;
        }
        rust_quan_web_base_url
            .or(quant_web_base_url)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_legacy_direct_live_exchange_order_allowed() -> Result<()> {
        let confirmation = std::env::var(Self::LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV).ok();
        Self::ensure_legacy_direct_live_exchange_order_allowed_from_env(confirmation.as_deref())
    }
    /// 校验输入和运行前置条件，提前暴露 交易执行与风控 的不可执行原因。
    fn ensure_legacy_direct_live_exchange_order_allowed_from_env(
        confirmation: Option<&str>,
    ) -> Result<()> {
        if confirmation.map(str::trim) == Some(Self::LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN) {
            return Ok(());
        }
        Err(anyhow!(
            "legacy direct live exchange mutation is blocked; route signals through quant_web execution tasks or set {}={} to acknowledge unaudited legacy direct order/cancel/transfer risk",
            Self::LEGACY_DIRECT_LIVE_ORDER_CONFIRM_ENV,
            Self::LEGACY_DIRECT_LIVE_ORDER_CONFIRM_TOKEN
        ))
    }
    /// 判断 交易执行与风控 条件是否满足，给上层流程提供布尔决策。
    fn should_manage_local_close_algos_after_open() -> bool {
        Self::should_manage_local_close_algos_after_open_from_env(
            std::env::var("STRATEGY_SIGNAL_DISPATCH_MODE")
                .ok()
                .as_deref(),
            std::env::var("RUST_QUAN_WEB_BASE_URL").ok().as_deref(),
            std::env::var("QUANT_WEB_BASE_URL").ok().as_deref(),
        )
    }
    /// 判断 交易执行与风控 条件是否满足，给上层流程提供布尔决策。
    fn should_manage_local_close_algos_after_open_from_env(
        mode: Option<&str>,
        rust_quan_web_base_url: Option<&str>,
        quant_web_base_url: Option<&str>,
    ) -> bool {
        !Self::should_dispatch_strategy_signal_to_quant_web_from_env(
            mode,
            rust_quan_web_base_url,
            quant_web_base_url,
        )
    }
    /// 提供smokeforced信号sidefrom环境变量的集中实现，避免交易执行调用方重复处理相同细节。
    fn smoke_forced_signal_side_from_env() -> Option<String> {
        std::env::var("RUST_QUANT_SMOKE_FORCE_SIGNAL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
    /// 执行 交易执行与风控 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_smoke_forced_signal_from_env(
        signal: &mut SignalResult,
        trigger_candle: &CandleItem,
    ) -> Result<bool> {
        let side = Self::smoke_forced_signal_side_from_env();
        Self::apply_smoke_forced_signal(signal, trigger_candle, side.as_deref())
    }
    /// 执行 交易执行与风控 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    fn apply_smoke_forced_signal(
        signal: &mut SignalResult,
        trigger_candle: &CandleItem,
        side: Option<&str>,
    ) -> Result<bool> {
        let Some(side) = side.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(false);
        };
        let side = side.to_ascii_lowercase();
        if matches!(
            side.as_str(),
            "disabled" | "disable" | "false" | "0" | "none" | "off"
        ) {
            return Ok(false);
        }
        signal.ts = trigger_candle.ts;
        signal.open_price = trigger_candle.c;
        signal.best_open_price = None;
        signal.atr_take_profit_ratio_price = None;
        signal.atr_stop_loss_price = None;
        signal.long_signal_take_profit_price = None;
        signal.short_signal_take_profit_price = None;
        signal.stop_loss_source = Some("smoke_forced_signal".to_string());
        signal.filter_reasons.clear();
        match side.as_str() {
            "buy" | "long" => {
                signal.should_buy = true;
                signal.should_sell = false;
                signal.signal_kline_stop_loss_price = Some(trigger_candle.c * 0.98);
                signal.direction = rust_quant_domain::SignalDirection::Long;
                Ok(true)
            }
            "sell" | "short" => {
                signal.should_buy = false;
                signal.should_sell = true;
                signal.signal_kline_stop_loss_price = Some(trigger_candle.c * 1.02);
                signal.direction = rust_quant_domain::SignalDirection::Short;
                Ok(true)
            }
            other => Err(anyhow!(
                "RUST_QUANT_SMOKE_FORCE_SIGNAL only supports buy/long/sell/short, got {}",
                other
            )),
        }
    }
    /// 将策略信号转换为统一订单方向，避免交易执行链路散落 buy/sell/long/short 字符串判断。
    fn trade_sides_from_signal(signal: &SignalResult) -> Result<(OrderSide, PositionSide)> {
        let order_side = if signal.should_buy {
            OrderSide::Buy
        } else if signal.should_sell {
            OrderSide::Sell
        } else {
            return Err(anyhow!("信号无效，无交易方向"));
        };
        Ok((order_side, PositionSide::from_order_side(order_side)))
    }
    /// 提供quantweb执行task配置from环境变量的集中实现，避免交易执行调用方重复处理相同细节。
    fn quant_web_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
        let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
            .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
            .map_err(|_| anyhow!("未配置 RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL"))?;
        let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
            .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
            .unwrap_or_default();
        Ok(ExecutionTaskConfig {
            base_url,
            internal_secret,
        })
    }
    #[allow(clippy::too_many_arguments)]
    /// 构建 交易执行与风控 请求或响应载荷，把字段组装规则集中在同一入口。
    fn build_strategy_signal_submit_request(
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
        config_id: i64,
        strategy_type: &str,
        exchange: Option<&str>,
        side: &str,
        pos_side: &str,
        client_order_id: &str,
    ) -> Result<StrategySignalSubmitRequest> {
        strategy_signal_payload::build_strategy_signal_submit_request(
            inst_id,
            period,
            signal,
            risk_config,
            config_id,
            strategy_type,
            exchange,
            side,
            pos_side,
            client_order_id,
            StrategySignalPayloadBuildOptions::default(),
        )
    }
    #[cfg(test)]
    fn build_strategy_signal_external_id(
        strategy_type: &str,
        config_id: i64,
        inst_id: &str,
        period: &str,
        signal_ts: i64,
        smoke_suffix: Option<&str>,
    ) -> String {
        strategy_signal_payload::build_strategy_signal_external_id(
            strategy_type,
            config_id,
            inst_id,
            period,
            signal_ts,
            smoke_suffix,
        )
    }
    #[allow(clippy::too_many_arguments)]
    /// 执行 交易执行与风控 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn dispatch_strategy_signal_to_quant_web(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
        config_id: i64,
        strategy_type: &str,
        exchange: Option<&str>,
        side: &str,
        pos_side: &str,
        client_order_id: &str,
    ) -> Result<()> {
        let client = ExecutionTaskClient::new(Self::quant_web_execution_task_config_from_env()?)?;
        let request = Self::build_strategy_signal_submit_request(
            inst_id,
            period,
            signal,
            risk_config,
            config_id,
            strategy_type,
            exchange,
            side,
            pos_side,
            client_order_id,
        )?;
        let external_id = request.external_id.clone();
        let response = client.submit_strategy_signal(request).await.map_err(|e| {
            anyhow!(
                "提交 rust_quan_web 策略信号失败: external_id={}, error={}",
                external_id,
                e
            )
        })?;
        info!(
            "✅ 已提交策略信号到 rust_quan_web: external_id={}, generated_tasks={}",
            external_id,
            response.generated_tasks.len()
        );
        Ok(())
    }
    /// 解析输入参数并收敛为 交易执行与风控 可使用的结构化值。
    fn parse_detail_object(detail: &str) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::from_str::<serde_json::Value>(detail) {
            Ok(serde_json::Value::Object(map)) => map,
            Ok(other) => {
                let mut map = serde_json::Map::new();
                map.insert("raw_detail".to_string(), other);
                map
            }
            Err(_) => {
                let mut map = serde_json::Map::new();
                map.insert(
                    "raw_detail".to_string(),
                    serde_json::Value::String(detail.to_string()),
                );
                map
            }
        }
    }
    /// 解析输入参数并收敛为 交易执行与风控 可使用的结构化值。
    fn extract_close_algo_ids(detail: &str) -> Vec<String> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return Vec::new();
        };
        let Some(ids) = value
            .get("close_algo")
            .and_then(|v| v.get("ids"))
            .and_then(|v| v.as_array())
        else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    }
    /// 持久化 交易执行与风控 结果，保证写入路径和幂等语义集中处理。
    fn upsert_close_algo_detail(
        detail: &str,
        algo_ids: &[String],
        tag: &str,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> String {
        let mut map = Self::parse_detail_object(detail);
        let mut close_algo = serde_json::Map::new();
        let ids = algo_ids
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        close_algo.insert("ids".to_string(), serde_json::Value::Array(ids));
        close_algo.insert(
            "updated_at".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                chrono::Utc::now().timestamp_millis(),
            )),
        );
        close_algo.insert(
            "tag".to_string(),
            serde_json::Value::String(tag.to_string()),
        );
        close_algo.insert(
            "stop_loss".to_string(),
            stop_loss
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        );
        close_algo.insert(
            "take_profit".to_string(),
            take_profit
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        );
        map.insert(
            "close_algo".to_string(),
            serde_json::Value::Object(close_algo),
        );
        serde_json::Value::Object(map).to_string()
    }
    /// 删除或清理 交易执行与风控 的临时数据，避免过期状态继续影响后续流程。
    fn remove_close_algo_detail(detail: &str) -> String {
        let mut map = Self::parse_detail_object(detail);
        map.remove("close_algo");
        serde_json::Value::Object(map).to_string()
    }
    /// 解析输入参数并收敛为 交易执行与风控 可使用的结构化值。
    fn parse_f64_value(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
    /// 解析输入参数并收敛为 交易执行与风控 可使用的结构化值。
    fn extract_close_algo_targets(detail: &str) -> (Option<f64>, Option<f64>) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return (None, None);
        };
        let Some(close_algo) = value.get("close_algo") else {
            return (None, None);
        };
        let stop_loss = close_algo.get("stop_loss").and_then(Self::parse_f64_value);
        let take_profit = close_algo
            .get("take_profit")
            .and_then(Self::parse_f64_value);
        (stop_loss, take_profit)
    }
    /// 解析输入参数并收敛为 交易执行与风控 可使用的结构化值。
    fn extract_entry_price(detail: &str) -> Option<f64> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return None;
        };
        value.get("entry_price").and_then(Self::parse_f64_value)
    }
    fn parse_opt_f64(input: Option<&str>) -> Option<f64> {
        input.and_then(|v| v.parse::<f64>().ok())
    }
    /// 生成 交易执行与风控 需要的派生数据，供后续执行、展示或审计使用。
    fn format_open_position_time(position: &OkxPosition) -> String {
        let millis = position
            .c_time
            .as_deref()
            .and_then(|v| v.parse::<i64>().ok())
            .or_else(|| {
                position
                    .u_time
                    .as_deref()
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
    }
    /// 判断 交易执行与风控 条件是否满足，给上层流程提供布尔决策。
    fn has_live_algo_for_side(&self, config_id: i64, side: TradeSide) -> bool {
        self.live_exit_targets
            .get(&config_id)
            .map(|target| target.trade_side == Some(side) && !target.algo_ids.is_empty())
            .unwrap_or(false)
    }
    /// 发送 交易执行与风控 通知或请求，并把投递错误留在当前边界处理。
    fn emit_guard_audit_log(
        stage: &str,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
        trigger_ts: i64,
        message: Option<String>,
    ) {
        let payload = serde_json::json!({
            "event": "live_guard",
            "stage": stage,
            "inst_id": inst_id,
            "period": period,
            "config_id": config_id,
            "side": format!("{:?}", side),
            "trigger_ts": trigger_ts,
            "message": message,
            "logged_at": chrono::Utc::now().timestamp_millis(),
        });
        warn!("LIVE_GUARD {}", payload);
    }
    #[cfg(not(test))]
    async fn compensate_for_guard(&self, config: &StrategyConfig, _side: TradeSide) -> Result<()> {
        self.compensate_close_algos_on_start(config).await
    }
    #[cfg(test)]
    async fn compensate_for_guard(&self, config: &StrategyConfig, side: TradeSide) -> Result<()> {
        self.guard_test_state
            .compensate_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.guard_test_state.compensate_fail.load(Ordering::SeqCst) {
            return Err(anyhow!("mock compensate failed"));
        }
        if self
            .guard_test_state
            .has_algo_after_compensate
            .load(Ordering::SeqCst)
        {
            self.live_exit_targets.insert(
                config.id,
                LiveExitTargets {
                    stop_loss: Some(1.0),
                    take_profit: None,
                    algo_ids: vec!["mock-algo".to_string()],
                    trade_side: Some(side),
                },
            );
        }
        Ok(())
    }
    #[cfg(not(test))]
    async fn close_for_guard(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
    ) -> Result<()> {
        self.close_position_internal(inst_id, period, config_id, side)
            .await
    }
    #[cfg(test)]
    async fn close_for_guard(
        &self,
        _inst_id: &str,
        _period: &str,
        _config_id: i64,
        _side: TradeSide,
    ) -> Result<()> {
        self.guard_test_state
            .close_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.guard_test_state.close_fail.load(Ordering::SeqCst) {
            return Err(anyhow!("mock close failed"));
        }
        Ok(())
    }
    /// 提供enforceopened仓位guard的集中实现，避免交易执行调用方重复处理相同细节。
    async fn enforce_opened_position_guard(
        &self,
        inst_id: &str,
        period: &str,
        config: &StrategyConfig,
        side: TradeSide,
        trigger_ts: i64,
    ) -> Result<()> {
        Self::emit_guard_audit_log(
            "sync_failed_after_open",
            inst_id,
            period,
            config.id,
            side,
            trigger_ts,
            Some("open succeeded but tp/sl sync failed".to_string()),
        );
        if let Err(comp_err) = self.compensate_for_guard(config, side).await {
            Self::emit_guard_audit_log(
                "compensate_failed",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                Some(comp_err.to_string()),
            );
        }
        if !self.has_live_algo_for_side(config.id, side) {
            Self::emit_guard_audit_log(
                "force_close_start",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                Some("compensation did not restore tp/sl".to_string()),
            );
            if let Err(close_err) = self.close_for_guard(inst_id, period, config.id, side).await {
                Self::emit_guard_audit_log(
                    "force_close_failed",
                    inst_id,
                    period,
                    config.id,
                    side,
                    trigger_ts,
                    Some(close_err.to_string()),
                );
                return Err(close_err);
            }
            Self::emit_guard_audit_log(
                "force_close_done",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                None,
            );
            self.live_exit_targets.remove(&config.id);
            self.live_states.insert(config.id, TradingState::default());
            return Err(anyhow!(
                "开仓后止盈止损同步失败，补偿未成功，已触发主动平仓"
            ));
        }
        Self::emit_guard_audit_log(
            "guard_resolved_by_compensate",
            inst_id,
            period,
            config.id,
            side,
            trigger_ts,
            Some("tp/sl restored after compensation".to_string()),
        );
        Ok(())
    }
    #[cfg(test)]
    fn configure_guard_test_state(
        &self,
        compensate_fail: bool,
        has_algo_after_compensate: bool,
        close_fail: bool,
    ) {
        self.guard_test_state
            .compensate_fail
            .store(compensate_fail, Ordering::SeqCst);
        self.guard_test_state
            .has_algo_after_compensate
            .store(has_algo_after_compensate, Ordering::SeqCst);
        self.guard_test_state
            .close_fail
            .store(close_fail, Ordering::SeqCst);
        self.guard_test_state
            .open_fail
            .store(false, Ordering::SeqCst);
        self.guard_test_state
            .compensate_calls
            .store(0, Ordering::SeqCst);
        self.guard_test_state.close_calls.store(0, Ordering::SeqCst);
    }
    #[cfg(test)]
    fn configure_open_failure_for_test(&self, open_fail: bool) {
        self.guard_test_state
            .open_fail
            .store(open_fail, Ordering::SeqCst);
    }
    #[cfg(test)]
    fn guard_test_calls(&self) -> (usize, usize) {
        (
            self.guard_test_state
                .compensate_calls
                .load(Ordering::SeqCst),
            self.guard_test_state.close_calls.load(Ordering::SeqCst),
        )
    }
    /// 从持仓信息重新恢复交易状态
    fn rehydrate_live_state_from_position(
        &self,
        config_id: i64,
        position: &OkxPosition,
        trade_side: TradeSide,
        detail: Option<&str>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) {
        let position_nums = position
            .pos
            .parse::<f64>()
            .ok()
            .map(f64::abs)
            .unwrap_or(0.0);
        let avg_px = Self::parse_opt_f64(position.avg_px.as_deref());
        let open_price = detail
            .and_then(Self::extract_entry_price)
            .or(avg_px)
            .unwrap_or(0.0);
        let mut state = self
            .live_states
            .get(&config_id)
            .map(|v| v.clone())
            .unwrap_or_default();
        let mut trade_position = state.trade_position.unwrap_or_default();
        trade_position.trade_side = trade_side;
        trade_position.position_nums = position_nums;
        trade_position.open_price = open_price;
        trade_position.open_position_time = Self::format_open_position_time(position);
        trade_position.signal_high_low_diff = trade_position.signal_high_low_diff.max(1e-8);
        trade_position.signal_kline_stop_close_price = stop_loss;
        trade_position.atr_stop_loss_price = stop_loss;
        trade_position.atr_take_ratio_profit_price = take_profit;
        if trade_side == TradeSide::Long {
            trade_position.long_signal_take_profit_price = take_profit;
        } else {
            trade_position.short_signal_take_profit_price = take_profit;
        }
        state.trade_position = Some(trade_position);
        self.live_states.insert(config_id, state);
    }
    /// 判断止损止盈目标是否发生变化
    fn targets_changed(prev: &LiveExitTargets, next: &ExitTargets, eps: f64) -> bool {
        !approx_eq_opt(prev.stop_loss, next.stop_loss, eps)
            || !approx_eq_opt(prev.take_profit, next.take_profit, eps)
    }
    /// 构建止损候选价列表（由上层选择最紧止损）
    fn build_stop_loss_candidates(
        side: OrderSide,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
    ) -> Result<Vec<f64>> {
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        strategy_signal_payload::validate_max_loss_percent(max_loss_percent)?;
        let max_loss_stop = if side == OrderSide::Sell {
            entry_price * (1.0 + max_loss_percent)
        } else {
            entry_price * (1.0 - max_loss_percent)
        };
        let mut candidates: Vec<f64> = vec![max_loss_stop];
        // 信号K线止损（若启用且信号提供）
        if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
            if let Some(px) = signal.signal_kline_stop_loss_price {
                candidates.push(px);
            }
        }
        Ok(candidates)
    }
}
