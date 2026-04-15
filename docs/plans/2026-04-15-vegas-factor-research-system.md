# Vegas Factor Research System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 Vegas 4H 搭建独立的外部因子研究系统，先用正式基线回测数据验证外部上下文因子，再决定是否回注到策略。

**Architecture:** 保留 Vegas 作为交易主框架，不直接改策略信号链。新增只读研究服务，从 `back_test_log/back_test_detail/filtered_signal_log/external_market_snapshots` 构建统一研究样本，输出稳定的文本研究报告。第一阶段优先支持 `funding/premium/open_interest/price+oi/on-chain flow proxy` 的事件分桶与 `BTC / ETH / 其他币种` 三层分组。

**Tech Stack:** Rust workspace, sqlx/MySQL, existing Hyperliquid + Dune snapshot storage, CLI example entrypoint

### Task 1: 建立研究领域模型与纯函数测试

**Files:**
- Create: `/Users/mac2/onions/rust_quant/crates/services/src/strategy/vegas_factor_research_service.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/services/src/strategy/mod.rs`
- Test: `/Users/mac2/onions/rust_quant/crates/services/tests/vegas_factor_research.rs`

**Step 1: Write the failing test**

覆盖：
- `BTC / ETH / 其他币种` 波动性分层
- 最近 `4H` 外部快照对齐
- `price + open_interest` 状态分类
- 因子结论标签：`可回注 / 仅观察 / 拒绝`

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-services vegas_factor_research -- --nocapture`
Expected: FAIL，因为服务和测试对象尚不存在

**Step 3: Write minimal implementation**

- 建立研究样本、快照特征、分桶统计和报告结构体
- 先实现纯函数，不接数据库

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-services vegas_factor_research -- --nocapture`
Expected: PASS

### Task 2: 接回数据库读取正式基线样本

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/services/src/strategy/vegas_factor_research_service.rs`
- Test: `/Users/mac2/onions/rust_quant/crates/services/tests/vegas_factor_research.rs`

**Step 1: Write the failing test**

增加最小数据库驱动测试替身，验证：
- 能按 baseline backtest ids 读取 open/close 样本
- 能把交易样本和最近快照按 `4H` 对齐
- 能提取至少 3 类外部因子概览

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-services vegas_factor_research -- --nocapture`
Expected: FAIL，因为尚未实现样本装载与快照拼接

**Step 3: Write minimal implementation**

- 用 `sqlx` 在服务层直接读取研究所需字段
- 将每笔交易与最近快照对齐
- 计算派生因子：
  - funding 变化
  - premium/basis
  - open interest 变化
  - `price + oi` 状态

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-services vegas_factor_research -- --nocapture`
Expected: PASS

### Task 3: 增加 CLI 研究入口与文本报告

**Files:**
- Create: `/Users/mac2/onions/rust_quant/crates/rust-quant-cli/examples/run_vegas_factor_research.rs`
- Modify: `/Users/mac2/onions/rust_quant/docs/external_market_data/README.md`

**Step 1: Write the failing test**

增加报告渲染测试，验证输出包含：
- 因子概览表
- 分桶统计表
- 三层分组统计
- 结论标签

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-services vegas_factor_research_report -- --nocapture`
Expected: FAIL，因为报告渲染或示例入口不存在

**Step 3: Write minimal implementation**

- 提供 CLI example，默认读取正式基线 `1428,1429,1430,1431`
- 支持环境变量覆盖 baseline ids、symbol filters、输出文件路径
- 先输出稳定文本报告，必要时可额外落地到 `tmp/`

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-services vegas_factor_research_report -- --nocapture`
Expected: PASS

### Task 4: 验证真实执行与文档更新

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/docs/VEGAS_ITERATION_LOG.md`

**Step 1: Build and run the research entrypoint**

Run: `cargo build -p rust-quant-cli --example run_vegas_factor_research`

**Step 2: Execute against current formal baseline**

Run: `DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' cargo run -p rust-quant-cli --example run_vegas_factor_research`

**Step 3: Confirm report contents**

Expected:
- 至少 3 类因子概览
- `BTC / ETH / 其他币种` 三层结果
- 至少一个明确结论标签

**Step 4: Record implementation**

- 更新 `docs/VEGAS_ITERATION_LOG.md`
- 说明这是“研究系统”基础设施，不是策略基线升级
