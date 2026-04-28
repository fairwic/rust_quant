# Rust Quant

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![DDD](https://img.shields.io/badge/architecture-DDD-blue.svg)](https://en.wikipedia.org/wiki/Domain-driven_design)
[![Workspace](https://img.shields.io/badge/workspace-14%20crates-blue.svg)]()
[![Status](https://img.shields.io/badge/status-active--development-yellow.svg)]()

> 基于 Rust Workspace 与 DDD 分层的量化交易系统。

## 项目概览

Rust Quant 是一个使用 Rust 实现的企业级量化交易系统。仓库采用 Workspace 组织方式，当前包含 14 个 crate，覆盖数据同步、技术指标、策略执行、风控、订单执行、任务编排与分析等核心能力。

README 侧重说明仓库入口、常用命令、运行方式与关键文档索引。更细的架构设计、启动说明和策略细节请查看文末文档索引。

## 核心能力

- 基于 DDD 的分层架构，模块职责清晰。
- 采用 Rust Workspace 管理多 crate 工程。
- 提供市场数据同步、技术指标计算与策略执行链路。
- 支持回测、模拟盘验证与实盘运行入口。
- 集成 Vegas 相关策略、流程图与调参辅助材料。

## 快速开始

### 克隆与编译

```bash
git clone <repository>
cd rust_quant
cargo build --workspace --release
```

### 常用命令

```bash
# 编译检查
cargo check --workspace

# 运行测试
cargo test --workspace

# 运行 clippy
cargo clippy --all

# 启动 CLI
cargo run --package rust-quant-cli --release

# 或直接运行构建产物
./target/release/rust-quant
```

### 数据库初始化

项目自带迁移文件，可直接执行：

```bash
cargo sqlx migrate run
```

### 环境配置

创建 `.env` 文件并按需配置：

```bash
APP_ENV=local

QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@127.0.0.1:5432/quant_core
DATABASE_URL=postgres://postgres:postgres123@127.0.0.1:5432/quant_core
REDIS_URL=redis://127.0.0.1:6379

IS_RUN_SYNC_DATA_JOB=true
IS_BACK_TEST=true
IS_OPEN_SOCKET=true
IS_RUN_REAL_STRATEGY=false
```

## 运行方式

### 回测

- 使用 `.env` 中 `IS_BACK_TEST=true` 启动回测模式。
- Vegas 指定回测通常依赖 `ENABLE_SPECIFIED_TEST_VEGAS=true`。
- 详细启动说明见 [docs/STARTUP_GUIDE.md](docs/STARTUP_GUIDE.md)。

### 模拟盘 / 实盘验证

- 模拟盘需设置 `OKX_SIMULATED_TRADING=1`，并提供 `OKX_SIMULATED_API_KEY`、`OKX_SIMULATED_API_SECRET`、`OKX_SIMULATED_PASSPHRASE`。
- 关闭回测并直连运行链路时，可使用：
  `IS_BACK_TEST=false IS_OPEN_SOCKET=true IS_RUN_REAL_STRATEGY=true`
- 启动示例：

```bash
OKX_SIMULATED_TRADING=1 cargo run -p rust-quant-cli
```

- 模拟盘端到端下单 / 平仓验证：

```bash
RUN_OKX_SIMULATED_E2E=1 \
OKX_TEST_INST_ID=ETH-USDT-SWAP \
OKX_TEST_SIDE=buy \
OKX_TEST_ORDER_SIZE=1 \
cargo test -p rust-quant-services --test okx_simulated_order_flow -- --ignored --nocapture
```

当前基线策略为 Vegas 4H，默认不启用止盈，仅按 `max_loss_percent` 止损；实盘下单的初始止损已对齐回测逻辑。

## Vegas 回测与调参

如需进行 Vegas 随机批量调参，建议按以下顺序操作：

1. 保持 `.env` 中 `ENABLE_RANDOM_TEST=false`、`ENABLE_RANDOM_TEST_VEGAS=false`、`ENABLE_SPECIFIED_TEST_VEGAS=true`，先运行一轮基线回测确认当前表现。
2. 调整 `strategy_config` 中尚未完全启用的信号项，例如 `leg_detection_signal`、`market_structure_signal`，先用指定回测观察信号分布和持仓行为。
3. 需要批量测试时，再切换为 `ENABLE_RANDOM_TEST=true`、`ENABLE_RANDOM_TEST_VEGAS=true`、`ENABLE_SPECIFIED_TEST_VEGAS=false`。
4. 找到更优参数后，恢复指定回测模式并重新生成 `back_test_detail` 供最终对比分析。

Vegas 回测分析与启动细节可参考 [docs/STARTUP_GUIDE.md](docs/STARTUP_GUIDE.md) 以及仓库内相关 UML 图。

## 使用示例

```rust
use rust_quant_orchestration::workflow::*;
use rust_quant_services::*;

// 数据同步
sync_tickers(&inst_ids).await?;
CandlesJob::new().sync_latest_candles(&inst_ids, &periods).await?;

// 策略执行
let service = StrategyExecutionService::new();

// 风控检查
let risk = RiskManagementService::new();
```

## 架构概览

### Workspace 结构

```text
crates/
├── rust-quant-cli/  # 程序入口
├── core/            # 核心基础设施
├── domain/          # 领域模型层
├── infrastructure/  # 基础设施实现层
├── services/        # 应用服务层
├── market/          # 市场数据层
├── indicators/      # 技术指标层
├── strategies/      # 策略引擎层
├── risk/            # 风险管理层
├── execution/       # 订单执行层
├── orchestration/   # 任务编排层
├── analytics/       # 分析报告层
├── ai-analysis/     # AI 分析层
└── common/          # 通用工具层
```

### 分层依赖

```text
rust-quant-cli
    ↓
orchestration
    ↓
services
    ↓
domain + infrastructure
    ↓
market + indicators
    ↓
core + common
```

完整架构说明见 [docs/quant_system_architecture_redesign.md](docs/quant_system_architecture_redesign.md)。

## Vegas 策略流程图

这组图只描述 Vegas 策略本身的信号判断、后置过滤与最终交易结果，不包含数据准备、K 线加载和回测任务编排。

### 总览预览

[![Vegas 策略流程总览](uml/image/vegas_signal_to_trade_detailed.png)](uml/image/vegas_signal_to_trade_detailed.png)

### 图纸目录

| 图纸 | PNG | PlantUML | 说明 |
| --- | --- | --- | --- |
| 总览导航图 | [查看](uml/image/vegas_signal_to_trade_detailed.png) | [源文件](uml/vegas_signal_to_trade_detailed.puml) | 策略流程导航入口 |
| 方向判断详图 | [查看](uml/image/vegas_signal_direction_detailed.png) | [源文件](uml/vegas_signal_direction_detailed.puml) | 指标计算到方向结论 |
| 后置过滤详图 | [查看](uml/image/vegas_post_filters_detailed.png) | [源文件](uml/vegas_post_filters_detailed.puml) | 过滤条件与拦截原因 |
| 交易结果详图 | [查看](uml/image/vegas_trade_outcomes_detailed.png) | [源文件](uml/vegas_trade_outcomes_detailed.puml) | 最终信号到开平仓结果 |

### 覆盖范围

- 方向判断：权重方向判断、Fib 大趋势覆盖、实验型方向覆盖，以及止损止盈初值来源。
- 后置过滤：Fib 严格趋势过滤、EMA/Fib 区间过滤、结构突破质量过滤、追涨追跌确认、Extreme K、Range Filter、MACD Falling Knife 与多空专属过滤。
- 交易结果：最终 `SignalResult` 到直接开仓、等待更优价格、持仓更新止损止盈、反向信号平仓与风控平仓的映射过程。

### 图片导出

如需重新生成图片，可执行：

```bash
plantuml -tpng -o image uml/vegas_signal_to_trade_detailed.puml
plantuml -tpng -o image uml/vegas_signal_direction_detailed.puml
plantuml -tpng -o image uml/vegas_post_filters_detailed.puml
plantuml -tpng -o image uml/vegas_trade_outcomes_detailed.puml
```

## 文档索引

| 文档 | 说明 |
| --- | --- |
| [docs/STARTUP_GUIDE.md](docs/STARTUP_GUIDE.md) | 启动方式、环境变量与运行说明 |
| [docs/quant_system_architecture_redesign.md](docs/quant_system_architecture_redesign.md) | 架构设计与分层说明 |
| [uml/image/vegas_signal_to_trade_detailed.png](uml/image/vegas_signal_to_trade_detailed.png) | Vegas 流程图总览 |
| [uml/image/vegas_signal_direction_detailed.png](uml/image/vegas_signal_direction_detailed.png) | Vegas 方向判断详图 |
| [uml/image/vegas_post_filters_detailed.png](uml/image/vegas_post_filters_detailed.png) | Vegas 后置过滤详图 |
| [uml/image/vegas_trade_outcomes_detailed.png](uml/image/vegas_trade_outcomes_detailed.png) | Vegas 交易结果详图 |

## 贡献

欢迎提交问题、改进建议和代码变更。提交前建议至少执行以下检查：

```bash
cargo fmt --all
cargo clippy --all
cargo test --workspace
```

## 许可证

MIT License

## 状态

- 开发状态：持续开发中
- 最近更新：2026-03-24
