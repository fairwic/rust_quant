# PA Quant Tree 趋势跟随确认 v5 训练报告

## 实验身份

- 预注册：`docs/PA_QUANT_TREE_FOLLOWTHROUGH_V5_PREREGISTRATION.md`。
- 训练协议：`pa-training-v5-trend-followthrough`。
- 新策略键：`pa_trend_followthrough_15m`、`pa_trend_followthrough_1h`。
- Feature Registry：`pa-feature-registry-v2`。
- 数据阶段：已查看历史训练窗口，不是密封 OOS 或 Forward Paper。
- 15m 联合数据指纹：`sha256:3dc3e4cffbdd86558fccf59a710ee2f7dd29ff75f4ff33ca28479a7fbce33adf`。
- Funding：Hyperliquid 单一来源代理，按持仓小时累计绝对费率；不是 OKX 实际资金费率事实。

## 实现审计

候选时序已独立为：

```text
t：原 pa_trend setup，冻结方向与结构止损
t+1：唯一已确认跟随棒通过方向、突破、收盘强度、棒长与止损存活门禁
t+2：下一棒开盘复核风险并入场
```

新候选保存 `setup_ts=t`，候选 `signal_ts=t+1`，执行 `entry_ts=t+2`。旧候选的 `setup_ts=None` 在序列化时省略；新增 v5 后再次运行 v4 baseline，JSON 与冻结原件字节级一致：

```text
sha256 1d4e38915cb0e89a1de10834c8c46ee17581cc930a51984a0105284713248ea1
```

首次 v5 有效输出发现候选数超过原趋势 setup，违反预注册合同。根因是确认函数直接调用内部趋势生成器，绕过了 v1 顶层 `regime == Trend` 门禁。该输出作废；修复为完整复用 `generate_pa_candidate` 后重跑，未使用作废结果调整阈值。

## 15m 结果

### 原始候选

| 市场 | 已结算 | 平均R | 胜率 | PF | 两倍成本平均R |
| --- | ---: | ---: | ---: | ---: | ---: |
| BTC | 64 | -0.264 | 43.75% | 0.662 | -0.660 |
| ETH | 48 | -0.416 | 33.33% | 0.498 | -0.666 |
| SOL | 78 | -0.379 | 32.05% | 0.527 | -0.559 |
| BCH | 65 | -0.603 | 24.62% | 0.338 | -0.822 |
| 合并 | 255 | -0.414 | 33.33% | 0.500 | -0.672 |

所有市场均为负，说明失败不是由单一币种拖累。

### Walk-forward 与组合

| 项目 | 结果 |
| --- | ---: |
| 入选模型 | M0 无过滤 |
| 验证保留 | 90 / 90 |
| 验证平均R | -0.376 |
| 验证标准误 | 0.137 |
| 7日共享市场块 bootstrap 均值 | -0.414 |
| bootstrap 单侧95%下界 | -0.597 |
| 共享组合最终权益 | 82.00U |
| 共享组合最大回撤 | 18.01% |

M2/M3/M4 没有形成比 M0 更可靠的可部署过滤；组合回撤超过 15% 门槛。

## 1h 结果

`pa_trend_followthrough_1h` 只有 97 笔已结算候选，低于冻结的至少 100 笔研究门槛。训练器在模型竞赛前拒绝该批次，没有降低样本门槛，也没有输出可被误读为有效的模型分数。

该结果同时违反“至少100笔验证期交易”和“每个正向市场至少30笔”的晋级前提，因此 1h 分支直接归档为样本不足。

## 预注册门槛核对

| 条件 | 15m | 1h |
| --- | --- | --- |
| 平均R单侧95%下界大于0 | 失败 | 未训练 |
| 至少100笔验证期交易 | 失败，验证90笔 | 失败，总计97笔 |
| 胜率大于60% | 失败，33.33% | 未训练 |
| PF大于1.2 | 失败，0.500 | 未训练 |
| 两倍成本后为正 | 失败，-0.672R | 未训练 |
| 至少三个市场为正 | 失败，0个 | 未训练 |
| 共享组合回撤低于15% | 失败，18.01% | 未训练 |

## 结论

1. v5 的“突破确认后再追下一棒开盘”机制不成立；15m 比原 v4 弱信号更差，1h 样本不足。
2. 失败的主要机制不是模型复杂度，而是确认延迟使入场更接近短期扩张末端，同时沿用 setup 止损扩大了价格风险与成本 R。
3. 不在同一训练窗口放宽 `0.65`、`1.5ATR` 或突破条件，不做 symbol 专用优化。
4. 不打开密封 OOS，不创建 Shadow/Paper Challenger，不注册默认执行器，不修改 Vegas。
5. 下一轮若继续研究，应改变机制而不是微调本确认阈值；候选方向是“原 setup 后等待浅回踩但不追突破”的独立事件，并必须先形成 v6 预注册。

## 验证记录

- `cargo test -p rust-quant-analytics pa_quant_tree --lib -- --nocapture`：20/20 通过，包含 setup、确认、入场三时点回归。
- `cargo test -p rust-quant-cli --bin pa_quant_tree_15m_research -- --nocapture`：2/2 通过，覆盖默认 baseline 与 followthrough 策略键/协议隔离。
- `cargo check -p rust-quant-cli --bin pa_quant_tree_15m_research --bin pa_quant_tree_funding_backfill`：通过，只有仓库既有 warning。
- 指定文件 `rustfmt --check` 与 `git diff --check`：通过。
- PA 相关最大 Rust 文件为研究 CLI 814 行，低于 1000 行目标。
- `rust-quant-strategies` 的 test profile 仍被无关 `range_breakout_drop` 测试初始化缺少 `long_term_ema`、`price_below_long_term_ema` 阻塞；本阶段未修改该模块。PA 逻辑已由 analytics 依赖构建和三时点测试覆盖。
