# 策略探索日志

记录所有探索阶段的策略想法，包括成功和失败的尝试。

## 格式

```markdown
### YYYY-MM-DD: [策略名称]
- 假设: [一句话假设]
- 结果: Win Rate X% [✅/❌]
- 原因: [成功/失败原因]
- 教训: [关键发现]
- 状态: [升级生产/继续优化/已放弃]
```

---

## 2026-07-09: 1H Momentum Reversal（探索中）

- **假设**: 当 BTC/ETH 在 1H 上 RSI < 30 且价格偏离 EMA(20) > 2 ATR 且出现反转形态时，未来 4-12H 内反弹 > 8% 概率 > 58%
- **结果**: 探索阶段进行中...
- **目标**: Win Rate > 55%, 月 PnL > 10u
- **状态**: 原型代码已生成，等待回测

---

## 历史记录

### 2026-06: Momentum Breakout Scalper (5m) ❌
- 假设: 5m EMA 回调后 resume candle 入场
- 结果: Win Rate 69.5% 但 PnL -4.15u/月 ❌
- 原因: 止盈/止损比例失衡（0.8 vs 2.0 ATR），费率占比过高
- 教训: 5m scalping 数学天花板，pullback 逻辑在熊市中捕获死猫跳
- 状态: DEPRECATED

### 2026-06: Range Reversion Scalper (5m) ❌
- 假设: RSI < 30 + 布林带下轨，均值回归
- 结果: Win Rate 64.2% 但 PnL +3u/月 ❌（目标 20u）
- 原因: 费率拖累致命（占比 20-33%）
- 教训: 即使零费率毛利也只有 +4.85u，无法达到目标
- 状态: DEPRECATED
