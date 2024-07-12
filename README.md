## rust 策略自动交易项目-okx
### 自动执行开仓与平仓


### 默认策略

```sql
INSERT INTO `rust_quant`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `time`, `created_at`, `updated_at`) VALUES (4, 'Engulfing', 'ETH-USDT-SWAP', '{\"num_bars\":3.0,\"heikin_ashi\":false}', '4H', '2024-07-09 15:21:40', '2024-07-10 05:23:02')

INSERT INTO `rust_quant`.`strategy_config` (`id`, `strategy_type`, `inst_id`, `value`, `time`, `created_at`, `updated_at`) VALUES (3, 'UtBoot', 'BTC-USDT-SWAP', '{\"key_value\":2.0,\"atr_period\":2,\"heikin_ashi\":false}', '4H', '2024-07-08 20:06:52', '2024-07-08 12:07:44')
```