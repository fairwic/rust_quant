# BSC Meme Event Replay Report

- Verdict: FAIL
- Data source: LBank public 5m klines
- Proof scope: price_volume_only_not_full_strategy
- Samples: 12
- Trades: 3
- Win rate: 33.33%
- Net R: -2.2407
- Avg net R: -0.7469
- Profit factor: 0.0258
- Net R without largest winner: -2.3000

This replay uses LBank public 5m klines only. It does not prove the full
strategy because OI, funding, depth, security, and wallet-flow fields are
missing for most samples.

| Symbol | Entered | Exit | Net R | Bars | Warning |
| --- | --- | --- | ---: | ---: | --- |
| rave_usdt | False | NO_ENTRY | 0.0000 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| bianrensheng_usdt | False | NO_ENTRY | 0.0000 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| 4_usdt | True | STOP_LOSS | -1.1500 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| palu_usdt | True | STOP_LOSS | -1.1500 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| kefuxiaohe_usdt | False | NO_ENTRY | 0.0000 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| xiuxian_usdt | False | NO_ENTRY | 0.0000 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| hajimi_usdt | False | NO_ENTRY | 0.0000 | 4033 | PRICE_VOLUME_ONLY_MISSING_OI_DEPTH_SECURITY |
| broccoli4_usdt | False | NO_ENTRY | 0.0000 | 4033 |  |
| bnbholder_usdt | False | NO_ENTRY | 0.0000 | 4033 |  |
| ai4_usdt | False | NO_ENTRY | 0.0000 | 4033 |  |
| npcz_usdt | False | NO_ENTRY | 0.0000 | 4033 |  |
| giggle_usdt | True | TIME_STOP | 0.0593 | 4033 |  |

Proof gate requires: trades >= 10, win rate >= 42%, avg win >= 2R,
avg loss <= 1R, avg net >= 0.25R, profit factor >= 1.35, and positive
net R after removing the largest winner.
