## rust 策略自动交易项目-okx
### 自动执行开仓与平仓


### 默认策略

```sql
INSERT INTO `rust_quant`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `time`, `created_at`, `updated_at`) VALUES (4, 'Engulfing', 'ETH-USDT-SWAP', '{\"num_bars\":3.0,\"heikin_ashi\":false}', '4H', '2024-07-09 15:21:40', '2024-07-10 05:23:02')

INSERT INTO `rust_quant`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `time`, `created_at`, `updated_at`) VALUES (3, 'UtBoot', 'BTC-USDT-SWAP', '{\"key_value\":2.0,\"atr_period\":2,\"heikin_ashi\":false}', '4H', '2024-07-08 20:06:52', '2024-07-08 12:07:44')
```


```json
one_bar_rate 0.48%

	"ema_signal": {
		"ema0_length": 12,
		"ema2_length": 144,
		"ema3_length": 169,
		"ema4_length": 576,
		"ema5_length": 676,
		"ema_breakthrough_threshold": 0.003,
		"is_open": true
	},
	"ema_touch_trend_signal": {
		"ema2_with_ema3_ratio": 1.012,
		"ema3_with_ema4_ratio": 1.012,
		"ema4_with_ema5_ratio": 1.012,
		"is_open": true,
		"price_with_ema_ratio": 1.005
	},
	"rsi_signal": {
		"is_open": true,
		"rsi_length": 12,
		"rsi_overbought": 85.0,
		"rsi_oversold": 25.0
	},
	"signal_weights": {
		"min_total_weight": 2.0,
		"weights": [
			["Breakthrough", 1.0],
			["VolumeTrend", 1.0],
			["Rsi", 1.0],
			["TrendStrength", 1.0],
			["EmaDivergence", 1.0],
			["PriceLevel", 1.0],
			["EmaTrend", 1.0]
		]
	},
	"volume_signal": {
		"is_open": true,
		"volume_bar_num": 3,
		"volume_decrease_ratio": 2.0,
		"volume_increase_ratio": 3.7
	}
}
```