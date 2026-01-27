## Dynamic Config Report

Date: 2026-01-27
Backtest ID: 16 (ETH-USDT-SWAP 4H)
Rows: 9752

### Adjustment Combinations (Top 10)

| Adjustments | Count |
| --- | --- |
| [] | 6501 |
| ["RANGE_TP_RATIO"] | 1146 |
| ["STOP_LOSS_ATR"] | 775 |
| ["STOP_LOSS_SIGNAL_KLINE", "STOP_LOSS_ATR"] | 472 |
| ["RANGE_TP_ONE_TO_ONE"] | 342 |
| ["RANGE_TP_RATIO", "STOP_LOSS_ATR", "TP_DYNAMIC_SHORT"] | 114 |
| ["RANGE_TP_RATIO", "STOP_LOSS_SIGNAL_KLINE", "STOP_LOSS_ATR", "TP_DYNAMIC_LONG"] | 105 |
| ["RANGE_TP_RATIO", "STOP_LOSS_ATR", "TP_DYNAMIC_LONG"] | 94 |
| ["RANGE_TP_RATIO", "STOP_LOSS_SIGNAL_KLINE", "STOP_LOSS_ATR", "TP_DYNAMIC_SHORT"] | 93 |
| ["RSI_EXTREME_EVENT"] | 37 |

### Query Used

```sql
select adjustments, count(*) as cnt
from dynamic_config_log
where backtest_id = 16
group by adjustments
order by cnt desc
limit 10;
```
