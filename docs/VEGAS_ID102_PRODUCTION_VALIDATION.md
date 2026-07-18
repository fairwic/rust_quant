# Vegas id102 生产环境校验文档

本文档是 `PRODUCTION_STRATEGY_STRICT_VALIDATION.md` 的一次具体实例化记录，只覆盖 `vegas / okx / ETH-USDT-SWAP / 4H / id102`，不作为其他策略、交易对或周期的通用放行结论。

## 1. 校验结论

校验时间：2026-07-05 14:38-14:40 CST。

`vegas / okx / ETH-USDT-SWAP / 4H` 已按实盘规则切到 `eth_4h_id102_live_v1`。

本次上线是明确的 ETH 4H 专用候选，不是跨币种通用 promote。胜率 `56.0417%` 未达到默认目标 `60%`，但当前候选满足最大回撤 `< 15%`，且是本轮验证中最优的可上线候选。

当前状态：

| 项目 | 结论 |
|---|---|
| Core 策略配置 | 通过，唯一 enabled 版本为 `eth_4h_id102_live_v1` |
| Worker 范围 | 通过，仅运行 `okx / ETH-USDT-SWAP / 4H` |
| Web 产品指针 | 通过，`vegas-eth-usdt-swap-4h` 指向 `eth_4h_id102_live_v1` |
| Release pointer | 通过，`live` channel 已激活 |
| 执行 worker | 通过，新镜像运行，`restart_count=0` |
| 止损要求 | 通过，Core 配置启用 `is_used_signal_k_line_stop_loss=true` |
| 当前新信号 | 暂无，等待下一根 4H K 线确认触发 |
| 账户级 blocker | 存在，OKX 持仓模式历史任务曾被交易所拒绝 |

## 2. 上线对象

| 字段 | 值 |
|---|---|
| strategy_key | `vegas` |
| exchange | `okx` |
| symbol | `ETH-USDT-SWAP` |
| timeframe | `4H` |
| candidate_id | `id102` |
| Core version | `eth_4h_id102_live_v1` |
| entry_rule_version | `eth_4h_id102_v1` |
| deployed commit | `e2844ca19a91315db1423e9dc739936043ce2a7c` |
| image | `ghcr.io/fairwic/quant-core-worker:sha-e2844ca19a91315db1423e9dc739936043ce2a7c` |

## 3. 回测口径

固定回归用例：`back_test_log.id=102`。

| 指标 | 值 |
|---|---:|
| trades | 480 |
| win_rate | 56.0417% |
| max_drawdown | 14.8046% |
| sharpe_ratio | 2.5392 |
| total_return_pct | 1163.2940 |

说明：

- 胜率低于默认目标 60%，这是本次人工确认接受的例外。
- 回撤满足 `< 15%` 的上线约束。
- 不允许用历史滚仓高收益作为对比依据。
- 不得把该结论外推到 BTC、SOL、BCH 或其他周期。

## 4. Core 生产校验

### 4.1 容器状态

只读命令：

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{.Name}} status={{.State.Status}} running={{.State.Running}} restarting={{.State.Restarting}} restart_count={{.RestartCount}} image={{.Config.Image}}' \
  quant-core-vegas-eth-4h-worker \
  quant-core-execution-worker \
  quant-core-market-velocity-live-handoff-scheduler \
  quant-core-internal-server"
```

当前证据：

| container | status | restart_count | image |
|---|---|---:|---|
| `quant-core-vegas-eth-4h-worker` | running | 0 | `sha-e2844ca...` |
| `quant-core-execution-worker` | running | 0 | `sha-e2844ca...` |
| `quant-core-market-velocity-live-handoff-scheduler` | running | 0 | `sha-e2844ca...` |
| `quant-core-internal-server` | running | 0 | `sha-e2844ca...` |

### 4.2 Worker 范围

只读命令：

```bash
ssh "$PROD_SSH" \
  "docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' quant-core-vegas-eth-4h-worker \
  | grep -E '^(APP_ENV|IS_RUN_REAL_STRATEGY|IS_RUN_EXECUTION_WORKER|LIVE_STRATEGY_ONLY_EXCHANGES|LIVE_STRATEGY_ONLY_INST_IDS|LIVE_STRATEGY_ONLY_PERIODS|MARKET_DATA_EXCHANGE|DEFAULT_EXCHANGE|STRATEGY_SIGNAL_DISPATCH_MODE)='"
```

期望值：

```text
APP_ENV=production
IS_RUN_REAL_STRATEGY=true
IS_RUN_EXECUTION_WORKER=false
LIVE_STRATEGY_ONLY_EXCHANGES=okx
LIVE_STRATEGY_ONLY_INST_IDS=ETH-USDT-SWAP
LIVE_STRATEGY_ONLY_PERIODS=4H
MARKET_DATA_EXCHANGE=okx
DEFAULT_EXCHANGE=okx
STRATEGY_SIGNAL_DISPATCH_MODE=web
```

### 4.3 Core 配置唯一性

只读命令：

```sql
SELECT
  COUNT(*) FILTER (WHERE enabled) AS enabled_count,
  string_agg(version, ',' ORDER BY version) FILTER (WHERE enabled) AS enabled_versions,
  bool_and((risk_config->>'is_used_signal_k_line_stop_loss')::boolean) FILTER (WHERE enabled) AS stop_loss_enabled,
  string_agg(config->>'candidate_id', ',' ORDER BY version) FILTER (WHERE enabled) AS candidate_ids
FROM strategy_configs
WHERE strategy_key = 'vegas'
  AND exchange = 'okx'
  AND symbol = 'ETH-USDT-SWAP'
  AND timeframe = '4H';
```

当前结果：

```text
1|eth_4h_id102_live_v1|t|id102
```

判定：

- `enabled_count=1`。
- `enabled_versions=eth_4h_id102_live_v1`。
- `stop_loss_enabled=t`。
- `candidate_ids=id102`。

## 5. Web 产品与发布指针校验

### 5.1 产品指针

只读命令：

```sql
SELECT
  p.core_strategy_version,
  p.back_test_log_id,
  p.display_win_rate_pct,
  p.display_max_drawdown_pct,
  p.display_trade_count,
  p.display_sharpe_ratio,
  rp.channel,
  rp.status,
  m.status AS manifest_status
FROM strategy_products p
JOIN strategy_release_pointers rp
  ON rp.product_id = p.id
 AND rp.symbol = 'ETH-USDT-SWAP'
 AND rp.channel = 'live'
JOIN strategy_manifests m
  ON m.manifest_hash = rp.manifest_hash
WHERE p.slug = 'vegas-eth-usdt-swap-4h';
```

当前结果：

```text
eth_4h_id102_live_v1|102|56.0417|14.8046|480|2.5392|live|active|live
```

判定：

- 产品 `vegas-eth-usdt-swap-4h` 已指向 Core version `eth_4h_id102_live_v1`。
- 展示指标来自 backtest id `102`。
- `live` release pointer 已激活。

### 5.2 Release manifest

当前 live manifest：

| 字段 | 值 |
|---|---|
| manifest_hash | `vegas_eth_4h_id102_live_v1_e2844ca` |
| human_label | `Vegas ETH-USDT-SWAP 4H id102 live v1` |
| status | `live` |
| promoted_by | `codex_id102_live_promotion` |

## 6. 用户实盘门禁校验

只读命令：

```sql
SELECT
  s.buyer_email,
  s.execution_exchange,
  s.service_mode,
  s.status AS subscription_status,
  c.exchange AS credential_exchange,
  c.status AS credential_status,
  c.last_check_code,
  r.risk_acknowledged,
  r.status AS risk_status,
  r.max_position_usdt,
  r.max_daily_trades,
  r.emergency_stop_enabled
FROM strategy_combo_subscriptions s
LEFT JOIN user_api_credentials c
  ON lower(c.buyer_email) = lower(s.buyer_email)
 AND lower(c.exchange) = lower(COALESCE(NULLIF(s.execution_exchange, ''), 'okx'))
LEFT JOIN combo_risk_settings r
  ON r.combo_id = s.id
WHERE s.strategy_slug = 'vegas-eth-usdt-swap-4h'
  AND s.symbol = 'ETH-USDT-SWAP'
ORDER BY s.buyer_email;
```

当前结果：

| buyer_email | execution_exchange | service_mode | subscription_status | credential_status | last_check_code | risk_status | risk_acknowledged | max_position_usdt | max_daily_trades |
|---|---|---|---|---|---|---|---|---:|---:|
| `723875705@qq.com` | `okx` | `api_trade_enabled` | `active` | `active` | `signed_exchange_preflight_passed` | `active` | true | 5.0000 | 1 |
| `chaoliushishangfaner@gmail.com` | empty | `api_trade_enabled` | `active` | empty | empty | empty | empty | empty | empty |

判定：

- `723875705@qq.com` 满足实盘前置条件：订阅 active、OKX credential active、signed preflight passed、risk acknowledged。
- `chaoliushishangfaner@gmail.com` 缺少匹配 credential 和风险设置，不得生成可执行实盘任务。

## 7. 信号与执行任务校验

### 7.1 当前信号

只读命令：

```sql
SELECT
  signal_key,
  exchange,
  symbol,
  timeframe,
  side,
  signal_status,
  generated_at,
  payload ? 'selected_stop_loss_price' AS has_sl
FROM strategy_signals
WHERE strategy_key = 'vegas'
  AND exchange = 'okx'
  AND symbol = 'ETH-USDT-SWAP'
  AND timeframe = '4H'
ORDER BY generated_at DESC
LIMIT 5;
```

当前结果：0 行。

解释：

- 这不是异常。
- 4H 策略需要等待下一根确认 K 线触发。
- 当前状态是策略已进入生产实盘等待，不代表已经生成新单。

### 7.2 执行任务

只读命令：

```sql
SELECT task_status, COUNT(*) AS n
FROM execution_tasks
WHERE strategy_slug = 'vegas-eth-usdt-swap-4h'
  AND symbol = 'ETH-USDT-SWAP'
GROUP BY task_status
ORDER BY task_status;
```

当前结果：

```text
failed|2
```

最近失败原因：

| task_id | 原因 |
|---:|---|
| 67 | OKX 当前账户还不是双向持仓模式，交易所拒绝自动开启；本次已在下单前阻断 |
| 66 | order size is below exchange step size after quantization |

判定：

- 当前没有 pending 执行任务。
- 历史失败是账户模式和数量量化门禁，不是 id102 策略配置未上线。
- 如果 OKX 账户持仓模式未处理，下一次信号仍可能在下单前阻断。

## 8. 日志校验

只读命令：

```bash
ssh "$PROD_SSH" \
  "docker logs --since 30m quant-core-vegas-eth-4h-worker 2>&1 \
  | grep -E '实时策略过滤后剩余|启动策略: ETH-USDT-SWAP - 4H - Vegas|WebSocket|订阅k线频道成功|legacy direct live exchange mutation is blocked' \
  | tail -n 40"
```

关键日志：

```text
实时策略过滤后剩余: before=9, after=1, inst_ids={"ETH-USDT-SWAP"}, periods={"4H"}, exchanges={"okx"}
启动策略: ETH-USDT-SWAP - 4H - Vegas
策略已预热并进入等待：ETH-USDT-SWAP - 4H - Vegas（等待WebSocket确认K线触发）
启动WebSocket监听: exchange=okx, targets=["ETH-USDT-SWAP"]
启动WebSocket数据流: inst_ids=["ETH-USDT-SWAP"], periods=["4H"]
订阅k线频道成功: "ETH-USDT-SWAP","4H"
legacy direct live exchange mutation is blocked
```

判定：

- worker 只加载 1 个实时策略。
- WebSocket 只订阅 ETH 4H。
- legacy direct live mutation 被阻断，实盘执行仍走 Web execution task 链路。

## 9. 实盘放行标准

下一次 ETH 4H 信号只有同时满足以下条件才允许进入真实执行：

1. Core 当前唯一 enabled 配置仍为 `eth_4h_id102_live_v1`。
2. 信号 payload 必须包含止损计划，至少包括 `selected_stop_loss_price` 或等价保护单计划。
3. Web combo 必须是 `api_trade_enabled / active`。
4. 用户 API credential 必须是 `active`，且最近 signed read-only preflight 通过。
5. 风险设置必须是 `risk_acknowledged=true` 且 `risk_status=active`。
6. 任务必须通过 execution worker lease、symbol filter、数量/价格精度、最小数量、余额与风控预算校验。
7. 不允许使用 legacy direct live mutation 绕过 Web execution task。

## 10. 当前 blocker 与处理建议

### 10.1 OKX 持仓模式

历史任务 67 已证明：OKX 当前账户还不是双向持仓模式，系统尝试通过 API 开启时被交易所拒绝。

处理建议：

1. 用户先在 OKX 侧确认没有相关挂单和冲突持仓。
2. 用户手动切换到双向持仓模式，或按交易所要求关闭冲突后重试。
3. 切换后重新做 signed read-only preflight。
4. 不要为了绕过该 blocker 开启 legacy direct live mutation。

### 10.2 最小下单数量

历史任务 66 证明曾出现 `order size is below exchange step size after quantization`。

当前 ETH-USDT-SWAP 交易所过滤器：

| 字段 | 值 |
|---|---:|
| min_qty | 0.01 |
| step_size | 0.01 |
| ctVal | 0.1 |

处理建议：

1. 下一次信号前确认 `max_position_usdt=5` 与当前 ETH 价格换算后不会低于量化后的最小张数。
2. 如仍低于最小步长，需要提高该 combo 的风险预算或保持阻断。
3. 不允许通过绕过 symbol filter 的方式提交裸单。

## 11. 日常复核清单

每次生产巡检按下面顺序执行：

1. 容器镜像和状态：4 个关键 Core 容器必须 running，`restart_count=0`。
2. Core 配置唯一性：`enabled_count=1`，版本必须是 `eth_4h_id102_live_v1`。
3. Worker 范围：必须是 `okx / ETH-USDT-SWAP / 4H`。
4. Web 产品指针：产品 version、release pointer、manifest 都必须是 id102 live。
5. 用户 readiness：至少目标用户 combo、credential、risk setting 都 active。
6. 信号 payload：新信号必须带止损计划。
7. 执行任务：pending/failed 任务需要按 attempts 原因分类，不要只看任务数量。
8. 交易所账户：OKX 持仓模式 blocker 处理前，不要宣称实盘可以成交。

## 12. 禁止动作

- 禁止手动绕过 Core worker 下单、撤单、平仓。
- 禁止打开 `LEGACY_DIRECT_LIVE_ORDER_CONFIRM` 来绕过 Web execution task 链路。
- 禁止在生产服务器直接构建或编译项目。
- 禁止在没有止损计划的情况下放行实盘任务。
- 禁止把 id102 宣称为跨币种、跨周期通用策略。

## 13. 回滚原则

触发回滚的条件：

- `quant-core-vegas-eth-4h-worker` 连续重启或无法订阅 ETH 4H。
- Core 配置出现多个 enabled ETH 4H Vegas 版本。
- 新信号缺少止损计划却进入 execution task。
- id102 产生非预期交易对、非预期周期信号。
- execution worker 出现策略版本相关的系统性失败。

回滚方式：

1. 优先通过 CI/CD rollback 或受控生产配置事务回滚。
2. 回滚前先导出当前 Core/Web 配置行用于审计。
3. 回滚后必须重新验证容器 revision、Core enabled version、Web release pointer 和 execution worker 状态。
4. 回滚不应删除历史 signal、task、attempt、order result 证据。
