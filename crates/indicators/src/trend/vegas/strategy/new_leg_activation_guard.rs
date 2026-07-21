impl VegasStrategy {
    /// 阻断只依赖当前冲击棒确认的新腿首棒入场。
    ///
    /// 延迟量能激活只读取此前已经完成的 K 线；未带该证据的新腿信号必须等后续 K 线
    /// 由完整 Vegas evaluator 重新确认，避免把同棒放量冲击直接当作可持续趋势。
    fn apply_new_leg_activation_guard(
        &self,
        values: &VegasIndicatorSignalValue,
        signal_result: &mut SignalResult,
    ) {
        if !self
            .entry_block_config
            .block_new_leg_without_delayed_activation_entry
            || !values.leg_detection_value.is_new_leg
            || values
                .fib_retracement_value
                .used_delayed_volume_confirmation
        {
            return;
        }

        let had_entry =
            signal_result.should_buy.unwrap_or(false) || signal_result.should_sell.unwrap_or(false);
        if !had_entry {
            return;
        }

        signal_result.should_buy = Some(false);
        signal_result.should_sell = Some(false);
        signal_result
            .filter_reasons
            .push("NEW_LEG_WITHOUT_DELAYED_ACTIVATION_ENTRY_BLOCK".to_string());
    }
}

#[cfg(test)]
mod new_leg_activation_guard_tests {
    use super::*;

    /// 构造显式开启门禁的研究策略，避免依赖默认配置语义。
    fn strategy() -> VegasStrategy {
        VegasStrategy {
            entry_block_config: EntryBlockConfig {
                block_new_leg_without_delayed_activation_entry: true,
                ..EntryBlockConfig::default()
            },
            ..VegasStrategy::default()
        }
    }

    #[test]
    fn blocks_new_leg_entry_without_prior_activation_for_both_directions() {
        for is_long in [true, false] {
            let mut values = VegasIndicatorSignalValue::default();
            values.leg_detection_value.is_new_leg = true;
            values
                .fib_retracement_value
                .used_delayed_volume_confirmation = false;
            let mut signal = SignalResult::empty();
            signal.should_buy = Some(is_long);
            signal.should_sell = Some(!is_long);

            strategy().apply_new_leg_activation_guard(&values, &mut signal);

            assert_eq!(signal.should_buy, Some(false));
            assert_eq!(signal.should_sell, Some(false));
            assert_eq!(
                signal.filter_reasons,
                vec!["NEW_LEG_WITHOUT_DELAYED_ACTIVATION_ENTRY_BLOCK"]
            );
        }
    }

    #[test]
    fn keeps_new_leg_entry_with_prior_delayed_activation() {
        let mut values = VegasIndicatorSignalValue::default();
        values.leg_detection_value.is_new_leg = true;
        values
            .fib_retracement_value
            .used_delayed_volume_confirmation = true;
        let mut signal = SignalResult::empty();
        signal.should_sell = Some(true);

        strategy().apply_new_leg_activation_guard(&values, &mut signal);

        assert_eq!(signal.should_sell, Some(true));
        assert!(signal.filter_reasons.is_empty());
    }
}
