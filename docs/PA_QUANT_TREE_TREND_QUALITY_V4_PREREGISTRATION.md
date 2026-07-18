# PA Quant Tree 趋势入场质量 v4 预注册

## 实验身份

- 协议版本：`pa-training-v4-trend-quality-features`。
- Feature Registry：`pa-feature-registry-v2`。
- 阶段：训练期结构迭代，不是密封 OOS。
- 适用策略：`pa_trend_15m`、`pa_trend_1h`；区间策略保留作为负对照。
- 数据：沿用 v3 的四市场 OKX K 线与 Hyperliquid 单一 funding 代理。
- 冻结时间：在 v4 回测结果产生前完成。

## 不变合同

- 趋势候选仍要求 EMA20 趋势、最近三棒触碰 EMA、当前棒重新收回趋势侧。
- 入场仍为下一根已确认 K 线开盘。
- 止损仍为最近三棒结构极值外加 `0.1 ATR14`。
- 目标仍为 `1.5R`。
- 手续费、滑点、绝对资金费率代理、共享组合回放和 walk-forward 切分不变。
- 不新增 symbol 专用参数，不读取未来 K 线，不打开密封 OOS。

## 新增 Feature Primitive

### 1. directional_reclaim_atr

```text
多头：(signal_close - EMA20) / ATR14
空头：(EMA20 - signal_close) / ATR14
非趋势：0
```

机制假设：回撤触碰 EMA 后，收盘重新越过 EMA 的距离越充分，趋势恢复而非弱反抽的概率越高。

### 2. directional_close_strength

```text
raw_close_position = (close - low) / (high - low)
多头：raw_close_position
空头：1 - raw_close_position
非趋势：0.5
```

机制假设：信号棒收在趋势方向一端，比只有实体方向一致但收盘居中的 K 线更具跟随意义。

### 3. signal_range_atr

```text
(signal_high - signal_low) / ATR14
```

机制假设：过小的信号棒可能只是噪音，过大的信号棒可能已经透支短期空间；该特征允许受限模型识别质量区间。

### 4. pullback_close_fraction_3

```text
最近三棒中：
多头统计 close <= 对应 EMA20 的比例
空头统计 close >= 对应 EMA20 的比例
非趋势：0
```

机制假设：真实回撤通常至少包含一次反向收盘；只有影线触碰 EMA、但没有反向收盘的候选可能缺乏可重复结构。

## 模型特征集合

M2、M3、M4 统一允许使用：

```text
ema_slope_atr_20_5
range_efficiency_20
mean_overlap_ratio_8
always_in_score
signal_body_ratio
pullback_depth_atr_3
directional_reclaim_atr
directional_close_strength
signal_range_atr
pullback_close_fraction_3
```

M0无过滤、M1固定规则保持不变。

## 预注册成功与失败条件

v4 只有同时满足以下训练期条件，才允许形成后续独立验证假设：

1. walk-forward 成本后平均 R 大于0。
2. 相对M0的平均R改善不只来自少于30笔或低于10%覆盖率。
3. 两倍成本后的完整候选路径仍为正。
4. 四市场多数为正，不依赖单一币种。
5. 7日共享市场块 bootstrap 下界接近或大于0。
6. 共享组合最大回撤低于15%。

任一关键条件失败：记录失败实验，不修改本文件中的公式或阈值，不进入Shadow/Paper，不打开密封OOS。
