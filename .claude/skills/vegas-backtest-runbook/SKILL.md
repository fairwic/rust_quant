---
name: vegas-backtest-runbook
description: Runbook for iterating the Vegas strategy in this repo. Use when running Vegas 4H backtests, querying back_test_log metrics, updating strategy_config/risk_config (typically id=11), and consulting iteration history.
---

# Vegas Backtest Runbook

## 快速流程（手动/半自动）

### 0) 常用环境变量（回测/实盘一致性）
- **回测入口（本地 MySQL 容器）**：`DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED'`
- **禁用代码层强制风控收紧（推荐）**：`TIGHTEN_VEGAS_RISK=0`
- **预热 K 线数量（实盘预热对齐回测）**
  - `STRATEGY_WARMUP_LIMIT=500`
  - `STRATEGY_WARMUP_LIMIT_MAX=10000`
  - 真实预热根数 = `max(STRATEGY_WARMUP_LIMIT, strategy.value.min_k_line_num)`，并受 MAX 限制
- **OKX 请求过期窗口（修复 expTime 过期）**：`OKX_REQUEST_EXPIRATION_MS=300000`

### 1) 运行回测
从仓库根目录执行：

```bash
TIGHTEN_VEGAS_RISK=0 \
DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' \
cargo run
```

### 2) 查询最新回测结果（back_test_log）
```bash
docker exec -i mysql mysql -uroot -pexample test -e "select id,win_rate,profit,final_fund,sharpe_ratio,annual_return,max_drawdown,volatility,created_at from back_test_log order by id desc limit 1\\G"
```

### 2.1) 查询指定回测 ID
```bash
docker exec -i mysql mysql -uroot -pexample test -e "select id,strategy_type,inst_type,time,win_rate,profit,final_fund,sharpe_ratio,annual_return,max_drawdown,volatility,created_at from back_test_log where id=<ID>\\G"
```

### 3) 查看/更新策略配置（通常：vegas 4H，strategy_config.id=11）
```bash
# 查看当前配置
docker exec -i mysql mysql -uroot -pexample test -e "select value,risk_config from strategy_config where id=11\\G"

# 更新配置（建议使用 JSON_OBJECT 避免转义地狱）
docker exec -i mysql mysql -uroot -pexample test -e 'UPDATE strategy_config SET value=JSON_OBJECT(...), risk_config=JSON_OBJECT(...) WHERE id=11;'
```

### 4) 迭代标准（判优/回退）
- **硬门槛（默认）**：`win_rate >= 0.55` 且 `profit > 0`
- **排序建议**：`Sharpe ↓ → MaxDD ↑ → Profit ↓`（收益偏好时可用 `Profit ↓ → win_rate ↓`）
- **回退规则**：一次迭代若胜率下降或盈利恶化，回退上一最佳参数，并记录“为什么变差”。
- **默认执行原则**：只要没有遇到“判断分支走不通”或“需要切换大优化方向”，就直接执行代码修改、编译、回测、结果复盘与文档记录，不等待额外确认。
- **ETH 先行闸门**：凡是从 `ETH` 单样本出发的新规则/止损/形态修正，必须先确认 `ETH` 单币种回测净正向，再去做 `BTC / SOL / BCH` 复核。
- **跨币种晋级闸门**：只有在 `ETH` 正向且其他币种未被明显拖坏后，才允许更新正式基线、默认 DB 配置和迭代日志中的“推荐基线/已确认基线”。
- **禁止事项**：不允许“ETH 尚未确认正向，就先扩到其他币种继续调”；也不允许“只修正单个案例，就直接升级为正式基线”。
- **文档落地要求**：完成一次有效实验后，默认同步更新技能 runbook 和迭代日志；只有在结果不确定、需要保留多个候选方向时，才暂缓写入“基线/推荐”类结论。
- **目标样本命中校验**：若规则是从某一笔具体交易倒推出来的，必须先确认新规则确实命中了该样本；若目标样本未命中，即便规则逻辑合理，也按“已验证但拒绝晋级”处理，并撤销实验代码。
- **止损归因校验**：分析“为什么在某个时点止损”时，必须先查询 close 行的 `stop_loss_update_history`，确认 `signal_ts/source/new_price`；不能只看开仓行 `signal_value`，否则会把止损来源误判成错误的 K 线或错误的信号链。
- **明细字段口径校验**：`back_test_detail` 的 close 行 `signal_value` 可能为空；分析入场形态、信号快照、指标组合时，必须改查同笔交易的 open/long/short 行。close 行只用于查看 `close_type`、`stop_loss_source` 和 `stop_loss_update_history`。

### 5) 配置/兼容守则（避免破坏历史回放）
- 只要“基线回测”的 `strategy_detail` 里出现某模块且 `is_open=true`，就不要删除该模块/字段/指标链路；下线只能通过配置关闭（例如 `is_open=false` 或权重设为 `0.0`），并保留最小实现兼容旧配置。
- `kline_hammer_signal`：必须存在（缺失会解析失败）。
- `SignalType` 值要用正确枚举名（例如 `SimpleBreakEma2through`，不是 `SimpleBreakEma2`）。

### 5.1) JSON 配置坑（常见）
- **实盘一致性开关（services 层）**
  - `LIVE_ATTACH_TP=1`：下单时附带止盈（默认不附带）
  - `LIVE_CLOSE_OPPOSITE_POSITION=1`：反向持仓先平仓再开仓
  - `LIVE_SKIP_IF_SAME_SIDE_POSITION=1`：已有同向持仓则跳过开新仓
- **SignalType 枚举（常用）**
  - `SimpleBreakEma2through, VolumeTrend, EmaTrend, Rsi, TrendStrength, EmaDivergence, PriceLevel, Bolling, Engulfing, KlineHammer, LegDetection, MarketStructure, FairValueGap, EqualHighLow, PremiumDiscount, FakeBreakout`
- **BasicRiskStrategyConfig 字段名（避免拼错）**
```rust
max_loss_percent: f64,
is_used_signal_k_line_stop_loss: Option<bool>,
is_one_k_line_diff_stop_loss: Option<bool>,
is_move_stop_open_price_when_touch_price: Option<bool>,
atr_take_profit_ratio: Option<f64>,
fixed_signal_kline_take_profit_ratio: Option<f64>,
is_counter_trend_pullback_take_profit: Option<bool>,
```

### 5.2) 常用 SQL（在 mysql 容器内执行）
```sql
-- 1) 查最新回测
SELECT id, win_rate, profit, final_fund, sharpe_ratio, max_drawdown, created_at
FROM back_test_log
WHERE strategy_type='Vegas'
ORDER BY id DESC
LIMIT 1;

-- 2) 查最佳（Sharpe ↓ → MaxDD ↑ → Profit ↓）
SELECT id, win_rate, profit, final_fund, sharpe_ratio, max_drawdown, created_at
FROM back_test_log
WHERE strategy_type='Vegas' AND inst_type='ETH-USDT-SWAP' AND time='4H' AND profit > 0
ORDER BY sharpe_ratio DESC, max_drawdown ASC, profit DESC
LIMIT 5;

-- 3) 查最近 N 条
SELECT id, win_rate, profit, final_fund, sharpe_ratio, max_drawdown, created_at
FROM back_test_log
WHERE strategy_type='Vegas' AND inst_type='ETH-USDT-SWAP' AND time='4H'
ORDER BY id DESC
LIMIT 20;

-- 4) 查某回测的明细统计
SELECT option_type, COUNT(*) AS cnt, SUM(CAST(profit_loss AS DECIMAL(16,4))) AS total_profit
FROM back_test_detail
WHERE back_test_id = 5552
GROUP BY option_type;
```

### 6) 迭代日志
- 本技能内置一份“可读版”日志：`references/VEGAS_ITERATION_LOG.md`（超长 SQL/INSERT 行已省略）。
- 原始完整日志在仓库：`docs/VEGAS_ITERATION_LOG.md`。
