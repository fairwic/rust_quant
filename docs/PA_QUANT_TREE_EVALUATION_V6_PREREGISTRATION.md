# PA Quant Tree 评估框架 v6 预注册

## 目标

修正 v1-v5 研究中“入选模型没有独立 OOF 路径 bootstrap”的统计缺口，不新增候选、特征、阈值、入场、止损或目标语义。

协议标识：

```text
pa-evaluation-v6-selected-oof-baseline
pa-evaluation-v6-selected-oof-followthrough
```

## 不变边界

- 默认 legacy 命令继续复现 v4/v5 JSON，不增加字段或改变联合指纹。
- 新评估必须显式使用 `--evaluation-protocol selected-oof-v6`。
- 继续使用已查看训练窗口，不打开密封 OOS。
- 不修改 Vegas，不创建新策略键，不注册默认执行器。
- 不产生 Shadow、Paper 或 Live PromotionRecord。

## OOF 决策合同

每个模型家族在每个 walk-forward 验证折必须为全部验证候选保存：

```text
fold_index
candidate_id
signal_ts
symbol
keep
net_r
```

其中 `keep` 只能由该折训练段拟合出的模型决定。验证候选的 `net_r` 只用于折后统计，不能反向参与该折模型训练或阈值选择。

OOF 明细只作为进程内研究证据，不写入 RuntimeManifest，也不加入 legacy JSON，避免改变冻结的 v4/v5 输出。

## 入选模型 OOF 诊断

模型家族仍按现有 one-standard-error 规则选择。选择完成后，只使用入选家族在各验证折产生的 OOF `keep=true` 路径计算：

- 基础成本 PerformanceMetrics。
- 两倍成本 PerformanceMetrics。
- 共享组合回放与最大回撤。
- 分市场指标。
- 7 日共享市场块 bootstrap 均值与单侧 95% 下界。

两倍成本和组合输入必须通过 `candidate_id` 回连原始结算结果；缺失、重复或不一致时拒绝生成报告。

## 解释限制

该诊断满足“验证候选没有参与本折拟合”，但模型家族仍由同一批 walk-forward 验证结果选择，因此：

```text
out_of_fold = true
outer_validated = false
family_selection_adjusted = false
promotion_eligible = false
```

它不能称为 nested outer OOS，也不能替代新增未来数据、外层 blocked test 或 Forward Paper。

## 验收标准

1. M0 的 OOF 决策数量等于全部验证候选数，且全部 `keep=true`。
2. 入选模型 OOF 保留数与 tournament 的 `kept_trade_count` 完全一致。
3. OOF 基础成本指标只读取验证折保留路径。
4. 两倍成本和组合路径通过同一 `candidate_id` 精确回连。
5. OOF bootstrap 只包含入选模型保留路径，不再重采样原始候选全集。
6. legacy v4 15m JSON 与冻结原件字节级一致。
7. 新协议报告明确携带四个解释限制字段。
8. 密封 OOS、Vegas 和生产执行路径保持不变。

## 后续顺序

本协议通过后，才允许实现：

1. v1/v5 的 A/B/C 同 setup 配对反事实。
2. 统一实验账本与 Holm 校正。
3. 冻结 Vegas 候选的 PA Meta-filter 配对研究。

在上述诊断完成前，不开发浅回踩 v6 候选。
