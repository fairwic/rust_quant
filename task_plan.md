# 任务计划：PA Quant Tree v1 基础设施

## 2026-07-20：交易系统目标架构文档收敛

- [x] 将目标物理目录收敛为 `domains / quant / contracts / adapters / platform` 五类，并将 Operations 改为边界更窄的 Reconciliation
- [x] 明确 Model、Policy、Use Case、Port、Adapter、App 与 Wire Contract 的代码位置和依赖方向
- [x] 明确数据库 Create/Read/Update/Delete、Query、SQLx transaction、Outbox 和 migration 的唯一放置规则
- [x] 明确 Web ExecutionRequest 与 Core Order/Fill/Protection 事实的 owner 迁移边界
- [x] 补齐部分成交、保护数量、撤单竞态、Unknown、最大未保护窗口与恢复协议
- [x] 将迁移改为 Golden Vertical Slice + shadow/parity + CI ratchet，而不是横向大搬迁
- [x] 补充 AI 编码护栏、Owner-scoped 持久化 ADR 和开源参考取舍
- [x] 用现有 Vegas/回测主链反向验证目标目录，修正 signal-worker 与账户级 Portfolio/Risk 的运行边界
- [x] 新增 Research Domain，明确 Experiment、BacktestRun、DatasetManifest、SimulationLedger 与 ResearchEvidence 的唯一 owner
- [x] 以 ADR-0009 取代 ADR-0008：`quant/backtest` 收敛为纯确定性内核，业务编排回到 Research Use Case
- [x] 将回测验证拆为 ResearchBar、PaperEvent、RecoveryHarness，避免普通策略回测虚假复刻整套生产 OMS
- [x] 明确 Vegas Evaluation State 的 run/deployment scope、多币种 decision-time barrier、SimulationLedger 与生产 AccountProjection 的隔离
- [x] 明确 StrategyArtifact 与 ResearchEvidence 分权，以及对象存储 + PostgreSQL 的 ResearchEvidence 原子可见发布协议
- [x] 重写 Vegas 迁移实战，补齐逐文件代码分配、Research 数据访问、迁移切片与分层 parity 验收矩阵
- [x] 将目标架构封装为项目级 `rust-quant-architecture` Skill，并完成官方结构校验和 umbrella 可发现安装

下一步：文档不等于实现；先冻结 Vegas Slice 0 的事件级基线、随机 symbol 输入顺序基线和证据指纹，再建立最小 Research owner 骨架与只拦新增违规的只读 `arch-check`；通过后迁移无副作用 Vegas Evaluator 和 ResearchBar 单资产切片，不能一开始就搬整套 OMS。

## 目标

在不改变 Vegas 现有交易行为的前提下，交付无 AI 运行时的 PA 量化树纵向切片，并建立可复现的离线 Challenger 评估与 Paper 生命周期基础。

## 阶段

- [x] 阶段 1：确认仓库边界、已有能力和脏工作树
- [x] 阶段 2：实现确定性特征、候选、DSL、Manifest 与独立策略
- [x] 阶段 3：实现 Vegas Shadow Meta-filter、研究评估和生命周期
- [x] 阶段 4：运行测试、回归、行数检查和注释审查
- [x] 阶段 5：同步设计文档和交付结论
- [x] 阶段 6：收紧策略标识，并接入研究样本与共享组合回放
- [x] 阶段 7：接入 BTC/ETH/BCH 15m 只读历史研究与成本结算
- [x] 阶段 8：使用现有 OKX backfill 补齐四市场 365 天 15m/1h 历史
- [x] 阶段 9：用四市场全年数据重新执行 15m 训练期研究
- [x] 阶段 10：独立执行四市场全年 1h 训练期研究
- [x] 阶段 11A：修正逻辑回归成本后期望阈值并独立重跑 15m/1h
- [x] 阶段 11B：补齐单一来源全年资金费率代理并按持仓小时保守计入成本
- [x] 阶段 12：预注册并验证趋势入场质量 Feature Registry v2
- [x] 阶段 13：预注册并实现独立的趋势跟随确认候选与新策略版本合同
- [x] 阶段 14A：记录模型竞赛 OOF 决策并生成入选模型 OOF 路径统计
- [x] 阶段 14B：完成 v1/v5 的 A/B/C 同 setup 配对反事实
- [x] 阶段 14C：建立统一实验账本与 Holm 多重检验校正
- [x] 阶段 14D：按预注册门禁归档 PA 独立策略，取消继续开发 PA Meta-filter
- [x] 阶段 15：修复工程构建基线，冻结新独立策略默认注册，并把后续资源切回 Market Velocity/Vegas
- [x] 阶段 16：只读核对 Market Velocity/Vegas 生产 revision、角色、调度、数据、readiness、task/lease、保护单和结果回写
- [x] 阶段 17：仅执行一次 Vegas 历史 active 仓位与保护单 signed read-only 对账；在交易所探针前按失败门禁停止，mutation count=0
- [x] 阶段 18：从旧 `quant_core_postgres` 卷迁移 Market Velocity 回放数据到独立本地数据库，修复权益回放时间对齐基线并精确复跑 Long / Breakdown Short
- [ ] 阶段 19：在补齐显式滑点合同后，只执行 Breakdown Short v6 的固定参数前向稳健性验证；不得同时开启其他策略实验

## 关键问题

1. 如何保证候选判断只读取信号时点及以前的已确认 K 线？
2. 如何让 Vegas Meta-filter 保留原始方向、入场和风险计划？
3. 如何冻结 Manifest，并让相同输入得到完全相同结果？
4. 如何让自动迭代只生成 Challenger，绝不在线修改 Champion？

## 已确定决策

- 运行时不依赖 AI、网络或自然语言结构判断。
- v1 使用下一根 K 线开盘执行 PA 独立候选。
- Vegas Meta-filter 仅输出 keep/reject 与审计轨迹，不修改原始信号。
- 训练与统计组件使用纯 Rust；不在策略迭代中使用 Python。
- 本轮不触发真实下单、部署、生产配置切换或 Live promote。
- 暂停浅回踩 v6；先修复入选模型 OOF bootstrap、配对反事实和多重检验闭环。
- `pa-v7-abc-counterfactual-once` 是 PA 独立策略最后一次训练窗口诊断；不重复运行、不调门槛、不追加新 PA 候选。
- v7 任一硬门禁失败即归档 PA 独立策略；PA 不再接入 Vegas Meta-filter，后续研究资源回到已有生产证据的 Market Velocity/Vegas。
- Market Velocity/Vegas 的下一步不是新参数或新机制；`prod-vegas-open-leg-readonly-reconciliation-20260715` 已因现有入口无法同时满足精确凭证解析和零写回而在交换所探针前失败，禁止重跑或旁路绕过。
- 在当前有效 combo、signed preflight、运行 slug/release、K 线 universe 和仓位证据同时闭环前，不把容器存活或历史成交解释为当前 live-ready。
- Market Velocity Long 当前生产 preset 在迁移样本上 784 个信号候选全部被 FVG 执行门禁阻塞，保持非 Live 晋级状态，不做阈值搜索。
- Breakdown Short v6 虽然费后权益为正且最大单 symbol 回撤低于 15%，但仅 43 笔、完整胜率 58.14%、去 Top3 后胜率 57.14%，严格门禁失败；唯一后续实验是固定参数的新增样本前向验证。

## 错误记录

- 本机 K 线覆盖统计的首次 SQL 因含连字符表名未加双引号失败；修正引用后只读查询成功，未产生数据变更。
- 策略 crate 曾受既有 `range_breakout_drop` 测试初始化缺字段与过期真实数据 smoke API 影响；已按当前公开合同修复，`cargo test -p rust-quant-strategies --no-run` 恢复通过。
- 真实 15m 候选的固定比例 purge 未能覆盖持仓结算期限，时间泄漏 Gate 拒绝训练；改为仅按 signal/settled 时间扩大 purge，不读取收益标签。
- 1h 训练发现逻辑回归使用胜负标签和固定 0.5 阈值，未直接优化成本后期望 R；该问题进入下一训练协议，不回写本批次结果。
- 本机 `quant_core.funding_rates` 表当前四市场均为 0 行；仓库 TSV 只有约 91 天，不能对 365 天回测宣称已包含资金费率。
- Binance 公共接口因地理限制返回451；改用单一 Hyperliquid 来源，并写入分源分片表，避免与OKX事实混写。
- Hyperliquid funding 时间带毫秒偏移，首次按整点终点查询少1小时；修正为同小时桶容差后，15m每市场8,760点、1h每市场8,759点，缺口为0。
- v4首次运行因逻辑回归权重扩为10维但梯度仍为2维而越界；修正为按注册特征数量分配梯度，并新增维度回归测试，首次失败未产生研究结果。
- 研究 CLI 的 `--help` 在数据库门禁之后执行，无 Core 连接变量时无法显示帮助；未为本阶段修改该非关键顺序。
- 首次可复现性重跑使用项目脚本示例默认密码，被当前本机 Postgres 拒绝；随后只在进程内读取 Podman 容器现有配置完成只读重跑，未输出凭证。
- v5首次研究运行调用了旧构建产物，参数解析失败且未产生有效报告；显式重建后重跑。
- v5首次有效输出发现确认候选数反而超过原趋势 setup；定位为确认函数绕过 v1 顶层 Trend regime 门禁。该结果作废，修复为完整复用 `generate_pa_candidate` 后再重跑，未据此修改任何预注册阈值。
- v5 1h 首次运行被既有最少100笔研究门禁拒绝；为保留失败证据，只增强错误上下文输出实际已结算数量，不降低样本门槛。
- 错误上下文增强后，`collect()` 在先调用 `len()` 时无法从后续构造函数推断集合类型；显式标注为 `Vec<_>` 后恢复编译，不改变研究逻辑。
- 新增三时点回归测试首次使用的陡坡夹具无法触碰EMA20，因而没有形成原趋势setup；将测试专用斜率降为0.1，使夹具同时满足趋势和EMA回踩门禁，未修改生产阈值。
- OOF 聚合逻辑拆分后，Rust 默认从 `src/bin/oof.rs` 查找模块；补充显式 `#[path]` 指向同名二进制目录，未改变研究行为。
- 生产 K 线新鲜度首次查询使用了错误的动态表命名和时间列；按实际 `*-usdt-swap_candles_*` 与毫秒 `ts` 重新执行后得到只读结果，未产生数据变更。
- 部分生产容器 JSON 日志包含 NUL，标准 `docker logs` 解析失败；审计只读清理 NUL 后读取原始日志，未改动生产日志文件。

## 状态

**PA 独立策略已归档；Vegas 当前安全证据仍未闭环；Market Velocity 两个生产 preset 均未通过 Live 晋级门禁**：独立回放库已恢复旧动量数据，Long 为 0 可执行入场；Breakdown Short v6 为正收益但只有 43 笔、完整胜率和去集中度结果不达标。当前不启动 Live；先补显式滑点合同，随后只允许固定 v6 的新增样本前向稳健性验证。Vegas 在发布可显式关闭全部写回的 signed read-only 入口前，仍不得把历史成交与保护单确认解释为当前安全。
