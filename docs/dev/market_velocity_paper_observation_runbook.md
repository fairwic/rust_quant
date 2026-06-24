# Market Velocity Paper Observation Runbook

This runbook is for production observation of Market Velocity paper outcomes.
It runs the Rust-native `market_velocity_paper_observation` binary and writes
observation-only rows into Web `market_velocity_paper_outcomes`.

## Safety Boundary

- No Python production path is used.
- No exchange order, cancel, close, or account mutation is performed.
- No Web `execution_tasks` are created by this command.
- The production observation entry owns the paper outcome sink and entry-trigger
  filter. Experimental filter overrides must use `market_velocity_event_backtest`
  outside this production runbook.
- The production observation command is locked to the named strategy preset
  `momentum_03sl_20r_v5`. Do not pass ad-hoc target R, stop-loss, reentry,
  FVG, profit-protection, or runner-exit parameters to the production observer.
- 当前 preset 也固定本轮优化后的入场过滤：`min_delta_rank=15`、
  `trend_min_average_distance_pct=0.0`、`entry_max_distance_pct=4.0`。
- 生产默认不带历史 symbol blocklist；如果要研究性复现历史黑名单结果，只能在
  `market_velocity_event_backtest` 中显式传 `--symbol-blocklist`。

## Required Environment

```bash
QUANT_CORE_DATABASE_URL=postgres://...
RUST_QUAN_WEB_BASE_URL=https://...
EXECUTION_EVENT_SECRET=...
MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS=21600
```

`DATABASE_URL` may also be set to the same value as `QUANT_CORE_DATABASE_URL`
for compatibility with existing Core deployment conventions.
`MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS` is optional and defaults to
21600 seconds, or 6 hours, in the deploy compose scheduler service.

## Production Image Requirement

The runtime image must include both binaries:

- `/usr/local/bin/rust_quant`
- `/usr/local/bin/market_velocity_paper_observation`

`Dockerfile.runtime` builds `rust-quant-cli --bins` and copies both binaries.

## One-Shot Run

Using the production deploy compose file:

```bash
podman compose -f docker-compose.deploy.yml --profile observation run --rm quant-core-market-velocity-paper-observation
```

The service is behind the `observation` profile and has `restart: "no"`, so it
does not start with the live radar or execution worker services. The compose
command runs:

```bash
market_velocity_paper_observation --paper-strategy-preset momentum_03sl_20r_v5
```

## Production Scheduler

For production observation, run the Rust-native scheduler profile:

```bash
podman compose -f docker-compose.deploy.yml --profile observation-scheduler up -d quant-core-market-velocity-paper-observation-scheduler
```

This starts `market_velocity_paper_observation --paper-strategy-preset
momentum_03sl_20r_v5 --loop-interval-seconds
${MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS:-21600}` with `restart:
unless-stopped`. A failed observation cycle is logged and retried after the next
interval; missing startup configuration still fails fast.

Run every 6-12 hours. The Admin diagnostic marks observation health as stale when
the latest production-filter paper outcome write is older than 48 hours.

The default Core CI/CD deploy and rollback scripts manage this scheduler
alongside `quant-core-market-velocity-radar` and `quant-core-execution-worker`.
After `up -d`, both scripts assert that every targeted long-running service has
a container and that Docker reports `.State.Running=true`; otherwise the deploy
or rollback exits non-zero instead of only printing `docker compose ps`.
If production overrides `DEPLOY_SERVICES`, keep
`quant-core-market-velocity-paper-observation-scheduler` in that list.

## Success Criteria

The command output should include:

```text
entry_trigger_filter    before=<n>    after=<m>    allowlist=breakout_previous_high,reclaim_ema
paper_outcomes_submitted=<n>
```

Admin `GET /admin/quant/market-velocity/diagnostics` should then show:

- `paperOutcomeObservationHealth.status = ok`
- `paperOutcomeObservationHealth.cadenceStatus = ok` after at least three
  production-filter observation batches are present in the latest 48 hours
- `paperOutcomeObservationHealth.expectedFilterVersion = entry_trigger_allowlist_v1`
- `paperOutcomeObservationHealth.productionFilterSampleCount60d > 0`
- `paperOutcomeObservationHealth.productionFilterObservationBatchCount48h >= 3`
- `paperOutcomeObservationHealth.readyForExecutionTaskEvaluation = true`
- `paperOutcomeObservationHealth.generatedExecutionTaskCount = 0`
- `paperOutcomeObservationHealth.observationOnly = true`
- `paperOutcomeOptimizationRecommendation.status = candidate` once the selected
  production-filter target R / horizon has enough resolved samples. A lower
  status such as `insufficient_sample` means the recommendation is visible but
  should not be promoted.
- The optimization recommendation first prefers target R / horizon buckets that
  meet the minimum resolved-sample gate, then ranks by risk-adjusted win-rate
  edge. The edge is the Wilson lower bound of resolved win rate minus the
  target R breakeven win rate, so small raw high-win-rate buckets do not outrank
  statistically stronger candidates.
- `paperOutcomeOptimizationRecommendation.riskAdjustedWinRateEdge > 0`
- `paperOutcomeOptimizationRecommendation.generatedExecutionTaskCount = 0`
- `paperOutcomeExecutionReadiness.status = ready`
- `paperOutcomeExecutionReadiness.nextAction = evaluate_execution_task_creation`
- `paperOutcomeExecutionReadiness.blockers = []`
- `paperOutcomeExecutionReadiness.mutationAllowed = false`
- `paperOutcomeExecutionTaskCreationEvaluation.status = ready_for_web_dry_run`
- `paperOutcomeExecutionTaskCreationEvaluation.dryRunOnly = true`
- `paperOutcomeExecutionTaskCreationEvaluation.requiresSeparateAuthorization = true`
- `paperOutcomeExecutionTaskCreationEvaluation.wouldCreateExecutionTask = false`
- `paperOutcomeExecutionTaskCreationEvaluation.requiredWebChecks` includes
  strategy rules, risk filters, tradable symbol, user entitlement, API key
  readiness, signed read-only preflight, and execution-task idempotency.

## Alert Conditions

Investigate before enabling any execution-task creation if:

- `paperOutcomeObservationHealth.status` is `missing_filter_version`, `stale`,
  `empty`, or `unavailable`.
- `paperOutcomeObservationHealth.cadenceStatus` is not `ok`.
- `paperOutcomeObservationHealth.readyForExecutionTaskEvaluation` is not `true`.
- `productionFilterSampleCount60d` drops to zero.
- `productionFilterObservationBatchCount48h` is lower than 3.
- `paperOutcomeOptimizationRecommendation.status` is not `candidate`.
- `paperOutcomeOptimizationRecommendation.riskAdjustedWinRateEdge` is not
  positive.
- `paperOutcomeExecutionReadiness.status` is not `ready`.
- `paperOutcomeExecutionReadiness.blockers` is not empty.
- `paperOutcomeExecutionTaskCreationEvaluation.status` is not
  `ready_for_web_dry_run`.
- `paperOutcomeExecutionTaskCreationEvaluation.wouldCreateExecutionTask` is not
  `false`.
- `generatedExecutionTaskCount` is not zero.
- The command fails before `paper_outcomes_submitted=...`.

## Current Production Filter

The production observation entry currently tracks:

```text
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
```

This is based on the recent local `quant_core` framework sweep where
`breakout_previous_high,reclaim_ema` balanced trade count, win rate, Sharpe, and
drawdown better than the wider trigger set.

## Current Strategy Preset

The production paper observer currently uses:

```text
paper_strategy_preset=momentum_03sl_20r_v5
entry_rule_version=rank_radar_4h_trend_15m_momentum_03sl_20r_v5
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
symbol_blocklist=
stop_reentry_mode=off
stop_loss_pct=0.03
target_r=2.0
horizons=24h,48h
```

This preset promotes the stronger anti-overfit candidate from the recent
backtest sweep: it does not depend on historical symbol-specific exclusions and
it disables stop-reentry after the local sample showed three stop-reentry
attempts all became second stop losses. The
previous `stop_reentry_03sl_30r_v3` result reached 178.85U in the framework
report, but it depended on a historical symbol blocklist and should not be used
as the production default without forward observation.

Latest local verification, using the current local `quant_core` database:

```text
Core candidate scale: candle_pairs=55, raw_candidate_events=62514, entry_pass=2912, entry_trigger_after=2494
Core CLI 48h: trades=41, win=26, loss=11, timeout=1, incomplete=3, resolved_win_rate=70.2703%, avg_r_complete=1.090933
Framework symbol_isolated_100u: symbols=27, trades=41, win_rate=65.8537%, trade_sharpe=4.608339744751571, max_drawdown_pct=3.40882238, total_profit=122.47823444
```

`raw_candidate_events` can drift slightly while new `market_rank_events` rows are
inserted; the framework trade count is unchanged in this verification.

2026-06-21 no-overfit continuation: local 15m backfill added 9342 rows for
`BICO-USDT-SWAP,RE-USDT-SWAP,RESOLV-USDT-SWAP,SAND-USDT-SWAP,O-USDT-SWAP`,
raising candle-pair coverage from 50 to 55 symbols. The production filter result
did not change because the added symbols remained blocked by 4h insufficiency,
15m overextension, price below averages, or missing timing confirmation. Generic
scans of all-rank price-change caps, wider `max_new_rank`, wider trigger sets,
target R, stop-loss width, 15m/1h and 1h/4h FVG entries, and 15m average-distance
limits did not produce a candidate that simultaneously met trades >= 30,
framework win-rate >= 65%, high Sharpe, max drawdown <= 30%, and total profit
>= 150U. Keep `momentum_03sl_20r_v5` as the non-overfit default until forward
paper observation or a broader sample supports another generic rule.

2026-06-21 continuation added a research-only `--max-delta-rank` filter to test
whether extreme rank jumps are noisy chase entries. The strongest OOS-stable
results improved win rate but reduced profit, so they are not promoted:

```text
--max-delta-rank 70 --target-rs 2.0: trades=31, win_rate=70.96774193548387%, trade_sharpe=4.76318458727161, max_drawdown_pct=3.40882238, total_profit=102.83844164, late_win_rate=72.72727272727273%
--max-delta-rank 79 --target-rs 2.0: trades=33, win_rate=69.6969696969697%, trade_sharpe=4.65928250362462, max_drawdown_pct=3.40882238, total_profit=106.11559397, late_win_rate=69.56521739130434%
entry_max_distance_pct=3.0 --target-rs 2.0: trades=36, win_rate=69.44444444444444%, trade_sharpe=4.452, max_drawdown_pct=6.04575100, total_profit=108.95, late_win_rate=68.18181818181817%
```

Wider distance settings increased trade count and sometimes profit, but failed
the win-rate and late-split gates; the best wider-distance profit was
`entry_max_distance_pct=8.0 --target-rs 2.4` with `70` trades and `141.21U`,
but only `51.43%` full win rate and `50.0%` late win rate.

For overfit checks, run the research CLI with `--equity-split-report`. It prints
early/late time splits using the same `symbol_isolated_100u` framework replay.
On 2026-06-21, the current default split as:

```text
early: trades=14, win_rate=71.42857142857143%, trade_sharpe=3.618907914530351, max_drawdown_pct=3.07000000, total_profit=51.29822363
late: trades=28, win_rate=64.28571428571429%, trade_sharpe=3.371265362245003, max_drawdown_pct=3.07000000, total_profit=76.63505327
```

The historical high-profit `stop_reentry_03sl_30r_v3` + symbol-blocklist
candidate no longer meets the full-sample win-rate gate on the current database
and degrades in the late split:

```text
full: trades=49, win_rate=61.224489795918366%, trade_sharpe=4.044472281948052, max_drawdown_pct=6.40196478, total_profit=169.73251503
early: trades=18, win_rate=66.66666666666666%, trade_sharpe=3.9490239669120095, max_drawdown_pct=6.40196478, total_profit=96.71270181
late: trades=32, win_rate=59.375%, trade_sharpe=2.4476985110437828, max_drawdown_pct=6.04575100, total_profit=81.89242033
```

2026-06-21 目标放宽为 framework 胜率 `>55%` 后，先补齐当前验收
symbol 的 15m K 线缺口。回填写入 `9600` 根旧缺口 15m K 线，剩余
48h 覆盖缺口均来自事件过近、未来 48h 尚未走完，而不是历史 K 线缺失。
回填后的当前默认基线为：

```text
momentum_03sl_20r_v5: symbols=33, trades=51, win_rate=58.82352941176471%, trade_sharpe=3.5516554411025902, max_drawdown_pct=7.55972182, total_profit=112.87621520
early: trades=18, win_rate=66.66666666666666%, trade_sharpe=3.3077875670257013, max_drawdown_pct=3.07000000, total_profit=58.24150839
late: trades=34, win_rate=52.94117647058824%, trade_sharpe=1.9260080478895372, max_drawdown_pct=4.75683918, total_profit=51.46292655
```

Under the relaxed `>55%` gate, generic scans that increased target R, widened
entry distance, widened `max_new_rank`, lowered volume confirmation, or included
all triggers increased neither robust profit nor late-split quality. The first
clean structural candidate was to remove the weaker `reclaim_ema` trigger and
run only `breakout_previous_high`; after fine scanning target R and the
framework-backed profit-protect stop update, the best breakout-only variant was:

```text
breakout_previous_high only, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=1.0R:
symbols=29, trades=37, win_rate=59.45945945945946%, trade_sharpe=3.7284868207002813, max_drawdown_pct=3.40882238, total_profit=137.74550261
early: trades=13, win_rate=61.53846153846154%, trade_sharpe=3.115743445763077, max_drawdown_pct=3.07000000, total_profit=66.16253770
late: trades=25, win_rate=56.00000000000001%, trade_sharpe=2.2149256472480614, max_drawdown_pct=3.07000000, total_profit=68.84681163
```

This candidate keeps both split profits positive and improves drawdown, but it
reduces trade count from `51` to `37` and still does not reach `150U`.

The first full-sample candidate that satisfied `55%+` win rate, `150U+` profit,
and non-small trade count used the existing trigger allowlist with a narrower
generic chase filter and late profit protection:

```text
command shape:
--target-rs 3.3
--entry-trigger-allowlist breakout_previous_high,reclaim_ema
--entry-max-distance-pct 4.0
--entry-min-volume-ratio 1.0
--min-delta-rank 15
--max-new-rank 30
--chase-top-rank 5
--chase-price-change-pct 8.0
--profit-protect-after-r 2.8
--profit-protect-stop-r 2.0
--equity-report --equity-split-report --min-trades 30

candle_pairs=56
raw_candidate_events=68416
entry_pass=3621
entry_trigger_filter_after=3107
framework: symbols=35, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.645696446371883, max_drawdown_pct=7.55972182, total_profit=158.04193447
early: symbols=14, trades=18, win_rate=61.111111111111114%, trade_sharpe=3.463678049044337, max_drawdown_pct=3.07000000, total_profit=90.30123180
late: symbols=26, trades=36, win_rate=52.77777777777778%, trade_sharpe=2.1423213742336467, max_drawdown_pct=6.04575100, total_profit=74.56631459
```

This is a research candidate, not a production preset yet. The full sample meets
the user's relaxed target and exceeds `150U`, but the late split win rate is
below `55%`; keep it in forward paper observation before promotion. The
`top=5` chase filter was stable across `chase_price_change_pct=4.0..10.0` in the
current sample, while `top=10` stayed below the win-rate gate and `top=15/20`
over-filtered. The no-protect `top=5` variant reached `154.29088330U` but only
`53.84615384615385%` win rate, so the late profit-protect update is the part
that crosses the relaxed win-rate gate.

Continuation scans after this candidate did not find a better non-overfit
promotion preset:

```text
profit-protect sweep, top=5:
best remains after=2.8R, stop=2.0R, target=3.3R with full win_rate=55.769230769230774%, total_profit=158.04193447, late win_rate=52.77777777777778%.
after=2.8R stop=1.5R kept full win_rate=55.769230769230774% but only total_profit=154.82329406 and the same late win_rate=52.77777777777778%.
after=3.0R and after=3.2R mostly reverted to no-protect behavior: full win_rate around 53.84615384615385% and late win_rate around 50%.

entry-distance sweep:
dist=3.5 reached full win_rate=55.319148936170215% but only total_profit=140.48569261 and late win_rate=53.125%.
dist=4.5/5.0/6.0 increased trades but degraded full and late win rates below 50%.

volume sweep:
vol=0.8/0.0 increased trades but late split degraded sharply; vol=1.2+ over-filtered and reduced both win rate and profit.

rank sweep:
min_delta=12 improved profit to 167.05573535 with 62 trades, but full win_rate=53.2258064516129% and late win_rate=52.38095238095239%.
min_delta=12 + max_delta=80 improved late win_rate to 54.285714285714285% but only full win_rate=54.71698113207547% and total_profit=147.99922309.
min_delta=12 + max_new=20 had full/late win rates above 59%/60%, but only total_profit=85.47273354 and 32 trades.
```

Trigger decomposition explains the tradeoff:

```text
breakout_previous_high only:
full trades=39, win_rate=58.97435897435898%, total_profit=136.93737158, max_drawdown_pct=3.55002519
late trades=27, win_rate=55.55555555555556%, total_profit=67.59172485

reclaim_ema only:
full trades=24, win_rate=37.5%, total_profit=26.02152552
late trades=17, win_rate=29.411764705882355%, total_profit=0.97990709

breakout_previous_high,reclaim_ema:
full trades=52, win_rate=55.769230769230774%, total_profit=158.04193447
late trades=36, win_rate=52.77777777777778%, total_profit=74.56631459
```

So the robust path is still unresolved: `breakout_previous_high` is the stable
entry trigger, while `reclaim_ema` provides the extra trade count/profit needed
to clear `150U` but introduces the late-split weakness. Avoid promoting a
reclaim-inclusive default until forward observation confirms that the late split
weakness is sample noise rather than structural decay.

2026-06-21 当前目标调整为 framework 胜率 `>55%`、优先提高盈利和开仓次数、
同时保持低回撤后，新增 `--equity-quartile-report` 做四分位时间切片，避免
只看 early/late 两段造成收益集中误判。当前本地 `quant_core` 样本已经轻微
漂移到 `raw_candidate_events=68483`；以下结果都使用同一套
`symbol_isolated_100u` framework replay，不使用 portfolio-level 共享资金。

高收益候选仍然只适合作为 research/paper observation：

```text
breakout_previous_high,reclaim_ema, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=56, raw_candidate_events=68483, entry_pass=3621, entry_trigger_after=3107
framework: symbols=35, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.645696446371883, max_drawdown_pct=7.55972182, total_profit=158.04193447
early: trades=18, win_rate=61.111111111111114%, total_profit=90.30123180
late: trades=36, win_rate=52.77777777777778%, total_profit=74.56631459
q1: trades=6, win_rate=66.66666666666666%, total_profit=25.19422684
q2: trades=13, win_rate=53.84615384615385%, total_profit=60.73227702
q3: trades=19, win_rate=47.368421052631575%, total_profit=52.30591919
q4: trades=19, win_rate=52.63157894736842%, total_profit=15.06774223
```

它满足全样本 `55%+`、`150U+`、交易数不少、回撤低，但 q3/q4 胜率弱，
尤其 q4 Sharpe 只有 `0.6821737744188278`，不能作为非过拟合默认参数。

当前更稳的候选是只保留 `breakout_previous_high`：

```text
breakout_previous_high only, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=2.0R
entry_trigger_after=2364
framework: symbols=31, trades=39, win_rate=58.97435897435898%, trade_sharpe=3.6639645901641575, max_drawdown_pct=3.55002519, total_profit=136.93737158
early: trades=13, win_rate=61.53846153846154%, total_profit=66.62053704
late: trades=27, win_rate=55.55555555555556%, total_profit=67.59172485
q1: trades=4, win_rate=50.0%, total_profit=8.85130784
q2: trades=10, win_rate=60.0%, total_profit=55.00101019
q3: trades=15, win_rate=53.333333333333336%, total_profit=48.43814202
q4: trades=14, win_rate=57.14285714285714%, total_profit=24.86399231
```

这组没有达到 `150U`，但比 reclaim-inclusive 候选更稳：late 胜率高于
`55%`，最大回撤只有 `3.55002519%`，四分位没有亏损段。它是当前更适合
继续 forward paper observation 的保守候选。

触发器拆分确认 `reclaim_ema` 是后段弱点：

```text
breakout_previous_high only with min_delta=15,max_delta=80:
symbols=26, trades=32, win_rate=59.375%, total_profit=113.51981010, late_win_rate=57.14285714285714%

reclaim_ema only with min_delta=15,max_delta=80:
symbols=16, trades=21, win_rate=42.857142857142854%, total_profit=35.23152552, late_win_rate=33.33333333333333%, q4_total_profit=-5.42392067
```

继续扫描结果：

```text
min_delta=12,max_delta=80,target=3.3: trades=53, win_rate=54.71698113207547%, total_profit=147.99922309, late_win_rate=54.285714285714285%
min_delta=15,max_delta=80,target=3.3: trades=43, win_rate=58.139534883720934%, total_profit=138.98542221, late_win_rate=55.172413793103445%, q4_win_rate=44.44444444444444%
max_new=28 with min_delta=12,max_delta=80,target=3.3: trades=49, win_rate=55.10204081632652%, total_profit=130.19962193, late_win_rate=54.54545454545454%
breakout-only dist=4.5/5.0/6.0: increased trades but late win rate fell to about 40-42.5%
breakout-only volume=0.8: trades=47 but late win_rate=48.148148148148145%
```

Conclusion for this iteration: do not promote the `158.04193447U` candidate yet.
Use it as a research/paper-observation candidate; use breakout-only as the
current non-overfit baseline. The unresolved work is to find a non-time,
non-symbol-specific filter that can admit a subset of `reclaim_ema` without
reintroducing the q3/q4 weakness.

Continuation found a better reclaim-inclusive research candidate by adding a
generic overheat cap on rank jumps. This does not use a symbol blocklist or a
time split:

```text
breakout_previous_high,reclaim_ema, target_r=3.3, min_delta_rank=12, max_delta_rank=72, max_new_rank=30, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=58, raw_candidate_events=71755, entry_pass=4057, entry_trigger_after=3644
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
early: trades=19, win_rate=52.63157894736842%, total_profit=72.73980197
late: trades=34, win_rate=55.88235294117647%, total_profit=75.29818102
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.53263319
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
```

Compared with the earlier `158.04193447U` candidate, this version gives up
about `6.97U` but fixes the late split from `52.77777777777778%` to
`55.88235294117647%` while keeping `52` trades and `150U+` profit. Adjacent
checks show the overheat cap is not a single-point `75` artifact:

```text
max_delta_rank=70: trades=52, win_rate=53.84615384615385%, total_profit=146.05777341, late_win_rate=52.94117647058824%
max_delta_rank=72: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
max_delta_rank=75: same framework result as 72 in the current sample
max_delta_rank=78: trades=53, win_rate=54.71698113207547%, total_profit=147.99922309, late_win_rate=54.285714285714285%
```

Target-R neighbor checks keep the same win rate but only `3.3R` clears the
current `150U` profit objective:

```text
target=3.1R: trades=52, win_rate=55.769230769230774%, total_profit=140.39442985, late_win_rate=55.88235294117647%
target=3.2R: trades=52, win_rate=55.769230769230774%, total_profit=145.72904404, late_win_rate=55.88235294117647%
target=3.3R: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
target=3.4R: trades=52, win_rate=55.769230769230774%, total_profit=138.99942377, late_win_rate=55.88235294117647%
```

Promotion caveat: this is now the best numeric fit for the user's relaxed
objective (`55%+` full win rate, `150U+` profit, non-small trade count, low
drawdown), but q3/q4 win rates are still below `55%`. Treat it as a stronger
research candidate than the prior `158U` version, not as a fully de-risked
production default, until forward paper observation confirms the q3/q4 weakness
does not persist.

Follow-up with `--equity-trigger-report` confirmed the trigger tradeoff under
the updated user target of `>=55%` framework win rate, high profit, non-small
trade count, and low drawdown. The local sample drifted to
`raw_candidate_events=71241`, but the framework result stayed unchanged:

```text
candidate:
breakout_previous_high,reclaim_ema, target_r=3.3, min_delta_rank=12, max_delta_rank=72, max_new_rank=30, chase_top_rank=5, chase_price_change_pct=8.0, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=58, raw_candidate_events=71241, entry_pass=4033, entry_trigger_after=3620
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
early: trades=19, win_rate=52.63157894736842%, total_profit=72.73980197
late: trades=34, win_rate=55.88235294117647%, total_profit=75.29818102
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.67828893
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
```

Trigger reports are standalone trigger replays, not additive attribution of the
combined sequence. They show why a simple trigger removal is not enough:

```text
breakout_previous_high standalone:
trades=40, win_rate=52.5%, trade_sharpe=3.047059450246726, max_drawdown_pct=4.89118170, total_profit=110.85371915

reclaim_ema standalone:
trades=27, win_rate=40.74074074074074%, trade_sharpe=1.3381271712433855, max_drawdown_pct=7.81241293, total_profit=40.18135478
```

Adjacent scans did not find a stronger non-overfit replacement:

```text
profit-protect:
after=2.6R stop=1.5R: trades=52, win_rate=55.769230769230774%, total_profit=131.60797648, late_win_rate=55.88235294117647%
after=2.6R stop=2.0R: trades=52, win_rate=55.769230769230774%, total_profit=139.22532242, late_win_rate=55.88235294117647%
after=2.8R stop=1.5R: trades=52, win_rate=55.769230769230774%, total_profit=147.83678948, late_win_rate=55.88235294117647%
after=3.0R stop=2.0R: trades=52, win_rate=53.84615384615385%, total_profit=147.23541273, late_win_rate=52.94117647058824%

chase top-rank:
top=4: trades=53, win_rate=54.71698113207547%, total_profit=147.66719457, late_win_rate=55.88235294117647%
top=5: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
top=6: same framework result as top=5 in the current sample
top=7: trades=51, win_rate=54.90196078431373%, total_profit=139.75257156, late_win_rate=54.54545454545454%

entry distance:
distance=3.8: trades=50, win_rate=54.0%, total_profit=142.96200542, late_win_rate=54.54545454545454%
distance=4.0: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
distance=4.2: trades=53, win_rate=52.83018867924528%, total_profit=145.63092117, late_win_rate=51.42857142857142%
distance=4.5: trades=57, win_rate=47.368421052631575%, total_profit=126.53738153, late_win_rate=44.73684210526316%

min_delta_rank with max_delta_rank=72:
min_delta=11: trades=54, win_rate=53.70370370370371%, total_profit=152.59850737, late_win_rate=52.77777777777778%
min_delta=12: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
min_delta=13: trades=49, win_rate=55.10204081632652%, total_profit=143.68879850, late_win_rate=54.83870967741935%
min_delta=15: trades=42, win_rate=59.523809523809526%, total_profit=142.05542221, late_win_rate=57.14285714285714%

max_new_rank / chase threshold:
max_new=28: trades=48, win_rate=56.25%, total_profit=133.26962193, late_win_rate=56.25%
max_new=29: trades=48, win_rate=56.25%, total_profit=136.73673534, late_win_rate=59.375%
max_new=31: trades=55, win_rate=52.72727272727272%, total_profit=144.52277580, late_win_rate=52.77777777777778%
chase_price_change_pct=6.0: same framework result as 8.0 in the current sample
chase_price_change_pct=10.0: same framework result as 8.0 in the current sample
```

Conclusion: keep the `min_delta=12,max_delta=72,target=3.3R` candidate as the
current best research fit for the updated `55%+` objective. It clears the
numeric gates with `52` trades and `151.06922309U`, but q3/q4 still argue
against declaring it production-stable without forward observation.

Continuation added `--equity-concentration-report` so symbol concentration can
be checked inside the same `symbol_isolated_100u` framework replay instead of
with ad hoc shell parsing. The current best research candidate has material
symbol concentration:

```text
framework: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309
remove top 1 positive symbol H-USDT-SWAP:
remaining_trades=49, remaining_win_rate=53.06122448979592%, remaining_total_profit=123.28979515

remove top 3 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,XPL-USDT-SWAP:
removed_profit=72.92816358, removed_share_pct=48.27466646887941%
remaining_trades=43, remaining_win_rate=46.51162790697674%, remaining_total_profit=78.14105951

remove top 5 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,XPL-USDT-SWAP,UNI-USDT-SWAP,KAT-USDT-SWAP:
removed_profit=101.41160282, removed_share_pct=67.12922774330893%
remaining_trades=40, remaining_win_rate=42.5%, remaining_total_profit=49.65762027
```

The inverse test, removing the top three losing symbols
`TON-USDT-SWAP,ASTER-USDT-SWAP,MRVL-USDT-SWAP`, reached `45` trades,
`64.44444444444444%` win rate, `3.07000000%` max drawdown, and
`169.84878971U` profit. Do not promote this directly: it is a historical symbol
blocklist and therefore a likely overfit path unless a non-symbol-specific
reason can be proven.

Narrow-window inspection of representative win/loss rank events did not reveal
a simple robust feature. Losing samples include both high and low `delta_rank`
and both high and low `price_change_pct`; winning samples also include high
`price_change_pct`, so a blanket chase cap is too blunt. Reusing the existing
`chase_top_rank=30` setting as an all-rank price-change cap confirmed this:

```text
all-rank price cap=30: trades=47, win_rate=53.191489361702125%, total_profit=124.80694351
all-rank price cap=25: trades=44, win_rate=54.54545454545454%, total_profit=123.60182010
all-rank price cap=20: trades=38, win_rate=50.0%, total_profit=88.77215627
all-rank price cap=15: trades=37, win_rate=43.24324324324324%, total_profit=73.59497349
all-rank price cap=12: trades=31, win_rate=41.935483870967744%, total_profit=47.99213226
all-rank price cap=10: trades=26, win_rate=46.15384615384615%, total_profit=49.13403550
all-rank price cap=8: trades=25, win_rate=48.0%, total_profit=50.83693063
```

Updated conclusion: the candidate still meets the user's numeric goal, but the
symbol concentration report strengthens the anti-overfit caveat. Treat it as a
paper-observation candidate, not a production default. The next useful research
step is not a symbol blocklist; it is finding a generic reason for
`TON/ASTER/MRVL`-type failures that does not remove the high-conviction winners.

2026-06-21 latest target adjustment: the active research gate is now framework
win rate `>=55%`, high total profit, non-small trade count, and low drawdown.
The current local sample used `58` candle pairs and `71317` raw rank events:

```text
stage_counts: raw=71317, trend_pass=63730, trend_blocked=7587, entry_pass=4033, entry_blocked=59697
entry_trigger_filter: before=4033, after=3620, allowlist=breakout_previous_high,reclaim_ema
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
```

The candidate still clears the numeric gate, but the feature report shows why it
should remain research-only:

```text
delta_rank=12_24: trades=32, win_rate=50.0%, total_profit=68.21089671
delta_rank=25_48: trades=20, win_rate=65.0%, total_profit=86.98804181
delta_rank=49_plus: trades=9, win_rate=55.55555555555556%, total_profit=28.82221074
new_rank=1_10: trades=7, win_rate=42.857142857142854%, total_profit=9.68666625
new_rank=11_20: trades=24, win_rate=58.333333333333336%, total_profit=66.32411145
new_rank=21_30: trades=28, win_rate=42.857142857142854%, total_profit=55.10172542
price_change_pct=lt5: trades=16, win_rate=37.5%, total_profit=13.40311751
price_change_pct=5_10: trades=16, win_rate=50.0%, total_profit=41.48305379
price_change_pct=10_20: trades=17, win_rate=58.82352941176471%, total_profit=56.53124763
price_change_pct=20_plus: trades=17, win_rate=64.70588235294117%, total_profit=69.12938275
```

Generic perturbations did not find a better replacement. `max_new_rank=25`
reduced profit to `112.77547025U`, `max_new_rank=20` dropped below the
`30`-trade gate, `min_delta_rank=15` improved win rate to
`59.523809523809526%` but only reached `142.05542221U`, and raising `target_r`
above `3.3R` lowered profit. Profit-protect sweeps also kept the current
`after=2.8R, stop=2.0R` as the only setting in this group that reached
`150U+` while preserving `>=55%` win rate.

Continuation added research-only `--min-price-change-pct` after the feature
report showed weak standalone results for `price_change_pct < 5`. This is a
generic event filter, not a symbol blocklist, and it is locked out of the
production preset override path. The scan showed it improves win rate and
time-split stability but gives up too much total profit to replace the current
`151.06922309U` research candidate:

```text
min_price_change_pct=2: trades=49, win_rate=57.14285714285714%, total_profit=145.66332704, late_win_rate=56.25%, q3_win_rate=43.75%, q4_win_rate=50.0%
min_price_change_pct=5: trades=42, win_rate=59.523809523809526%, total_profit=143.43244199, late_win_rate=58.620689655172406%, q3_win_rate=56.25%, q4_win_rate=60.0%
min_price_change_pct=8: trades=33, win_rate=63.63636363636363%, total_profit=123.13295089
min_price_change_pct=10: trades=31, win_rate=64.51612903225806%, total_profit=117.51299616
```

The best conservative branch was to combine the minimum price-change filter with
a slightly wider rank-jump floor:

```text
min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3:
candle_pairs=54, raw_candidate_events=59867, entry_pass=3378, entry_trigger_after=3012
framework: symbols=27, trades=45, win_rate=57.77777777777777%, trade_sharpe=3.7652834461775884, max_drawdown_pct=7.55972182, total_profit=149.14914524
late: trades=34, win_rate=58.82352941176471%, total_profit=105.93154004
q1: trades=3, win_rate=33.33333333333333%, total_profit=0.50918469
q2: trades=10, win_rate=60.0%, total_profit=48.11488234
q3: trades=19, win_rate=57.89473684210527%, total_profit=77.68677536
q4: trades=16, win_rate=56.25%, total_profit=23.77040977
```

This branch is more time-stable than the `151U` candidate, but it still misses
the `150U` objective and does not solve symbol concentration:

```text
remove top 3 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,UNI-USDT-SWAP:
removed_profit=68.29615335, removed_share_pct=45.79050938858432%, remaining_trades=37, remaining_win_rate=48.64864864864865%, remaining_total_profit=80.85299189
```

Target-R neighbors did not recover the missing profit: `3.32R` dropped to
`146.15390720U`, `3.35R` to `143.50746025U`, `3.4R` to `140.94903225U`, and
`3.5R` to `144.97882742U`. Do not promote this as the main candidate; keep it
as a conservative forward-observation branch if the next priority becomes
time-split stability over total profit.

After the target was relaxed to `win_rate >= 55%` with higher priority on
profit, trade count, and low drawdown, the CLI gained
`--equity-symbol-window-report`. It replays the same `symbol_isolated_100u`
framework equity report inside the existing quartile windows and prints the top
profit symbols per window. This is a diagnostic only; it does not change entry
or exit behavior.

Current `150U` profit candidate:

```text
params: min_delta_rank=12, max_delta_rank=72, target_r=3.3, no min_price_change_pct
candle_pairs=58, raw_candidate_events=71352, entry_pass=4033
paper 24h: trades=52, resolved_win_rate=55.00000000000001%, avg_r_complete=0.8866904314838523
paper 48h: trades=51, resolved_win_rate=59.09090909090909%, avg_r_complete=1.1065604654229744
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.67828893
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
top window contributors: q1=H, q2=H/HOME/ENA, q3=XPL/JTO/KAT, q4=UNI/JTO/BICO
remove top 3 positive symbols H,JTO,XPL: removed_share_pct=48.27466646887941%, remaining_trades=43, remaining_win_rate=46.51162790697674%, remaining_total_profit=78.14105951
```

This is the only branch currently above `150U` while meeting `>=55%` framework
win rate and `>=30` trades, but it is still not clean enough for promotion:
`q3` and `q4` are weak by win rate, and the top three symbols explain nearly
half of profit.

Current conservative branch:

```text
params: min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3
candle_pairs=54, raw_candidate_events=59880, entry_pass=3378
paper 24h: trades=44, resolved_win_rate=55.55555555555556%, avg_r_complete=1.0509472765480348
paper 48h: trades=44, resolved_win_rate=56.09756097560976%, avg_r_complete=1.0844600256157642
framework: symbols=27, trades=45, win_rate=57.77777777777777%, trade_sharpe=3.7652834461775884, max_drawdown_pct=7.55972182, total_profit=149.14914524
q1: trades=3, win_rate=33.33333333333333%, total_profit=0.50918469
q2: trades=10, win_rate=60.0%, total_profit=48.11488234
q3: trades=19, win_rate=57.89473684210527%, total_profit=77.68677536
q4: trades=16, win_rate=56.25%, total_profit=23.77040977
top window contributors: q1=INJ, q2=H/BNB/ENA, q3=JTO/KAT/TAO, q4=UNI/JTO/BICO
remove top 3 positive symbols H,JTO,UNI: removed_share_pct=45.79050938858432%, remaining_trades=37, remaining_win_rate=48.64864864864865%, remaining_total_profit=80.85299189
```

This branch is the better forward-observation candidate under the new `55%`
target because q2/q3/q4 all clear `55%`, Sharpe is higher, drawdown remains
low, and trades stay at `45`. It misses `150U` by `0.85085476U`, so do not call
the optimization complete. Parameter checks around it did not find a better
generic replacement:

```text
target_r scan with min_delta=11,min_price=5: 3.3R remains best; 3.1R=139.19753118, 3.2R=144.17146091, 3.25R=146.65983375, 3.35R=143.50746025, 3.4R=140.94903225, 3.5R=144.97882742
min_price scan with min_delta=11/12: lower min_price increases weak late-window trades or lowers win rate; min_delta=12,min_price=5 improves win_rate to 59.523809523809526% but drops to 42 trades and 143.43244199U
min_delta scan at min_price=5: min_delta=8 gives 62 trades but only 50.0% win; min_delta=15 gives 35 trades and 65.71428571428571% win but only 143.07318759U; min_delta=20/25 are too few or too low profit
max_new_rank scan at min_delta=11,min_price=5: tightening to 24/26/28 drops profit to 96.13386384/113.25882880/130.09621907U; keep max_new_rank=30
```

`--profit-protect-*` now changes `symbol_isolated_100u` framework equity replay
through same-side framework stop updates and is covered by focused tests,
including a conservative guard that refuses to place a new protected stop if the
trigger candle closes below that stop. `--runner-*` and `--stop-reentry-mode`
still only affect the CLI outcome simulation and should not be used as framework
equity evidence until mapped into the existing framework risk system.

`avg_r_complete` excludes incomplete rows, matching the Core CLI result output.
The Web summary API `avg_result_r` includes all non-null `result_r`, including
incomplete rows, so that value is lower and should not be compared directly to
CLI `avg_r_complete`.

2026-06-21 chase filter audit: the `new_rank <= 5 &&
price_change_pct >= 8` exclusion was originally a generic anti-chase guard. Its
purpose was to avoid entries that had already jumped into the top ranks after a
large same-event price move. The latest local evidence shows that this guard is
too blunt for the relaxed target. With the conservative branch unchanged except
for disabling the chase filter via `--chase-top-rank 0`, the framework replay
crossed the profit target but weakened q4:

```text
min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3, chase disabled
candle_pairs=54, raw_candidate_events=64439, entry_pass=3478, entry_trigger_after=3112
framework: symbols=27, trades=47, win_rate=57.446808510638306%, trade_sharpe=3.7811324199202496, max_drawdown_pct=7.55972182, total_profit=151.65980730
q1: trades=4, win_rate=50.0%, total_profit=6.43918469
q2: trades=9, win_rate=55.55555555555556%, total_profit=38.28488234
q3: trades=20, win_rate=60.0%, total_profit=87.43579249
q4: trades=17, win_rate=52.94117647058824%, total_profit=20.50068431
```

The 48h paper-outcome diff against the filtered branch added only two settled
events:

```text
LAB-USDT-SWAP, new_rank=4, delta_rank=21, price_change_pct=54.09166054, reclaim_ema, +2.0R
ALLO-USDT-SWAP, new_rank=5, delta_rank=33, price_change_pct=97.48468656, reclaim_ema, -1.0R
```

So the old overheat filter did not simply remove bad trades; it removed one
good high-momentum follow-through and one bad chase. A blanket top-rank
price-change cap is therefore not the right generic explanation.

The better simplified research candidate is to remove the chase filter and
raise the generic rank-jump floor instead:

```text
params: min_delta_rank=13, max_delta_rank=72, max_new_rank=30, min_price_change_pct=5, target_r=3.3, chase disabled, no profit-protect
candle_pairs=54, raw_candidate_events=56375, entry_pass=2993, entry_trigger_after=2671
paper 24h: trades=41, resolved_win_rate=55.88235294117647%, avg_r_complete=1.3408920140693348
paper 48h: trades=41, resolved_win_rate=56.41025641025641%, avg_r_complete=1.4162710947769892
framework: symbols=27, trades=43, win_rate=60.46511627906976%, trade_sharpe=4.034706652682123, max_drawdown_pct=7.55972182, total_profit=161.05575703
early: trades=15, win_rate=60.0%, total_profit=70.65064943
late: trades=29, win_rate=58.620689655172406%, total_profit=85.05309245
q1: trades=5, win_rate=60.0%, total_profit=18.55417809
q2: trades=11, win_rate=63.63636363636363%, total_profit=62.22825234
q3: trades=16, win_rate=56.25%, total_profit=67.19231019
q4: trades=15, win_rate=60.0%, total_profit=23.77921676
```

Adjacent checks support this as a region rather than a single fragile point.
`min_delta_rank=13,target=3.3` without chase but with `profit_protect_after=2.8R`
still produced `148.56271569U` and all quartiles above `56%`; moving the
profit-protect activation to `3.0R` produced `157.16714096U`. Setting no
profit-protect matched late activation near the target, so the cleanest version
is the no-protect command above.

Remaining caveat: symbol concentration is still material. Removing the top three
positive symbols `H-USDT-SWAP,JTO-USDT-SWAP,UNI-USDT-SWAP` removes
`73.00057862U` (`45.326277042410126%` of profit), leaving `35` trades,
`51.42857142857142%` win rate, and `88.05517841U` profit. This candidate is
the current best research fit for the user's relaxed objective, but it should
still go through forward paper observation before being promoted as a production
default.

Follow-up neighbor scans did not find a cleaner replacement. Trigger replay
confirmed that `breakout_previous_high` is the stable trigger and `reclaim_ema`
is a lower-win-rate profit extender:

```text
breakout_previous_high standalone: trades=30, win_rate=60.0%, total_profit=110.86083951
reclaim_ema standalone: trades=21, win_rate=47.61904761904761%, total_profit=59.50537895
combined sequence replay: trades=43, win_rate=60.46511627906976%, total_profit=161.05575703
```

Generic neighbor scans around the simplified candidate showed:

```text
target_r: 3.2R=155.39358913U, 3.3R=161.05575703U, 3.4R=146.27300167U, 3.5R=151.26762696U but q3 win_rate=46.666666666666664%, 3.6R full win_rate=54.761904761904766%
max_new_rank: 24/26/28 all reduced profit to 119.74811540U / 136.84811476U / 144.26045117U; keep 30
min_price_change_pct: 6/8/10/12 improved some headline win rates but reduced profit to 145.59392074U / 137.33840578U / 128.72304104U / 134.86304104U
entry_max_distance_pct: 3.5 improved win rate but only 147.10092705U; 4.5/5.0/6.0 added trades but degraded full/q4 win rate; keep 4.0
entry_min_volume_ratio: 0.0/0.8 added many low-quality trades and hurt q4; 1.2/1.5 over-filtered and collapsed profit; keep 1.0
```

Current interpretation: the best simplification is not an overheat chase cap,
not profit-protect tuning, and not trigger removal. It is a stricter generic
rank-jump floor (`min_delta_rank=13`) plus the existing distance and volume
quality gates. The unresolved weakness is still cross-symbol concentration, not
time-split instability.

2026-06-21 refresh scan against the live local `quant_core` sample confirmed
the same direction. The host-side CLI connection issue was environmental:
`docker-compose.yml` still documents `example`, but the current persistent
Postgres volume uses the older local password from the repo runbooks. The replay
was run with the current working connection string and the same
`symbol_isolated_100u` framework path.

```text
no_chase_filter:
  candle_pairs=54, raw_candidate_events=56406, entry_trigger_after=2671
  trades=43, symbols=27, win_rate=60.46511627906976%, trade_sharpe=4.034706652682123, max_drawdown_pct=7.55972182, total_profit=161.05575703
  q1/q2/q3/q4 win_rate=60.0%/63.63636363636363%/56.25%/60.0%

old_chase_filter (new_rank<=5 and price_change_pct>=8 excluded):
  candle_pairs=54, raw_candidate_events=52671, entry_trigger_after=2571
  trades=41, symbols=27, win_rate=60.97560975609756%, trade_sharpe=3.972352350037274, max_drawdown_pct=7.55972182, total_profit=154.65647890
  q1/q2/q3/q4 win_rate=50.0%/63.63636363636363%/60.0%/53.333333333333336%
```

The old chase filter still improves headline win rate by only `0.510493477%`
while removing two trades and reducing profit by `6.39927813U`. It also weakens
q1 and q4. Therefore the reason to keep it is not supported by the current
sample; it is a conservative anti-chase guard, not an optimal strategy filter.

The "make it more nuanced" tail-rank hypothesis also did not beat the simple
candidate. The tested rule was: if `new_rank` is in the tail bucket, require a
higher same-event `price_change_pct`; otherwise keep the base filters.

```text
tail21_price8:  trades=38, win_rate=63.1578947368421%,  sharpe=4.10559024022797,  max_dd=7.55972182, profit=153.28930022, q2_win=44.44444444444444%, q4_win=50.0%
tail21_price10: trades=37, win_rate=62.16216216216216%, sharpe=3.854419026876245, max_dd=7.55972182, profit=141.69428160, q2_win=37.5%, q4_win=53.333333333333336%
tail21_price12: trades=36, win_rate=63.888888888888886%, sharpe=4.014699619115618, max_dd=7.55972182, profit=144.76428160, q2_win=37.5%, q4_win=57.14285714285714%
tail21_price15: trades=35, win_rate=62.857142857142854%, sharpe=3.7042337773056997, max_dd=7.74122662, profit=129.79273953, q2_win=44.44444444444444%, q4_win=53.84615384615385%
tail21_price20: trades=29, win_rate=62.06896551724138%, sharpe=3.2278521885887272, max_dd=7.74122662, profit=101.25662535, min_trades=false

tail25_price8:  trades=40, win_rate=60.0%,              sharpe=3.9374199357825668, max_dd=7.55972182, profit=150.32145082, q2_win=42.857142857142854%, q4_win=53.333333333333336%
tail25_price10: trades=39, win_rate=58.97435897435898%, sharpe=3.6943993089982214, max_dd=7.55972182, profit=138.72643220, q2_win=33.33333333333333%, q4_win=50.0%
tail25_price12: trades=38, win_rate=60.526315789473685%, sharpe=3.8411986383922385, max_dd=7.55972182, profit=141.79643220, q2_win=37.5%, q4_win=53.333333333333336%
tail25_price15: trades=38, win_rate=60.526315789473685%, sharpe=3.7946510879089135, max_dd=7.55972182, profit=139.77112752, q2_win=33.33333333333333%, q4_win=53.333333333333336%
```

The tail-rank variants can raise headline win rate, but they either lose
`7.76645681U` to `59.79913168U` versus the simple no-chase candidate, or drop
below the 30-trade floor. They also introduce a poor q2 split. The current best
tradeoff remains:

```text
min_delta_rank=13
max_delta_rank=72
max_new_rank=30
min_price_change_pct=5
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
entry_max_distance_pct=4.0
entry_min_volume_ratio=1.0
target_r=3.3
stop_loss_pct=0.03
chase_top_rank=0
stop_reentry_mode=off
profit_protect=off
```

The next scan tested whether the improvement should come from exit geometry
instead of more entry filtering. With the same no-chase entry set, changing
`stop_loss_pct` changes both the stop price and the target price used by the
framework replay, so it is a real risk/exit change rather than a cosmetic R
rescale.

Coarse stop/target grid:

```text
stop_loss=0.025:
  best nearby: target=3.5R, trades=48, win_rate=52.083333333333336%, sharpe=3.0143711900292396, max_dd=7.19385133, profit=105.20204509
  verdict: too many low-quality exits; below 55% win-rate and far below profit target.

stop_loss=0.030:
  target=3.2R, trades=43, win_rate=60.46511627906976%, sharpe=3.991091233539193, max_dd=7.55972182, profit=155.39358913
  target=3.3R, trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
  target=3.4R, trades=42, win_rate=57.14285714285714%, sharpe=3.721401095957837, max_dd=7.55972182, profit=146.27300167
  verdict: 3.3R is the Sharpe-first local optimum.

stop_loss=0.035:
  target=2.8R, trades=42, win_rate=64.28571428571429%, sharpe=3.9438887488027543, max_dd=8.51094850, profit=161.13258507
  target=2.9R, trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  verdict: slight profit lift, but higher drawdown and lower Sharpe than 0.03/3.3R.

stop_loss=0.0375:
  target=2.6R, trades=42, win_rate=61.904761904761905%, sharpe=3.958164886224208, max_dd=8.98471703, profit=162.12393256
  target=2.7R, trades=41, win_rate=60.97560975609756%, sharpe=3.9587117307028654, max_dd=8.98471703, profit=164.05047908
  verdict: best profit found in this scan, but it pays for the extra 2.99472205U
  versus the base with higher drawdown and lower Sharpe.

stop_loss=0.040:
  best nearby: target=2.4R, trades=42, win_rate=57.14285714285714%, sharpe=3.717357630927375, max_dd=9.45725569, profit=153.26828599
  verdict: wider stop becomes worse on win-rate, Sharpe, and profit.
```

Full replay on the two wider-stop challengers:

```text
base_sharpe 0.030/3.3R:
  trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
  early/late win_rate=60.0%/58.620689655172406%, profit=70.65064943/85.05309245
  q1/q2/q3/q4 win_rate=60.0%/63.63636363636363%/56.25%/60.0%
  remove top3 positives: remaining_trades=35, remaining_win_rate=51.42857142857142%, remaining_profit=88.05517841

wider_stop_profit_mid 0.035/2.9R:
  trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  early/late win_rate=66.66666666666666%/59.25925925925925%, profit=82.84156449/74.31425185
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/57.14285714285714%/60.0%
  remove top3 positives: remaining_trades=33, remaining_win_rate=54.54545454545454%, remaining_profit=89.04428246

wider_stop_profit_high 0.0375/2.7R:
  trades=41, win_rate=60.97560975609756%, sharpe=3.9587117307028654, max_dd=8.98471703, profit=164.05047908
  early/late win_rate=66.66666666666666%/55.55555555555556%, profit=81.03428123/77.47794172
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/57.14285714285714%/53.333333333333336%
  remove top3 positives: remaining_trades=33, remaining_win_rate=51.515151515151516%, remaining_profit=91.35378129
```

Current decision after the exit scan:

- Sharpe-first / balanced default remains `0.03 stop_loss_pct + 3.3R target`.
- Profit-first research candidate is `0.0375 stop_loss_pct + 2.7R target`, but
  it is not a strict upgrade because drawdown rises to `8.98471703%`, Sharpe
  falls below the base, and q4 win-rate drops to `53.333333333333336%`.
- A more conservative profit-lift candidate is `0.035 stop_loss_pct + 2.9R
  target`: it improves top-line win-rate and keeps q4 at `60%`, but the extra
  profit is only `1.48094976U` over the base with higher drawdown.

Therefore the best current framing is two presets, not one forced replacement:
keep `0.03/3.3R` as the cleaner Sharpe/default candidate, and track
`0.0375/2.7R` as the PnL-seeking paper-observation challenger.

Profit protection was then tested as a way to make the wider-stop challenger
less fragile. This is still framework-level per-symbol replay: once a trade has
moved far enough in R, the strategy emits an updated stop-loss signal for that
same symbol position. It does not use portfolio-level capital sharing.

Base `0.03/3.3R` with protection:

```text
after=1.4, stop=0.4: trades=49, win_rate=67.3469387755102%, sharpe=3.415977215043625, max_dd=7.55972182, profit=116.46887617
after=1.4, stop=0.8: trades=49, win_rate=65.3061224489796%, sharpe=3.4194074957603187, max_dd=7.55972182, profit=105.06698143
after=1.4, stop=1.2: trades=48, win_rate=66.66666666666666%, sharpe=3.899867399886567, max_dd=7.55972182, profit=104.41601312
after=1.8, stop=0.4: trades=45, win_rate=62.22222222222222%, sharpe=3.646954878203686, max_dd=7.55972182, profit=135.58149312
after=1.8, stop=0.8: trades=45, win_rate=62.22222222222222%, sharpe=3.340548963984261, max_dd=7.55972182, profit=113.73320805
after=1.8, stop=1.2: trades=43, win_rate=62.7906976744186%, sharpe=3.076584016385959, max_dd=7.55972182, profit=88.79874356
after=2.2, stop=0.4: trades=44, win_rate=59.09090909090909%, sharpe=3.451892131094753, max_dd=7.55972182, profit=129.89569571
after=2.6, stop=0.4: trades=43, win_rate=60.46511627906976%, sharpe=3.676393389467742, max_dd=7.55972182, profit=141.88666482
```

Profit-first `0.0375/2.7R` with protection:

```text
after=1.2, stop=0.3: trades=47, win_rate=65.95744680851064%, sharpe=3.1680416505272184, max_dd=8.98471703, profit=113.21438375
after=1.5, stop=0.3: trades=44, win_rate=63.63636363636363%, sharpe=3.343959665730574, max_dd=8.98471703, profit=126.63852746
after=1.8, stop=0.3: trades=43, win_rate=60.46511627906976%, sharpe=3.4093688044223636, max_dd=8.98471703, profit=138.43910982
after=2.1, stop=0.3: trades=42, win_rate=61.904761904761905%, sharpe=3.498088361812747, max_dd=8.98471703, profit=140.66861461
after=2.1, stop=0.6: trades=42, win_rate=61.904761904761905%, sharpe=3.296955985056664, max_dd=8.98471703, profit=128.82913145
after=2.1, stop=1.2: trades=42, win_rate=61.904761904761905%, sharpe=3.321762933561542, max_dd=8.98471703, profit=123.47347713
```

The protection layer raises headline win-rate in some cases, but it does not
reduce the max drawdown on this replay and it gives up too much profit. It is
therefore not the right fix for the wider-stop candidate. The remaining
tradeoff is unchanged: `0.03/3.3R` is cleaner on Sharpe and drawdown;
`0.0375/2.7R` is a PnL-seeking challenger with higher drawdown.

2026-06-21 further replay added a framework trade-level report
(`--equity-trade-report`) so overheat attribution can use the actual
`symbol_isolated_100u` closed trades instead of standalone feature-bucket
replays. The exact full-combination attribution for the simple `0.03/3.3R`
candidate was:

```text
all trades: trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
new_rank<=5 and price_change_pct>=8: trades=2, wins=1, losses=1, win_rate=50.0%, profit=6.45821900
other trades: trades=41, wins=25, losses=16, win_rate=60.97560975609756%, profit=154.59753804
```

Both overheat trades were `reclaim_ema` entries. The old broad filter
`new_rank<=5 && price_change_pct>=8` removes one winner and one loser, but the
net removed bucket is positive. That explains why the broad chase filter raises
headline win-rate slightly while reducing total PnL. It is not a good optimal
filter on the current sample.

A refined chase scan kept the same entry set and only changed
`chase_top_rank/chase_price_change_pct`:

```text
no_chase:     trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
top3_pct8-80: identical closed-trade metrics to no_chase; no actual trade removed
top5_pct8:   trades=41, win_rate=60.97560975609756%, sharpe=3.972352350037274, max_dd=7.55972182, profit=154.65647890
top5_pct50:  trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
top5_pct80:  trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
top10_pct8:  trades=37, win_rate=59.45945945945946%, sharpe=3.821499961127567, max_dd=7.55972182, profit=146.43071424
top10_pct80: trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
```

The useful nuance is therefore not "top rank plus 8% is too hot"; it is only
the very extreme same-event jump. In the current closed-trade set, `top5_pct80`
removes `ALLO-USDT-SWAP` event `1653251` (`new_rank=5`,
`price_change_pct=102.08968412`, `reclaim_ema`, `-3.37178100U`) and slightly
improves a later same-symbol trade because the symbol lock no longer starts
from the losing entry.

Combining the refined extreme-chase filter with the exit geometry scan produced
two stronger candidates:

```text
balanced_extreme_chase 0.035/2.9R + top5_pct80:
  trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380
  early/late win_rate=69.23076923076923%/64.28571428571429%, profit=76.69142049/98.09566180
  q1/q2/q3/q4 win_rate=60.0%/77.77777777777779%/66.66666666666666%/53.333333333333336%
  remove top3 positives: remaining_trades=32, remaining_win_rate=56.25%, remaining_profit=92.98893946
  remove top5 positives: remaining_trades=30, remaining_win_rate=53.333333333333336%, remaining_profit=70.27828031

pnl_extreme_chase 0.0375/2.7R + top5_pct80:
  trades=40, win_rate=62.5%, sharpe=4.13105376665384, max_dd=8.98471703, profit=168.25947388
  early/late win_rate=69.23076923076923%/60.71428571428571%, profit=75.18338223/101.42409295
  q1/q2/q3/q4 win_rate=60.0%/77.77777777777779%/66.66666666666666%/46.666666666666664%
  remove top3 positives: remaining_trades=32, remaining_win_rate=53.125%, remaining_profit=95.56277608
  remove top5 positives: remaining_trades=30, remaining_win_rate=50.0%, remaining_profit=72.90269584
```

Current decision: use `0.035/2.9R + chase_top_rank=5 +
chase_price_change_pct=80` as the stronger balanced research candidate. It
beats the previous simple default on PnL, win-rate, and post-top3 concentration,
while keeping drawdown far below 30%. The `0.0375/2.7R + top5_pct80` version is
the pure-PnL challenger, but q4 falls below 50%, so it should not replace the
balanced candidate without more forward paper evidence. This refined filter is
still sample-small and mostly justified by one extreme ALLO loss; treat it as a
paper-observation candidate, not production proof.

Follow-up robustness scans kept the balanced candidate as the current best
tradeoff. A full-combination q4 trade-level cut showed the weakest recent bucket
was low same-event movement:

```text
q4 actual closed trades after entry_ts>=1781582400000:
  trades=13, wins=8, losses=5, win_rate=61.53846153846154%, profit=22.48072370
  price_change 5_10: trades=4, wins=1, losses=3, win_rate=25.0%, profit=-10.48967040
  price_change 10_20: trades=4, wins=4, losses=0, win_rate=100.0%, profit=16.57867767
  price_change 20_plus: trades=5, wins=3, losses=2, win_rate=60.0%, profit=16.39171643
```

Raising the global `min_price_change_pct` improves q4 but over-filters the full
sample:

```text
min_price=5:  trades=40, win_rate=65.0%,              sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%, top3_removed_profit=92.98893946
min_price=6:  trades=36, win_rate=66.66666666666666%, sharpe=3.9546222252002257, max_dd=8.51094850, profit=151.52527679, q4_win=66.66666666666666%, top3_removed_profit=78.03285245
min_price=8:  trades=32, win_rate=65.625%,            sharpe=3.7038963246218675, max_dd=6.89059169, profit=131.07163927, q4_win=58.333333333333336%, top3_removed_profit=59.83404964
min_price=10: trades=30, win_rate=66.66666666666666%, sharpe=3.6256665960556225, max_dd=3.57000000, profit=122.68291102, q4_win=66.66666666666666%, top3_removed_profit=60.89200500
```

Tail-rank variants were also weaker than the balanced candidate. The best
tail-filtered profit in this pass was `tail25_price8` with `156.40637991U`,
still below `166.48136380U`, and most tail variants either reduced q4 or cut too
many trades.

Entry-quality neighbor scans did not improve the tradeoff either:

```text
distance=3.5, volume=1.0: trades=35, win_rate=65.71428571428571%, sharpe=3.8183571920867347, max_dd=8.51094850, profit=150.46332669, q4_win=58.333333333333336%
distance=4.0, volume=1.0: trades=40, win_rate=65.0%,              sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%
distance=5.0, volume=1.0: trades=45, win_rate=60.0%,              sharpe=3.7363653728534647, max_dd=8.51094850, profit=164.74915474, q4_win=50.0%
distance=4.0, volume=0.8: trades=49, win_rate=55.10204081632652%, sharpe=3.097666076354121,  max_dd=9.31258359, profit=147.11458866, q4_win=33.33333333333333%
distance=4.0, volume=1.2: trades=35, win_rate=54.285714285714285%, sharpe=2.869615487369861, max_dd=6.89059169, profit=109.44354870, q4_win=63.63636363636363%
```

`stop_reentry_mode=breakout_reclaim` was a no-op on this candidate: it produced
the same `40` trades, `65.0%` win rate, `166.48136380U` profit, Sharpe
`4.096343901750493`, and `8.51094850%` max drawdown as `stop_reentry_mode=off`.

Current practical decision is unchanged: keep `0.035/2.9R + top5_pct80` as the
balanced paper candidate; keep `0.0375/2.7R + top5_pct80` as a profit-only
challenger; do not raise the global price-change floor or tighten entry distance
unless forward paper data confirms the q4 low-price-change weakness persists.

2026-06-21 follow-up refresh with the current local snapshot kept the same
decision. The active data now spans `54` candle pairs and `55,422` filtered
candidate events for this parameter family, from
`2026-05-29 05:57:58.203531+00` to `2026-06-21 10:29:15.44423+00`. The closed
trade report for the balanced candidate spans `40` trades from
`2026-05-30 12:30:00` to `2026-06-21 09:45:00` Beijing time.

The latest trade-level overheat attribution makes the old
`new_rank <= 5 && price_change_pct >= 8` filter look too broad:

```text
no_chase, 0.035/2.9R:
  trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  bucket new_rank<=5 && price_change_pct>=8:
    LAB-USDT-SWAP rank=4 price_change=48.87948597 reclaim_ema +10.08000000
    ALLO-USDT-SWAP rank=5 price_change=102.08968412 reclaim_ema -3.92985600
    bucket_total=+6.15014400, bucket_win_rate=50.0%

old_filter_top5_pct8:
  trades=39, win_rate=64.1025641025641%, sharpe=3.888392315439138, max_dd=8.51094850, profit=156.53420633

current_filter_top5_pct80:
  trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380
  remaining old bucket after current filter:
    LAB-USDT-SWAP rank=4 price_change=48.87948597 reclaim_ema +10.08000000
```

So the `8%` cap removes a net-positive high-rank bucket. The narrower `80%`
cap removes only the extreme ALLO failure in this closed-trade set and keeps the
LAB follow-through win. This is still not production proof because the
justification is small-sample and mostly one loser, but it is a better paper
candidate than the old blanket `8%` anti-chase rule.

Profit-protection scans did not improve the balanced candidate under the same
framework equity replay. Early protection raised headline win rate but cut too
much trend follow-through:

```text
base_no_protect:              trades=40, win_rate=65.0%,              sharpe=4.096343901750493,  max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%, top3_removed_profit=92.98893946
protect_after_1.4_stop_0.3:  trades=44, win_rate=68.18181818181817%, sharpe=3.5353705792456154, max_dd=8.51094850, profit=131.82143204, q4_win=52.94117647058824%,  top3_removed_profit=82.16644074
protect_after_2.2_stop_0.0:  trades=41, win_rate=65.0%,              sharpe=3.739914211836979,  max_dd=8.51094850, profit=149.46615467, q4_win=53.333333333333336%, top3_removed_profit=88.18827758
protect_after_2.6_stop_0.0:  trades=41, win_rate=65.85365853658537%, sharpe=3.96079296532936,   max_dd=8.51094850, profit=161.68070192, q4_win=53.333333333333336%, top3_removed_profit=88.18827758
protect_after_2.6_stop_1.0:  trades=41, win_rate=65.85365853658537%, sharpe=3.9425845966679858, max_dd=8.51094850, profit=158.68250845, q4_win=53.333333333333336%, top3_removed_profit=85.19008411
```

No tested protection setting beat `166.48136380U`, and max drawdown stayed
unchanged at `8.51094850%`, so protection is not buying a meaningful risk
reduction here. The runner flags were also sanity checked and currently leave
`framework_equity_result` unchanged (`40` trades, `65.0%`, `166.48136380U`) for
sample `4R/4.5R/5R` runner settings. Do not use runner output as comparable
framework evidence until partial-runner behavior is mapped into the existing
Vegas-style replay path.

A refreshed stop/target neighborhood scan also kept the same balanced
candidate:

```text
0.0325/3.1R: trades=40, win_rate=62.5%, sharpe=3.983635272132361,  max_dd=8.03595010, profit=157.91230058, q4_win=53.333333333333336%
0.0350/2.7R: trades=41, win_rate=65.85365853658537%, sharpe=4.046050711002911,  max_dd=8.51094850, profit=158.12318006, q4_win=53.333333333333336%
0.0350/2.9R: trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%
0.0375/2.7R: trades=40, win_rate=62.5%, sharpe=4.13105376665384,  max_dd=8.98471703, profit=168.25947388, q4_win=46.666666666666664%
0.0400/2.5R: trades=40, win_rate=57.49999999999999%, sharpe=3.902078264206322, max_dd=9.45725569, profit=160.28097721, q4_win=40.0%
```

`0.0375/2.7R` remains the pure-PnL challenger, but it fails the robustness smell
test because q4 win rate is below `50%` and post-top3 concentration win rate is
only `53.125%`. Keep `0.035/2.9R + chase_top_rank=5 +
chase_price_change_pct=80` as the current balanced research candidate.

2026-06-21 later scan: widening the 15m entry distance, then adding a
trigger/rank research blocklist, produced a stronger candidate without adding
symbol-specific rules. The first step was to relax `entry_max_distance_pct`
from `4.0` to the `5.25-5.75` neighborhood and re-scan exits:

```text
distance=5.25, stop=0.0375, target=2.7R: trades=45, win_rate=62.22222222222222%, sharpe=3.81281391465377,  max_dd=8.98471703, profit=176.35564812, q4_win=57.14285714285714%
distance=5.50, stop=0.0375, target=2.7R: trades=46, win_rate=63.04347826086957%, sharpe=3.9102594530141572, max_dd=8.98471703, profit=180.69231859, q4_win=57.14285714285714%
distance=5.75, stop=0.0375, target=2.7R: trades=48, win_rate=60.416666666666664%, sharpe=3.717325826337162, max_dd=8.98471703, profit=178.40968598, q4_win=53.333333333333336%
distance=6.00, stop=0.0375, target=2.7R: trades=49, win_rate=59.183673469387756%, sharpe=3.625912947690713, max_dd=8.98471703, profit=176.02193582, q4_win=50.0%
```

`5.5/0.0375/2.7R` is therefore a better high-trade-count base than the earlier
`4.0/0.035/2.9R` candidate, but its trade-level attribution showed one weak
bucket:

```text
reclaim_ema + new_rank 11-20:
  trades=7, wins=2, win_rate=28.571428571428573%, profit=-18.55061472
```

To test that as a reusable research filter, the CLI now supports
`--entry-trigger-rank-blocklist trigger:min-max`, for example
`--entry-trigger-rank-blocklist reclaim_ema:11-20`. This only filters confirmed
events in `market_velocity_event_backtest`; it is not part of the production
paper observer preset.

The conservative blocklist candidate improves both return and risk:

```text
params:
  stop_loss_pct=0.0375
  target_r=2.7
  entry_max_distance_pct=5.5
  entry_min_volume_ratio=1.0
  min_delta_rank=13
  max_delta_rank=72
  max_new_rank=30
  min_price_change_pct=5
  chase_top_rank=5
  chase_price_change_pct=80
  entry_trigger_allowlist=breakout_previous_high,reclaim_ema
  entry_trigger_rank_blocklist=reclaim_ema:11-20

result:
  trades=43, win_rate=62.7906976744186%, sharpe=4.131473618558458, max_dd=7.37274802, profit=184.26477392
  early/late win_rate=58.333333333333336%/65.625%, profit=55.12916184/138.78470167
  q1/q2/q3/q4 win_rate=66.66666666666666%/50.0%/68.42105263157895%/57.14285714285714%
  remove top3 positives: remaining_trades=36, remaining_win_rate=61.111111111111114%, remaining_profit=127.71674999
  remove top5 positives: remaining_trades=32, remaining_win_rate=59.375%, remaining_profit=98.66855857
```

Neighbor checks suggest the idea is not a single-threshold accident:

```text
distance=5.25, 0.0375/2.7R, block 11-20: trades=42, win_rate=61.904761904761905%, sharpe=4.030466224165957,  max_dd=7.37274802, profit=179.92810345, q4_win=61.53846153846154%
distance=5.75, 0.0375/2.7R, block 11-20: trades=46, win_rate=58.69565217391305%,  sharpe=3.7363130716706876, max_dd=7.49407600, profit=175.57155029, q4_win=57.14285714285714%
distance=5.50, 0.0350/2.9R, block 11-20: trades=44, win_rate=61.36363636363637%,  sharpe=4.091146859498289,  max_dd=6.89059169, profit=180.35389347, q4_win=53.333333333333336%
```

The more aggressive adjacent rank ranges scored even higher, but should be
treated as challengers until forward paper data confirms the rank boundary:

```text
block reclaim_ema:13-20: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block reclaim_ema:11-22: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
```

2026-06-21 follow-up neighborhood scan kept the same broad shape but found a
better middle-rank `reclaim_ema` block. First, `min_price_change_pct=5` and
`entry_min_volume_ratio=1.0` stayed best. Raising the price-change floor to `6`
or tightening volume improved some recent slices but reduced full-sample profit
too much; loosening volume to `0.9` added weak trades and pushed q4 below the
target:

```text
block 11-20, min_price=5, vol=0.9: trades=49, win_rate=55.10204081632652%, sharpe=3.370892422663588,  max_dd=7.49407600, profit=158.67533783, q4_win=46.666666666666664%
block 11-20, min_price=5, vol=1.0: trades=43, win_rate=62.7906976744186%,  sharpe=4.131473618558458,  max_dd=7.37274802, profit=184.26477392, q4_win=57.14285714285714%
block 11-20, min_price=6, vol=1.0: trades=40, win_rate=62.5%,              sharpe=3.843919482315516,  max_dd=7.37274802, profit=166.04234420, q4_win=61.53846153846154%
block 13-20, min_price=5, vol=1.0: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 11-22, min_price=5, vol=1.0: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
```

Distance and exit neighborhood checks also favored the same region:

```text
block 13-20, distance=5.25, 0.0375/2.7R: trades=42, win_rate=64.28571428571429%, sharpe=4.158529966947208,  max_dd=7.37274802, profit=184.36693095, q4_win=61.53846153846154%
block 13-20, distance=5.50, 0.0375/2.7R: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 13-20, distance=5.50, 0.0350/2.9R: trades=44, win_rate=63.63636363636363%, sharpe=4.218999044078994,  max_dd=6.89059169, profit=184.77805614, q4_win=53.333333333333336%
block 13-20, distance=5.75, 0.0375/2.7R: trades=46, win_rate=60.86956521739131%, sharpe=3.8483688592034606, max_dd=7.49407600, profit=180.01037779, q4_win=57.14285714285714%

block 11-22, distance=5.25, 0.0375/2.7R: trades=41, win_rate=63.41463414634146%, sharpe=4.187645867333588,  max_dd=3.82000000, profit=183.60699826, q4_win=66.66666666666666%
block 11-22, distance=5.50, 0.0375/2.7R: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
block 11-22, distance=5.75, 0.0375/2.7R: trades=45, win_rate=60.0%,              sharpe=3.868927872958535,  max_dd=7.49407600, profit=179.25044509, q4_win=61.53846153846154%
```

The adjacent rank-boundary scan then showed `reclaim_ema:12-22`,
`reclaim_ema:13-21`, and `reclaim_ema:13-22` all produce the same strongest
closed-trade result in the current snapshot:

```text
block 12-20: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 12-22: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 13-21: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 13-22: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 14-22: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=3.82000000, profit=188.56249622, q4_win=66.66666666666666%
block 15-22: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=3.82000000, profit=188.56249622, q4_win=66.66666666666666%
```

`reclaim_ema:13-22` is the clearest label for that result because it blocks the
middle-rank reclaim bucket while keeping rank-11 mixed evidence and rank-23+
tail reclaim entries. Full report:

```text
params:
  stop_loss_pct=0.0375
  target_r=2.7
  entry_max_distance_pct=5.5
  entry_min_volume_ratio=1.0
  min_delta_rank=13
  max_delta_rank=72
  max_new_rank=30
  min_price_change_pct=5
  chase_top_rank=5
  chase_price_change_pct=80
  entry_trigger_allowlist=breakout_previous_high,reclaim_ema
  entry_trigger_rank_blocklist=reclaim_ema:13-22

result:
  trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767, max_dd=3.82000000, profit=192.38249622
  early/late win_rate=66.66666666666666%/64.28571428571429%, profit=73.97808104/113.57103129
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/64.70588235294117%/66.66666666666666%
  remove top1 positive: remaining_trades=39, remaining_win_rate=64.1025641025641%, remaining_profit=170.36508313
  remove top3 positives: remaining_trades=35, remaining_win_rate=62.857142857142854%, remaining_profit=132.53867897
  remove top5 positives: remaining_trades=30, remaining_win_rate=63.33333333333333%, remaining_profit=103.02859534
```

Additional stress checks show `13-22` is materially stronger than the previous
balanced and conservative-complex candidates after removing top contributors:

```text
old_balanced 4.0/0.035/2.9R:
  base: trades=40, win=65.00%, profit=166.48136378
  remove top3 symbols: trades=32, win=56.25%, profit=92.98893944
  remove top5 symbols: trades=30, win=53.33%, profit=70.27828030
  remove top5 trades:  trades=35, win=60.00%, profit=107.32296949
  remove positive singleton symbols: trades=31, win=54.84%, profit=94.95916082

block 11-20:
  base: trades=43, win=62.79%, profit=184.26477390
  remove top3 symbols: trades=36, win=61.11%, profit=127.71674998
  remove top5 symbols: trades=32, win=59.38%, profit=98.66855855
  remove top5 trades:  trades=38, win=57.89%, profit=113.16617557
  remove positive singleton symbols: trades=33, win=51.52%, profit=79.83937793

block 13-22:
  base: trades=42, win=66.67%, profit=192.38249620
  remove top3 symbols: trades=35, win=62.86%, profit=132.53867895
  remove top5 symbols: trades=30, win=63.33%, profit=103.02859532
  remove top5 trades:  trades=37, win=62.16%, profit=121.28389787
  remove positive singleton symbols: trades=32, win=56.25%, profit=87.95710023
```

Single-symbol and single-trade concentration is acceptable for a small sample:
the largest positive symbol is `H-USDT-SWAP` with `22.01741309U`; removing it
still leaves `39` trades, `64.10%` win rate, and `170.36508311U` profit. The
largest single winning trade contributes `19.17030790U` (`9.96%` of total
profit), top three winning trades contribute `47.03952737U` (`24.45%`), and top
five winning trades contribute `71.09859833U` (`36.96%`). The main remaining
risk cluster is a five-loss run from `2026-06-14 11:45:00` to
`2026-06-16 11:45:00`, totaling `-17.10629145U`.

The `13-22` improvement over `11-20` is not simple set subtraction because the
symbol-isolated replay lock changes which neighboring signal gets used. In the
current closed trades, `13-22` removes four trades totaling `-14.38489611U` and
adds three trades totaling `-6.56828872U`, a net improvement of about `7.82U`:

```text
removed versus 11-20:
  H breakout_previous_high rank=10 profit=-2.18019217
  BSB reclaim_ema rank=22 profit=-4.10332529
  UNI breakout_previous_high rank=11 profit=-4.42248384
  ASTER reclaim_ema rank=21 profit=-3.67889481

added versus 11-20:
  H reclaim_ema rank=11 profit=+0.81448625
  BSB reclaim_ema rank=23 profit=-4.10332529
  UNI reclaim_ema rank=11 profit=-3.27944968
```

`stop_reentry_mode=breakout_reclaim` and late profit-protection settings
(`2.2/0.0`, `2.6/0.0`, `2.6/0.3`) were no-ops on this candidate: all reproduced
the same `42` trades, `66.66666666666666%` win rate, `192.38249622U` profit,
Sharpe `4.426683134074767`, and `3.82000000%` max drawdown.

Current practical decision: keep `5.5/0.0375/2.7R +
entry_trigger_rank_blocklist=reclaim_ema:13-22` as the strongest complex
research candidate, with `reclaim_ema:11-20` retained as the more conservative
explanation-first candidate. The `13-22` result is strong enough to paper
observe, but it is still a sample-derived middle-rank filter and must not be
used as a production-live rule without forward paper evidence.

## Episode 专用参数扫描 - 2026-06-22

本轮重新按 `--event-source episodes` 做专用扫描。核心结论是：
`raw_state` 下最强的 `reclaim_ema:13-22` 不是 episode 专用候选；它在
episode 口径下只剩 `2` 笔交易，不能作为独立机会回测结论。episode
样本里主要有效触发来自 `breakout_previous_high`，但 `pullback_hold_ema`
和少量 `reclaim_ema` 在当前样本中不是负贡献，直接做 breakout-only 会
降低总收益。

基线提醒：

```text
default episodes:
  raw_candidate_events=125
  stage_counts: raw=125, trend_pass=61, entry_pass=7
  2R equity: trades=7, win_rate=57.14285714285714%, max_dd=6.04575100, profit=14.42219800

raw_state research best reclaim_ema:13-22 on episodes:
  raw_candidate_events=31
  entry_pass=3, after trigger rank block=2
  2.7R equity: trades=2, win_rate=100%, max_dd=0, profit=20.11000000
```

更适合 episode 的方向是放宽 episode 入场过滤，而不是继续使用 raw_state
的中段 `reclaim_ema` blocklist。小网格扫描覆盖：
`min_delta_rank=5/8/10/12`、`max_new_rank=20/30/40/50`、
`entry_max_distance_pct=4.0/5.5/7.0`，固定
`stop_loss_pct=0.03`、`entry_min_volume_ratio=0.8`、
`chase_top_rank=5`、`chase_price_change_pct=80`，并扫
`target_r=2.0/2.2/2.4/2.6/2.7`。

推荐 episode research 候选：

```text
balanced episode candidate:
  event_source=episodes
  stop_loss_pct=0.03
  target_r=2.4
  entry_max_distance_pct=7.0
  entry_min_volume_ratio=0.8
  min_delta_rank=5
  max_new_rank=30
  chase_top_rank=5
  chase_price_change_pct=80

  raw_candidate_events=248
  stage_counts: raw=248, trend_pass=111, entry_pass=25
  equity: trades=20, win_rate=70.0%, sharpe=3.7954335353236894,
          max_dd=6.04575100, profit=81.33033807
  early/late: trades=10/10, win_rate=70.0%/70.0%,
              profit=40.70000000/40.48110900
  q1/q2/q3/q4: trades=6/4/6/4,
               win_rate=66.67%/75.0%/50.0%/100.0%,
               profit=22.38000000/18.32000000/12.18000000/28.52000000
  remove top1 positive: remaining_trades=17, win_rate=70.58823529411765%,
                        profit=70.08535800
  remove top3 positives: remaining_trades=15, win_rate=66.66666666666666%,
                         profit=55.82535800
  remove top5 positives: remaining_trades=13, win_rate=61.53846153846154%,
                         profit=41.56535800
```

更保守、解释更简单的候选：

```text
conservative episode candidate:
  event_source=episodes
  stop_loss_pct=0.03
  target_r=2.4
  entry_max_distance_pct=7.0
  entry_min_volume_ratio=0.8
  min_delta_rank=5
  max_new_rank=20
  chase_top_rank=5
  chase_price_change_pct=80

  raw_candidate_events=158
  stage_counts: raw=158, trend_pass=81, entry_pass=17
  equity: trades=13, win_rate=84.61538461538461%,
          sharpe=5.234284636997629, max_dd=3.07000000,
          profit=71.85221800
  early/late: trades=7/6, win_rate=85.71428571428571%/83.33333333333334%,
              profit=39.71000000/32.36110900
```

保守候选做 breakout-only 后仍成立，但收益下降：

```text
max_new_rank=20 + breakout_previous_high only:
  target_r=2.4
  trades=12, win_rate=83.33333333333334%,
  sharpe=4.737642092344642, max_dd=3.07000000,
  profit=64.94110900
```

`max_new_rank=30` 上直接屏蔽 `breakout_previous_high:21-30` 会提升
收益和胜率，但它是更强的样本内过滤，当前只保留为研究观察，不作为推荐
默认：

```text
block breakout_previous_high:21-30:
  target_r=2.4
  trades=14, win_rate=85.71428571428571%,
  sharpe=5.730542875847696, max_dd=3.07000000,
  profit=78.98221800
```

保护止盈 sanity check：

```text
balanced candidate + profit_protect_after_r=1.8 + profit_protect_stop_r=0.8:
  target_r=2.4
  trades=20, win_rate=75.0%, sharpe=3.501183519467381,
  max_dd=6.04575100, profit=67.53033807
```

保护止盈提高 resolved/complete 胜率，但牺牲 `13.80U` 左右利润；当前不作
episode 默认优化，只作为“更平滑但收益更低”的风控备选。

当前 practical decision：

- episode 专用首选研究候选：`min_delta_rank=5 / max_new_rank=30 /
  entry_max_distance_pct=7.0 / entry_min_volume_ratio=0.8 / 0.03SL / 2.4R`。
- episode 专用保守候选：`max_new_rank=20` 版本，利润略低但回撤和胜率更好。
- 不把 raw_state 的 `reclaim_ema:13-22` 升级为 episode 结论；它仍保留为
  raw_state/paper observation research candidate。
- 由于 episode 当前最多只有 `20` 笔候选交易级别，本轮结论必须继续标记为
  research，需要 forward paper evidence 后再考虑生产默认。

## Episode Forward Paper Observation - 2026-06-22

新增 episode 专用 forward paper observation 预设：

```text
paper_strategy_preset=research_episode_momentum_03sl_24r_rank5_30_v1
entry_rule_version=rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1
event_source=episodes
entry_trigger_filter_version=unfiltered_v1
stop_reentry_mode=off
stop_loss_pct=0.03
target_r=2.4
entry_max_distance_pct=7.0
entry_min_volume_ratio=0.8
min_delta_rank=5
max_new_rank=30
chase_top_rank=5
chase_price_change_pct=80
```

这不是生产默认。该 preset 用于前向纸面观察，把样本内 episode 候选固定成
可审计的 entry rule version；生产默认仍保持当前 production preset。preset
会锁定 `--event-source` 和核心调参项，避免用同一个研究名称混入
`raw_events` / `raw_state` 或手工调参结果。

本地只读干跑可以先用 `jsonl` sink 复核口径，不向 Web 写入：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-sink jsonl \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all
```

2026-06-22 local dry-run result:

```text
raw_candidate_events=248
stage_counts: raw=248, trend_pass=111, entry_pass=25
framework_equity_result: trades=20, win_rate=70.0%, sharpe=3.7954335353236894,
  max_drawdown_pct=6.04575100, total_profit=81.33033807
early/late: trades=10/10, win_rate=70.0%/70.0%,
  profit=40.70000000/40.48110900
paper_outcomes_generated=40
```

Forward observation 写入 Web 时使用：

```bash
market_velocity_paper_observation \
  --paper-strategy-preset research_episode_momentum_03sl_24r_rank5_30_v1
```

需要配置 `QUANT_CORE_DATABASE_URL`、`RUST_QUAN_WEB_BASE_URL` 或
`QUANT_WEB_BASE_URL`、以及 `EXECUTION_EVENT_SECRET` /
`RUST_QUAN_WEB_INTERNAL_SECRET` / `ALPHA_EXECUTION_INTERNAL_SECRET`。该命令仍是
observation-only：Core 不下单、不撤单、不平仓；Web 如果返回
`generated_execution_task_count != 0`，CLI 会报错退出。

## Episode Backtest Detail - 2026-06-22

Vegas 回测会写 legacy `back_test_log` / `back_test_detail`，所以可以在
Admin 或 SQL 中查看每次开仓和平仓。动量 episode 回测此前是 CLI research
路径：默认只打印汇总，`--equity-trade-report` 只把单笔交易输出到 stdout，
没有落 legacy 明细表。

现在可以在一次性 `market_velocity_event_backtest` 中显式加
`--save-backtest-detail`。该开关默认关闭，不属于
`market_velocity_paper_observation` forward loop，避免观察任务重复写同一批历史
回测明细。每个 `target_r` 会写一条 `back_test_log`，每笔交易写两条
`back_test_detail`：

- `option_type=long, full_close=false`：开仓记录。
- `option_type=close, full_close=true`：平仓记录。

`signal_value` 中保留 `rank_event_id`、`entry_trigger`、`new_rank`、
`delta_rank`、`price_change_pct`、`target_r` 和 `entry_rule_version`，便于回看每笔
交易对应的动量事件。

确认允许写本地 `quant_core` 后运行：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all \
  --save-backtest-detail
```

查询最近写入的动量 episode 回测：

```sql
select id,
       strategy_type,
       inst_type,
       time,
       win_rate,
       open_positions_num,
       final_fund,
       profit,
       created_at
from back_test_log
where strategy_type = 'market_velocity_episode'
order by id desc
limit 5;
```

查看每笔开平仓：

```sql
select id,
       inst_id,
       option_type,
       open_position_time,
       close_position_time,
       open_price,
       close_price,
       quantity,
       profit_loss,
       full_close,
       close_type,
       signal_value
from back_test_detail
where back_test_id = <back_test_log_id>
order by open_position_time, id;
```

## Episode FVG And Early Exit Experiment - 2026-06-22

本轮在 episode 首选 research 参数上继续测试两个方向：

- 开启已有 FVG 入场过滤：`--fvg-entry-mode 15m_to_1h` 和 `1h_to_4h`。
- 新增早退研究参数：`--early-exit-no-profit-candles N`。含义是开仓后的第
  `N` 根 15m K 线收盘价仍未高于开仓价，则用该根 K 线收盘价平多仓；不包含
  开仓那根 K 线。该参数只用于 `market_velocity_event_backtest`，不属于
  `market_velocity_paper_observation` preset。

实现边界：

- `simulate_trade` 和 framework equity replay 都支持早退。
- framework 明细只在真实反向信号平仓时把 `close_type` 映射为
  `early_exit_no_profit`；如果同一根 K 线已经由止损/止盈先平仓，保留原风控
  close_type，例如 `Signal_Kline_Stop_Loss`。
- `market_velocity_paper_observation` 禁止该参数，避免 forward observation
  混入临时研究口径。

测试命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all \
  --fvg-entry-mode off \
  --early-exit-no-profit-candles 2 \
  --equity-report \
  --equity-trade-report \
  --min-trades 1
```

结果矩阵：

| FVG 模式 | 早退 | 交易数 | 胜率 | Sharpe | 最大回撤 | 总收益 | early-exit 平仓 |
|---|---:|---:|---:|---:|---:|---:|---:|
| off | 关闭 | 20 | 70.0% | 3.7954 | 6.045751% | 81.33033807U | 0 |
| off | 1 根 | 20 | 42.1053% | 3.0637 | 1.418650% | 51.98018016U | 12 |
| off | 2 根 | 20 | 50.0% | 3.8482 | 1.673744% | 66.86798181U | 10 |
| 15m_to_1h | 关闭 | 10 | 50.0% | 1.1941 | 3.070000% | 20.30000000U | 0 |
| 15m_to_1h | 1 根 | 10 | 37.5% | 1.2372 | 3.070000% | 15.50346853U | 6 |
| 15m_to_1h | 2 根 | 10 | 37.5% | 1.1961 | 3.070000% | 15.09025361U | 6 |
| 1h_to_4h | 关闭 | 5 | 40.0% | 0.4042 | 3.070000% | 5.05000000U | 0 |
| 1h_to_4h | 1 根 | 5 | 0.0% | -2.8833 | 1.632714% | -3.61118471U | 5 |
| 1h_to_4h | 2 根 | 5 | 0.0% | -3.0877 | 1.632714% | -4.03888891U | 5 |

结论：

- 现有 FVG 模式过于收窄样本，收益和 Sharpe 均明显弱于 FVG 关闭；当前不建议把
  FVG 开进 episode forward paper observation。
- 早退对 XAG 这类长期拖到止损的交易有效。例如 event `4280` 从原始
  `-3.07U` 止损，变为 1 根早退 `-0.47784107U` 或 2 根早退
  `-0.54362189U`。
- 早退也会砍掉部分后续反弹盈利单。`2` 根早退比 `1` 根更平衡，但样本内总收益
  仍低于不开早退。因此它只能作为风控研究备选，不进入当前 forward paper
  observation 默认 preset。

## Episode Runner Detail Backtest - 2026-06-22

本轮补齐了 framework 回测明细中的分批退出记录：

- 默认全平口径仍保持 1 条 `long` + 1 条 `close`。
- 开启 runner 后，命中基础止盈时先写一条部分平仓 close：
  `close_type=runner_base_target_hit`，`full_close=false`。
- 尾仓触发 runner 目标或 runner stop 时再写最终 close：
  `full_close=true`，`close_type=runner_target_hit` 或 `runner_stop_hit`。
- 每条 close 明细记录自己的 `close_position_time`、`close_price`、`quantity`、
  `profit_loss`，并在 `signal_value` 中写入 `exit_reason`、`leg_result_r`、
  `runner_target_r`、`runner_fraction`、`runner_stop_r`。

写库回测：

| back_test_log_id | runner | 交易数 | 明细行 | 部分平仓行 | 胜率 | Sharpe | 最大回撤 | 总收益 |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 19 | 关闭 | 20 | 40 | 0 | 70.0% | 3.7954 | 6.045751% | 81.33033807U |
| 20 | 8R / 30% / 0R stop | 20 | 54 | 14 | 70.0% | 3.7576 | 6.140000% | 108.88600000U |

`back_test_log_id=20` 的退出分布：

| close_type | full_close | 行数 | 合计收益 |
|---|---|---:|---:|
| runner_base_target_hit | false | 14 | 69.8740U |
| runner_target_hit | true | 8 | 57.4320U |
| runner_stop_hit | true | 6 | 0.0000U |
| stop_hit | true | 6 | -18.4200U |

典型样例 `BICO-USDT-SWAP`：

| 阶段 | 时间 | 价格 | 数量 | full_close | 收益 | R |
|---|---|---:|---:|---|---:|---:|
| 开仓 | 2026-06-19 18:15:00 | 0.02304 | 4340.27777778 | false | 0 | - |
| 基础止盈 | 2026-06-19 18:30:00 | 0.02469888 | 3038.19444444 | false | 4.9910U | 2.4 |
| 尾仓止盈 | 2026-06-20 00:00:00 | 0.02856960 | 1302.08333333 | true | 7.1790U | 8.0 |

runner 目标扫描，均为基础 `2.4R` 止盈、尾仓 `30%`、runner stop `0R`：

| runner_target_r | 48h 胜/亏/超时/不完整 | 48h avg R | framework Sharpe | framework 最大回撤 | framework 总收益 |
|---:|---|---:|---:|---:|---:|
| 6R | 12 / 3 / 5 / 0 | 2.032859 | 3.9450 | 6.140000% | 105.24400000U |
| 8R | 12 / 3 / 5 / 0 | 2.092859 | 3.7576 | 6.140000% | 108.88600000U |
| 10R | 11 / 3 / 6 / 0 | 2.305281 | 3.7862 | 6.140000% | 123.28600000U |
| 12R | 8 / 3 / 8 / 1 | 2.210719 | 3.4257 | 6.140000% | 105.85397357U |

结论：

- runner 8R 明细口径已能解释 BICO 这类强趋势单：先落袋 70%，再让 30% 尾仓继续吃趋势。
- 样本内 `10R` 账面总收益最高，但 48h 胜单减少、超时增加，更依赖后续行情延续。
- 当前更适合作为 forward paper observation 候选的是 `8R / 30% / 0R stop`；比原全平口径多
  `27.55566193U`，回撤只从 `6.045751%` 增至 `6.140000%`。

Forward observation 候选运行方式：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=<internal-secret> \
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_episode_runner_03sl_24r_8r30_v1
```

持续观察时增加 loop 参数：

```bash
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_episode_runner_03sl_24r_8r30_v1 \
  --loop-interval-seconds 21600
```

该 preset 会写入独立口径：

```text
entry_rule_version=rank_radar_4h_trend_15m_episode_runner_03sl_24r_8r30_v1
event_source=episodes
target_r=2.4
runner_target_r=8.0
runner_fraction=0.3
runner_stop_r=0.0
```

## Result Table Queries

Paper outcomes are written to Web table `market_velocity_paper_outcomes`.

Aggregate the active preset:

```sql
select target_r,
       horizon_hours,
       count(*) as trades,
       count(*) filter (where outcome_status = 'win') as win,
       count(*) filter (where outcome_status = 'loss') as loss,
       count(*) filter (where outcome_status = 'timeout') as timeout,
       count(*) filter (where outcome_status = 'incomplete') as incomplete,
       round((count(*) filter (where outcome_status = 'win')::numeric
         / nullif(count(*) filter (where outcome_status in ('win','loss','flat')), 0)) * 100, 4)
         as resolved_win_rate_pct,
       round(avg(result_r) filter (where outcome_status <> 'incomplete')::numeric, 6)
         as avg_r_complete
from market_velocity_paper_outcomes
where entry_rule_version = 'rank_radar_4h_trend_15m_momentum_03sl_20r_v5'
group by target_r, horizon_hours
order by target_r, horizon_hours;
```

Inspect recent losses:

```sql
select id,
       symbol,
       rank_event_id,
       entry_at,
       entry_trigger,
       target_r,
       horizon_hours,
       outcome_status,
       exit_reason,
       round(result_r::numeric, 6) as result_r
from market_velocity_paper_outcomes
where entry_rule_version = 'rank_radar_4h_trend_15m_momentum_03sl_20r_v5'
  and outcome_status = 'loss'
order by entry_at desc
limit 20;
```
