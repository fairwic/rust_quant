# External Market Data Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 Vegas 策略引入可复用的外部市场数据接入基础设施，优先落地 Hyperliquid funding/open-interest 快照与 Dune 查询模板，并为后续 OKX/Binance 扩展保留统一接口。

**Architecture:** 先在 infrastructure 层新增独立的 Hyperliquid 公共客户端，不直接耦合策略。领域层补充“外部市场快照”实体与仓储抽象，服务层新增同步服务；第一阶段只要求“可抓取、可序列化、可保存、可查询”，不直接接入交易信号。Dune 先提供查询模板与接入说明，避免没有 API key 时阻塞实现。

**Tech Stack:** Rust, reqwest, serde, sqlx, MySQL, tokio

### Task 1: 建立计划与数据模型边界

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/domain/src/entities/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/domain/src/entities/external_market_snapshot.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/domain/src/traits/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/domain/src/traits/external_market_snapshot_repository.rs`
- Create: `/Users/mac2/onions/rust_quant/migrations/20260409090000_create_external_market_snapshots.sql`

**Step 1: Write the failing test**

为 `ExternalMarketSnapshot` 增加最小序列化/反序列化测试，覆盖：
- `source`
- `symbol`
- `metric_time`
- `funding_rate`
- `premium`
- `open_interest`
- `raw_payload`

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-domain external_market_snapshot -- --nocapture`
Expected: FAIL，因为实体/模块尚不存在

**Step 3: Write minimal implementation**

- 新增领域实体
- 新增仓储 trait
- 补 migration，唯一键使用 `(source, symbol, metric_type, metric_time)`

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-domain external_market_snapshot -- --nocapture`
Expected: PASS

### Task 2: 接入 Hyperliquid 公共客户端

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/lib.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/exchanges/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/exchanges/hyperliquid_adapter.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/exchanges/factory.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/infrastructure/Cargo.toml`

**Step 1: Write the failing test**

为 Hyperliquid 响应解析写单测，覆盖两个方法：
- `fundingHistory`
- `metaAndAssetCtxs`

测试不依赖网络，使用固定 JSON 样本断言字段提取正确：
- funding `time / fundingRate / premium`
- meta `coin / markPx / oraclePx / openInterest`

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-infrastructure hyperliquid -- --nocapture`
Expected: FAIL，因为 adapter 和 DTO 尚不存在

**Step 3: Write minimal implementation**

- 新增 `HyperliquidPublicAdapter`
- 使用 `reqwest` POST `https://api.hyperliquid.xyz/info`
- 提供：
  - `fetch_funding_history(coin, start, end)`
  - `fetch_meta_and_asset_ctxs()`
- 不先塞进 `ExchangeMarketData` trait，避免污染现有市场 K 线抽象

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-infrastructure hyperliquid -- --nocapture`
Expected: PASS

### Task 3: 建立外部市场快照仓储与同步服务

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/repositories/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/infrastructure/src/repositories/external_market_snapshot_repository.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/services/src/market/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/services/src/market/external_market_sync_service.rs`
- Modify: `/Users/mac2/onions/rust_quant/crates/orchestration/src/workflow/mod.rs`
- Create: `/Users/mac2/onions/rust_quant/crates/orchestration/src/workflow/external_market_sync_job.rs`

**Step 1: Write the failing test**

写 service 级测试，验证：
- Hyperliquid funding 行能被转换成 `ExternalMarketSnapshot`
- `metaAndAssetCtxs` 能拆成当前快照
- 同一 `(source, symbol, metric_type, metric_time)` 重复写入不会报错

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-services external_market_sync -- --nocapture`
Expected: FAIL，因为 service/repository 尚不存在

**Step 3: Write minimal implementation**

- repo 提供 `save` / `save_batch` / `find_range`
- service 先支持 `Hyperliquid`
- orchestration 增加 job，但默认不挂启动流程

**Step 4: Run test to verify it passes**

Run: `cargo test -p rust-quant-services external_market_sync -- --nocapture`
Expected: PASS

### Task 4: 提供 Dune 查询模板与说明

**Files:**
- Create: `/Users/mac2/onions/rust_quant/docs/external_market_data/README.md`
- Create: `/Users/mac2/onions/rust_quant/docs/external_market_data/dune/ethereum_cex_flow.sql`
- Create: `/Users/mac2/onions/rust_quant/docs/external_market_data/dune/hyperliquid_funding_basis.sql`
- Create: `/Users/mac2/onions/rust_quant/docs/external_market_data/dune/eth_whale_transfer.sql`

**Step 1: Write the failing test**

无需自动化测试；用文档完整性检查替代：
- 文件存在
- README 列出参数、时间窗口、预期字段

**Step 2: Implement**

- README 说明：
  - Dune 需要 API key
  - 查询用途
  - 输出字段如何映射到策略特征
- SQL 模板使用占位变量，不硬编码 query id

### Task 5: 为 OKX/Binance 预留扩展点

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/services/src/market/external_market_sync_service.rs`
- Modify: `/Users/mac2/onions/rust_quant/docs/external_market_data/README.md`

**Step 1: Write the failing test**

为 provider 枚举和 symbol normalizer 写测试，断言：
- `ETH-USDT-SWAP -> ETH`
- `ETHUSDT -> ETH`
- `source=Hyperliquid/OKX/Binance` 能走到不同分支

**Step 2: Run test to verify it fails**

Run: `cargo test -p rust-quant-services external_market_provider -- --nocapture`
Expected: FAIL

**Step 3: Write minimal implementation**

- 增加 `ExternalMarketSource`
- 增加 symbol normalizer
- 先返回 `NotImplemented` 给 OKX/Binance provider，但接口保留

**Step 4: Run all targeted checks**

Run:
- `cargo test -p rust-quant-domain external_market_snapshot -- --nocapture`
- `cargo test -p rust-quant-infrastructure hyperliquid -- --nocapture`
- `cargo test -p rust-quant-services external_market_sync -- --nocapture`
- `cargo build --bin rust_quant`

Expected: PASS
