# Simplify Vegas Baseline Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 基于 back_test_id=116 的实际使用情况，移除未触发的 Vegas 策略逻辑并保持回测指标与基线一致（≤1% 偏差）。

**Architecture:** 先通过 back_test_detail 与 filtered_signal_log 生成“指标使用清单”，再在 Vegas 策略中删除未触发分支与仅测试用函数，最后运行一次回测并对比指标，更新迭代日志。

**Tech Stack:** Rust（策略实现）、MySQL（回测记录）、Python（back_test_detail 分析脚本）。

**Notes:** 用户要求不使用 git worktree；允许跳过新增测试，验证改用“回测对比 + 指标使用清单”。

---

### Task 1: 记录基线使用清单（back_test_id=116）

**Files:**
- Modify: `docs/VEGAS_ITERATION_LOG.md`

**Step 1: 运行 back_test_detail 分析**
Run:
```
.venv/bin/python /Users/mac2/.codex/skills/vegas-backtest-analysis/scripts/analyze_backtest_detail.py \
  --back-test-id 116 --db-host 127.0.0.1 --db-port 33306 \
  --db-user root --db-password example --db-name test --limit 3000
```
Expected: 输出 Top indicators（如 VolumeTrend/Rsi/Bolling 等）。

**Step 2: 查询过滤器命中情况**
Run:
```
podman exec -i mysql mysql -uroot -pexample test -e \
"select filter_reasons, count(*) as cnt from filtered_signal_log where backtest_id=116 group by filter_reasons order by cnt desc;"
```
Expected: 不出现 `EMA_DISTANCE_FILTER_LONG` / `MACD_MOMENTUM_WEAK_*`。

**Step 3: 获取基线指标**
Run:
```
podman exec -i mysql mysql -uroot -pexample test -e \
"select id,win_rate,profit,final_fund,sharpe_ratio,annual_return,max_drawdown,volatility,open_positions_num,kline_nums from back_test_log where id=116\G"
```
Expected: 记录基线指标用于对比。

---

### Task 2: 移除未触发逻辑（Vegas 策略）

**Files:**
- Modify: `crates/indicators/src/trend/vegas/strategy.rs`
- Modify: `crates/indicators/src/trend/vegas/config.rs`
- Modify: `docs/VEGAS_STRATEGY_LIVE_GUIDE.md`

**Step 1: 写失败测试（TDD）**
- 跳过：用户明确同意不新增测试，改用回测验证。

**Step 2: 实施最小改动**
- 删除 `EMA_DISTANCE_FILTER_LONG` 分支与依赖的 `price_to_ema4` 变量。
- 删除 `MACD_MOMENTUM_WEAK_*` 相关分支与 `require_momentum_confirm` 配置字段。
- 删除 `detect_multi_body_engulfing` 及其测试用例。
- 更新文档中对 `require_momentum_confirm` 的说明。

**Step 3: 运行相关测试（TDD）**
- 跳过：用户允许不跑新增测试。

**Step 4: Commit**
- 跳过：用户未要求提交。

---

### Task 3: 回测对比与指标清单验证

**Files:**
- Modify: `docs/VEGAS_ITERATION_LOG.md`

**Step 1: 运行回测**
Run:
```
TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' cargo run
```
Expected: 日志包含“回测日志保存成功 back_test_id=...”。

**Step 2: 读取新回测指标**
Run:
```
podman exec -i mysql mysql -uroot -pexample test -e \
"select id,win_rate,profit,final_fund,sharpe_ratio,annual_return,max_drawdown,volatility,open_positions_num,kline_nums from back_test_log order by id desc limit 1\G"
```
Expected: 与基线 116 的指标偏差 ≤1%。

**Step 3: 输出新回测指标使用清单**
Run:
```
.venv/bin/python /Users/mac2/.codex/skills/vegas-backtest-analysis/scripts/analyze_backtest_detail.py \
  --back-test-id <NEW_ID> --db-host 127.0.0.1 --db-port 33306 \
  --db-user root --db-password example --db-name test --limit 3000
```
Expected: Top indicators 与基线一致。

---

### Task 4: 更新迭代日志

**Files:**
- Modify: `docs/VEGAS_ITERATION_LOG.md`

**Step 1: 记录删减项与对比结果**
- 写入删减项清单（未触发分支/测试函数）。
- 记录新回测 ID 与关键指标、与 116 对比结论。
- 附上新的指标使用清单摘要。

**Step 2: Commit**
- 跳过：用户未要求提交。

---

**Skill References:** @superpowers:executing-plans, @superpowers:test-driven-development, @vegas-backtest-analysis, @vegas-backtest-optimizer
