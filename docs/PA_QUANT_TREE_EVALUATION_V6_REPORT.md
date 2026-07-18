# PA Quant Tree 评估框架 v6 报告

## 协议身份

- 预注册：`docs/PA_QUANT_TREE_EVALUATION_V6_PREREGISTRATION.md`。
- Baseline：`pa-evaluation-v6-selected-oof-baseline`。
- Followthrough：`pa-evaluation-v6-selected-oof-followthrough`。
- 阶段：已查看训练窗口上的评估纠偏，不是外层 OOS 或 Forward Paper。
- 候选、特征、模型拟合、入场、止损和目标均未改变。

## 修正内容

模型竞赛现在为每个验证候选保留进程内 OOF 决策。入选家族的基础成本、两倍成本、共享组合、分市场结果和 7 日共享市场块 bootstrap 全部只读取 `keep=true` 的 OOF 路径。

报告明确冻结：

```text
out_of_fold=true
outer_validated=false
family_selection_adjusted=false
promotion_eligible=false
```

因此本报告修复了“bootstrap 重采样原始全集”的问题，但没有把现有 walk-forward 升级为 nested outer OOS。

## 15m Baseline OOF

| 策略 | 入选家族 | OOF保留 | 平均R | 两倍成本R | 胜率 | PF | Bootstrap下界 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `pa_trend_15m` | M2逻辑回归 | 114 / 940 | +0.090 | -0.005 | 47.37% | 1.156 | -0.238 |
| `pa_range_15m` | M0 | 284 / 284 | -0.678 | -1.324 | 33.80% | 0.386 | -0.894 |

`pa_trend_15m` OOF共享组合最终权益105.37U，最大回撤7.48%。该组合为正，但双倍成本均值和 block bootstrap 下界均未通过。

趋势分市场：

| 市场 | OOF样本 | 平均R | 两倍成本R |
| --- | ---: | ---: | ---: |
| BTC | 28 | +0.312 | +0.196 |
| ETH | 33 | -0.021 | -0.104 |
| SOL | 26 | +0.157 | +0.064 |
| BCH | 27 | -0.070 | -0.159 |

正向结果只覆盖 BTC、SOL，且单市场都少于30笔，不能证明跨市场稳定性。

## 1h Baseline OOF

| 策略 | 入选家族 | OOF保留 | 平均R | 两倍成本R | 胜率 | PF | Bootstrap下界 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `pa_trend_1h` | M0 | 348 / 348 | -0.127 | -0.260 | 40.23% | 0.810 | -0.306 |
| `pa_range_1h` | M0 | 60 / 60 | -0.484 | -0.780 | 26.67% | 0.489 | -0.782 |

两个1h分支保持失败，不需要为其增加模型复杂度。

## 15m Followthrough OOF

`pa_trend_followthrough_15m` 选择 M0，验证90笔：

```text
平均R                  -0.376
两倍成本平均R          -0.696
胜率                   36.67%
PF                     0.552
Bootstrap单侧95%下界  -0.818
```

跟随确认机制继续归档，不进入 A/B/C 以外的新策略开发。

## Legacy兼容

新增 OOF 明细通过 `serde(skip)` 隔离，聚合字段只在显式 `--evaluation-protocol selected-oof-v6` 下输出。默认15m baseline重跑与冻结 v4 JSON 字节级一致：

```text
sha256 1d4e38915cb0e89a1de10834c8c46ee17581cc930a51984a0105284713248ea1
```

## 决策

1. v4 M2 从“弱正向候选”降级为“待机制诊断线索”，不创建 Challenger。
2. 停止浅回踩 v6 和其他独立 PA 候选开发。
3. 下一步只做同 setup 的 A/B/C 配对反事实，区分确认筛选价值与延迟追价损失。
4. A/B/C 完成后建立统一实验账本和 Holm 校正，再接 Vegas Meta-filter。
5. 密封 OOS、Vegas 和生产执行路径保持不变。

## 验证记录

- PA analytics 定向测试：21/21 通过，新增 M0 全验证候选 OOF 记录测试。
- 研究 CLI 参数测试：3/3 通过，覆盖 legacy、followthrough 与显式 selected-oof-v6 协议。
- `pa_quant_tree_15m_research`、`pa_quant_tree_funding_backfill` 编译检查通过，只有仓库既有 warning。
- 指定文件 `rustfmt --check` 与 `git diff --check` 通过。
- 主研究 CLI 907 行、OOF 模块166行、trainer 604行，均低于1000行目标。
