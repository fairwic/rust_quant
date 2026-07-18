# Market Velocity / Vegas 生产证据审计

## 决策结论

截至 2026-07-15，本轮停止新策略开发、把资源集中回 Market Velocity/Vegas 的方向是对的；偏差出现在生产证据治理，而不是策略研究方向。

当前状态不能表述为“Market Velocity/Vegas 实盘链路已闭环”。容器、调度和核心门禁代码都存在，但生产身份绑定、短周期 K 线覆盖、有效订阅的 signed preflight、slug/release contract 和仓位结果对账没有同时成立。继续做新参数或新策略增量，会把运行合同问题误当成策略问题。

本审计全程只读。未创建 readiness preview、execution task、订单或写回；未重启、改配置、部署、提交或推送。

## 审计身份

| 项目 | 生产事实 |
| --- | --- |
| 审计窗口 | `2026-07-15T13:18:48Z`—`2026-07-15T13:36:18Z` |
| Core 镜像 revision | `d502eca668a84c4e1b8efb38fdb131234c3c5ba6` |
| Web 镜像 revision | `54f535973d83edffbbd7585dbfc0a85d059d764f` |
| Core 本地可核对源码 | 本地 HEAD 与生产 Core revision 相同 |
| Web 本地可核对源码 | 生产 revision 是本地 HEAD 的祖先；代码合同按生产 commit 读取 |
| 运行状态 | 所有列出的 Core 角色与 Web backend 均 running，restart=0 |

## 运行角色与调度

| 角色 | 周期 / 入口 | 审计判断 |
| --- | --- | --- |
| Market Velocity radar | 10 秒；radar-only；直接 signal dispatch 禁用 | 活跃，负责排名事实，不直接证明策略可执行 |
| candle backfill | 5 分钟；126 个 radar symbols；1m/5m/15m 最近 2 天 | 进程活跃，但覆盖面小于 scanner 的 269 symbols，不能修复未进入 backfill universe 的旧表 |
| kline scanner | 60 秒；lookback 30 分钟；写事件 | 活跃；扫描 269 symbols，但受单 symbol 动态 K 线表新鲜度影响 |
| 主 Market Velocity live handoff | 60 秒 | 活跃，但绑定已过期 combo 4 与 credential 1 |
| Breakdown Short live handoff | 300 秒 | 活跃，但 buyer/combo/credential 为空，且 slug/release contract 漂移 |
| Market Velocity paper observers | 6 小时 | 三个不同 preset 并行；只属于观察，不是 live readiness 证据 |
| Vegas worker | ETH-USDT-SWAP 4H；Web signal dispatch | 活跃，WebSocket 心跳正常，4H 数据新鲜；近期没有新信号 |
| exchange symbol sync | 1 小时；OKX | 最近成功写入 423 行；四个主交易对过滤器小于 1 小时 |
| execution worker | 约 5 秒轮询；lease limit=1；处理 pending/pending_close 与 execute_signal/close_position | 活跃但持续 handled=0；当前没有任务可验证 lease 到回写的动态闭环 |

所有 Core 角色使用同一 revision。运行角色本身没有重复 lease 或重复消费证据，问题不是“容器太多”，而是各角色消费的身份与数据合同没有对齐。

## 数据新鲜度

审计时 `market_rank_snapshots` 共约 12.70M 行，最新约 15 秒；过去 24 小时 rank events 为 7,308 条，448 个 active episodes 的 `last_seen_at` 同样约 15 秒。排名雷达是新鲜的。

短周期 K 线不是整体新鲜：

| 数据表 | 最新已保存 K 线 | 审计时滞后 |
| --- | --- | --- |
| BTC 1m | `2026-06-30T05:16:00Z` | 约 15 天 |
| BTC 5m | `2026-07-05T06:10:00Z` | 约 10 天 |
| BTC 15m | `2026-07-05T06:00:00Z` | 约 10 天 |
| ETH 15m | `2026-07-05T06:00:00Z` | 约 10 天 |
| SOL 15m | `2026-06-20T01:00:00Z` | 约 25 天 |
| BCH 15m | `2026-07-15T13:15:00Z` | 约 21 分钟 |
| ETH 4H | `2026-07-15T12:00:00Z` | 当前已确认 4H K 线 |

因此“radar 新鲜”和“backfill 日志无失败”不能推导出“Market Velocity 15m 生产输入新鲜”。backfill 每轮报告 126 个 symbols 无缺口，而 scanner 扫描 269 个 symbols；BTC/ETH/SOL 正是覆盖错位的反证。另有部分 Docker JSON 日志包含 NUL，标准 `docker logs` 无法读取，虽可从原始日志只读恢复，但属于可观测性退化。

## 信号到 readiness、task、lease

### Market Velocity 主链路

- 近 7 天 `market_velocity_live_handoff_states` 为 29,211 条，全部 `blocked`，没有 pass 或 dispatch。
- 最近 24 小时 4,140 条统一外层 blocker 为 `market_velocity_live_entry_shell_blocked`；细节包括 volume、15m average、drawdown、Bollinger 和 FVG 条件未满足。
- live handoff 固定绑定 combo 4 / credential 1。credential 1 的 OKX signed preflight 在审计时是新鲜的，但 combo 4 已于 7 月 9 日过期，因此不满足 Web entitlement gate。
- 当前有效的 Market Velocity combo 7 到 7 月 31 日，但它属于另一组 Binance credential；该 signed risk snapshot 已于 7 月 5 日过期。
- 生产 Web readiness 代码要求 exact combo/buyer/strategy/symbol、active API-trade、未过期、exchange 匹配、risk acknowledged、verified credential 以及未过期的 signed read-only risk snapshot；所以当前没有一组身份能同时通过。

### Breakdown Short

- 近 7 天 409 条 handoff 全部 blocked。
- 运行时 strategy slug 为 `market_velocity_breakdown_short`，产品/有效 API-trade 订阅为 `market-velocity-breakdown-short`。underscore 的 combo 5 是 signal-only，最近 7 天收到 7 条 skipped；hyphen 的 combo 8 是 API-trade，但同期没有 delivery log。
- 产品状态为 DRAFT，却存在 active `production_default` 和 `paper_observing` release pointer，同时 live scheduler 正在运行。这是 catalog、release 和 runtime 三方治理不一致。

### Vegas

- ETH 4H worker、K 线和 WebSocket 当前正常，但最近信号停留在 7 月 7 日附近，当前 Core 启动后没有新 execution task。
- 当前有效 combo 6 的 signed Binance risk snapshot 已于 7 月 5 日过期，因此即使新信号到达也不能视为 ready。
- 历史 task 68 是唯一成功证据：combo 2、OKX、ETH long，成交 0.02，filled quote 35.7476 USDT；当时保护同步完成，止损触发价 1758.52，保护单已确认。

### Task 与 lease

execution worker 运行正常、约每 5 秒轮询、lease limit=1，但审计窗口内 handled=0，当前 Core 启动后新增任务数为 0。历史 Market Velocity 5 个任务全 failed；Vegas 为 1 completed、4 failed。由此只能证明 worker 处于待命，不能证明当前 signal -> readiness -> task -> lease -> result 的动态闭环。

## signed preflight、精度、止损与回写

代码合同没有缺位：生产 Core revision 会加载 exchange filters、量化数量/价格、拒绝缺失或方向非法的 `selected_stop_loss_price`，并在 live 路径完成 signed credential/readiness preflight、保护单创建与确认；生产 Web revision 的 live readiness endpoint 明确 `read_only=true`、`mutation_allowed=false`，并要求新鲜 signed risk snapshot。

生产过滤器事实也新鲜：OKX BTC/ETH/SOL/BCH 均为 `live`；例如 ETH tick size=0.01、step size=0.01、min qty=0.01，最近同步小于 1 小时。

但当前证据仍有三处断裂：

1. **有效订阅和新鲜 preflight 不属于同一身份。** 过期 combo 2/4 对应的新鲜 OKX preflight，当前有效 combo 6/7/8 对应的 Binance preflight 已过期。
2. **Vegas 保护单只有历史确认，没有当前对账。** task 68 对应 position leg 仍是 `active/confirmed`、open_qty=0.02，更新时间停在 7 月 6 日；Web 只有同日 `execution_result` position snapshot，`protective_order_status` 为空，没有后续 exchange-signed position history。Core exchange audit 也只有 7 月 6 日下单相关 endpoint，没有今天的 signed position/open-order 证据；execution worker 未配置常驻 reconciliation 角色。
3. **结果写入与用户可见 delivery 未收敛。** task 68 是 completed、order result 是 filled、position leg 是 active/confirmed，但对应 `execution_result` delivery 一直为 `pending`，更新时间仍是 7 月 6 日。

## 是否走偏

没有在策略方向上继续走偏：归档 PA、停止新策略、回到已有生产证据是正确决策。

生产工程层面已经走偏到“进程活跃优先、合同一致性滞后”：

- 把新鲜 radar 与局部 backfill 当成完整 K 线新鲜度；
- live handoff 依赖固定过期 combo/credential，而不是 owner service 的当前有效身份；
- short 的 slug、产品状态和 release pointer 各自成立但彼此不一致；
- 把一次历史成交与保护单确认延伸为当前持仓安全结论；
- task/order/position 已写入，但 delivery 状态未收敛。

因此现在不应选择任何新的策略机制或参数问题。先补实盘安全证据，再处理身份绑定、K 线 universe 和 contract drift。

## 唯一下一项增量验证

只执行 `prod-vegas-open-leg-readonly-reconciliation-20260715`：对 task 68 / combo 2 / ETH-USDT-SWAP 做一次 signed read-only 的 position、open orders 和 recent fills 对账。

固定要求：

- 快照不超过 5 分钟，单一 credential/combo/task 身份命中；
- 如果仓位仍为 0.02 long，必须证明有效保护单覆盖剩余数量；
- 如果仓位已为零，必须给出对应 close fill 与保护单终态；
- 任一歧义、缺保护、数量不一致或证据过期即失败；
- 全程禁止 task/order/position mutation，禁止 Web 写回，mutation count 必须为 0。

这个问题优先于 Market Velocity 增量验证，因为它涉及一笔 Web 仍标记 active 的历史真实仓位。验证完成后也不自动修复；任何 close-fill 或状态写回都必须另行申请明确授权。

## 2026-07-16 预注册验证执行结果

`prod-vegas-open-leg-readonly-reconciliation-20260715` 在交换所探针前失败并停止，失败码为 `mutation_safe_execution_path_unavailable`。

生产 Core revision 仍为 `d502eca668a84c4e1b8efb38fdb131234c3c5ba6`。该版本的 reconciliation runtime 需要内部密钥调用 Web 精确解析 credential；但只要同一内部密钥非空，runtime 就会无条件构造并写回 exchange account snapshot。配置中的 `RECONCILIATION_SNAPSHOT_REPORT=false` 只关闭 reconciliation issue report，不控制 account snapshot 写回。因此执行现有入口会违反预注册的“禁止 Web 写回、mutation count=0”硬边界。

本次没有启动 reconciliation 进程，没有向交易所发起 signed 请求，也没有向 Web 发起写请求：signed exchange request count=0、Web write request count=0、mutation count=0。由于没有获得新快照，task 68 的当前仓位、open orders、recent fills 和保护单状态仍未知，不能把历史保护确认解释为当前安全。

按预注册停止规则，不使用临时代理、一次性直连脚本或其他旁路绕过 Core owner path，也不在本次验证内修改代码、部署或写回生产。机器可读证据见 [Vegas open leg 只读对账执行门禁](evidence/vegas_open_leg_readonly_reconciliation_20260716.json)。
