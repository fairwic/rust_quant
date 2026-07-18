# ⚠️ DEPRECATED - Momentum Breakout Scalper Strategy

**状态**: 实验性 / 已废弃  
**原因**: 无法在目标约束下达到盈利要求  
**日期**: 2026-06

## 目标 vs 实际

| 指标 | 目标 | 最佳实现 | 状态 |
|------|------|----------|------|
| 胜率 | >60% | 33-69% | ⚠️ (取决于配置) |
| 频次 | 高(≥60/月) | 48-109/月 | ✅ |
| 回撤 | <10% | 8-15% | ⚠️ |
| **月PnL** | **≥20u/100u** | **负值** | **❌ 全部配置亏损** |

## 测试覆盖

- **参数组合**: 3,888 configs (单target) + 3,888 configs (3-tier trailing)
- **数据量**: 34,560根K线 (BTC/ETH 5m, 2个月)
- **市场环境**: 强熊市 (BTC -23%, ETH -31%)
- **止盈模式**: 
  - 单一目标 (T0.8-T2.2 × ATR)
  - 三档trailing (T1=0.8/1.0/1.5, T2=2.0/2.5/3.0, T3=4.0/5.0/6.0)

## 最优配置（损失最小）

```rust
MomentumBreakoutBacktestTuning {
    fast_ema_period: 20,
    slow_ema_period: 40,
    min_trend_strength_pct: 0.2,
    max_pullback_atr: 0.8,
    min_resume_body_ratio: 0.55,
    stop_atr_mult: 2.0,
    target_atr_mult: 0.8,  // 单目标模式
    // 或 3-tier trailing:
    // target_atr_mult_1: 1.5,
    // target_atr_mult_2: 3.0,
    // target_atr_mult_3: 6.0,
    cooldown_candles: 6,
    allow_short: true,
}
```

**结果**: 
- 单目标: Win 69.5%, Freq 52.5/月, PnL -4.15u/月, DD 8.1%
- Trailing: Win 33.3%, Freq 87/月, PnL -6.28u/月, DD 12.2%

## 为什么失败

1. **Pullback入场问题**: 在强趋势中的回调往往是反弹陷阱，不是真正的趋势恢复
   - 在熊市中，回调到EMA的"买点"实际是死猫跳
   - 策略在等待resume candle时错过真正趋势，捕获的是假突破

2. **止盈止损不对称的毁灭性**:
   - 单目标模式: TP=0.8 ATR, SL=2.0 ATR → 即使69%胜率，gross EV仍为负
   - Trailing模式: 大部分交易在达到level_1前就止损，胜率暴跌到33%

3. **冷却周期在震荡中失效**: cooldown=6根K线(30分钟)在5m震荡中错过真正机会

4. **市场环境致命**: 强熊市中趋势跟随需要纯做空，但pullback逻辑在下跌中捕获的是"抄底陷阱"

## 数学问题

```
最佳单目标配置 (Win 69.5%, T0.8, S2.0):
Gross EV = 0.695 × 0.8 - 0.305 × 2.0 = 0.556 - 0.61 = -0.054 ATR
加上0.1%费率 → 更负

Trailing配置 (Win 33%, T1.5/3.0/6.0, S2.0):
大多数交易未达T1即止损 → 有效TP≈0，有效SL=2.0
Win rate 33% << 盈亏平衡所需 66.7%
```

## 不推荐用于

- ❌ 任何生产环境
- ❌ 5分钟周期
- ❌ 熊市/震荡市
- ❌ Pullback-based入场逻辑

## 根本设计缺陷

该策略的核心逻辑"等待回调到均线后的恢复K线"在**真实趋势中滞后**、在**震荡中频繁假信号**：

- **真趋势**: 回调幅度小，resume candle出现时已错过大部分move
- **假突破**: 震荡中频繁触发，resume candle是噪音
- **熊市**: 所有"恢复"都是死猫跳，多头被屠杀

## 替代方向

如需趋势跟随策略，应考虑：
1. **纯趋势突破**（不等pullback，直接追入）+ 小仓位 + 宽止损
2. **反向策略**：在熊市中fade反弹（做空反弹到阻力位）
3. **更高周期**：日线/4小时的趋势更清晰，pullback更有效
4. **结合外部确认**：funding rate极值、OI异常、大单流入

---

**结论**: 策略技术实现正确（3档trailing等高级功能正常工作），但交易逻辑在测试的市场环境中fundamentally broken。在强趋势市场的5分钟周期上，pullback-resume模式无法产生正期望。标记为废弃，不推荐任何实盘使用。
