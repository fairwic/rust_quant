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
