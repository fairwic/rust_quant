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
  `stop_reentry_025sl_24r_v1`. Do not pass ad-hoc target R, stop-loss, reentry,
  FVG, profit-protection, or runner-exit parameters to the production observer.

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
market_velocity_paper_observation --paper-strategy-preset stop_reentry_025sl_24r_v1
```

## Production Scheduler

For production observation, run the Rust-native scheduler profile:

```bash
podman compose -f docker-compose.deploy.yml --profile observation-scheduler up -d quant-core-market-velocity-paper-observation-scheduler
```

This starts `market_velocity_paper_observation --paper-strategy-preset
stop_reentry_025sl_24r_v1 --loop-interval-seconds
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
entry_trigger_filter    before=<n>    after=<m>    allowlist=breakout_previous_high
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

The production observation entry currently tracks only:

```text
entry_trigger_allowlist=breakout_previous_high
```

This is based on the recent 60-day paper outcome comparison where
`breakout_previous_high` had the strongest resolved win-rate bucket among the
available 15m entry triggers.

## Current Strategy Preset

The production paper observer currently uses:

```text
paper_strategy_preset=stop_reentry_025sl_24r_v1
entry_rule_version=rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1
entry_trigger_allowlist=breakout_previous_high
stop_reentry_mode=breakout_reclaim
stop_loss_pct=0.025
target_r=2.4
horizons=24h,48h
```

This preset promotes the best current profit-focused candidate from the recent
backtest sweep. The 48h bucket is the primary profitability read because it
keeps the same 35 historical trades while allowing delayed continuation after a
valid breakout.

Latest local verification, using the current Core/Web databases:

```text
24h: trades=35, win=19, loss=9, timeout=3, incomplete=4, resolved_win_rate=67.8571%, avg_r_complete=1.146163
48h: trades=35, win=21, loss=9, timeout=0, incomplete=5, resolved_win_rate=70.0000%, avg_r_complete=1.246667
```

`avg_r_complete` excludes incomplete rows, matching the Core CLI result output.
The Web summary API `avg_result_r` includes all non-null `result_r`, including
incomplete rows, so that value is lower and should not be compared directly to
CLI `avg_r_complete`.

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
where entry_rule_version = 'rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1'
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
where entry_rule_version = 'rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1'
  and outcome_status = 'loss'
order by entry_at desc
limit 20;
```
