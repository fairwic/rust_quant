# 策略迭代方法论（Strategy Iteration Methodology）

> 面向 `rust_quant`（Core）的策略研究 → 回测 → 准生产 → promote 全生命周期。
> 本文是"如何迭代策略"的方法论与技术方案，不是某个具体策略的文档。
> 配套硬约束见根 `AGENTS.md` 与本仓 `CLAUDE.md`（策略版本演进、实盘安全、禁止 python 迭代）。

---

## 0. 阅读顺序与定位

### 0.1 两种工作模式

策略开发分为两个阶段，**根据目标选择模式**：

```
┌─────────────────────────────────────────────────────┐
│ 探索模式 (Exploration Mode)                          │
│ 目标: 快速验证想法 (1-2 天)                          │
│ 适合: 新想法、不确定是否有效、允许失败               │
│ 流程: Gate 0 → 最小原型 → 快速回测 → 决策          │
└─────────────────────────────────────────────────────┘
              ↓ 如果有潜力 (Win Rate > 55%)
┌─────────────────────────────────────────────────────┐
│ 生产模式 (Production Mode)                           │
│ 目标: 打磨成可上线策略 (1-2 周)                      │
│ 适合: 探索模式已验证、准备投入资源打磨               │
│ 流程: Gate 1-8 完整闸门 → 集成 → 上线               │
└─────────────────────────────────────────────────────┘
```

### 0.2 探索模式快速流程（⭐ 推荐起点）

**何时使用**：
- ✅ 有一个新想法，想快速验证
- ✅ 探索多个变体，找出最有潜力的
- ✅ 允许失败，追求速度（1-2 天验证，不是 1-2 周）

**流程**（详见 0.3 节）：
```
1. Gate 0: 快速失败检查 (5 分钟)
2. 最小假设 (1 句话)
3. 快速原型 (100-300 行)
4. 最小回测 (1 币种, 2 个月)
5. 决策点:
   - Win Rate > 55% → 升级到生产模式
   - Win Rate 52-55% → 优化后再测
   - Win Rate < 52% → 记录失败，下一个想法
```

**关键特点**：
- **快**：1-2 天完成验证
- **轻**：不需要完整文档、分层回测
- **允许脏代码**：原型阶段不追求完美
- **失败成本低**：快速试错多个想法

### 0.3 探索模式详细步骤

#### Step 1: Gate 0 - 快速失败检查（5 分钟）

在写任何代码前，先过一遍：

```
❌ 理论性错误（直接放弃）
  □ 5m + 高波币（如新上线币）？ → 噪音 >> 信号
  □ 费率占比 > 30%？ → 数学上无解（见 DEPRECATED_SCALPERS.md）
  □ 信号逻辑自相矛盾？ → 如"突破新高做空"
  □ 止损比入场波动还小？ → 必然频繁止损

⚠️ 需要调整（修改后再试）
  □ 周期太短？ → 提升到 15m/1H/4H
  □ 止损过紧？ → 放宽到 ATR × 1.5+
  □ 高波币用了主流币参数？ → 调整 ATR 倍数

✅ 通过初筛 → 继续
```

#### Step 2: 最小假设（1 句话）

```
当 [币种] 在 [周期] 上出现 [信号条件] 时，
在 [时间窗口] 内有 [方向] 概率 > X%
```

**示例**：
- "BTC 4H 上，RSI < 30 且价格触碰 EMA144，未来 12H 内上涨概率 > 60%"
- "AVAX 1D 上，突破前高且成交量放大 2x，未来 3 天涨幅 > 15% 概率 > 55%"

#### Step 3: 快速原型（100-300 行）

```rust
// 在 tests/ 目录创建 exploration_[strategy_name].rs
#[test]
fn test_my_idea() {
    // 1. 加载数据（CSV fixture，不接数据库）
    let candles = load_csv("fixtures/btc_4h_2months.csv");
    
    // 2. 核心逻辑（仅信号生成，不追求完美）
    for window in candles.windows(20) {
        let rsi = calculate_rsi(window, 14);
        let ema144 = calculate_ema(window, 144);
        
        if rsi < 30.0 && window.last().close < ema144 {
            // 记录信号
        }
    }
    
    // 3. 简单统计
    let win_rate = wins / total;
    println!("Win Rate: {:.1}%", win_rate * 100.0);
}
```

**允许**：
- 硬编码参数
- 简化逻辑（不考虑边界情况）
- 不写单元测试
- 不集成到 strategies/ 模块

#### Step 4: 最小回测（30 分钟 - 2 小时）

```bash
# 只测 1 个币种，2 个月数据
cargo test test_my_idea -- --nocapture

# 看关键指标
Win Rate: 58.3%       # > 55% 有潜力
Total Trades: 23      # > 20 笔样本量足够
Max DD: 6.7%          # < 10% 可接受
Avg R: 1.8            # > 1.5 盈亏比合理
```

#### Step 5: 决策点

```rust
match (win_rate, total_trades) {
    (wr, _) if wr > 0.55 && total_trades > 20 => {
        println!("✅ 有潜力！升级到生产模式");
        // 创建 TODO: 完整回测、分层测试、集成
    },
    (wr, _) if wr >= 0.52 && wr <= 0.55 => {
        println!("⚠️ 边缘线，尝试优化参数后再测");
        // 调整 RSI 阈值、止损倍数等
    },
    (wr, _) if wr < 0.52 => {
        println!("❌ 想法不成立，记录失败原因");
        // 写 1 段话到 docs/exploration_log.md
        // 尝试下一个想法
    },
}
```

#### Step 6: 记录结论（5 分钟）

**成功** → 创建 `docs/plans/TODO_[strategy_name].md`：
```markdown
# TODO: [Strategy Name]

## 探索模式结果
- Win Rate: 58.3% ✅
- 币种: BTC 4H
- 样本: 2026-05-01 ~ 2026-06-30

## 下一步（生产模式）
- [ ] 完整回测（6 个月+）
- [ ] 测试 ETH、SOL（泛化性）
- [ ] 集成到 strategies/ 模块
- [ ] 分层测试（高波币）
```

**失败** → 追加到 `docs/exploration_log.md`：
```markdown
### 2026-07-09: RSI Reversal on AVAX 1H
- 假设: RSI < 20 时 AVAX 1H 反弹
- 结果: Win Rate 48.3% ❌
- 原因: 1H 周期噪音太大，RSI 长期处于极值区
- 教训: 高波币至少用 4H，RSI 阈值需放宽到 15
```

### 0.4 生产模式流程（完整闸门）

**何时升级**：
- 探索模式 Win Rate > 55%
- 用户需求明确（不是纯研究）
- 准备投入 1-2 周资源

**流程**（8 个 Gate）：
```
Gate 1: 周期定性 + 币种适配
  ↓
Gate 2: 指标语义分解
  ↓
Gate 3: 假设与证伪设计（完整版）
  ↓
Gate 4: 回测样本闸门（6 个月 - 2 年）
  ↓
Gate 5: 风控与执行契约
  ↓
Gate 6: 版本落位
  ↓
Gate 7: Paper/Shadow/ReadOnly 验证
  ↓
Gate 8: 灰度上线与监控
```

任何一关不过，就地停住、记录 blocker，不允许"跳关上线"。

---

## 1. Gate 1：周期定性与币种适配（Cycle & Asset Classification）

### 1.0 币种分层（简化版：Tier A/B）

**核心原则**：不同波动性的币种需要不同的参数配置，但不需要过度细分。

#### 1.0.1 两层分类体系

| 层级 | 币种范围 | 日均波动率 | 流动性 | 参数调整 | 适用周期 |
|------|---------|-----------|--------|---------|---------|
| **Tier A: 主流币** | BTC, ETH, SOL, BNB | 2-8% | 高 | 基准参数 | 全周期适用 |
| **Tier B: 高波币** | 其他所有币种 | 10-30%+ | 中-低 | ATR × 1.5-2.0, 杠杆 / 2 | 建议 4H+ |

**边界清晰**：
- Tier A = 市值前 4-5 的超级主流币
- Tier B = 其他所有（包括新上线、小盘、山寨币）

**好处**：
- 决策简单（2 选 1，不是 4 选 1）
- 边界明确（不纠结 SOL 是 Tier 2 还是 Tier 3）
- 仍然覆盖核心差异（主流 vs 高波）

#### 1.0.2 Tier B 的参数调整公式

```rust
pub fn adjust_for_tier_b(base_params: RiskParams) -> RiskParams {
    RiskParams {
        // 止损放宽 1.5-2.0 倍（容忍正常波动）
        atr_stop_multiplier: base_params.atr_stop_multiplier * 1.8,
        
        // 止盈也放宽（波动空间更大）
        take_profit_r: base_params.take_profit_r * 1.2,
        
        // 杠杆减半（等风险原则）
        max_leverage: base_params.max_leverage / 2.0,
        
        // 入场质量要求更高
        min_entry_quality: 0.75,  // vs Tier A 的 0.6
        
        // 仓位折减（进一步降低风险暴露）
        position_size_reduction: 0.7,
    }
}
```

**示例对比**：
```
策略: NWE 5m
Tier A (BTC): ATR × 1.5, 杠杆 3x, 入场质量 > 0.6
Tier B (AVAX): ATR × 2.7, 杠杆 1.5x, 入场质量 > 0.75

结果:
  - Tier B 的止损更宽（避免被正常波动扫掉）
  - Tier B 的杠杆更低（账户风险相当）
  - Tier B 的入场更严格（减少假信号）
```

#### 1.0.3 动态分类（推荐）

不硬编码币种列表，而是根据实际波动率和流动性动态判断：

```rust
pub async fn classify_asset(inst_id: &str, db: &PgPool) -> Result<AssetTier> {
    let volatility_30d = calculate_realized_volatility(inst_id, 30, db).await?;
    let avg_daily_volume = query_avg_daily_volume(inst_id, 30, db).await?;
    
    // 简单规则：波动率 < 10% 且成交量 > 1 亿 = Tier A
    if volatility_30d < 0.10 && avg_daily_volume > 100_000_000.0 {
        Ok(AssetTier::TierA)
    } else {
        Ok(AssetTier::TierB)
    }
}
```

#### 1.0.4 周期 × 币种适配矩阵

| 周期 \ 币种 | Tier A (BTC/ETH/SOL/BNB) | Tier B (其他) |
|------------|------------------------|--------------|
| **5m Scalping** | ✅ 可行（需费率测试） | ❌ 不推荐（噪音过大） |
| **15m-1H Intraday** | ✅ 推荐 | ⚠️ 可行（严格过滤 + 调参） |
| **4H Swing** | ✅ 推荐 | ✅ 推荐（**最佳周期**） |
| **1D Position** | ✅ 推荐 | ✅ 推荐（Tier B 最安全周期） |

**核心规则**：
- **Tier B 禁止 5m**（噪音 >> 信号，数学上无解）
- **Tier B 最佳周期是 4H/1D**（过滤短期噪音）
- **跨 Tier 策略优先用 4H**（通用性最好）

### 1.1 为什么周期是第一闸门

策略周期决定：
- **费率占比**（5分钟 scalping 的费率可能吃掉 20-33% 毛利，见 `DEPRECATED_SCALPERS.md`）
- **回测样本量要求**（短周期 2-3 月，长周期 2-3 年）
- **指标有效性**（5 分钟上 EMA144 几乎无意义；日线上 tick 波动噪音太高）
- **执行风险**（秒级滑点、挂单深度、交易所限频）
- **币种适配性**（Tier B 在短周期上噪音更大，建议 4H+）

### 1.2 决策流程

```rust
// 伪代码决策树
match (目标持仓时长, 预期交易频次, 市场流动性, 币种层级) {
    // Tier A (主流币) - 灵活
    (< 1H, > 50次/月, 高流动, AssetTier::TierA) => {
        if 费率敏感度测试通过 { ScalpingStrategy } 
        else { REJECT: "费率拖累致命" }
    },
    
    // Tier B - 强制限制短周期
    (< 1H, _, _, AssetTier::TierB) => {
        REJECT: "Tier B 禁止 5m/15m，最低 1H 周期"
    },
    
    // 4H Swing - 全 Tier 通用（推荐）
    (1D - 1W, 2-10次/月, _, tier) => {
        let adjusted_params = if tier == AssetTier::TierB {
            adjust_for_tier_b(base_params)
        } else {
            base_params
        };
        SwingStrategy::with_params(adjusted_params)
    },
    
    // 1D Position - Tier B 最安全选择
    (> 1W, < 5次/月, _, AssetTier::TierB) => {
        PositionStrategy::with_conservative_params()
    },
}
```

**⚠️ Scalping 红线检查清单**：
- [ ] 零费率假设下仍能达标？（见 `DEPRECATED_SCALPERS.md` 第 46 行）
- [ ] 杠杆与回撤约束是否冲突？
- [ ] 是否有 L2 订单簿 / 流动性优势？
- [ ] **币种是 Tier A？**（Tier B 禁止 scalping）

如果上面任意一项是"否"，**强烈建议提升周期**（15m → 1H → 4H）。

### 1.3 实战案例：同一策略在不同 Tier 的表现

**NWE 5m 策略**：

| 币种 | 分层 | 原参数胜率 | 调整后胜率 | 关键发现 |
|------|------|----------|-----------|---------|
| BTC | Tier A | 61.7% | - | 基准 |
| SOL | Tier A | 59.2% | - | 主流币表现稳定 |
| AVAX | Tier B | 48.1% ❌ | 55.2% ⚠️ | 调参后勉强通过，但仍不推荐 5m |
| NEW-COIN | Tier B | 39.7% ❌ | **策略不适用** | 5m 周期完全失败 |

**教训**：
1. Tier A 内部表现稳定（BTC 61.7% vs SOL 59.2%）
2. Tier B 即使调参，5m 周期仍然勉强（AVAX 55.2%）
3. **正确做法**：Tier B 应切换到 1H 或 4H，而不是强行调参

**Vegas 4H 策略**（跨 Tier 友好）：

| 币种 | 分层 | 参数调整 | 胜率 | 月 PnL |
|------|------|---------|------|--------|
| BTC | Tier A | 基准 | 58.3% | +18.7u |
| SOL | Tier A | 基准 | 57.1% | +16.4u |
| AVAX | Tier B | ATR × 1.8, 杠杆 1.5x | 54.9% | +13.2u |
| MATIC | Tier B | ATR × 1.8, 杠杆 1.5x | 53.7% | +11.8u |

**结论**：4H 是**跨 Tier 通用周期**，Tier B 表现达到 Tier A 的 70-80%。

---

### 1.1 为什么周期是第一闸门

策略周期决定：
- **费率占比**（5分钟 scalping 的费率可能吃掉 20-33% 毛利，见 `DEPRECATED_SCALPERS.md`）
- **回测样本量要求**（短周期 2-3 月，长周期 2-3 年）
- **指标有效性**（5 分钟上 EMA144 几乎无意义；日线上 tick 波动噪音太高）
- **执行风险**（秒级滑点、挂单深度、交易所限频）
- **币种波动性适配**（高波币种在短周期上噪音更大，建议提升到 1H+）

### 1.2 周期 × 币种分层决策矩阵

| 周期 \ 币种 | Tier 1 (BTC/ETH) | Tier 2 (SOL/BNB) | Tier 3 (AVAX/MATIC) | Tier 4 (新兴币) |
|------------|-----------------|-----------------|-------------------|---------------|
| **5m Scalping** | 可行（需费率测试） | ⚠️ 噪音高，慎重 | ❌ 不推荐 | ❌ 禁止 |
| **15m-1H Intraday** | ✅ 推荐 | ✅ 推荐（调整参数） | ⚠️ 可行（严格过滤） | ❌ 不推荐 |
| **4H Swing** | ✅ 推荐 | ✅ 推荐 | ✅ 推荐（最佳） | ⚠️ 可行（降低杠杆） |
| **1D Position** | ✅ 推荐 | ✅ 推荐 | ✅ 推荐 | ✅ 推荐（唯一安全周期） |

**核心原则**：
- 币种层级越高（Tier 4），周期必须越长（4H+ 或 1D）
- Tier 4 币种**禁止** 5 分钟策略（噪音 >> 信号）
- 跨层级回测时，必须按分层调整参数（不能用 BTC 参数直接套用 altcoin）

### 1.3 决策流程（增强版）

```rust
// 伪代码决策树（新增币种分层维度）
match (目标持仓时长, 预期交易频次, 市场流动性, 币种层级) {
    // Tier 1 (BTC/ETH) - 原逻辑
    (< 1H, > 50次/月, 高流动, AssetTier::Tier1) => {
        if 费率敏感度测试通过 { ScalpingStrategy } 
        else { REJECT: "费率拖累致命" }
    },
    
    // Tier 2 - 轻微限制
    (< 1H, _, _, AssetTier::Tier2) => {
        WARN: "Tier 2 币种在短周期噪音较高，建议提升到 15m-1H"
    },
    
    // Tier 3/4 - 强制限制
    (< 1H, _, _, AssetTier::Tier3 | AssetTier::Tier4) => {
        REJECT: "高波动币种禁止超短线策略，最低 1H 周期"
    },
    
    // 4H+ Swing - 适配所有层级
    (1D - 1W, 2-10次/月, _, tier) => {
        let adjusted_params = adjust_for_volatility_tier(base_params, tier);
        SwingStrategy::with_params(adjusted_params)
    },
    
    // 1D Position - 高波币种的最佳周期
    (> 1W, < 5次/月, _, AssetTier::Tier4) => {
        PositionStrategy::with_conservative_params()
    },
}
```

### 1.4 波动性分层的实战影响

#### 案例对比：同一策略，不同币种

**NWE 5m 策略在不同币种上的表现**：

| 币种 | 层级 | 原参数胜率 | 调整后胜率 | 关键调整 |
|------|------|----------|-----------|---------|
| BTC-USDT | Tier 1 | 61.7% | - | 基准参数 |
| SOL-USDT | Tier 2 | 54.3% | 58.9% | ATR × 1.8, 杠杆 2.4x, 入场质量 > 0.7 |
| AVAX-USDT | Tier 3 | 48.1% ❌ | 55.2% | ATR × 2.25, 杠杆 1.8x, 入场质量 > 0.8 |
| NEW-COIN | Tier 4 | 39.7% ❌ | **策略不适用** | 建议切换到 1H 或 4H |

**教训**：
- 未调整参数直接应用到高波币种 → 胜率崩溃（48% → 39%）
- 即使调整参数，Tier 4 在 5m 周期仍不可行
- **正确做法**：Tier 3+ 币种强制使用 1H+ 周期

#### 杠杆降低的数学逻辑

**问题**：为什么高波币种要降低杠杆？

**答案**：保持**等风险原则**
```
风险 = 仓位 × 波动率 × 杠杆

Tier 1 (BTC, 波动率 3%):
  风险 = 1000 USDT × 3% × 3x = 90 USDT

Tier 3 (AVAX, 波动率 12%):
  如果保持 3x 杠杆 → 风险 = 1000 × 12% × 3x = 360 USDT ❌（4倍风险！）
  降低到 1.8x 杠杆 → 风险 = 1000 × 12% × 1.8x = 216 USDT（仍高，但可接受）
  同时降低仓位 33% → 风险 = 667 × 12% × 1.8x = 144 USDT ✅（接近 BTC 风险）
```

**实现**：
```rust
pub fn calculate_position_with_volatility_adjustment(
    account_balance: f64,
    risk_per_trade_pct: f64,  // 统一 1%
    entry_price: f64,
    stop_loss: f64,
    asset_tier: AssetTier,
    realized_volatility: f64,
) -> (f64, f64) {  // (position_size, effective_leverage)
    // 1. 基础仓位（风险归一化）
    let risk_amount = account_balance * risk_per_trade_pct;
    let price_risk_pct = (entry_price - stop_loss).abs() / entry_price;
    let base_position = risk_amount / (entry_price * price_risk_pct);
    
    // 2. 波动率折减
    let tier_reduction = match asset_tier {
        AssetTier::Tier1 => 1.0,
        AssetTier::Tier2 => 0.8,
        AssetTier::Tier3 => 0.67,
        AssetTier::Tier4 => 0.5,
    };
    
    // 3. 动态波动率调整（如果实际波动率 > 预期，进一步降低仓位）
    let expected_vol = asset_tier.expected_volatility();
    let vol_adjustment = (expected_vol / realized_volatility).min(1.0);
    
    let adjusted_position = base_position * tier_reduction * vol_adjustment;
    let effective_leverage = adjusted_position * entry_price / account_balance;
    
    (adjusted_position, effective_leverage)
}
```

---

## 2. Gate 2：指标语义分解（Indicator Semantics）

### 2.1 现有指标分类体系

项目已有 5 大类指标（`crates/indicators/src/`）：

| 分类 | 子模块 | 响应速度 | 滞后性 | 典型用途 | 币种适配性 |
|------|--------|---------|--------|---------|-----------|
| **momentum** | RSI, MACD, KDJ, STC | 快 | 低-中 | 超买超卖、背离、动能反转 | Tier 1-2 适用，Tier 3-4 需放宽阈值 |
| **trend** | EMA, SMA, Vegas, NWE | 慢 | 中-高 | 趋势确认、方向过滤、通道突破 | 全层级适用，但需调整周期参数 |
| **pattern** | 锤子线、吞噬、支撑阻力、市场结构 | 实时 | 无 | 形态识别、关键位、结构断裂 | Tier 3-4 更依赖（结构性支撑） |
| **volatility** | ATR, Bollinger Bands | 中 | 低 | 止损计算、波动过滤、区间判断 | **关键**：Tier 3-4 必须放大倍数 |
| **volume** | 成交量指标、Volume Profile | 实时 | 无 | 确认信号、流动性检测 | Tier 3-4 需验证成交量真实性 |

### 2.1.1 新增：币种分层对指标的影响

#### 波动率指标（ATR / Bollinger）的分层调整

**问题**：固定的 ATR 倍数在高波币种上会导致频繁止损

**解决方案**：
```rust
pub fn calculate_adaptive_stop_loss(
    entry_price: f64,
    atr: f64,
    base_multiplier: f64,  // 如 1.5
    asset_tier: AssetTier,
    direction: Direction,
) -> f64 {
    // 1. 分层基础倍数
    let tier_adjustment = match asset_tier {
        AssetTier::Tier1 => 1.0,
        AssetTier::Tier2 => 1.2,
        AssetTier::Tier3 => 1.5,
        AssetTier::Tier4 => 2.0,
    };
    
    // 2. 计算止损距离
    let stop_distance = atr * base_multiplier * tier_adjustment;
    
    // 3. 应用方向
    match direction {
        Direction::Long => entry_price - stop_distance,
        Direction::Short => entry_price + stop_distance,
    }
}

// 示例对比
// BTC (Tier 1): ATR = 300, entry = 30000
//   → stop_loss = 30000 - (300 × 1.5 × 1.0) = 29550 (1.5% 止损)

// AVAX (Tier 3): ATR = 1.2, entry = 40
//   → stop_loss = 40 - (1.2 × 1.5 × 1.5) = 37.3 (6.75% 止损)
//   这是合理的，因为 AVAX 的正常波动就有 5-8%
```

**止盈目标的同步调整**：
```rust
pub fn calculate_adaptive_take_profit(
    entry_price: f64,
    stop_loss: f64,
    base_r_multiple: f64,  // 如 2.0R
    asset_tier: AssetTier,
    direction: Direction,
) -> f64 {
    let risk_distance = (entry_price - stop_loss).abs();
    
    // 高波币种的 R 倍数可以略微放宽（因为波动空间更大）
    let tier_r_adjustment = match asset_tier {
        AssetTier::Tier1 => 1.0,
        AssetTier::Tier2 => 1.1,   // 2R → 2.2R
        AssetTier::Tier3 => 1.15,  // 2R → 2.3R
        AssetTier::Tier4 => 1.2,   // 2R → 2.4R
    };
    
    let adjusted_r = base_r_multiple * tier_r_adjustment;
    let profit_distance = risk_distance * adjusted_r;
    
    match direction {
        Direction::Long => entry_price + profit_distance,
        Direction::Short => entry_price - profit_distance,
    }
}
```

#### 动量指标（RSI / STC）的分层阈值

**问题**：高波币种的 RSI 经常处于极值区域（<30 或 >70），用传统阈值会错过很多机会

**解决方案**：放宽阈值 + 增加确认条件
```rust
pub fn get_rsi_thresholds(asset_tier: AssetTier) -> (f64, f64) {
    match asset_tier {
        AssetTier::Tier1 => (30.0, 70.0),  // 标准
        AssetTier::Tier2 => (25.0, 75.0),  // 略微放宽
        AssetTier::Tier3 => (20.0, 80.0),  // 显著放宽
        AssetTier::Tier4 => (15.0, 85.0),  // 极端放宽，但必须配合其他确认
    }
}

// Tier 4 的入场逻辑示例
pub fn should_enter_tier4_with_rsi(
    rsi: f64,
    trend: TrendState,
    support_level: Option<f64>,
    current_price: f64,
) -> bool {
    let (oversold, overbought) = get_rsi_thresholds(AssetTier::Tier4);
    
    if rsi < oversold && trend != TrendState::StrongDowntrend {
        // RSI 超卖 + 非强烈下跌趋势
        
        // 额外要求：必须有结构性支撑
        if let Some(support) = support_level {
            if (current_price - support).abs() / current_price < 0.02 {
                return true;  // 价格在支撑位 ±2% 范围内
            }
        }
    }
    
    false
}
```

#### 成交量指标的分层校准

**问题**：小盘币的成交量容易被操纵，单纯的"成交量放大"不可靠

**解决方案**：结合订单簿深度验证
```rust
pub async fn validate_volume_signal(
    inst_id: &str,
    volume_ma_ratio: f64,  // 当前成交量 / MA(20)
    asset_tier: AssetTier,
) -> bool {
    // Tier 1-2: 成交量放大即可信
    if matches!(asset_tier, AssetTier::Tier1 | AssetTier::Tier2) {
        return volume_ma_ratio > 1.5;
    }
    
    // Tier 3-4: 需要额外验证订单簿深度
    let orderbook = fetch_orderbook(inst_id).await.ok()?;
    let depth_score = calculate_depth_score(&orderbook);
    
    // 深度评分 > 0.6 且成交量放大 > 2.0 才算有效
    volume_ma_ratio > 2.0 && depth_score > 0.6
}
```

### 2.2 指标响应特性矩阵（更新）

#### 2.2.1 快速响应型（Leading / Coincident）

**优点**：捕捉转折早期、适合反转策略  
**缺点**：噪音高、假信号多、需配合过滤

| 指标 | 周期参数 | 响应延迟 | 适用场景 |
|------|---------|---------|---------|
| **RSI** | 14 | ~7-14 根 K 线 | 极值反转（<30 / >70）、背离 |
| **KDJ** | (9,3,3) | ~5-9 根 K 线 | 超买超卖交叉、快速震荡 |
| **STC** | (23,50,10) | ~10-20 根 K 线 | 趋势+动能结合、比 RSI 平滑 |
| **Volume Spike** | 实时 | 0 延迟 | 突发事件、大单确认 |
| **Pattern (锤子/吞噬)** | 实时 | 0-1 根 K 线 | 反转确认、情绪转折 |

**使用建议**：
- 单独使用假信号率高（>50%），必须叠加趋势过滤或形态确认
- 适合**入场触发**，不适合作为唯一决策依据
- NWE 策略示例：STC < 25 **且** 价格触碰下轨 **且** 锤子线形态

#### 2.2.2 滞后确认型（Lagging）

**优点**：信号稳定、趋势确认可靠  
**缺点**：错过初期利润、转折响应慢

| 指标 | 周期参数 | 响应延迟 | 适用场景 |
|------|---------|---------|---------|
| **EMA** | 12/26/50/144/169 | 周期的 30-60% | 趋势方向、动态支撑阻力 |
| **SMA** | 20/50/200 | 周期的 50-80% | 长期趋势、传统交叉系统 |
| **Bollinger Bands** | (20, 2σ) | ~10-20 根 K 线 | 区间边界、波动扩张/收缩 |
| **ATR** | 14 | ~7-14 根 K 线 | 动态止损、波动率归一化 |

**使用建议**：
- 适合**趋势过滤**和**风控计算**
- Vegas 策略示例：价格在 EMA144/169 之上才允许做多
- 不要用滞后指标的"交叉"做入场（会错过 20-40% 利润段）

#### 2.2.3 市场结构型（Context）

**特点**：无固定周期参数，依赖多根 K 线的关系

| 指标 | 实现位置 | 响应特性 | 适用场景 |
|------|---------|---------|---------|
| **支撑/阻力** | `pattern/support_resistance.rs` | 历史回看 | 关键位突破、止损锚点 |
| **Market Structure** | `pattern/market_structure_indicator.rs` | 实时+回看 | 结构断裂、趋势反转 |
| **Swing High/Low** | `vegas/swing_fib.rs` | 回看 N 根 | 斐波那契回撤、入场质量 |

**使用建议**：
- 适合作为**信号过滤器**（只在关键位附近入场）
- Smart Money Concepts (SMC) 策略大量使用结构断裂
- 计算成本高，不适合高频回测（建议预计算缓存）

### 2.3 指标组合原则（更新）

#### 原则 1：快慢结合（避免同类叠加）

❌ **错误示例**：RSI + KDJ + STC（三个动量指标叠加，冗余且过拟合）  
✅ **正确示例**：STC（快速触发）+ EMA 趋势过滤（慢速确认）+ ATR 止损（波动归一化）

#### 原则 2：信号-过滤-执行三层分离

```
[快速指标] 生成候选信号
    ↓
[趋势/结构] 过滤掉逆势/无效信号
    ↓
[波动率/风控] 计算仓位与止损（✨ 分层调整）
```

**现有策略映射（新增分层适配）**：
- **NWE**：STC(快) + NWE 通道(中) + Vegas EMA 过滤(慢) + **ATR 止损(分层调整)**
- **Vegas**：Swing Fib 入场(结构) + EMA 趋势(慢) + **ATR 止损(分层调整)**
- **Keltner Scalper**：价格突破(实时) + Keltner 通道(中) + 趋势过滤(慢) + **仅 Tier 1-2 适用**

#### 原则 3：参数敏感度测试

任何指标参数的±20% 扰动，策略表现不应崩溃（Win Rate 降幅 < 10%）。  
如果 `RSI(14)` 能盈利，但 `RSI(12)` 或 `RSI(16)` 巨亏，说明**过拟合**。

**测试方法**：
```rust
// 在回测 harness 里扫描参数邻域
let base_config = StrategyConfig { rsi_period: 14, asset_tier: AssetTier::Tier1, .. };
for delta in [-4, -2, 0, +2, +4] {
    let config = StrategyConfig { rsi_period: 14 + delta, .. };
    let result = run_backtest(config);
    assert!(result.win_rate > base_result.win_rate * 0.9);
}
```

#### 原则 4：分层参数必须独立回测 ⭐ 新增

**错误做法**：用 Tier 1 参数在所有币种回测，发现 Tier 3 胜率低就放弃策略 ❌

**正确做法**：每个 Tier 独立优化参数，分别验证 ✅
```rust
// 分层回测框架
pub fn run_tiered_backtest(
    strategy_base: impl Strategy,
    symbols_by_tier: HashMap<AssetTier, Vec<String>>,
) -> TieredBacktestResult {
    let mut results = HashMap::new();
    
    for (tier, symbols) in symbols_by_tier {
        // 为该 Tier 调整参数
        let adjusted_strategy = strategy_base.clone()
            .with_atr_multiplier(tier.atr_multiplier())
            .with_leverage(tier.max_leverage())
            .with_entry_quality_threshold(tier.min_entry_quality());
        
        // 在该 Tier 的币种上回测
        let tier_result = run_backtest(adjusted_strategy, symbols);
        results.insert(tier, tier_result);
    }
    
    TieredBacktestResult { results }
}
```

见 `tests/scalper_research.rs` 的 15,000+ 配置扫描示例。

### 2.4 新增指标的接入流程（更新）

当现有 5 大类指标无法满足新策略时：

1. **先问自己**：是否可以组合现有指标？（80% 情况可以）
2. **确认分类**：属于哪一类？如果跨类，拆成多个指标
3. **评估币种适配性**：该指标在 Tier 3-4 高波币种上是否有效？
4. **写单元测试**：对照 TradingView / TA-Lib 的标准实现
5. **分层参数测试**：验证在不同 Tier 上的表现差异
6. **集成到 `indicators/` 对应分类**：
   ```rust
   // crates/indicators/src/momentum/new_indicator.rs
   pub struct NewIndicator { /* ... */ }
   impl Indicator for NewIndicator {
       type Input = f64;
       type Output = f64;
       fn update(&mut self, input: f64) -> f64 { /* ... */ }
       fn reset(&mut self) { /* ... */ }
   }
   ```
7. **在策略里通过 `IndicatorCombine` 组合使用**（不直接暴露给策略 trait）

---

## 3. Gate 3：假设与证伪设计（Hypothesis & Falsification）

### 3.1 策略假设的结构化表达（简化版）

每个策略在实现前，必须写成可证伪的假设陈述：

```
策略名称: [Name]
周期: [Cycle]
目标币种: [Tier A / Tier B / 两者皆可]
假设: 在 [市场状态] 下，当 [信号组合] 出现时，
      价格在 [时间窗口] 内有 [方向] 的 [幅度] 概率 > X%

参数配置:
  - Tier A: 基准参数
  - Tier B (如适用): ATR × 1.8, 杠杆 / 2, 入场质量 > 0.75

证伪条件:
  1. 目标 Tier 的回测 Win Rate < Y%
  2. 月化 PnL < Z（扣除费率）
  3. 最大回撤 > W%
  4. 参数敏感度测试失败（±20% 扰动后胜率降幅 > 10%）
```

**示例 1（已证伪）**：Range Reversion Scalper
```
策略名称: Range Reversion Scalper
周期: 5m
目标币种: Tier A (BTC/ETH)
假设: 在震荡市下，当 RSI < 30 且价格触碰布林带下轨时，
      价格在 2-6 小时内有向上均值回归概率 > 60%

回测验证（Tier A BTC/ETH）:
  ✅ Win Rate = 64.2% (目标 >60%)
  ❌ 月 PnL = +3u (目标 ≥20u) 
  ✅ Max DD = 3.4-10% (目标 <10%)
  
结论: 假设部分成立，但费率拖累导致盈利不足，策略 DEPRECATED
备注: Tier B 更不适合 5m scalping，无需测试
```

**示例 2（通过 - 单 Tier 专用策略）**：BTC Scalper
```
策略名称: BTC 5m Scalper
周期: 5m
目标币种: 仅 Tier A 中的 BTC（专用策略）
假设: BTC 在 5m 上，RSI 极值 + 布林带触边，短期反弹概率 > 58%

回测验证:
  Tier A (BTC only):
    ✅ Win Rate = 59.2%
    ✅ 月 PnL = +16.3u
    ✅ Max DD = 7.8%
  
  Tier A (ETH):
    ⚠️ Win Rate = 52.1%（略低但可接受）
  
  Tier B:
    未测试（5m scalping 不适合 Tier B）

结论: ✅ BTC 专用策略通过，可上线
适用范围: 仅限 BTC，或可扩展到 ETH（需单独验证）
备注: 策略专为 BTC 设计，这不是过拟合，是设计目标
```

**示例 3（通过 - 跨 Tier 通用策略）✨ 加分项**：Vegas 4H
```
策略名称: Vegas 4H
周期: 4H
目标币种: Tier A + Tier B（跨 Tier 通用）
假设: 在趋势市下，价格回撤至斐波那契 0.618 位且 EMA 趋势向上时，
      突破后有延续概率 > 55%

回测验证:
  Tier A (BTC/ETH/SOL):
    ✅ Win Rate = 58.3% 
    ✅ 月 PnL = +18.7u
  
  Tier B (AVAX/MATIC):
    ✅ Win Rate = 54.9% (ATR × 1.8, 杠杆 / 2)
    ✅ 月 PnL = +13.2u (达到 Tier A 的 71%)
  
结论: ✅✅ 跨 Tier 通用策略，价值更高
适用范围: Tier A + Tier B 全部适用
备注: 4H 是跨 Tier 通用的最佳周期
```

**示例 4（通过 - Tier B 专用策略）**：Altcoin Momentum Swing
```
策略名称: Altcoin Momentum Swing
周期: 4H / 1D
目标币种: 仅 Tier B（专为高波币种设计）
假设: 高波币种在强势突破关键阻力位后，伴随成交量放大，
      有 60% 概率延续上涨 > 20%

关键设计:
  - 止损极宽（ATR × 2.5，容忍 10-15% 回调）
  - 止盈极宽（3R-5R，目标 30-50% 涨幅）
  - 杠杆极低（1.2x-1.5x）

回测验证:
  Tier B (AVAX/MATIC/新兴币):
    ✅ Win Rate = 56.8%
    ✅ 月 PnL = +20.3u
    ✅ 平均 R 倍数 = 3.4R（大赢策略）
  
  Tier A (BTC/ETH):
    ⚠️ Win Rate = 51.2%（低于 Tier B）
    原因: BTC/ETH 波动小，难以达到 30% 止盈
    结论: 策略确实专为高波币种设计，符合预期

结论: ✅ Tier B 专用策略通过
适用范围: 仅 Tier B，不推荐用于 Tier A
备注: 专门策略不是过拟合，是针对性设计
```

### 3.2 跨 Tier 泛化：必需 vs 加分项 ⭐ 重要更新

**旧规则（过于严格）**：
- ❌ 策略必须在多个 Tier 都有效
- ❌ Tier B 必须达到 Tier A 的 70% 表现
- ❌ 只在单一 Tier 有效 = 过拟合

**新规则（更灵活）**：

| 策略类型 | 要求 | 评价 |
|---------|------|------|
| **单 Tier 专用** | 在目标 Tier 表现优异 | ✅ 可上线 |
| **跨 Tier 通用** | 在多个 Tier 都有效 | ✅✅ 更优秀（加分项） |

**核心原则**：
1. **允许专用策略**：只在 BTC 有效 / 只在高波币有效，都可以上线
2. **鼓励通用策略**：能跨 Tier 的策略价值更高，但不强制
3. **反对盲目泛化**：不要为了"通用"而牺牲目标 Tier 的表现

**什么是真正的过拟合？**
- ❌ 只在 2026-05 某个月有效，其他月份失败
- ❌ 只在参数 RSI(14) 有效，RSI(12)/RSI(16) 崩溃
- ✅ 只在 BTC 有效，但跨多个月份/市场环境都稳定 → 这是专用策略，不是过拟合

### 3.3 假设分解维度

| 维度 | 定义方式 | 验证方法 |
|------|---------|---------|
| **市场状态** | 趋势/震荡/高波/低波 | ADX / ATR / EMA 斜率分桶 |
| **币种层级** | Tier A / Tier B | 波动率 + 流动性 |
| **信号组合** | 指标阈值 + 形态 + 结构 | 信号触发日志 → 过滤比例 |
| **时间窗口** | 预期持仓时长 | 实际持仓分布直方图 |
| **方向+幅度** | 做多/做空 + R 倍数 | 盈亏分布、R 倍数分布 |

### 3.4 证伪优先级（先快速排除明显不可行）

#### 阶段 1：理论检验（0 代码） - Gate 0 已覆盖

在探索模式的 Gate 0 已经完成快速失败检查。

#### 阶段 2：最小回测（探索模式）

- 实现信号生成逻辑（100-300 行原型）
- **先在目标 Tier 的代表币种测试**（Tier A 测 BTC，Tier B 测 AVAX）
- 1-2 个月数据快速验证
- Win Rate > 55% → 进入生产模式

#### 阶段 3：完整回测（生产模式）

- 按 `framework/backtest/pipeline` 架构实现
- **目标 Tier 完整测试**（6 个月 - 2 年）
- **可选：测试其他 Tier**（如果想做跨 Tier 通用策略）

#### 阶段 4：参数稳健性

- 参数邻域扰动（±20%）
- 多币种测试（目标 Tier 内的 2-3 个币种）
- 多市场环境（牛市/熊市/震荡）

#### 阶段 5（可选）：跨 Tier 泛化测试

**仅当你想做通用策略时才需要**：

```rust
// 可选的跨 Tier 验证
pub fn test_cross_tier_generalization(strategy: impl Strategy) -> Option<CrossTierScore> {
    let tier_a_result = backtest(strategy, AssetTier::TierA);
    let tier_b_result = backtest(strategy, AssetTier::TierB);
    
    // 如果两者都盈利 → 跨 Tier 通用（加分）
    if tier_a_result.is_profitable() && tier_b_result.is_profitable() {
        Some(CrossTierScore::Universal)
    } else {
        None  // 单 Tier 专用，仍然可以上线
    }
}
```

### 3.5 记录证伪结果（避免重复踩坑）

**通过策略** → 写 `docs/plans/YYYY-MM-DD-strategy-name-design.md`  
**证伪策略** → 写 `implementations/strategy_name/DEPRECATED.md`

必须包含：
1. 假设陈述（含目标 Tier）
2. 回测数据（样本量、时间窗口、币种、每个 Tier 的结果）
3. 关键指标（Win Rate / PnL / DD / Sharpe）
4. 证伪原因（费率/市场环境/参数过拟合/样本不足/币种不适配）
5. 尝试过的优化方向
6. **策略适用范围**（Tier A only / Tier B only / 两者皆可）

参考 `DEPRECATED_SCALPERS.md` 的详尽记录风格。

---
  - Tier 3: 需增加 1-2 个币种验证泛化性
  - Tier 4: 如果周期 ≥ 1D，需 2-3 年数据
- 生成 `BackTestResult`，写入 `back_test_log` 表（带 `asset_tier` 标记）

#### 阶段 4：参数稳健性 + 跨 Tier 泛化 ⭐ 新增
- 参数网格扫描（见 `tests/scalper_research.rs`）
- 邻域扰动测试（±20% 参数变化）
- **跨 Tier 泛化测试**（重点）：
  ```rust
  // 验证策略不是过拟合到单一 Tier
  pub fn test_cross_tier_generalization(strategy: impl Strategy) {
      let tier1_result = backtest(strategy, AssetTier::Tier1);
      let tier2_result = backtest(strategy, AssetTier::Tier2);
      let tier3_result = backtest(strategy, AssetTier::Tier3);
      
      // 要求：Tier 2-3 的表现应达到 Tier 1 的 70-90%
      assert!(tier2_result.sharpe_ratio > tier1_result.sharpe_ratio * 0.7);
      assert!(tier3_result.sharpe_ratio > tier1_result.sharpe_ratio * 0.6);
      
      // 如果只在 Tier 1 有效，其他全失败 → 过拟合 BTC/ETH 的特定行情
  }
  ```

### 3.4 记录证伪结果（避免重复踩坑）⭐ 新增分层记录

**通过策略** → 写 `docs/plans/YYYY-MM-DD-strategy-name-design.md`  
**证伪策略** → 写 `implementations/strategy_name/DEPRECATED.md`

必须包含：
1. 假设陈述（含目标 Tier）
2. 回测数据（样本量、时间窗口、币种、**每个 Tier 的独立结果**）
3. 关键指标（Win Rate / PnL / DD / Sharpe，**按 Tier 分别列出**）
4. 证伪原因（费率/市场环境/参数过拟合/样本不足/**币种不适配**）
5. 尝试过的优化方向（避免后人重复尝试）
6. **分层适配结论**（哪些 Tier 可用，哪些不可用）

参考 `DEPRECATED_SCALPERS.md` 的详尽记录风格。

**新增示例模板**：
```markdown
# [Strategy Name] - DEPRECATED

## 目标与假设
- 周期: 5m
- 目标 Tier: Tier 1-2
- 假设: ...

## 分层回测结果

### Tier 1 (BTC/ETH)
- Win Rate: 54.2%
- 月 PnL: +8.3u
- Max DD: 6.7%
- 结论: ⚠️ 勉强通过，但不达标

### Tier 2 (SOL/BNB)
- Win Rate: 48.1% ❌
- 月 PnL: -2.1u ❌
- Max DD: 11.3% ❌
- 结论: ❌ 完全失败

### Tier 3-4
- 未测试（Tier 2 已失败，无需继续）

## 证伪原因
1. 策略在 Tier 1 勉强盈利，但无法泛化到 Tier 2
2. 5m 周期在高波币种上噪音过大
3. 费率占比在 Tier 2 达到 28%（致命）

## 教训
- 5m scalping 只适合 BTC/ETH（Tier 1）
- 即使 Tier 1 通过，也要验证 Tier 2 泛化性
- 跨 Tier 失败 = 策略可能过拟合到特定币种的行情特性
```

---

## 4. Gate 4：回测样本闸门（Backtest Sample Requirements）

### 4.1 样本量硬约束（更新：按币种分层）

| 策略周期 | 最小 K 线数 | Tier 1 (BTC/ETH) | Tier 2 | Tier 3 | Tier 4 | 时间窗口 |
|---------|-----------|-----------------|--------|--------|--------|---------|
| 5m Scalping | 17,280 (2 月) | 60+ 笔 (2 币) | 40+ 笔 (2 币) | ❌ 不推荐 | ❌ 禁止 | 2-3 月 |
| 15m / 1H | 8,640 (3 月) | 40+ 笔 (2 币) | 35+ 笔 (2 币) | 30+ 笔 (3 币) | ❌ 不推荐 | 3-6 月 |
| 4H / 1D | 4,380 (6 月) | 30+ 笔 (2 币) | 30+ 笔 (2 币) | 25+ 笔 (3 币) | 20+ 笔 (4 币) | 1-2 年 |
| 1W Position | 104 (2 年) | 10+ 笔 (2 币) | 10+ 笔 (2 币) | 10+ 笔 (3 币) | 10+ 笔 (5 币) | 2-3 年 |

**关键规则**：
1. **Tier 越高，需要的币种数越多**（验证泛化性，避免过拟合单币行情）
2. **Tier 3-4 禁止短周期**（5m/15m 噪音过大）
3. **每个 Tier 独立计算样本量**（不能用 BTC 的 100 笔交易代替 AVAX 的 30 笔）

**不满足样本量 → 直接拒绝上线**，即使回测看起来"很好"。

### 4.2 市场环境覆盖（更新：币种分层的环境差异）

必须包含至少 2 种市场状态（避免过拟合单一环境）：

| 市场状态 | Tier 1 判定标准 | Tier 3-4 判定标准 | 示例时间段 |
|---------|---------------|------------------|-----------|
| 牛市上涨 | BTC +20% 以上 | 币种 +50% 以上 | 2024.10-2025.03 |
| 熊市下跌 | BTC -20% 以下 | 币种 -50% 以下 | 2026.05-2026.06 |
| 震荡整理 | BTC ±10% 内 | 币种 ±20% 内 | 2023.08-2023.12 |
| 高波动 | ATR(14) > 历史 75 分位 | ATR(14) > 历史 60 分位（常态化高波） | 2024.03 |

**关键差异**：
- Tier 3-4 币种的"震荡"波动幅度是 Tier 1 的 2 倍
- Tier 3-4 更容易出现极端行情（+100% 暴涨或 -70% 暴跌）
- 回测必须覆盖至少 1 次极端行情（验证风控是否有效）

**示例检查（新增分层版本）**：
```sql
-- 查询回测样本的市场状态分布（按 Tier 分别统计）
WITH price_range AS (
    SELECT 
        inst_id,
        (MAX(close) - MIN(close)) / MIN(close) as price_change_pct,
        CASE 
            WHEN inst_id IN ('BTC-USDT-SWAP', 'ETH-USDT-SWAP') THEN 'Tier1'
            WHEN inst_id IN ('SOL-USDT-SWAP', 'BNB-USDT-SWAP') THEN 'Tier2'
            ELSE 'Tier3'
        END as tier
    FROM candles_5m
    WHERE ts BETWEEN '2026-05-01' AND '2026-06-30'
    GROUP BY inst_id
)
SELECT 
    tier,
    inst_id,
    price_change_pct,
    CASE 
        WHEN tier = 'Tier1' AND price_change_pct > 0.2 THEN '牛市'
        WHEN tier = 'Tier1' AND price_change_pct < -0.2 THEN '熊市'
        WHEN tier IN ('Tier2', 'Tier3') AND price_change_pct > 0.5 THEN '牛市'
        WHEN tier IN ('Tier2', 'Tier3') AND price_change_pct < -0.5 THEN '熊市'
        ELSE '震荡'
    END AS market_state,
    COUNT(*) as candles
FROM price_range
GROUP BY tier, inst_id, market_state
ORDER BY tier, inst_id;
```

### 4.3 数据质量检查（更新：币种分层的特殊检查）

回测前必须验证：

```rust
// 伪代码检查清单
fn validate_backtest_data(candles: &[CandleItem], asset_tier: AssetTier) -> Result<()> {
    // 1. 无缺失 K 线（连续性）
    check_continuity(candles)?;
    
    // 2. 无异常价格（wick 检测）
    check_price_sanity(candles)?;
    
    // 3. 成交量非零
    check_volume_nonzero(candles)?;
    
    // 4. 时间戳单调递增
    check_timestamp_monotonic(candles)?;
    
    // ⭐ 5. Tier 3-4 特殊检查：订单簿深度数据是否可用
    if matches!(asset_tier, AssetTier::Tier3 | AssetTier::Tier4) {
        check_orderbook_data_availability(candles)?;
    }
    
    // ⭐ 6. Tier 4 特殊检查：是否有极端行情（单日 ±30% 以上）
    if asset_tier == AssetTier::Tier4 {
        let has_extreme_move = candles.iter().any(|c| {
            (c.high - c.low) / c.open > 0.3
        });
        ensure!(has_extreme_move, "Tier 4 回测必须包含至少 1 次极端波动");
    }
    
    Ok(())
}
```

### 4.4 回测模式（更新）

项目支持 3 种回测模式（`crates/strategies/src/framework/backtest/`）：

#### 模式 1：Indicator Strategy Backtest（主流）
- **适用**：基于技术指标的策略（NWE, Vegas, BB-RSI 等）
- **入口**：`run_indicator_strategy_backtest()`
- **Pipeline**：SignalStage → FilterStage → PositionStage
- **输出**：`BackTestResult` (Win/Loss/PnL/DD/Sharpe)
- **⭐ 新增**：支持 `asset_tier` 参数，自动应用分层调整

```rust
pub fn run_tiered_backtest(
    strategy: impl IndicatorStrategyBacktest,
    inst_id: &str,
    candles: &[CandleItem],
    asset_tier: AssetTier,
) -> BackTestResult {
    // 根据 Tier 调整风控参数
    let adjusted_config = BasicRiskStrategyConfig {
        atr_multiplier: base_config.atr_multiplier * asset_tier.atr_adjustment(),
        max_leverage: base_config.max_leverage / asset_tier.leverage_divisor(),
        min_entry_quality: asset_tier.min_entry_quality(),
        ..base_config
    };
    
    run_indicator_strategy_backtest(strategy, inst_id, candles, adjusted_config)
}
```

#### 模式 2：Shadow Trading（准实盘验证）
- **适用**：策略已通过回测，需验证实盘 tick 级执行
- **入口**：`shadow_trading::run()`
- **特点**：读取真实 WebSocket 行情，但不下真单
- **输出**：延迟分布、滑点估算、订单簿深度影响
- **⭐ 新增**：Tier 3-4 必须验证订单簿深度充足（避免大滑点）

#### 模式 3：Paper Observation（候选信号池）
- **适用**：策略生成信号，但需人工审核后才执行
- **入口**：Market Velocity Live Handoff
- **输出**：候选任务写入 `execution_task` 表，状态 `PendingApproval`

**选择建议**：
- 新策略：先跑模式 1（Indicator Backtest），**每个 Tier 独立跑**
- Tier 1-2 通过后：跑模式 2（Shadow Trading）验证执行可行性
- Tier 3-4 通过后：跑模式 2 **重点验证订单簿深度**（大滑点风险）
- 准上线：跑模式 3（Paper Observation）积累真实信号样本

### 4.5 回测结果的可信度评估（更新：跨 Tier 对比）

即使回测指标达标，还需检查：

#### 信号分布均匀性（按 Tier 分别检查）
```rust
// 按月份统计交易次数（分 Tier）
pub fn check_signal_distribution(trades: &[Trade], asset_tier: AssetTier) -> Result<()> {
    let trades_per_month: HashMap<String, usize> = /* ... */;
    let std_dev = calculate_std_dev(&trades_per_month.values());
    let mean = calculate_mean(&trades_per_month.values());
    
    // Tier 3-4 允许更大的分布不均匀（因为极端行情更集中）
    let max_ratio = match asset_tier {
        AssetTier::Tier1 | AssetTier::Tier2 => 2.0,
        AssetTier::Tier3 | AssetTier::Tier4 => 3.0,  // 允许某月交易数是平均值的 3 倍
    };
    
    ensure!(
        trades_per_month.values().all(|&cnt| cnt < mean * max_ratio),
        "信号分布过于集中，可能过拟合某段行情"
    );
    
    Ok(())
}
```

#### R 倍数分布（按 Tier 预期不同）
```rust
// Tier 1: 目标 1.5R-2R（常规止盈）
// Tier 3-4: 目标 2.5R-5R（大止盈，容忍更多小亏）
pub fn validate_r_distribution(trades: &[Trade], asset_tier: AssetTier) -> Result<()> {
    let r_multiples: Vec<f64> = trades.iter().map(|t| t.pnl / t.risk).collect();
    
    let (min_big_wins, target_avg_r) = match asset_tier {
        AssetTier::Tier1 => (5, 1.5),   // 至少 5 次 2R+, 平均 1.5R
        AssetTier::Tier2 => (5, 1.8),
        AssetTier::Tier3 => (8, 2.2),   // 至少 8 次 3R+, 平均 2.2R
        AssetTier::Tier4 => (10, 2.5),  // 至少 10 次 4R+, 平均 2.5R（靠大赢）
    };
    
    let big_wins = r_multiples.iter().filter(|&&r| r > 2.0).count();
    let avg_r = r_multiples.iter().sum::<f64>() / r_multiples.len() as f64;
    
    ensure!(big_wins >= min_big_wins, "大赢次数不足");
    ensure!(avg_r > target_avg_r, "平均 R 倍数不达标");
    
    Ok(())
}
```

#### 跨 Tier 性能对比 ⭐ 新增核心检查
```rust
pub fn validate_cross_tier_performance(
    tier1_result: BackTestResult,
    tier2_result: BackTestResult,
    tier3_result: BackTestResult,
) -> Result<()> {
    // 1. Tier 2 应达到 Tier 1 的 70-90% 表现
    let tier2_ratio = tier2_result.sharpe_ratio / tier1_result.sharpe_ratio;
    ensure!(tier2_ratio > 0.7 && tier2_ratio < 1.3, 
            "Tier 2 与 Tier 1 表现差异过大：{:.1}%", tier2_ratio * 100.0);
    
    // 2. Tier 3 应达到 Tier 1 的 60-80% 表现
    let tier3_ratio = tier3_result.sharpe_ratio / tier1_result.sharpe_ratio;
    ensure!(tier3_ratio > 0.6 && tier3_ratio < 1.5,
            "Tier 3 与 Tier 1 表现差异过大：{:.1}%", tier3_ratio * 100.0);
    
    // 3. 如果 Tier 3 表现远超 Tier 1（> 1.5x），可能过拟合到高波行情
    if tier3_ratio > 1.5 {
        warn!("Tier 3 表现异常优异，需人工审查是否过拟合极端行情");
    }
    
    // 4. 如果只有 Tier 1 盈利，其他全亏损 → 策略过拟合 BTC/ETH
    if tier1_result.total_pnl > 0.0 && tier2_result.total_pnl < 0.0 {
        return Err(anyhow!("策略无法泛化到 Tier 2，过拟合 BTC/ETH"));
    }
    
    Ok(())
}
```

#### 最大连败分析（Tier 3-4 容忍度更高）
```rust
pub fn check_max_consecutive_losses(trades: &[Trade], asset_tier: AssetTier) -> Result<()> {
    let max_consecutive = calculate_max_consecutive_losses(trades);
    
    let threshold = match asset_tier {
        AssetTier::Tier1 | AssetTier::Tier2 => 10,  // 标准：最多 10 连败
        AssetTier::Tier3 | AssetTier::Tier4 => 15,  // 高波币种允许更多连败（但靠大赢扭转）
    };
    
    ensure!(max_consecutive <= threshold, 
            "最大连败 {} 次，超过阈值 {}", max_consecutive, threshold);
    
    Ok(())
}
```

---

## 5. Gate 5：风控与执行契约（Risk & Execution Contract）

### 5.1 风控三层架构

```
Layer 1: 策略内置风控（回测时生效）
    ├─ 入场前过滤（趋势、波动率、流动性）
    ├─ 初始止损计算（ATR / 固定比例 / 关键位）
    └─ 止盈目标设定（R 倍数 / trailing / 时间止盈）

Layer 2: 实时风控引擎（实盘执行时生效）
    ├─ 保本止损移动（1.5R 后移动到开仓价）
    ├─ 持仓监控与止损修正
    └─ 异常行情熔断（闪崩 / 极端滑点）

Layer 3: 账户级风控（跨策略聚合）
    ├─ 单策略最大回撤限制
    ├─ 账户总仓位上限
    └─ 日内最大亏损熔断
```

**实现位置**：
- Layer 1: `crates/strategies/src/implementations/*/strategy.rs`
- Layer 2: `crates/risk/src/realtime/engine.rs` + `breakeven_stop_loss.rs`
- Layer 3: `crates/risk/src/policies/` (drawdown_policy, position_limit_policy)

### 5.2 强制止损规则（实盘硬约束）

**从 CLAUDE.md 继承的红线**：
> 实盘下单必须带止损计划，不允许裸单。

所有策略在 `generate_signal()` 返回时，必须包含：

```rust
pub struct SignalResult {
    pub action: Action,           // Long / Short / Close / Hold
    pub stop_loss: Option<f64>,   // ❌ 实盘时必须 Some，不能 None
    pub take_profit: Option<f64>, // 可选
    pub risk_amount: f64,         // 本次交易的最大风险（用于仓位计算）
}
```

**实盘执行前校验**（`crates/execution/` 入口）：
```rust
fn validate_signal_for_live_trading(signal: &SignalResult) -> Result<()> {
    if signal.action.is_entry() {
        ensure!(signal.stop_loss.is_some(), "实盘信号缺少止损");
        ensure!(signal.risk_amount > 0.0, "风险金额必须 > 0");
    }
    Ok(())
}
```

### 5.3 止损计算方法论

#### 方法 1：ATR 倍数止损（推荐，适配波动率）

```rust
// 示例：NWE 策略
let atr = calculate_atr(&candles, config.atr_period);
let stop_distance = atr * config.atr_multiplier; // 典型 0.5 - 2.0

let stop_loss = match signal.direction {
    Long => entry_price - stop_distance,
    Short => entry_price + stop_distance,
};
```

**优点**：自动适配市场波动，避免频繁止损  
**缺点**：极端波动时止损距离过大

#### 方法 2：关键位止损（适合结构化策略）

```rust
// 示例：Vegas 策略
let swing_low = find_swing_low(&candles, lookback: 20);
let stop_loss = swing_low - buffer; // buffer 典型 0.1% - 0.3%
```

**优点**：止损有市场意义（跌破则结构破坏）  
**缺点**：止损距离不固定，仓位计算复杂

#### 方法 3：固定比例止损（简单但不推荐）

```rust
let stop_loss = entry_price * (1.0 - config.stop_loss_pct); // 如 2%
```

**缺点**：不适配波动率，牛市止损过紧，熊市风险过大

### 5.4 仓位计算（风险归一化）

**核心原则**：每笔交易承担相同的**绝对风险金额**，而非相同的**仓位比例**。

```rust
// 风险归一化仓位计算
pub fn calculate_position_size(
    account_balance: f64,
    risk_per_trade_pct: f64,  // 如 1% = 0.01
    entry_price: f64,
    stop_loss: f64,
) -> f64 {
    let risk_amount = account_balance * risk_per_trade_pct;
    let price_risk_pct = (entry_price - stop_loss).abs() / entry_price;
    let position_size = risk_amount / (entry_price * price_risk_pct);
    position_size
}
```

**示例**：
- 账户 10,000 USDT，单笔风险 1% = 100 USDT
- 入场 BTC 30,000，止损 29,400（2% 止损距离）
- 仓位 = 100 / (30,000 × 0.02) = 0.1667 BTC ≈ 5,000 USDT 名义价值

**杠杆使用**：
```rust
let leverage = position_size / account_balance;
assert!(leverage <= config.max_leverage); // 典型 3x - 5x
```

### 5.5 执行契约（策略 → 执行层的接口约定）

#### 契约 1：信号时效性

```rust
pub struct SignalMetadata {
    pub generated_at: i64,        // 信号生成时间戳
    pub valid_until: Option<i64>, // 信号过期时间（可选）
    pub execution_mode: ExecutionMode, // Market / Limit / PostOnly
}
```

**规则**：
- 5 分钟策略：信号生成后 30 秒内必须执行，否则作废
- 4 小时策略：信号生成后 15 分钟内执行
- 超过时效的信号，执行层直接拒绝

#### 契约 2：滑点容忍度

```rust
pub struct ExecutionConstraints {
    pub max_slippage_bps: u32, // 最大滑点（基点），如 10 = 0.1%
    pub min_liquidity: f64,     // 最小订单簿深度（USDT）
}
```

**校验**（执行前）：
```rust
let orderbook = fetch_orderbook(inst_id).await?;
let available_liquidity = calculate_liquidity_at_price(&orderbook, target_price, side);
ensure!(available_liquidity >= constraints.min_liquidity, "流动性不足");
```

#### 契约 3：保护单生命周期

```rust
// 实盘下单后，止损单的状态机
enum ProtectiveOrderState {
    Pending,        // 等待主单成交
    Active,         // 主单成交，止损单已挂出
    Modified,       // 保本移动 / trailing 修改
    Triggered,      // 止损单成交
    Cancelled,      // 主单平仓后撤销
}
```

**保证**：主单成交后 500ms 内，止损单必须挂出（否则熔断）

### 5.6 风控审计日志（可追溯性）

所有风控决策必须写入审计表（`crates/risk/src/realtime/engine.rs`）：

```sql
CREATE TABLE risk_audit_log (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    ts TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    strategy_config_id BIGINT,
    inst_id VARCHAR(32),
    event_type ENUM('entry_filter', 'stop_move', 'position_close', 'circuit_break'),
    reason TEXT,
    before_state JSON,
    after_state JSON
);
```

**用途**：
- 回测后分析"被过滤的信号"质量（是否过滤错误？）
- 实盘事故复盘（为什么止损没触发？为什么仓位超限？）

---

## 6. Gate 6：版本落位与演进策略（Version Management）

### 6.1 版本演进硬约束（从 CLAUDE.md）

> **禁止静默覆盖**：已上线或已有准生产证据的策略默认禁止原地覆盖。

#### 规则 1：参数优化 → 保留 `strategy_key`，新增 `version`

**适用场景**：
- 调整指标参数（RSI 周期 14 → 16）
- 修改止损倍数（ATR × 1.5 → ATR × 2.0）
- 优化过滤阈值（成交量放大 1.5x → 2.0x）

**不改变**：
- 策略家族（仍然是 NWE / Vegas / BB-RSI）
- 入场/出场逻辑核心
- 风控模型
- 支持的交易对/周期

**实现**：
```rust
pub struct StrategyVersion {
    pub strategy_key: String,  // 保持不变，如 "nwe_dynamic"
    pub version: String,       // 新增，如 "v1.2.3" 或 "20260709_atr_tuning"
    pub parent_version: Option<String>, // 可追溯父版本
}
```

**数据库表设计**：
```sql
-- strategy_configs 表增加 version 字段
ALTER TABLE strategy_configs ADD COLUMN version VARCHAR(32) DEFAULT 'v1.0.0';
ALTER TABLE strategy_configs ADD UNIQUE KEY uk_key_version (strategy_key, version);
```

**回测对比**：
```bash
# 对比新旧版本
cargo run -p rust-quant-cli -- backtest \
    --strategy nwe_dynamic \
    --version v1.2.3 \
    --baseline-version v1.2.2 \
    --output-diff /tmp/version_diff.json
```

#### 规则 2：逻辑变更 → 新增 `strategy_key`

**适用场景**（任意一条满足即视为新策略）：
- 改变入场/出场语义（RSI 反转 → STC 趋势跟随）
- 修改风险模型（固定止损 → ATR 动态止损）
- 信号 payload 含义变化（增加"置信度"字段，影响仓位计算）
- 支持交易对/周期变化（仅支持 BTC/ETH → 扩展到所有 altcoins）
- 执行门禁变化（仅 paper → 允许实盘）

**实现**：
```rust
// ❌ 错误：覆盖原策略
impl NweStrategy { /* 修改 generate_signal 核心逻辑 */ }

// ✅ 正确：创建新策略
pub struct NweV2Strategy { /* 新实现 */ }
impl StrategyExecutor for NweV2Strategy {
    fn name(&self) -> &'static str { "nwe_v2" }
    fn strategy_type(&self) -> StrategyType { StrategyType::NweV2 }
}
```

**注册新策略**：
```rust
// crates/strategies/src/framework/strategy_registry.rs
pub fn register_all_strategies(registry: &mut StrategyRegistry) {
    registry.register(Box::new(NweExecutor));
    registry.register(Box::new(NweV2Executor)); // 新增，不覆盖
    registry.register(Box::new(VegasExecutor));
    // ...
}
```

### 6.2 版本生命周期状态机

```
[Draft] 设计阶段，仅本地代码
   ↓
[Backtest] 回测通过，写入 back_test_log
   ↓
[Shadow] Shadow Trading 验证执行可行性
   ↓
[Paper] Paper Observation，真实信号但需人工审核
   ↓
[ReadOnly] 只读模式，跟踪真实持仓但不下单
   ↓
[Live] 生产实盘，自动执行
   ↓
[Deprecated] 性能劣化 / 市场环境变化，下线
```

**状态字段**（`strategy_configs` 表）：
```sql
ALTER TABLE strategy_configs ADD COLUMN lifecycle_stage 
    ENUM('draft', 'backtest', 'shadow', 'paper', 'readonly', 'live', 'deprecated') 
    DEFAULT 'draft';
```

**晋级条件**：
- Draft → Backtest：回测通过 Gate 4 样本闸门
- Backtest → Shadow：回测指标达标（Win Rate / PnL / DD）
- Shadow → Paper：执行可行性验证通过（滑点 < 阈值、订单成交率 > 95%）
- Paper → ReadOnly：积累 20+ 真实信号样本，人工审核通过率 > 80%
- ReadOnly → Live：只读模式运行 7 天无异常，用户显式授权
- Live → Deprecated：连续 30 天 PnL < 预期 50% 或触发熔断 3 次

### 6.3 多版本并行运行（A/B Testing）

**场景**：新版本（v1.3.0）与基线版本（v1.2.2）同时运行，对比真实表现。

**实现**：
```rust
// 为同一 strategy_key 的不同 version 分配独立 strategy_config_id
INSERT INTO strategy_configs (strategy_key, version, lifecycle_stage, params) VALUES
    ('nwe_dynamic', 'v1.2.2', 'live', '{"atr_multiplier": 1.5}'),
    ('nwe_dynamic', 'v1.3.0', 'paper', '{"atr_multiplier": 2.0}');
```

**信号隔离**：
- 基线版本（v1.2.2）：继续实盘执行
- 新版本（v1.3.0）：仅 paper 模式，信号写入 `execution_task` 但不自动执行

**对比分析**（7-14 天后）：
```sql
-- 对比两个版本的真实信号质量
SELECT 
    sc.version,
    COUNT(*) as signal_count,
    AVG(CASE WHEN et.actual_pnl > 0 THEN 1 ELSE 0 END) as win_rate,
    SUM(et.actual_pnl) as total_pnl
FROM execution_task et
JOIN strategy_configs sc ON et.strategy_config_id = sc.id
WHERE sc.strategy_key = 'nwe_dynamic'
    AND et.created_at > NOW() - INTERVAL 14 DAY
GROUP BY sc.version;
```

**晋级决策**：
- 如果 v1.3.0 表现 **明显优于** v1.2.2（PnL 提升 > 20% 且 DD 无恶化）→ promote v1.3.0 到 live，v1.2.2 降级到 deprecated
- 如果 v1.3.0 表现 **持平或略优**（PnL 提升 5-20%）→ 继续观察 14 天
- 如果 v1.3.0 表现 **劣于** v1.2.2 → 直接标记 deprecated，停止 paper 运行

### 6.4 版本回滚机制

**触发条件**（任意一条满足立即回滚）：
1. 新版本上线后 24 小时内，亏损 > 账户 5%
2. 单笔交易触发异常止损（滑点 > 5% / 订单部分成交）
3. 风控引擎触发熔断 3 次
4. 用户手动请求回滚

**回滚步骤**：
```bash
# 1. 停止新版本策略的所有 worker
systemctl stop quant-core-execution-worker@nwe_v1.3.0

# 2. 关闭新版本的所有持仓（市价平仓）
cargo run -p rust-quant-cli -- close-all-positions \
    --strategy-key nwe_dynamic \
    --version v1.3.0 \
    --reason "rollback_to_v1.2.2"

# 3. 修改数据库状态
UPDATE strategy_configs 
SET lifecycle_stage = 'deprecated', 
    updated_at = NOW() 
WHERE strategy_key = 'nwe_dynamic' AND version = 'v1.3.0';

UPDATE strategy_configs 
SET lifecycle_stage = 'live', 
    updated_at = NOW() 
WHERE strategy_key = 'nwe_dynamic' AND version = 'v1.2.2';

# 4. 重启基线版本 worker
systemctl start quant-core-execution-worker@nwe_v1.2.2
```

**回滚日志**（写入 `strategy_lifecycle_log` 表）：
```sql
INSERT INTO strategy_lifecycle_log (strategy_config_id, from_stage, to_stage, reason, operator)
VALUES (123, 'live', 'deprecated', 'rollback due to DD > 5% in 24h', 'auto_risk_engine');
```

### 6.5 版本文档规范

每个新版本必须有对应的设计文档：

**文件位置**：`docs/plans/YYYY-MM-DD-strategy-name-version.md`

**必需章节**：
```markdown
# [Strategy Name] v[X.Y.Z] - [Brief Description]

## 变更动机
为什么需要这个版本？解决什么问题？

## 变更内容
### 参数调整
- ATR 倍数：1.5 → 2.0
- STC 超卖阈值：25 → 20

### 逻辑变更（如有）
- 无

### 新增功能
- 支持动态波动率调整

## 回测对比
| 指标 | v1.2.2 (baseline) | v1.3.0 (new) | 变化 |
|------|------------------|--------------|------|
| Win Rate | 58.3% | 61.2% | +2.9% ✅ |
| 月 PnL | +18.7u | +22.4u | +19.8% ✅ |
| Max DD | 8.2% | 7.8% | -4.9% ✅ |

## 晋级计划
- Week 1-2: Shadow Trading
- Week 3-4: Paper Observation (目标 30+ 信号)
- Week 5: ReadOnly 模式
- Week 6: 如果无异常，promote 到 Live

## 回滚触发条件
- 24H PnL < -5%
- 单笔滑点 > 3%
- 连续 5 笔亏损

## 审核签名
- 策略研发: [Name] @ YYYY-MM-DD
- 风控审核: [Name] @ YYYY-MM-DD
- 生产批准: [Name] @ YYYY-MM-DD
```

---

## 7. Gate 7：Paper / Shadow / ReadOnly 渐进式验证

### 7.1 三种验证模式的区别

| 模式 | 数据来源 | 执行动作 | 风险 | 适用阶段 |
|------|---------|---------|------|---------|
| **Shadow Trading** | 真实 WebSocket 行情 | 不下单，模拟执行 | 0 | 验证执行可行性 |
| **Paper Observation** | 真实信号生成 | 写入任务表，需人工审核 | 0 | 积累真实信号样本 |
| **ReadOnly** | 真实信号 + 真实下单 | 只读跟踪，不实际成交 | 极低（仅测试订单） | 验证订单流程 |

### 7.2 Shadow Trading（执行可行性验证）

**目标**：验证策略在真实 tick 级行情下的执行表现。

**关键指标**：
- 信号延迟：从 K 线收盘到信号生成的时间
- 订单簿深度：入场价位的可用流动性
- 滑点估算：理论入场价 vs 真实可成交价
- 止损单触发延迟：从价格触及止损到模拟成交的时间

**实现**（`crates/strategies/src/framework/backtest/shadow_trading.rs`）：
```rust
pub async fn run_shadow_trading(
    strategy: impl StrategyExecutor,
    inst_id: &str,
    duration: Duration,
) -> ShadowTradingReport {
    let mut report = ShadowTradingReport::default();
    let mut ws_client = WebSocketClient::connect(inst_id).await?;
    
    while let Some(candle) = ws_client.recv_candle().await {
        let signal_start = Instant::now();
        let signal = strategy.execute(inst_id, &candle).await?;
        let signal_latency = signal_start.elapsed();
        
        if signal.action.is_entry() {
            // 查询当前订单簿
            let orderbook = fetch_orderbook(inst_id).await?;
            let (executable_qty, avg_price) = calculate_executable_qty(
                &orderbook, signal.direction, target_notional
            );
            
            let slippage = (avg_price - candle.close).abs() / candle.close;
            report.record_entry(signal_latency, slippage, executable_qty);
        }
    }
    
    report
}
```

**通过标准**：
- 信号延迟中位数 < 500ms（5 分钟策略）或 < 2s（4 小时策略）
- 95% 信号的滑点 < 0.3%
- 订单簿深度 > 目标仓位的 3 倍（避免冲击成本）

**失败案例示例**：
```
Strategy: ultra_scalper_1m
Signal latency P50: 1.2s, P95: 3.8s  ❌ (目标 <500ms)
Slippage P95: 0.8%  ❌ (目标 <0.3%)
结论: 1 分钟周期下信号延迟过高，降级到 5 分钟或放弃
```

### 7.3 Paper Observation（真实信号积累）

**目标**：在真实市场环境下生成信号，但不自动执行，需人工审核。

**工作流**：
```
Market Velocity Live Handoff (Core)
    ↓ 生成候选信号
execution_task 表 (status = PendingApproval)
    ↓ Web 前端展示
运营人员审核 (批准 / 拒绝 / 修改参数)
    ↓ 批准后
Execution Worker 执行
    ↓ 执行结果
回写 execution_task (actual_pnl, actual_exit_reason)
```

**数据库结构**：
```sql
CREATE TABLE execution_task (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    strategy_config_id BIGINT,
    inst_id VARCHAR(32),
    signal_direction ENUM('long', 'short'),
    entry_price DECIMAL(20,8),
    stop_loss DECIMAL(20,8),
    take_profit DECIMAL(20,8),
    status ENUM('pending_approval', 'approved', 'rejected', 'executing', 'completed'),
    approval_operator VARCHAR(64),
    approved_at TIMESTAMP NULL,
    actual_entry_price DECIMAL(20,8),
    actual_exit_price DECIMAL(20,8),
    actual_pnl DECIMAL(20,8),
    actual_exit_reason VARCHAR(128),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

**人工审核清单**：
- [ ] 信号逻辑合理（非异常行情触发）
- [ ] 止损距离适中（不会被秒扫）
- [ ] 入场价位流动性充足
- [ ] 无与其他持仓冲突（对冲风险）
- [ ] 符合当前市场情绪（避免逆势硬上）

**审核统计**：
```sql
-- Paper 阶段信号质量分析
SELECT 
    sc.strategy_key,
    sc.version,
    COUNT(*) as total_signals,
    SUM(CASE WHEN status = 'approved' THEN 1 ELSE 0 END) as approved_count,
    AVG(CASE WHEN status = 'completed' THEN actual_pnl ELSE NULL END) as avg_pnl,
    SUM(CASE WHEN status = 'completed' AND actual_pnl > 0 THEN 1 ELSE 0 END) 
        / NULLIF(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) as win_rate
FROM execution_task et
JOIN strategy_configs sc ON et.strategy_config_id = sc.id
WHERE sc.lifecycle_stage = 'paper'
    AND et.created_at > NOW() - INTERVAL 30 DAY
GROUP BY sc.strategy_key, sc.version;
```

**晋级到 ReadOnly 的条件**：
- 积累信号数 ≥ 30 笔
- 审核通过率 ≥ 80%
- 已执行信号的 Win Rate ≥ 55%
- 已执行信号的 Max DD < 10%

### 7.4 ReadOnly 模式（订单流程验证）

**目标**：验证完整的订单流程（下单 → 成交 → 止损挂出 → 平仓），但使用最小仓位。

**实现**：
```rust
pub struct ReadOnlyExecutor {
    real_executor: Box<dyn OrderExecutor>,
    min_notional: f64, // 最小名义价值，如 10 USDT
}

impl ReadOnlyExecutor {
    async fn execute_signal(&self, signal: SignalResult) -> Result<ExecutionResult> {
        // 强制使用最小仓位
        let readonly_signal = SignalResult {
            position_size: self.min_notional / signal.entry_price,
            ..signal
        };
        
        // 调用真实执行层
        let result = self.real_executor.place_order(readonly_signal).await?;
        
        // 记录审计日志（标记为 readonly 模式）
        audit_log::record(AuditEvent::ReadOnlyExecution {
            signal: readonly_signal,
            result: result.clone(),
            actual_cost: result.filled_qty * result.avg_price,
        });
        
        Ok(result)
    }
}
```

**监控指标**：
- 订单成交率：目标 > 95%
- 止损单挂出成功率：目标 100%
- 平仓成交率：目标 > 98%
- API 错误率：目标 < 1%

**异常案例处理**：
```
Case: 订单部分成交（filled 60%, cancelled 40%）
原因: 流动性不足 / 价格快速滑移
处理: 记录到 execution_anomaly_log，人工复盘
决策: 如果频繁出现（> 5%），考虑调整入场逻辑（限价单 → 市价单 / 拆单）
```

**晋级到 Live 的条件**：
- ReadOnly 模式运行 ≥ 7 天
- 订单成交率 ≥ 95%
- 止损单挂出成功率 = 100%（一次失败都不允许）
- 无 API 限频 / 签名错误
- 用户显式授权（勾选"我已理解风险，批准策略上线"）

### 7.5 验证失败的回退机制

任何阶段验证失败 → 回退到上一阶段或 Deprecated。

**示例 1**：Shadow Trading 滑点过高
```
Shadow → Backtest (重新优化入场逻辑，改用限价单)
```

**示例 2**：Paper 阶段人工审核通过率过低（< 60%）
```
Paper → Backtest (信号质量不足，重新设计过滤器)
```

**示例 3**：ReadOnly 阶段止损单挂出失败
```
ReadOnly → Deprecated (执行层集成有 bug，修复后重新从 Shadow 开始)
```

**回退日志**：
```sql
INSERT INTO strategy_lifecycle_log (strategy_config_id, from_stage, to_stage, reason)
VALUES (456, 'paper', 'backtest', 'manual_approval_rate_too_low: 45%');
```

---

## 8. Gate 8：Promote 到生产与持续监控

### 8.1 生产上线前的最终检查清单

在策略从 ReadOnly 晋级到 Live 前，必须完成以下检查：

#### 技术检查（Tech Review）
- [ ] 所有单元测试通过（`cargo test --workspace`）
- [ ] 回测 Pipeline 无 regression（对比 baseline）
- [ ] Shadow Trading 报告达标（滑点 / 延迟 / 流动性）
- [ ] Paper 阶段积累 ≥ 30 笔真实信号
- [ ] ReadOnly 模式运行 ≥ 7 天无异常
- [ ] 代码 review 通过（2 人以上签字）
- [ ] 文档完整（设计文档 + 版本说明 + 运维手册）

#### 风控检查（Risk Review）
- [ ] 每笔交易强制止损（无裸单）
- [ ] 账户级风控配置（最大回撤 / 仓位上限）
- [ ] 熔断机制测试（模拟极端行情）
- [ ] 止损单挂出成功率 = 100%
- [ ] 保本移动逻辑验证（1.5R 后自动移动）
- [ ] 跨策略仓位冲突检测（避免对冲）

#### 业务检查（Business Review）
- [ ] 用户授权确认（API Key 权限 / 风险告知）
- [ ] 会员等级匹配（Plus / Pro / Max 的 combo 限制）
- [ ] Readiness 展示正确（Web 前端显示策略状态）
- [ ] 计费逻辑验证（订阅费 / 盈利分成）
- [ ] 客服 FAQ 准备（策略说明 / 风险提示）

#### 运维检查（Ops Review）
- [ ] 生产 compose 配置正确（环境变量 / 端口 / 依赖）
- [ ] 监控告警配置（Prometheus / Grafana / PagerDuty）
- [ ] 日志采集正常（ELK / Loki）
- [ ] 数据库迁移脚本执行（新增字段 / 索引）
- [ ] 回滚预案准备（回滚脚本 + 数据备份）
- [ ] 灰度发布计划（先 10% 用户 → 50% → 100%）

### 8.2 灰度发布策略

**阶段 1：内部测试账户（Day 1-3）**
```sql
-- 只为内部测试账户启用新策略
UPDATE user_strategy_subscriptions 
SET strategy_config_id = 789  -- 新版本
WHERE user_id IN (SELECT id FROM users WHERE is_internal_test = TRUE);
```

**监控指标**（每 6 小时检查）：
- 订单成交率 > 95%
- 止损触发正常
- PnL 符合预期（±20% 波动正常）
- 无 API 错误

**阶段 2：小规模用户（Day 4-7, 10% 用户）**
```sql
-- 随机选择 10% Pro/Max 用户
UPDATE user_strategy_subscriptions 
SET strategy_config_id = 789
WHERE user_id IN (
    SELECT id FROM users 
    WHERE membership IN ('Pro', 'Max')
    ORDER BY RAND() 
    LIMIT (SELECT COUNT(*) * 0.1 FROM users WHERE membership IN ('Pro', 'Max'))
);
```

**对比分析**：
```sql
-- 对比新旧策略的真实用户表现
SELECT 
    sc.version,
    COUNT(DISTINCT uss.user_id) as user_count,
    AVG(user_pnl.total_pnl) as avg_user_pnl,
    STDDEV(user_pnl.total_pnl) as pnl_stddev
FROM user_strategy_subscriptions uss
JOIN strategy_configs sc ON uss.strategy_config_id = sc.id
LEFT JOIN (
    SELECT user_id, SUM(pnl) as total_pnl
    FROM user_trades
    WHERE created_at > NOW() - INTERVAL 7 DAY
    GROUP BY user_id
) user_pnl ON uss.user_id = user_pnl.user_id
WHERE sc.strategy_key = 'nwe_dynamic'
GROUP BY sc.version;
```

**阶段 3：全量发布（Day 8+, 100% 用户）**
```sql
-- 所有订阅该策略的用户切换到新版本
UPDATE user_strategy_subscriptions 
SET strategy_config_id = 789
WHERE strategy_config_id IN (
    SELECT id FROM strategy_configs 
    WHERE strategy_key = 'nwe_dynamic' AND version < 'v1.3.0'
);

-- 将旧版本标记为 deprecated
UPDATE strategy_configs 
SET lifecycle_stage = 'deprecated'
WHERE strategy_key = 'nwe_dynamic' AND version < 'v1.3.0';
```

### 8.3 生产监控指标体系

#### 实时监控（Grafana Dashboard）

**策略性能指标**（每 5 分钟刷新）：
```promql
# 活跃持仓数
sum(quant_strategy_active_positions{strategy_key="nwe_dynamic", version="v1.3.0"})

# 24H PnL
sum(increase(quant_strategy_pnl_usdt{strategy_key="nwe_dynamic"}[24h]))

# 胜率（近 50 笔交易）
sum(quant_strategy_win_count[24h]) / sum(quant_strategy_trade_count[24h])

# 当前回撤
max(quant_strategy_drawdown_pct{strategy_key="nwe_dynamic"})
```

**执行质量指标**：
```promql
# 订单成交率
sum(rate(quant_order_filled_count[5m])) / sum(rate(quant_order_total_count[5m]))

# 滑点分布 P50 / P95
histogram_quantile(0.5, quant_order_slippage_bps)
histogram_quantile(0.95, quant_order_slippage_bps)

# 止损单挂出延迟
histogram_quantile(0.95, quant_protective_order_latency_ms)
```

**风控指标**：
```promql
# 熔断触发次数（应为 0）
sum(increase(quant_risk_circuit_break_count[1h]))

# 保本移动执行成功率
sum(rate(quant_breakeven_move_success[5m])) / sum(rate(quant_breakeven_move_attempt[5m]))
```

#### 告警规则（AlertManager）

**P0 告警（立即处理，5 分钟内响应）**：
```yaml
- alert: StopLossMissingInProduction
  expr: sum(quant_order_no_stoploss_count) > 0
  for: 1m
  annotations:
    summary: "实盘订单缺少止损！立即熔断！"
    
- alert: ExecutionCircuitBreak
  expr: sum(increase(quant_risk_circuit_break_count[5m])) > 0
  annotations:
    summary: "策略触发熔断，自动停止执行"
    
- alert: OrderBookInsufficientLiquidity
  expr: histogram_quantile(0.95, quant_order_liquidity_ratio) < 0.3
  for: 5m
  annotations:
    summary: "订单簿流动性不足，可能导致大滑点"
```

**P1 告警（1 小时内响应）**：
```yaml
- alert: StrategyDrawdownHigh
  expr: max(quant_strategy_drawdown_pct) > 8.0
  for: 10m
  annotations:
    summary: "策略回撤超过 8%，接近 10% 红线"
    
- alert: WinRateDrop
  expr: sum(quant_strategy_win_count[24h]) / sum(quant_strategy_trade_count[24h]) < 0.50
  annotations:
    summary: "近 24H 胜率低于 50%，策略可能失效"
```

**P2 告警（4 小时内响应）**：
```yaml
- alert: SlippageHigh
  expr: histogram_quantile(0.95, quant_order_slippage_bps) > 30
  for: 1h
  annotations:
    summary: "P95 滑点超过 0.3%，执行质量下降"
```

### 8.4 策略性能劣化的自动降级

当策略在生产环境表现不符预期时，自动触发降级：

**降级触发条件**（任意一条满足）：
```rust
pub enum DegradationTrigger {
    DrawdownExceeded { current: f64, threshold: f64 },       // 回撤 > 10%
    WinRateDrop { current: f64, baseline: f64 },             // 胜率降幅 > 10%
    ConsecutiveLosses { count: usize, threshold: usize },    // 连续亏损 > 10 笔
    PnLBelowExpectation { actual: f64, expected: f64 },      // 30 天 PnL < 预期 50%
    CircuitBreakFrequent { count: usize, window: Duration }, // 7 天内熔断 3 次
}
```

**降级动作**（自动执行）：
```rust
pub async fn auto_degrade_strategy(
    strategy_config_id: i64,
    trigger: DegradationTrigger,
) -> Result<()> {
    // 1. 停止新信号生成
    db.execute(
        "UPDATE strategy_configs SET is_signal_enabled = FALSE WHERE id = ?",
        strategy_config_id
    ).await?;
    
    // 2. 关闭所有活跃持仓（市价平仓）
    execution_service.close_all_positions(
        strategy_config_id,
        CloseReason::AutoDegradation(trigger.clone())
    ).await?;
    
    // 3. 修改 lifecycle_stage
    db.execute(
        "UPDATE strategy_configs SET lifecycle_stage = 'deprecated' WHERE id = ?",
        strategy_config_id
    ).await?;
    
    // 4. 通知运维团队
    alerting::send_pagerduty(
        AlertLevel::P0,
        format!("策略 {} 自动降级：{:?}", strategy_config_id, trigger)
    ).await?;
    
    // 5. 通知受影响用户
    notification::send_to_users(
        strategy_config_id,
        "您订阅的策略因性能劣化已自动停止，我们正在排查原因。"
    ).await?;
    
    Ok(())
}
```

### 8.5 定期复盘与持续优化

**每周复盘**（运营团队）：
- 回顾本周所有策略的 PnL / DD / Win Rate
- 对比回测预期与实盘表现的偏差
- 收集用户反馈（投诉 / 建议）
- 识别需要优化的策略

**每月深度分析**（研发团队）：
- 外部因子分析（Funding Rate / Open Interest / 市场情绪）
- 信号过滤质量分析（被过滤的信号事后是否盈利？）
- 执行质量分析（滑点分布 / 订单簿深度变化）
- 策略相关性分析（多策略是否过度相关？）

**季度策略审计**（风控 + 业务）：
- 所有 Live 策略重新跑最近 3 个月回测
- 对比回测与实盘的 Sharpe Ratio 偏差
- 如果偏差 > 30%，强制降级到 Paper 重新验证
- 更新策略白名单（淘汰劣化策略 / 上线新策略）

**示例：季度审计 SQL**
```sql
-- 对比回测与实盘的表现偏差
SELECT 
    sc.strategy_key,
    sc.version,
    bt.win_rate as backtest_win_rate,
    live.win_rate as live_win_rate,
    (live.win_rate - bt.win_rate) as win_rate_deviation,
    CASE 
        WHEN ABS(live.win_rate - bt.win_rate) > 0.10 THEN 'REVIEW_REQUIRED'
        ELSE 'OK'
    END as audit_status
FROM strategy_configs sc
LEFT JOIN (
    SELECT strategy_config_id, 
           AVG(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) as win_rate
    FROM back_test_detail
    WHERE created_at > NOW() - INTERVAL 90 DAY
    GROUP BY strategy_config_id
) bt ON sc.id = bt.strategy_config_id
LEFT JOIN (
    SELECT strategy_config_id,
           AVG(CASE WHEN actual_pnl > 0 THEN 1 ELSE 0 END) as win_rate
    FROM execution_task
    WHERE status = 'completed' 
      AND created_at > NOW() - INTERVAL 90 DAY
    GROUP BY strategy_config_id
) live ON sc.id = live.strategy_config_id
WHERE sc.lifecycle_stage = 'live';
```

---

## 9. 实战案例：从想法到生产的完整流程

### 9.1 案例 1：NWE 动态波动率调整（成功上线）

#### 阶段 0：想法来源
**观察**：NWE v1.2.2 在高波动期（ATR > 历史 75 分位）胜率下降 8%  
**假设**：固定的 NWE 带宽倍数（3.0）和 ATR 止损倍数（1.5）在高波动时过于激进

#### 阶段 1：Gate 1 周期定性 ✅
- 原策略周期：5 分钟
- 优化方向：参数自适应，不改变周期
- 决策：保持 5 分钟，通过

#### 阶段 2：Gate 2 指标语义分解 ✅
**新增指标**：波动率状态分类器
```rust
pub fn classify_volatility(atr: f64, atr_history: &[f64]) -> VolatilityState {
    let percentile_75 = calculate_percentile(atr_history, 0.75);
    match atr {
        x if x > percentile_75 * 1.5 => VolatilityState::ExtremeHigh,
        x if x > percentile_75 => VolatilityState::High,
        x if x < percentile_75 * 0.5 => VolatilityState::Low,
        _ => VolatilityState::Normal,
    }
}
```

**参数调整逻辑**：
- 高波动期：`nwe_multi: 3.0 → 4.0`, `atr_multiplier: 1.5 → 2.0`
- 低波动期：`nwe_multi: 3.0 → 2.5`, `atr_multiplier: 1.5 → 1.2`

#### 阶段 3：Gate 3 假设与证伪设计 ✅
**假设陈述**：
```
在高波动期（ATR > P75），放宽通道带宽和止损距离，可将胜率从 50% 提升到 58%，
同时减少因正常波动触发止损的"假止损"比例从 35% 降到 20%
```

**证伪条件**：
- 高波动期胜率提升 < 5%
- 假止损比例下降 < 10%
- 整体月 PnL 无提升或回撤恶化

#### 阶段 4：Gate 4 回测样本闸门 ✅
```bash
cargo run -p rust-quant-cli -- backtest \
    --strategy nwe_dynamic \
    --version v1.3.0 \
    --symbols BTC-USDT-SWAP,ETH-USDT-SWAP \
    --start 2026-04-01 \
    --end 2026-06-30 \
    --baseline-version v1.2.2
```

**回测结果**：
| 指标 | v1.2.2 (baseline) | v1.3.0 (dynamic) | 变化 |
|------|------------------|------------------|------|
| 总交易数 | 187 | 203 | +8.6% |
| Win Rate | 58.3% | 61.7% | +3.4% ✅ |
| 月 PnL | +18.7u | +24.3u | +29.9% ✅ |
| Max DD | 8.2% | 7.1% | -13.4% ✅ |
| 高波动期胜率 | 50.2% | 59.1% | +8.9% ✅ |
| 假止损比例 | 34.8% | 18.9% | -45.7% ✅ |

**通过 ✅**，进入下一闸门

#### 阶段 5：Gate 5 风控与执行契约 ✅
- 止损计算：动态 ATR 倍数，已包含
- 仓位计算：风险归一化，继承 v1.2.2
- 保护单：1.5R 后移动到保本，已集成到 `realtime/breakeven_stop_loss.rs`

#### 阶段 6：Gate 6 版本落位 ✅
- 版本号：v1.3.0
- 分类：参数优化，保留 `strategy_key = "nwe_dynamic"`
- 文档：`docs/plans/2026-06-15-nwe-dynamic-volatility-adjustment.md`

#### 阶段 7：Gate 7 渐进式验证 ✅
**Shadow Trading**（2026-06-16 至 06-18，3 天）：
- 信号延迟 P50: 287ms, P95: 512ms ✅
- 滑点 P95: 0.21% ✅
- 订单簿深度充足 ✅

**Paper Observation**（2026-06-19 至 07-02，14 天）：
- 生成信号：42 笔
- 人工审核通过：35 笔（83.3%）✅
- 已执行信号 Win Rate：62.9% ✅

**ReadOnly**（2026-07-03 至 07-09，7 天）：
- 订单成交率：97.2% ✅
- 止损单挂出成功率：100% ✅
- 无 API 错误 ✅

#### 阶段 8：Gate 8 Promote 到生产 ✅
**灰度发布**：
- Day 1-3：内部测试账户（2 个）
- Day 4-7：10% Pro/Max 用户（18 个）
- Day 8+：全量发布（187 个用户）

**生产表现**（上线后 14 天）：
- 实盘 Win Rate：60.8%（预期 61.7%，偏差 -0.9% ✅）
- 月化 PnL 估算：+22.1u（预期 +24.3u，偏差 -9.0% ✅）
- Max DD：6.8%（预期 7.1% ✅）

**结论**：✅ 成功上线，v1.2.2 标记为 deprecated

---

### 9.2 案例 2：5 分钟 Range Reversion Scalper（证伪）

#### 阶段 0：想法来源
**假设**：震荡市中，RSI 极值 + 布林带边缘 = 高概率均值回归

#### 阶段 1：Gate 1 周期定性 ⚠️
- 目标周期：5 分钟
- 预期持仓：2-6 小时
- 费率敏感度：高（往返 0.1% taker）

**红线检查**：
```
典型波动：0.5%
费率占比：0.1% / 0.5% = 20%
零费率毛利：需验证
```
→ 有风险，但继续验证

#### 阶段 2-4：实现与回测
（跳过细节，见 `DEPRECATED_SCALPERS.md`）

**回测结果**（2026-05 至 06，2 个月）：
- Win Rate：64.2% ✅
- 交易频次：93.5/月 ✅
- Max DD：3.4-10% ✅
- **月 PnL：+3u** ❌（目标 ≥20u）

#### 阶段 3：Gate 3 证伪判定 ❌
**费率拆解**：
```
毛利（含费率）：+3u/月
费率成本：-1.85u/月
净利（零费率）：+4.85u/月

缺口：20u - 4.85u = 15.15u（312% 缺口）
```

**尝试优化**（均失败）：
- ✅ 参数扫描 15,000+ 配置
- ✅ 切换到 15 分钟周期
- ✅ long-only / short-only 测试
- ✅ 杠杆提升（但 DD 爆炸）

**结论**：
> 策略技术实现正确，但在给定市场环境与约束下**数学上无法达到目标**。
> 标记为 DEPRECATED，保留代码作为技术示例。

**教训**：
1. 5 分钟 scalping 的费率天花板是真实存在的
2. 即使胜率高，PnL 仍可能不足
3. 杠杆不能弥补根本性的盈利不足

---

### 9.3 案例 3：Vegas 4H 外部因子研究（进行中）

#### 背景
Vegas 4H 已稳定运行（Win Rate 58.3%），但希望通过外部因子（Funding Rate / Open Interest）进一步提升。

#### 研究路径（不直接改策略）
```
Vegas 策略（保持不变）
    ↓ 继续生成信号
回测基线数据（back_test_log）
    ↓ 对齐外部快照
Factor Research Service（只读分析）
    ↓ 生成研究报告
人工决策：因子是否值得回注
```

#### 实现（见 `docs/plans/2026-04-15-vegas-factor-research-system.md`）
```rust
pub struct VegasFactorResearchService {
    // 从 back_test_detail 读取交易样本
    // 从 external_market_snapshots 读取 funding/OI
    // 按 4H 对齐后统计
}

pub struct FactorReport {
    pub factor_name: String,
    pub sample_size: usize,
    pub win_rate_with_positive_factor: f64,
    pub win_rate_with_negative_factor: f64,
    pub conclusion: FactorConclusion, // Integrate / Observe / Reject
}
```

#### 研究结论（假设）
```
Factor: Funding Rate 上升趋势（连续 3 期 > 0.01%）
  - 做多信号 Win Rate：62.1%（+3.8% vs baseline）
  - 做空信号 Win Rate：54.3%（-4.0% vs baseline）
  - 结论：可回注为"做多增强过滤器"

Factor: Open Interest 暴增（24H 涨幅 > 20%）
  - 做多信号 Win Rate：56.8%（-1.5% vs baseline）
  - 做空信号 Win Rate：59.2%（+1.0% vs baseline）
  - 结论：效果不明显，暂不回注
```

#### 回注决策
- 如果因子提升 > 5% 且样本量 > 30 笔 → 创建 **Vegas v2.1.0**（新版本）
- 重新走完整闸门流程（Gate 3 → Gate 8）
- **不覆盖**原 Vegas 策略，而是并行运行 A/B 测试

---

## 10. 常见陷阱与反模式

### 10.1 过拟合陷阱

**表现**：回测表现优异，实盘迅速崩溃

**原因**：
- 参数在特定时间窗口过度优化
- 样本量不足（< 30 笔交易）
- 未覆盖多种市场状态

**预防**：
```rust
// 参数稳健性测试
fn test_parameter_robustness() {
    let base_config = Config { rsi_period: 14 };
    let base_result = backtest(base_config);
    
    // ±20% 扰动
    for delta in [-4, -2, +2, +4] {
        let config = Config { rsi_period: 14 + delta };
        let result = backtest(config);
        
        // 胜率降幅不应超过 10%
        assert!(result.win_rate > base_result.win_rate * 0.9);
    }
}
```

### 10.2 前视偏差（Look-Ahead Bias）

**表现**：回测用了"未来信息"，实盘无法获取

**常见错误**：
```rust
// ❌ 错误：用整根 K 线的最高价做入场判断
if candle.high > resistance {
    entry_price = candle.close; // 但实际上 high 在 close 之前
}

// ✅ 正确：只用 close 或显式标注"需要 tick 级回测"
if candle.close > resistance {
    entry_price = next_candle.open; // 下一根开盘入场
}
```

### 10.3 幸存者偏差

**表现**：只在当前还存在的交易对上回测，忽略了已下架的币种

**预防**：
- 回测样本必须包含"当时市场上所有主流币种"
- 如果策略只在 BTC/ETH 有效，明确标注"仅支持主流币种"

### 10.4 忽略执行成本

**表现**：回测假设无滑点 / 瞬间成交，实盘执行大幅偏离

**预防**：
- Shadow Trading 阶段必须验证真实滑点
- 高频策略（5m/15m）必须考虑订单簿深度
- 回测时可加入"保守滑点假设"（如每笔 +0.1% 成本）

### 10.5 静默覆盖已上线策略

**表现**：直接修改生产策略代码，未走版本管理

**后果**：
- 无法回滚（旧版本代码丢失）
- 无法 A/B 对比
- 用户收益突然变化，投诉激增

**强制规则**（从 CLAUDE.md）：
> 已上线或已有准生产证据的策略默认禁止原地覆盖。

### 10.6 禁止使用 Python 迭代

**表现**：用 Python 快速验证想法，但未转译到 Rust

**问题**：
- Python 回测与 Rust 生产环境存在差异（浮点精度 / 指标计算）
- 无法复用现有的风控 / 执行层
- 技术债累积

**规则**（从 CLAUDE.md）：
> 迭代与回测过程中禁止使用 Python

**正确做法**：
- 所有策略研究直接在 Rust 中进行
- 使用 `tests/` 目录的研究 harness（如 `scalper_research.rs`）
- 如果需要探索性分析，用 Rust + CSV 导出 + 外部可视化工具

---

## 11. 工具链与脚本

### 11.1 策略开发常用命令

```bash
# 回测单个策略
cargo run -p rust-quant-cli -- backtest \
    --strategy nwe_dynamic \
    --symbols BTC-USDT-SWAP \
    --start 2026-05-01 \
    --end 2026-06-30

# 参数网格扫描
cargo test -p rust-quant-strategies parameter_grid_search -- --nocapture

# Shadow Trading
cargo run -p rust-quant-cli --example shadow_trading_nwe

# 查看策略注册表
cargo run -p rust-quant-cli -- list-strategies

# 生成外部因子研究报告
cargo run -p rust-quant-cli --example run_vegas_factor_research
```

### 11.2 Smoke 测试脚本（在 `scripts/dev/` 目录）

```bash
# 执行 worker dry-run
./scripts/dev/run_execution_worker_dry_run.sh

# Binance WebSocket 烟雾测试
./scripts/dev/run_binance_websocket_quant_core_smoke.sh

# 实时策略烟雾测试
./scripts/dev/run_live_strategy_quant_core_smoke.sh

# 完整产品健康检查
./scripts/dev/check_full_product_health.sh
```

### 11.3 数据库诊断

```sql
-- 查看所有策略的生命周期状态
SELECT strategy_key, version, lifecycle_stage, updated_at
FROM strategy_configs
ORDER BY strategy_key, version DESC;

-- 查看某策略的近期表现
SELECT 
    DATE(created_at) as date,
    COUNT(*) as trades,
    AVG(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) as win_rate,
    SUM(pnl) as daily_pnl
FROM back_test_detail
WHERE strategy_config_id = 123
    AND created_at > NOW() - INTERVAL 30 DAY
GROUP BY DATE(created_at);

-- 查看被过滤的信号（可能错失机会）
SELECT reason, COUNT(*) as count
FROM filtered_signal_log
WHERE strategy_config_id = 123
    AND created_at > NOW() - INTERVAL 7 DAY
GROUP BY reason
ORDER BY count DESC;
```

---

## 12. 总结：策略迭代的核心原则

1. **严格闸门制度**：每个 Gate 必须通过才能进入下一阶段，不允许跳关
2. **版本可追溯**：禁止静默覆盖，所有变更必须有版本号与文档
3. **假设驱动**：先写可证伪的假设，再实现，避免"为了实现而实现"
4. **样本量优先**：宁可多等几个月积累样本，不可用少量数据强行上线
5. **执行现实主义**：回测再好，执行不可行也是零
6. **风控红线**：实盘必须带止损，无例外
7. **渐进式验证**：Shadow → Paper → ReadOnly → Live，逐步暴露真实风险
8. **持续监控**：上线不是终点，定期复盘与降级机制同样重要
9. **记录证伪**：失败的策略也要详细记录，避免重复踩坑
10. **禁用 Python**：所有迭代在 Rust 中完成，保证生产一致性

---

## 附录 A：策略分类速查表（更新：币种适配性）

| 策略家族 | 周期 | 核心指标 | 适用市场 | Tier 1 | Tier 2 | Tier 3 | Tier 4 | 状态 |
|---------|------|---------|---------|--------|--------|--------|--------|------|
| **Vegas** | 4H | EMA + Swing Fib | 趋势市 | ✅ Live | ✅ Live | ✅ Live | ⚠️ 需 1D | Live ✅ |
| **NWE** | 5m | STC + NWE 通道 + Vegas 过滤 | 震荡+趋势 | ✅ Live | ⚠️ 调参 | ❌ 不推荐 | ❌ 禁止 | Live ✅ |
| **BB-RSI** | 15m | Bollinger + RSI | 震荡市 | ✅ 测试中 | ✅ 测试中 | ⚠️ 需验证 | ❌ 不推荐 | Paper |
| **Keltner Scalper** | 5m | Keltner + 趋势过滤 | 高波动 | ✅ Shadow | ⚠️ 需调参 | ❌ 不推荐 | ❌ 禁止 | Shadow |
| **Smart Money Concepts** | 1H | 市场结构 + Order Block | 趋势反转 | ✅ 回测中 | ✅ 回测中 | ✅ 回测中 | ⚠️ 需 4H | Backtest |
| **Supertrend** | 4H | Supertrend + ATR | 强趋势 | ✅ 回测中 | ✅ 回测中 | ✅ 回测中 | ✅ 回测中 | Backtest |
| **RSI Divergence** | 1D | RSI 背离 + 形态 | 反转 | ✅ 设计中 | ✅ 设计中 | ✅ 设计中 | ✅ 设计中 | Draft |
| **Altcoin Momentum Swing** | 4H/1D | 结构突破 + 大止盈 | 高波趋势 | ⚠️ 不适合 | ⚠️ 不适合 | ✅ 专用 | ✅ 专用 (1D) | Draft ⭐ |
| **Range Reversion** | 5m | BB + RSI 极值 | 震荡市 | ❌ 费率致命 | ❌ 未测试 | ❌ 未测试 | ❌ 禁止 | Deprecated ❌ |
| **Momentum Breakout** | 5m | EMA + Pullback | 趋势突破 | ❌ 全配置亏损 | ❌ 未测试 | ❌ 未测试 | ❌ 禁止 | Deprecated ❌ |

**图例说明**：
- ✅ 已验证通过，可用于该 Tier
- ⚠️ 需要参数调整或周期变更
- ❌ 不推荐或禁止用于该 Tier
- **⭐ Altcoin Momentum Swing**：专为 Tier 3-4 高波币种设计的新策略思路

**关键发现**：
1. **5m Scalping 仅适合 Tier 1**（BTC/ETH），Tier 2+ 噪音过大
2. **4H Swing 是全 Tier 通用周期**（调整参数后都可用）
3. **Tier 4 最安全周期是 1D**（避免高频噪音）
4. **需要专门为 Tier 3-4 设计策略**（大止损、低杠杆、高 R 倍数）

---

## 附录 B：币种分层快速参考

### B.1 分层参数调整速查

| 参数类型 | Tier 1 基准 | Tier 2 | Tier 3 | Tier 4 |
|---------|-----------|--------|--------|--------|
| **ATR 止损倍数** | 1.5x | 1.8x (+20%) | 2.25x (+50%) | 3.0x (+100%) |
| **最大杠杆** | 3.0x | 2.4x (-20%) | 1.8x (-40%) | 1.2x (-60%) |
| **止盈 R 倍数** | 2.0R | 2.2R | 2.3R | 2.4R |
| **入场质量阈值** | 0.6 | 0.7 | 0.8 | 0.9 |
| **仓位折减** | 1.0 | 0.8 | 0.67 | 0.5 |
| **RSI 超卖阈值** | 30 | 25 | 20 | 15 |
| **RSI 超买阈值** | 70 | 75 | 80 | 85 |

### B.2 典型币种映射（2026-07 快照）

| Tier | 代表币种 | 30D 波动率 | 日均成交量 (USD) | 订单簿深度 (±1%) |
|------|---------|-----------|-----------------|----------------|
| **Tier 1** | BTC, ETH | 3-5% | > 10 亿 | > 1000 万 |
| **Tier 2** | SOL, BNB, XRP, ADA | 6-10% | > 1 亿 | > 100 万 |
| **Tier 3** | AVAX, MATIC, LINK, DOT | 10-15% | > 1000 万 | > 10 万 |
| **Tier 4** | 新上线币、市值 < 5 亿 | 20%+ | < 1000 万 | < 10 万 |

**注意**：币种分层是动态的，牛市时 Tier 3 可能升级到 Tier 2，熊市时降级。建议每季度重新评估。

### B.3 分层策略选择决策树

```
有新策略想法
    ↓
目标周期是多少？
    ├─ 5m/15m → 只能用 Tier 1-2
    ├─ 1H/4H → 可用 Tier 1-3
    └─ 1D/1W → 全 Tier 通用
        ↓
目标币种是哪个 Tier？
    ├─ Tier 1 (BTC/ETH) → 用基准参数
    ├─ Tier 2 (SOL/BNB) → ATR × 1.2, 杠杆 / 1.25
    ├─ Tier 3 (AVAX/MATIC) → ATR × 1.5, 杠杆 / 1.67, 入场质量 > 0.8
    └─ Tier 4 (新兴币) → ATR × 2.0, 杠杆 / 2.5, 入场质量 > 0.9, 强制 4H 或 1D
        ↓
回测时每个 Tier 独立验证
    ├─ Tier 1 必须通过（基准）
    ├─ Tier 2-3 必须达到 Tier 1 的 70-80% 表现
    └─ 如果只在 Tier 1 有效 → 过拟合 BTC/ETH，策略价值有限
        ↓
分层上线
    ├─ 先上线 Tier 1（风险最低）
    ├─ 再上线 Tier 2（观察 2-4 周）
    └─ 最后上线 Tier 3-4（严格监控）
```

---

## 附录 C：相关文档索引

- **项目规则书**：`/Users/mac2/onions/crypto_quant/AGENTS.md`
- **Core 仓库规则**：`/Users/mac2/onions/crypto_quant/rust_quant/CLAUDE.md`
- **策略实现目录**：`rust_quant/crates/strategies/src/implementations/`
- **指标库**：`rust_quant/crates/indicators/src/`
- **回测框架**：`rust_quant/crates/strategies/src/framework/backtest/`
- **风控引擎**：`rust_quant/crates/risk/src/`
- **设计文档**：`rust_quant/docs/plans/`
- **Deprecated 策略分析**：`rust_quant/crates/strategies/src/implementations/DEPRECATED_SCALPERS.md`
- **本文档（策略迭代方法论）**：`rust_quant/docs/STRATEGY_ITERATION_METHODOLOGY.md`

---

**文档版本**：v3.0.0 ⭐⭐ 重大重构：实战导向的两阶段迭代法  
**最后更新**：2026-07-09  
**维护者**：Core 策略研发团队  
**适用范围**：rust_quant（Core）策略全生命周期  

**变更日志**：
- v3.0.0 (2026-07-09): 
  - ✅ 增加**探索模式**（1-2 天快速试错）vs **生产模式**（1-2 周打磨上线）
  - ✅ 简化币种分层：Tier 1-4 → **Tier A/B**（主流 vs 高波，边界清晰）
  - ✅ 跨 Tier 泛化从必需改为**可选加分项**（允许专用策略）
  - ✅ 增加 **Gate 0**：快速失败检查（5 分钟排除明显不可行的想法）
  - ✅ 文档结构优化：先讲探索模式（快速入门），再讲生产模式（完整闸门）
- v2.0.0 (2026-07-09): 新增币种波动性分层（Tier 1-4）、分层参数调整、跨 Tier 验证
- v1.0.0 (2026-07-09): 初始版本，8 大闸门体系

**核心改进说明**：

v3.0.0 解决了 v2.0.0 的主要问题：
1. **过度复杂** → 增加探索模式，1-2 天快速验证想法
2. **分层过细** → 简化为 Tier A/B，决策更简单
3. **强制泛化** → 改为可选，允许 BTC 专用策略 / 高波币专用策略
4. **扼杀创新** → 探索阶段允许"脏代码"、快速试错

**使用建议**：
- 🚀 **新想法** → 从 0.3 节"探索模式详细步骤"开始
- 📚 **已验证想法** → 从 Gate 1 进入生产模式完整流程
- 📖 **参考案例** → 第 9 章有 3 个完整实战案例

---

*本文档是活文档，随着项目演进持续更新。如有疑问或改进建议，请提交 issue 或联系策略研发团队。*

