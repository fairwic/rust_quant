# 交易体系（第一性原理·工程落地版）

> **修订核心原则**：
> 1.  **模糊正确 > 精确错误**：用连续评分（Score）替代硬阈值（Threshold），避免边界闪烁。
> 2.  **生存优先于规则**：风控拥有最高中断权，即使数据源异常，止损逻辑也必须能强制触发（紧急逃生）。
> 3.  **资产安全 > 价格波动**：引入预言机偏差与稳定币脱锚检测。

---

## 一、 第一性原理：从“预测”转向“生存与反应”

### 1.1 核心目标（优先级递减）
1.  **资产保全（Survival）**：在交易所跑路、脱锚、API 挂死等极端情况下，保留本金。
2.  **执行完备（Execution）**：信号必须转化为可成交的订单，否则视为无效。
3.  **概率优势（Edge）**：只在统计期望 > 成本的场景下暴露风险。

### 1.2 边际优势来源（可证伪）
- **趋势惯性（Trend）**：资金流向的自我强化（通过多周期共振捕捉）。
- **均值回归（Reversion）**：流动性耗尽后的结构性回撤（通过背离+结构捕捉）。
- **微观失衡（Imbalance）**：由清算或事件引发的短期供需错配。

---

## 二、 系统状态与连续评分（解决“硬阈值”闪烁）

### 2.1 市场状态评分（Market Regime Score）
不再使用简单的 `Trend/Shock` 二元状态，而是计算 **Trend Score (0~100)**。

- **计算因子**：
  - `ADX_Score`：`min(ADX(14)/50, 1.0)`
  - `Ema_Dist_Score`：`min(dist(ema144, ema576) / 3%, 1.0)`
  - `Vol_Score`：`min(dvol / MA(dvol, 20), 2.0)`

- **合成 Regime**：
  - `Trend_Score > 60`：趋势策略权重 `1.0`，震荡策略权重 `0.0`
  - `Trend_Score < 30`：震荡策略权重 `1.0`，趋势策略权重 `0.0`
  - `30-60`：过渡期，仓位系数 `0.5`，允许双向互斥（见裁决章）。

### 2.2 执行健康度（Execution Health Score）
不再因为一个指标超标就 HALT，而是降低仓位。

- **因子**：
  - `Spread_Score`：`1 - min(spread / 0.1%, 1.0)`
  - `Depth_Score`：`min(depth5 / 100k_USDT, 1.0)`
  - `Latency_Score`：`1 - min(latency / 1000ms, 1.0)`

- **应用**：
  - `Exec_Score = Spread_Score * 0.4 + Depth_Score * 0.4 + Latency_Score * 0.2`
  - **最终仓位系数** = `Strategy_Size * Exec_Score`
  - **强制熔断**：若 `Exec_Score < 0.2`，进入 `HALT`（仅允许减仓）。

---

## 三、 数据分级与降级策略（反脆弱依赖）

### 3.1 预言机与资产风控（新增）
> 这是一个基于 USDT 的系统，如果 USDT 本身出问题，所有逻辑作废。

- **脱锚检测**：
  - 监控 `USDC/USDT` 或 `DAI/USDT` 交易对。
  - 若偏离 `1.0` 超过 `2%`，触发 **Global Emergency Exit**（清仓并尝试换回 USD/BTC，或停止交易）。

- **价格偏差检测（Oracle Guard）**：
  - 对比 `CEX_Mid_Price` 与 `Chainlink_Oracle_Price`（或另一头部 CEX 价格）。
  - 若 `|Myself - Oracle| > 3%`：视为**本交易所数据毒化**或**插针**。即刻停止开新仓，持仓止损改用 Oracle 价格触发（如果交易所支持）或强制本地模拟止损。

### 3.2 数据源降级
- **L1/L2 失效时**：若无 depth/spread，强制使用 `Default_Conservative_Params`（仓位减半，滑点容忍度加倍），而不是直接停机（避免无法止损）。
- **非关键数据（Funding/Calendar）失效时**：策略降级运行，假设当前为 `High_Risk` 状态，降低杠杆。

---

## 四、 策略模块（闭环与互斥）

### 4.1 核心策略：趋势（Trend Following）
- **入场**：
  - 信号：`Trend_Score > 60` 且 `Price` 回调至 `EMA Zone` 或突破 `Structure` 确认。
  - 确认：`Exec_Score > 0.5`。
- **出场**：
  - 止损：`ATR` 止损或结构位止损（取更近者）。
  - 止盈：分批（`2R` / `Trend_Score < 40` 时全平）。

### 4.2 核心策略：均值回归（Mean Reversion）
- **入场**：
  - 信号：`Trend_Score < 30` 且 触及 `Bollinger Band` + `Rejection` 形态（吞没/插针）。
  - 确认：需要 `Volume` 配合（如缩量回踩或放量拒绝）。
- **出场**：
  - 目标：均值（EMA 20/50）。
  - 止损：极窄结构止损（亏损必须小）。

### 4.3 裁决引擎（Conflict Resolution）
当趋势与震荡信号冲突时：
1.  计算 **Expected Value (EV)**：`Win_Rate * Reward - Loss_Rate * Risk`（基于历史回测数据）。
2.  若 `Trend_Score > 50`，优先趋势。
3.  若 `Trend_Score < 50` 且 `Rev_EV > Trend_EV`，优先均值回归。
4.  **互斥锁**：同一品种同一时间只能持有一个方向的仓位。

---

## 五、 风控体系（硬约束）

### 5.1 紧急逃生通道（Emergency Exit）
> 解决死锁问题：当市场剧烈波动导致 Spread 极大时，普通逻辑会禁止交易，导致无法止损。

- **规则**：
  - **开仓**：必须满足 `Exec_Score > Threshold`。
  - **平仓（止损）**：**忽略所有状态机限制**。只要触及硬止损线，强制以市价单（IOC）或激进限价单甩卖，宁可承受 5% 滑点也不承担归零风险。

### 5.2 仓位管理（Kelly 的分数应用）
- `Risk_Per_Trade` = `Account_Equity * 2% * Regime_Confidence * Exec_Score`
  - 基准风险：2%
  - 市场信心：`High Trend = 1.0`, `Weak Trend = 0.5`
  - 执行质量：`Good Depth = 1.0`, `Bad Depth = 0.5`
- 结果：环境不好时自动做小仓位，而不是不做。

### 5.3 组合相关性（动态杠杆）
- 监控持仓品种的 `Correlation Matrix`。
- 若 `Corr(A, B) > 0.8`，则 A 和 B 的合并仓位即使不超过单品种上限，也必须满足**板块上限（Sector Cap）**。

---

## 六、 附录：关键默认参数表（可配置）

| 参数 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `RISK_BASE` | 2.0% | 基础风险单元 |
| `MAX_DRAWDOWN` | 20% | 强制停机线 |
| `ORACLE_DEV_TH` | 3.0% | 预言机偏差阈值 |
| `USDT_PEG_TH` | 0.98 | USDT 脱锚阈值 |
| `EXEC_SCORE_MIN` | 0.2 | 开仓最低健康分 |
| `TREND_TH` | 60 | 趋势策略启用分 |
| `RANGE_TH` | 30 | 震荡策略权重启用分 |
