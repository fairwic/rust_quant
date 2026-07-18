# 研究笔记：PA Quant Tree

## 当前仓库事实

- `rust_quant` 是独立 Git 仓库，工作树已有大量用户修改，必须保留。
- 现有回测入口支持 `IndicatorStrategyBacktest`，可由状态化策略在当前棒执行上一棒产生的候选。
- Vegas 已有指标快照、过滤原因、Shadow 信号和路径影响研究能力，适合做只读 Meta-filter 对照。
- `crates/analytics` 已依赖 `rust-quant-strategies` 和 `rand`，适合承载离线模型与验证纯函数。
- 当前公共回测结果不足以直接证明多资产组合统计优势；本轮先建立标准化研究类型和确定性评估。

## 本次实现结果

- 新增 `strategies::implementations::pa_quant_tree`：100 根确认 K 线预热、EMA20/ATR14/效率比/重叠率/Always In/信号棒特征、趋势回撤和区间边界候选、下一棒开盘执行、止损与 RR 复核、受限 DSL、稳定 manifest hash 和只读 Vegas shadow。
- 新增 `analytics::pa_quant_tree`：时间一致研究数据集、密封 OOS 单次打开标记、purged walk-forward、成本后 R 指标、Vegas 配对路径增量、M0-M4 竞赛、训练折拟合、one-standard-error 选择和无 live 状态的 Paper 生命周期。
- `PaQuantTreeStrategy` 不进入现有默认 `StrategyExecutor` 注册表：该注册表没有 manifest/Champion/Paper contract，提前注册会绕过版本与晋级门禁。
- 训练器不会读取已打开 OOS；每个 walk-forward 折仅从训练段拟合逻辑回归或 CART 参数，验证段不参与阈值推导。
- 还没有把候选、成交结果和共享组合权益曲线接到真实历史回测数据源，因此没有、也不能宣称任何 PA 或 Vegas Meta-filter 的统计优势。
- 阶段 6 新增 `PaStrategyKey` 白名单，独立策略只能使用 `pa_range_15m`、`pa_range_1h`、`pa_trend_15m`、`pa_trend_1h`；`vegas_pa_meta_filter` 只能走 Vegas shadow。研究样本额外保存不可变 `strategy_version` 和 `manifest_hash`。
- 新增共享组合回放：初始 100U、单笔目标风险 0.5%、总开放风险 2%、单笔名义 25%、总名义 100%、同币种单持仓、同刻候选同比缩放。最大回撤仅从共享权益曲线计算。
- 新增 `ResearchEvidenceManifest`、证据门控的 Shadow/Paper Champion Promote API、共享市场时间块 bootstrap 和 Holm-Bonferroni 校正。
- 新增数据准备度 Gate：BTC、ETH、其他币种必须各有至少 1,000 根目标周期确认 K 线并共享时间窗口，才允许创建正式训练批次；该 Gate 不是统计优势证明。

## 本机数据准备度（2026-07-14，只读核对）

| 周期 | BTC | ETH | SOL | BCH | 结论 |
| --- | ---: | ---: | ---: | ---: | --- |
| 15m | 17,762 | 17,762 | 200 | 7,781 | BTC/ETH/BCH 具备 1,000 根和公共窗口，可作为离线研究候选；SOL 不足。 |
| 1h | 250 | 250 | 250 | 1,450 | 不满足 BTC/ETH/其他币种三层 Gate，禁止正式 OOS/训练批次。 |

## 验证记录

- `cargo check -p rust-quant-strategies`：通过，只有既有 warning。
- `cargo test -p rust-quant-analytics pa_quant_tree --lib -- --nocapture`：13/13 通过。
- `cargo test -p rust-quant-strategies pa_quant_tree --lib -- --nocapture`：被现有 `range_breakout_drop` 的测试初始化遗漏 `long_term_ema` 与 `price_below_long_term_ema` 阻塞；未修改该无关模块。
- 新增 Rust 文件均低于 300 行；仓库中未找到 `scripts/dev/check_code_file_line_limit.sh`，已用逐文件行数检查替代。

## 实现边界

- 新策略模块：确定性特征、趋势/区间候选、运行时模型、DSL、Manifest、Vegas Shadow 包装器。
- 新研究模块：时间切分、基础指标、配对路径影响、简单模型竞赛和 Champion/Challenger 状态机。
- 不把 AI client 引入 `strategies` 或 `analytics`。
- 不改 Vegas 信号生成函数，不把 Shadow 拒绝转换成真实过滤。

## 风险

- 计划完整范围相当于多阶段策略研发；代码落地只能建立研究和运行基础，统计优势仍需真实历史数据、密封 OOS 和 Forward Paper 产生证据。
- 当前回测框架是单标的单仓语义，共享组合回放需使用独立研究类型，不能把多标的单测结果直接相加后宣称回撤有效。

## 15m 真实训练期结果（2026-07-15）

- 新增 `historical` 纯 Rust 结算器与 `pa_quant_tree_15m_research` 只读 CLI；严格下一棒开盘、跳空止损、同棒双触发按止损、基础/两倍成本。
- 公共窗口为 2026-04-16 至 2026-07-06，每市场 7,780 根确认 K 线，数据指纹为 `sha256:6538380471001500fe801db492802746398bf3664932a7c0947c3aaa7c5e9d0c`。
- `pa_trend_15m`：377 笔，平均 -0.399R，胜率 36.34%，PF 0.520；共享组合 92.19U，最大回撤 9.73%。
- `pa_range_15m`：116 笔，平均 -0.800R，胜率 31.90%，PF 0.318；共享组合 95.92U，最大回撤 4.54%。
- 7 日共享市场块 bootstrap 下界分别为 -0.540R、-1.109R；M0-M4 无合格 Challenger。
- 训练过程中修复两个统计漏洞：固定比例 purge 未覆盖实际持仓期限；all-reject 模型利用 0R 战胜负收益 M0。现已使用 outcome-horizon purge，并要求至少 30 笔和 10% 覆盖率。
- 详细报告见 `docs/PA_QUANT_TREE_15M_TRAINING_REPORT.md`。

## 365 天历史补齐与重跑（2026-07-15）

- 使用已有 `market_velocity_candle_backfill`，以幂等 UPSERT 补齐 BTC/ETH/SOL/BCH 的15m与1h数据；未新增抓取器。
- 15m 四市场各35,039根，1h各8,759根；全部 `actual = expected`，缺口为0，首尾时间完全一致。
- 15m 全年训练结果：趋势2,350笔、平均-0.265R；区间713笔、平均-0.703R，M0-M4均无合格 Challenger。
- 趋势 `cost_r <= 0.10` 在基础成本下略正但两倍成本为负；区间 `cost_r <= 0.20` 只有29笔且胜率/PF不达门槛。
- 旧81天报告已标记为预检；当前详见 `docs/PA_QUANT_TREE_15M_365D_TRAINING_REPORT.md`。

## 1h 真实训练期结果（2026-07-15）

- 研究入口支持 `--timeframe 15m|1h`，1h 使用独立的 `pa_trend_1h`、`pa_range_1h` 策略标识、manifest 和数据指纹。
- 公共窗口为 2025-07-15 03:00:00 UTC 至 2026-07-15 01:00:00 UTC，每市场 8,759 根确认 K 线，数据指纹为 `sha256:eba26a8bca1a1c409ce23f83b4fcfb67d21a50cca71e997f5b4350ef0233a474`。
- `pa_trend_1h`：879 笔，平均 -0.096R，胜率 41.07%，PF 0.855；共享组合 88.45U，最大回撤 16.84%。
- `pa_range_1h`：193 笔，平均 -0.384R，胜率 31.61%，PF 0.569；共享组合 90.80U，最大回撤 11.48%。
- 两个策略的 7 日共享市场块 bootstrap 下界均小于 0，M0-M4 没有合格 Challenger，禁止进入 Shadow/Paper。
- 逻辑回归当前使用胜负标签和固定 0.5 概率阈值，未直接优化成本后期望 R；下一轮需在训练折内拟合期望或选择阈值，验证折只评分。
- 资金费率未接入，报告保持 `promotion_eligible=false`。详见 `docs/PA_QUANT_TREE_1H_365D_TRAINING_REPORT.md`。

## 成本后期望阈值 v2（2026-07-15）

- 修复 M2 逻辑回归固定 `0.5` 阈值：模型仍拟合胜率概率，但 keep 阈值只在每个训练折内按成本后平均 R 选择，至少保留 `max(50, 训练样本10%)`。
- 新增低于50%胜率但正期望 payoff 的阈值测试，PA analytics focused tests 更新为 17/17 通过。
- 15m 趋势 M2 在验证期保留716/940笔，平均 -0.034R，one-standard-error 选择 M2；改善不足以转正，禁止 Shadow/Paper。
- 15m 区间、1h趋势、1h区间仍选择M0；修复消除了 all-reject 假象，但没有证明当前候选结构有统计优势。
- 本机 `quant_core.funding_rates` 表存在但四市场均为0行；仓库已有273条/市场 TSV 仅覆盖约91天，不能用于全年完整成本证据。
- 详细报告见 `docs/PA_QUANT_TREE_EXPECTED_R_THRESHOLD_V2_REPORT.md`。

## 单一资金费率代理 v3（2026-07-15）

- Binance funding history 因本机区域限制返回HTTP 451，未产生数据库写入；按用户要求不再拉取多个来源。
- 使用现有 `crypto_exc_all` Hyperliquid `fundingHistory`，写入 `source=hyperliquid` 的独立分片表，不污染旧 `funding_rates` 或OKX来源。
- 15m公共窗口每市场8,760个小时费率点，1h窗口每市场8,759点，小时桶缺口均为0。
- 回测按持仓跨越小时累计 `abs(funding_rate)`，负费率不作为收益；K线、funding与协议版本共同生成联合指纹。
- v3结果：15m趋势 -0.274R、15m区间 -0.711R、1h趋势 -0.109R、1h区间 -0.393R，全部禁止晋级。
- `funding_cost_included=true`，但因 `funding_cost_is_proxy=true` 和密封OOS未打开，`promotion_eligible=false`。
- 详细报告见 `docs/PA_QUANT_TREE_FUNDING_PROXY_V3_REPORT.md`。

## 趋势入场质量 v4（2026-07-15）

- 在结果产生前预注册4个确定性特征：方向化EMA收回距离、方向化收盘强度、信号棒ATR长度、三棒反侧收盘比例。
- 新增 `pa-feature-registry-v2`；M2/M3/M4统一允许使用10个已批准特征，候选和执行语义不变。
- 首次运行发现梯度数组仍为2维导致越界；修复后新增模型维度测试，未使用失败运行结果调公式。
- `pa_trend_15m` M2 walk-forward保留114/940笔，平均+0.090R、标准误0.117R；均值转正但下界仍为负。
- 全训练重拟合保留254笔，基础+0.032R、PF1.052、组合101.28U、回撤7.71%；两倍成本-0.079R。
- BTC/ETH为负，SOL/BCH为正；胜率45.67%，不满足跨市场、双倍成本和60%胜率门槛。
- v4归档为弱信号，不进入Shadow/Paper。详见 `docs/PA_QUANT_TREE_TREND_QUALITY_V4_REPORT.md`。

## 趋势跟随确认 v5（2026-07-15）

- 新增独立策略键 `pa_trend_followthrough_15m`、`pa_trend_followthrough_1h`，时序固定为 setup `t`、确认 `t+1`、开盘入场 `t+2`；不覆盖原趋势策略。
- 15m 共255笔，四市场全部为负，合并平均 -0.414R、两倍成本 -0.672R；M0 walk-forward验证均值 -0.376R。
- 15m共享组合最终82.00U、最大回撤18.01%，bootstrap下界 -0.597R。
- 1h只有97笔已结算候选，被至少100笔门禁拒绝，没有训练模型。
- 首次输出发现绕过Trend regime门禁，结果作废；修复后未调阈值并重新运行。
- 新增能力后 v4 baseline JSON仍与冻结原件字节级一致；v5不进入Shadow/Paper。详见 `docs/PA_QUANT_TREE_FOLLOWTHROUGH_V5_REPORT.md`。

## 入选模型 OOF 评估纠偏 v6（2026-07-15）

- 模型竞赛为全部验证候选保存不序列化的 OOF keep/reject 决策；legacy v4 JSON仍字节级一致。
- 15m趋势 M2 OOF保留114/940，基础+0.090R、两倍成本-0.005R、block bootstrap下界-0.238R。
- 15m趋势 OOF仅BTC/SOL为正，ETH/BCH为负；单个正向市场均少于30笔。
- 15m区间、1h趋势、1h区间和15m跟随确认的 OOF 路径继续为负。
- OOF报告显式标记 `outer_validated=false`、`family_selection_adjusted=false`，不得解释为nested OOS或晋级证据。
- 下一步转入同setup A/B/C反事实，不开发浅回踩新策略。详见 `docs/PA_QUANT_TREE_EVALUATION_V6_REPORT.md`。
