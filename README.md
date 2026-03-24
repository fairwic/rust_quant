# Rust Quant - 企业级量化交易系统

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![DDD](https://img.shields.io/badge/architecture-DDD-blue.svg)](https://en.wikipedia.org/wiki/Domain-driven_design)
[![Workspace](https://img.shields.io/badge/workspace-14%20crates-blue.svg)]()
[![Status](https://img.shields.io/badge/status-active--development-yellow.svg)]()

> **企业级 DDD Workspace 架构 | 完整数据同步 | 持续开发中**

---

## 🎯 项目简介

Rust Quant 是一个基于**领域驱动设计(DDD)**的企业级量化交易系统，使用 Rust 语言实现。

采用 **Rust Workspace** 模式，包含 **14 个独立的 crate 包**，遵循严格的分层架构：

- ✅ **DDD Workspace 架构** - 14 个 crate 包，职责清晰
- ✅ **完整的数据同步** - 数据同步任务已实现
- ✅ **服务层框架** - 策略、订单、风控服务
- ✅ **泛型基础设施** - 缓存、Repository、工具
- ✅ **技术指标库** - 丰富的技术指标实现
- ✅ **策略引擎** - 支持多种策略实现
- ✅ **编译通过** - 有少量警告，不影响功能

---

## 🚀 快速开始

### 安装和编译

```bash
git clone <repository>
cd rust_quant
cargo build --workspace --release
```

### 运行

```bash
# 编译检查
cargo check --workspace

# 运行测试
cargo test --workspace

# 启动CLI (开发模式)
cargo run --package rust-quant-cli --release

# 或直接运行编译后的可执行文件
./target/release/rust-quant
```

### 🚦 实盘/模拟验证快捷流程

- 使用模拟盘：`OKX_SIMULATED_TRADING=1`，填写 `OKX_SIMULATED_API_KEY/SECRET/PASSPHRASE`。
- 关闭回测直奔实盘链路：`IS_BACK_TEST=false IS_OPEN_SOCKET=true IS_RUN_REAL_STRATEGY=true`（按需设置 `RUN_STRATEGY_PERIOD` 确保订阅周期与策略一致）。
- 启动：`OKX_SIMULATED_TRADING=1 cargo run -p rust-quant-cli`。
- 端到端下单/平仓自测（模拟盘）：
  `RUN_OKX_SIMULATED_E2E=1 OKX_TEST_INST_ID=ETH-USDT-SWAP OKX_TEST_SIDE=buy OKX_TEST_ORDER_SIZE=1 cargo test -p rust-quant-services --test okx_simulated_order_flow -- --ignored --nocapture`
  - 会下单（附带 TP/SL）、等待持仓出现、尝试改单到保本、平仓并校验持仓消失。
  - 若订单瞬时成交导致改单返回 “already filled or canceled”，测试已容错。

> 基线（Vegas 4H）默认无止盈，仅按 `max_loss_percent` 止损；实盘下单的初始止损已对齐回测（信号K线/1R/最大亏损取更紧者）。

### 🗄️ 初始化数据库（表结构）

项目自带迁移文件，推荐用 `migrations/` 直接初始化：

```bash
cargo sqlx migrate run
```

### 环境配置

创建 `.env` 文件并配置：

```bash
# 应用环境
APP_ENV=local

# 数据库配置
DATABASE_URL=mysql://root:password@127.0.0.1:3306/rust_quant

# Redis 配置
REDIS_URL=redis://127.0.0.1:6379

# 功能开关
IS_RUN_SYNC_DATA_JOB=true     # 数据同步
IS_BACK_TEST=true   # 回测
IS_OPEN_SOCKET=true # WebSocket 实盘启动
IS_RUN_REAL_STRATEGY=false  # 实盘策略 实盘启动
```

### 🎛️ 随机调参流程（后续启用）

如果要启动 Vegas 的随机批量调参，请遵循：

1. 保持 `.env` 中 `ENABLE_RANDOM_TEST=false`、`ENABLE_RANDOM_TEST_VEGAS=false`、`ENABLE_SPECIFIED_TEST_VEGAS=true`，先用当前 `back_test_log` 的基线配置（比如 `id=5039`）跑一次回测并确认基础指标。
2. 修改 `strategy_config` 中尚未明确的信号（如 `leg_detection_signal`/`market_structure_signal`），先手工打开 `is_open=true` 并调节阈值。用 `skills/vegas-backtest-analysis/scripts/analyze_backtest_detail.py` 与 `visualize_backtest_detail.py` 验证生成的 signal 分布与持仓行为。
3. 若要批量测试这些新信号，先把 `.env` 改为 `ENABLE_RANDOM_TEST=true`、`ENABLE_RANDOM_TEST_VEGAS=true`、`ENABLE_SPECIFIED_TEST_VEGAS=false`，随机任务会避开写 `back_test_detail`（依赖 `.env` 的 `ENABLE_RANDOM_TEST` 逻辑）。
4. 找到更优结果后，再恢复 `.env`：关闭随机选项、开启 `ENABLE_SPECIFIED_TEST_VEGAS=true`，用新的参数重跑指定回测，以便生成 `back_test_detail` 供最终分析对比基线。

详见: [启动指南](docs/STARTUP_GUIDE.md)

### 使用示例

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

## 🏗️ 架构

### Workspace 结构 (14 个 crate 包)

```
crates/
├── rust-quant-cli/  # 程序入口 (CLI)
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
├── ai-analysis/     # AI分析层
└── common/          # 通用工具层
```

### DDD 分层依赖

```
rust-quant-cli (CLI入口)
    ↓
orchestration (任务编排)
    ↓
services (应用服务层)
    ↓
domain (领域层) + infrastructure (基础设施层)
    ↓
market/indicators (数据层)
    ↓
core/common (核心基础设施)
```

详见: [架构设计文档](docs/quant_system_architecture_redesign.md)

## 📈 Vegas 策略流程图

这组图只描述 **Vegas 策略本身** 的信号判断、后置过滤以及最终交易结果，不包含前置的数据准备、K 线加载、回测任务编排等流程。

### 总览预览

[![Vegas 策略流程总览](uml/image/vegas_signal_to_trade_detailed.png)](uml/image/vegas_signal_to_trade_detailed.png)

### 图纸目录

1. **总览导航图**
   [PNG](uml/image/vegas_signal_to_trade_detailed.png) / [PlantUML](uml/vegas_signal_to_trade_detailed.puml)
2. **方向判断详图**
   [PNG](uml/image/vegas_signal_direction_detailed.png) / [PlantUML](uml/vegas_signal_direction_detailed.puml)
3. **后置过滤详图**
   [PNG](uml/image/vegas_post_filters_detailed.png) / [PlantUML](uml/vegas_post_filters_detailed.puml)
4. **交易结果详图**
   [PNG](uml/image/vegas_trade_outcomes_detailed.png) / [PlantUML](uml/vegas_trade_outcomes_detailed.puml)

### 覆盖范围

- **方向判断**：展示指标计算后的方向候选、权重判断、Fib 大趋势覆盖、实验型方向覆盖，以及初步止损/止盈来源。
- **后置过滤**：细化 `get_trade_signal()` 内的后置过滤逻辑，包括 `Fib` 严格趋势过滤、`EMA/Fib` 区间过滤、结构突破质量过滤、追涨追跌确认、Extreme K、Range Filter、MACD Falling Knife，以及多空专属过滤。
- **交易结果**：展示最终 `SignalResult` 如何映射到直接开仓、等待更优价格、持仓中更新止损止盈、反向信号平仓/反手，以及风控平仓结果。

### 图片导出

如需重新生成图片，可执行：

```bash
plantuml -tpng -o image uml/vegas_signal_to_trade_detailed.puml
plantuml -tpng -o image uml/vegas_signal_direction_detailed.puml
plantuml -tpng -o image uml/vegas_post_filters_detailed.puml
plantuml -tpng -o image uml/vegas_trade_outcomes_detailed.puml
```

## 📚 文档

- **启动指南**: [docs/STARTUP_GUIDE.md](docs/STARTUP_GUIDE.md) - 详细的启动和配置说明
- **架构设计**: [docs/quant_system_architecture_redesign.md](docs/quant_system_architecture_redesign.md) - 完整的架构设计文档
- **Vegas 流程图总览**: [uml/image/vegas_signal_to_trade_detailed.png](uml/image/vegas_signal_to_trade_detailed.png) - 策略视角的导航图
- **Vegas 方向判断详图**: [uml/image/vegas_signal_direction_detailed.png](uml/image/vegas_signal_direction_detailed.png) - 指标计算到方向结论
- **Vegas 后置过滤详图**: [uml/image/vegas_post_filters_detailed.png](uml/image/vegas_post_filters_detailed.png) - 过滤条件与拦截原因
- **Vegas 交易结果详图**: [uml/image/vegas_trade_outcomes_detailed.png](uml/image/vegas_trade_outcomes_detailed.png) - 最终信号到开平仓结果

---

## 🤝 贡献

欢迎贡献！请遵循：

- DDD 架构规范
- Rust 最佳实践
- 完整的测试覆盖

---

## 📜 许可证

MIT License

---

## 🎉 致谢

感谢所有为 Rust Quant DDD 架构重构做出贡献的开发者！

**状态**: ✅ 持续开发中  
**更新**: 2026-01-16
