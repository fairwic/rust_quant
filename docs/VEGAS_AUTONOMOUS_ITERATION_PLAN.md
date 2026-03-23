# Vegas 长期自动迭代规则

## 1. 当前主目标

Vegas 4H 后续优化统一按以下优先级执行：

1. `max_drawdown` 更低
2. `volatility` 更低
3. `win_rate` 更高
4. `profit` 保持较好，不接受明显塌陷
5. `sharpe_ratio` 作为综合质量确认项

当前代码可复现的风险优先基线固定为：

- `back_test_id = 15867`
- `win_rate = 57.5290%`
- `profit = 49771.11`
- `sharpe = 5.61787`
- `max_drawdown = 16.9719%`
- `volatility = 51.1519%`
- 当前标签：`stack-level cross-asset accepted`

跨币种伴随对照：

- `BTC`: `15868`, `59.6452% / 91.81 / 0.36053 / 35.7094% / 41.6456%`
- `SOL`: `15869`, `45.7792% / 383.47 / 0.76673 / 50.8326% / 86.1492%`

历史风险优先前沿原本是 `15805`，但当前代码树已经跑出更优的 `15867/15868/15869` 跨币种版本。  
因此，除非后续出现更优候选，否则一律以这组结果作为“当前代码对照基线”。

## 2. 长期执行原则

### 2.1 只做窄实验

- 每轮只验证 `1` 条假设
- 代码改动必须可开关，优先使用环境变量
- 不同时叠加多个新规则去看结果

### 2.2 先修大亏损，再修低质量高频信号

优化优先级：

1. 最大单笔亏损样本
2. 某类反复出现、且统计上稳定亏损的信号
3. 只在局部样本成立、但会破坏全局路径的规则，直接拒绝

### 2.3 不为了修单笔样本破坏全局

如果一条规则：

- 只修 `1~2` 笔坏单
- 但导致全局 `profit` 或 `sharpe` 明显下降

则视为局部正确、全局错误，不保留。

### 2.3.1 新规则必须验证普适性，避免过耦合

后续所有新增优化，默认必须先经过“普适性闸门”：

1. 先做分布验证，再做真实 A/B
2. 优先接受“至少 `2` 笔同类亏损样本”支持的规则
3. 如果只命中 `1` 笔，则默认视为高耦合候选，不直接晋级
4. 单样本规则只有在同时满足以下条件时，才允许例外接受：
   - `max_dd` 不恶化
   - `volatility` 不恶化或只极轻微波动
   - `win_rate / sharpe / profit` 有明显提升
   - 且后续路径改善不是偶然性的单笔替换
5. 对所有候选规则，必须额外回答这 3 个问题：
   - 它修的是“单个时间点”，还是“一类盘面状态”
   - 同类样本在历史里有几笔，盈亏分布如何
   - 放宽一个维度后，是否会立刻误伤有效单

如果以上 3 个问题回答不清，规则不晋级，只能保留为候选观察。

### 2.3.2 新规则必须验证跨币种普适性

后续所有新增参数和环境变量规则，除了通过 `ETH 4H` 的本地基线验证外，还必须追加“跨币种闸门”：

1. 先过 `ETH`
   - `max_dd` 不恶化
   - `volatility` 不恶化
   - `win_rate / profit / sharpe` 至少有 `2` 项改善
2. 再复查 `BTC / SOL`
   - 优先同周期 `4H`
   - 优先比较 `max_dd / volatility / win_rate`
   - `profit / sharpe` 允许轻微波动，但不能明显塌陷
3. 如果 `BTC` 或 `SOL` 任一币种出现以下情况，则该规则不得直接晋级为“长期主线规则”
   - `max_dd` 明显恶化
   - `volatility` 明显恶化
   - 明显只对 ETH 波动分布或 ETH 价格结构成立
4. 如果当前无法完成 `BTC / SOL` 回测验证
   - 该规则只能标记为 `ETH provisional`
   - 不得写成“已验证普适”
   - 必须在日志里写明阻塞原因
5. 只有同时满足以下条件，才允许标记为 `cross-asset accepted`
   - `ETH` 通过
   - `BTC` 不恶化
   - `SOL` 不恶化

额外约束：

- `stack-level cross-asset accepted` 不等于“单条规则全部已验证普适”
- 如果是整套规则栈一起通过 `ETH / BTC / SOL`，只能先标记为：
  - `stack-level cross-asset accepted`
- 只有单条规则在独立开关下也完成 `ETH / BTC / SOL` 复跑，才允许单独标记为：
  - `rule-level cross-asset accepted`
- 后续日志必须明确区分：
  - 是“规则栈跨币种通过”
  - 还是“单条规则跨币种通过”

默认判断原则：

- 更可能普适：趋势状态、MACD 相位、布林带冲突、Fib/结构确认缺失、量价背离
- 更可能过耦合：整数位、单一价格刻度、只对 ETH 价格分布成立的阈值、只命中单时间段局部行为
- 若一条规则语义合理、但只在 ETH 明显生效，优先尝试：
  - 价格尺度分层阈值
  - `price` 归一化阈值
  - percentile / z-score / rolling rank
  再决定它是“无效规则”还是“未缩放规则”
- 若经过尺度归一化后出现以下结果：
  - `ETH` 改善
  - `BTC` 至少不恶化
  - `SOL` 改善或不恶化
  则优先把它归类为“参数尺度已验证”，而不是继续当作纯 `ETH provisional`

### 2.4 代码基线与实验基线分离

- 默认代码必须保持“当前有效基线”
- 试验规则只能通过环境变量启用
- 失败实验不得默认生效

## 3. 每轮固定执行流程

### Step 1: 选择样本

按当前基线回测结果，从以下池中选一个目标：

- 最大亏损 `long/short`
- 最近新增明显异常的交易
- 某一类过滤/入场模式的统计性弱点

### Step 2: 做结构分析

每个目标样本必须先输出：

- 当前 K 线 OHLCV
- 前后至少 `2~4` 根 K 线
- `ema / macd / rsi / boll / fib / leg / market_structure`
- 当前信号为什么成立
- 为什么从盘面上看不该开，或为什么该保护而不是开反向
- 同类样本是否至少有 `2` 笔，还是只有单样本

### Step 3: 提出单条假设

假设只能属于以下 4 类之一：

1. `entry block`
2. `entry allow`
3. `protective stop / breakeven`
4. `late confirmation / delay entry`

每次只能选一种。

### Step 4: 环境变量实验

实验实现要求：

- 新规则默认 `off`
- 命名必须能看出用途
- 必须在 `filtered_signal_log` 留下明确 reason

示例：

- `VEGAS_DEEP_NEGATIVE_MACD_SHORT_BLOCK_MODE=v3`
- `VEGAS_RECENT_UPPER_SHADOW_LONG_BLOCK=v3`

### Step 5: 先跑 ETH 4H，本地确认主效果

统一命令：

```bash
TIGHTEN_VEGAS_RISK=0 \
IS_RUN_SYNC_DATA_JOB=0 \
SYNC_ONLY_INST_IDS=ETH-USDT-SWAP \
DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' \
cargo run --bin rust_quant
```

### Step 5.1: 追加 BTC / SOL 普适性复查

在 ETH 结果通过后，必须继续做：

- `BTC-USDT-SWAP 4H`
- `SOL-USDT-SWAP 4H`

并输出：

- 是否找到可运行配置
- 是否成功跑出回测
- 相对各自基线的 `max_dd / volatility / win_rate / profit / sharpe`

如果这一步做不了，必须明确写：

- 是配置缺失
- 是加载条件不一致
- 还是回测入口本身没有覆盖到该币种

未完成这一步时，该规则默认只能算 `ETH provisional`。

### Step 6: 对比并判定

每轮必须输出：

- 新 `back_test_id`
- 与基线的 `win_rate / profit / sharpe / max_dd / volatility / open_positions_num`
- 新规则命中次数
- 命中的样本清单
- 最大改善点
- 最大回吐点
- 同类样本数量与分布结论
- `ETH / BTC / SOL` 三个币种的验证状态：`passed / provisional / blocked`
- 若未做 `BTC / SOL`，必须写明原因，以及该规则暂时只能属于哪一级

## 4. 判优与拒绝标准

### 4.1 直接晋级条件

满足以下任一组即可晋级：

1. `max_dd` 更低，`volatility` 更低，`win_rate` 更高，且 `profit` 不低于基线的 `95%`
2. `max_dd` 持平或更低，`volatility` 持平或更低，`profit` 更高，`win_rate` 不降
3. `profit` 与 `win_rate` 略降，但 `max_dd` 和 `volatility` 明显下降，且符合风险优先目标

### 4.2 直接拒绝条件

出现以下任一情况直接拒绝：

1. `max_dd` 明显恶化
2. `volatility` 明显恶化
3. `profit` 明显下降，且并没有换来更低回撤
4. 规则命中范围过大，明显从“窄实验”变成“大面积收缩”
5. 规则只修局部样本，但全局路径损伤更大
6. 规则明显只服务于单一样本，缺少同类分布支持

### 4.3 路径敏感样本处理

对于这类样本：

- 单笔看是坏单
- 去掉后整体反而变差

统一处理为：

- 不再继续尝试 `entry block`
- 改研究 `stop`、`breakeven` 或 `exit timing`

## 5. 长期优先优化方向

后续固定按以下顺序轮动：

1. `short` 侧大亏损异常样本
2. `long` 侧大亏损异常样本
3. 已有仓位保护逻辑
4. 少量高质量 near-miss 提前开仓
5. 最后才是参数层搜索

不再优先做：

- 宽泛的 `long entry block`
- 大面积过滤 `bullish continuation`
- 只凭单笔图形直觉直接上全局规则

## 6. 日志要求

每次迭代必须记录到 [VEGAS_ITERATION_LOG.md](/Users/xu/onions/rust_quant/docs/VEGAS_ITERATION_LOG.md)，最少包括：

- 日期
- 基线 ID
- 新回测 ID
- 实验规则
- 结果对比
- 为什么变好 / 变差
- 是否晋级
- 是否通过跨币种闸门
- 若未通过，标记为 `ETH provisional`，不能写成“普适规则”

## 6.1 规则外创新因子池

除了当前 Vegas 主线里的结构、形态、止损与过滤规则，后续自动优化允许周期性插入一类“规则外但市场已广泛认可”的标准因子实验。

这些因子不作为默认主线，而是作为第二优先级探索池：

1. `ADX / DMI`
   用于区分趋势扩张与震荡修复，优先解决“趋势末端追单”和“震荡中误判趋势延续”。

2. `ATR Percentile / NATR`
   用于识别当前波动处于历史高位还是低位，适合做：
   - 高波动收紧止损
   - 低波动不追突破
   - 极端波动后的恢复期过滤

3. `Anchored VWAP / VWAP Deviation`
   用于判断价格是否已经偏离事件锚点、波段锚点或近期均衡价格太远。
   重点用于：
   - 低位追空过滤
   - 高位追多过滤
   - 均值回归保护止损

4. `Donchian Breakout / Channel Width`
   用于识别真假突破、通道压缩后的扩张，以及突破是否具备“新高/新低确认”。

5. `Keltner Channel / Squeeze`
   用于识别布林带压缩与释放，尤其适合补当前 Vegas 在“爆发前横盘”和“爆发后衰竭”上的识别能力。

6. `CMF / OBV / A-D Line`
   用于验证量价是否同向，不只看单根 volume_ratio，而看资金累积/派发方向。

7. `Stochastic RSI`
   用于处理普通 RSI 不够敏感的场景，尤其是深负 MACD 区反弹、零轴上方转弱这类拐点。

8. `CCI / Z-Score`
   用于做价格偏离程度判断，优先作为“不过度追高/追低”的辅助因子，而不是主触发因子。

9. `Market Regime`
   使用收益波动、趋势强度、布林宽度、ATR 百分位等组合成：
   - Trend
   - Range
   - Expansion
   - Post-shock repair
   这类状态标签优先只用于风控，不直接做开仓。

10. `MTF Confirmation`
   使用更高一级周期做轻量确认，例如：
   - 4H 入场，日线只做斜率/位置过滤
   - 不允许高周期明显相反时做低周期追单

### 创新因子接入原则

- 只引入“市场已广泛接受”的标准因子，不优先发明黑箱特征
- 每次只接入 `1` 个因子或 `1` 类状态标签
- 首轮必须以 `权重=0 / 过滤关闭 / 仅记录` 的方式验证分布
- 只有确认对当前主目标有帮助，才允许进第二轮真实 A/B
- 创新因子优先用于：
  - `protective stop`
  - `regime filter`
  - `late confirmation`
- 创新因子不优先用于：
  - 直接覆盖 Vegas 现有主触发
  - 一次性大面积替换现有信号系统

### 何时切换到创新因子池

当满足以下任一情况时，可以从主线样本优化切到创新因子池一轮：

1. 连续 `3` 轮窄实验都被拒绝
2. 当前最大亏损样本已经重复验证但无法全局改进
3. 当前退化主要来自“市场状态识别不足”，而不是单一入场/止损逻辑

## 7. 自动执行边界

后续默认允许自动继续以下工作，无需逐次确认：

- 分析样本
- 提出窄实验
- 编码实现
- 跑本地 ETH 4H 回测
- 写迭代日志
- 拒绝失败实验

后续默认不自动做以下动作，除非明确优于基线：

- 覆盖当前基线配置
- 修改 `strategy_config.id=11`
- 把实验规则改成默认生效

## 8. 当前立即执行策略

从现在开始，后续 Vegas 自动优化遵循：

1. 以当前正式跨币种风险优先基线为锚：
   - `ETH = 15867`
   - `BTC = 15868`
   - `SOL = 15869`
2. 只做窄实验
3. 优先级顺序：
   大亏损样本 -> 同类样本分布验证 -> 退出/保护逻辑 -> 少量高质量 near-miss
4. 每轮跑完立即写日志
5. 实验失败就回退到“默认关闭”
6. 接受规则默认标准：
   `max_dd` 不变或更低，且 `volatility` 不显著恶化；在此前提下优先提高 `win_rate / sharpe / profit`
7. 新增规则默认必须先证明“修的是一类状态”，不是只修一根 K 线
8. “巨量”分支的默认解释：
   巨量本身不是单向做多/做空信号，而是优先视为“强趋势确认或反转/分歧状态”。
   因此后续默认先检查：
   - 是否已有结构性 BOS/CHOCH 确认
   - 是否出现布林/形态冲突
   - 是否处在 `TooFar` 衰竭区
   - 是否只是巨量后的错误追价
   只有当这些状态支持时，才允许把巨量做成新过滤或保护规则。
9. 巨量分支的晋级闸门：
   - 优先接受 `>=2` 笔同类亏损且 `0` 盈利样本的规则
   - 若存在极少量盈利样本，必须同时满足：
     - 跨年/跨阶段分布
     - 亏损样本占主导
     - `win_rate / sharpe / profit` 显著改善
10. “整数关口首次极端触达”分支的默认解释：
   - 整数位本身不是单独信号
   - 只有当它与“长期站上/压下该关口 + 短期首次极端触达 + 明显放量 + 当根拒绝/回收形态”共同出现时，才允许作为反转候选
   - 后续默认优先把它视为“极端波动后的流动性反转位”，不是普通支撑阻力回踩
11. 跨币种参数尺度优化优先级：
   - 若一条规则在 `ETH/BTC/SOL` 都有命中，但混入少量盈利样本，优先尝试“参数尺度修正”
   - 优先缩放：
     - `abs(signal_line) / price`
     - `abs(histogram) / price`
     - `volume percentile / z-score`
   - 不优先继续调绝对阈值，除非先证明该规则对不同价格尺度没有偏差
12. 规则晋级说明：
   - 在实验规则栈里获得改善的规则，只能标为 `scaling-improved candidate`
   - 只有在当前正式跨币种基线 `15867/15868/15869` 上完成单规则开关复核后，才能升级为：
     - `rule-level cross-asset accepted`
13. 跨币种复核的额外否决条件：
   - 即使某条规则在 `ETH/BTC/SOL` 的命中样本里都是纯亏损，也不能直接晋级
   - 只要它在任一币种的完整回测路径上导致：
     - `profit / sharpe` 下降
     - 或 `max_dd` 恶化
   - 则只能记为 `mixed cross-asset candidate`
14. 小币种参数优化流程：
   - 当目标从 `ETH` 切换到 `SOL / BCH` 这类更高波动币种时，默认先复制当前 `ETH` 最优策略逻辑
   - 第一阶段不改规则，只允许做 `volatility-only tuning`
   - 仅允许调整：
     - `range_filter_signal.bb_width_threshold`
     - `range_filter_signal.tp_kline_ratio`
     - `extreme_k_filter_signal.min_move_pct`
     - `extreme_k_filter_signal.min_body_ratio`
     - `risk.max_loss_percent`
     - `risk.atr_take_profit_ratio`
   - 不允许同时改：
     - 信号权重
     - 新过滤规则
     - 新保护逻辑
   - 选择标准：
     - 以目标币种（如 `SOL`）为主目标
     - 以次级币种（如 `BCH`）为鲁棒性验证
     - 若主目标仅有极小收益提升，但次级币种显著恶化，则拒绝
     - 优先接受“主目标 `sharpe / max_dd / volatility` 改善，且次级币种风险同步收敛”的版本
   - 若第一阶段结束后：
     - 主目标币种已基本收敛
     - 次级币种仍显著偏弱
   - 则允许进入第二阶段：
     - 锁定主目标币种参数不动
     - 仅对次级币种继续做 `volatility-only tuning`
   - 第二阶段的接受标准：
     - 不要求次级币种立刻转正
   - 但必须同时改善至少 `profit / sharpe / max_dd` 中的多数指标
   - 若连续 2 到 3 轮只能改善风险、无法改善收益，则判定“仅靠 volatility-only tuning 已到上限”
15. 单币种主线运行约束：
   - 当明确切回 `ETH-only` 主线时，后续回测默认使用：
     - `BACKTEST_ONLY_INST_IDS=ETH-USDT-SWAP`
   - 不再默认混跑 `BTC / SOL / BCH`
   - 只有当任务明确要求跨币种复核时，才恢复多币种回测
16. 风险参数实验闸门：
   - 若某个风险参数实验同时导致：
     - `profit` 明显下降
     - `sharpe` 明显下降
     - `max_dd` 恶化
   - 则立即判定为错误方向，不再沿该参数继续细磨
   - 这种情况默认回退基线参数，重新回到 signal / rule 层继续拆亏损簇
