# Workspace 迁移实施计划

## 📋 总体规划

### 时间表
- **总耗时**: 5-6 周
- **团队**: 1-2 人
- **风险等级**: 🟡 中等（需谨慎执行）

---

## 🚀 迁移阶段

### **阶段 0: 准备工作（1-2 天）** ✅

**目标**: 创建 Workspace 骨架结构

**执行步骤**:
```bash
# 1. 创建重构分支
git checkout -b refactor/workspace-migration

# 2. 运行自动化脚本
chmod +x scripts/workspace_migration_setup.sh
./scripts/workspace_migration_setup.sh

# 3. 验证骨架编译
cargo check --workspace

# 4. 提交骨架结构
git add .
git commit -m "feat: 创建 Workspace 骨架结构"
```

**验收标准**:
- ✅ Workspace 骨架创建成功
- ✅ 所有包的 Cargo.toml 配置正确
- ✅ 基础 lib.rs 编译通过

---

### **阶段 1: 迁移公共模块（1 周）** 🎯

**目标**: 迁移 common 和 core 包

#### **Day 1-2: 迁移 common 包**

**迁移内容**:
```bash
src/trading/types.rs → crates/common/src/types/
src/trading/utils/ → crates/common/src/utils/
src/time_util.rs → crates/common/src/utils/time.rs
src/trading/constants/ → crates/common/src/constants/
src/enums/ → crates/common/src/types/enums/
src/error/ → crates/common/src/errors/
```

**执行脚本**:
```bash
./scripts/migrate_phase1_common_core.sh
```

**手动调整**:
1. 修复导入路径（从 `crate::` 到 `rust_quant_common::`）
2. 更新 mod.rs 导出
3. 补充文档注释

**验收标准**:
```bash
# 编译通过
cargo check --package rust-quant-common

# 测试通过（如有）
cargo test --package rust-quant-common

# Clippy 无警告
cargo clippy --package rust-quant-common -- -D warnings
```

---

#### **Day 3-5: 迁移 core 包**

**迁移内容**:
```bash
src/app_config/ → crates/core/src/
  ├── db.rs → database/connection_pool.rs
  ├── redis_config.rs → cache/redis_client.rs
  ├── log.rs → logger/setup.rs
  ├── env.rs → config/environment.rs
  └── shutdown_manager.rs → config/shutdown_manager.rs
```

**重点注意**:
- 数据库连接池的全局状态管理
- Redis 客户端的初始化逻辑
- 日志系统的配置

**验收标准**:
```bash
cargo check --package rust-quant-core
cargo test --package rust-quant-core
```

**提交代码**:
```bash
git add crates/common crates/core
git commit -m "feat: 迁移 common 和 core 包"
```

---

### **阶段 2: 迁移市场数据层（1 周）** 🎯

**目标**: 迁移 market 包

#### **Day 1-3: 迁移数据模型**

**迁移内容**:
```bash
src/trading/model/market/ → crates/market/src/models/
  ├── candles.rs → models/candle.rs
  ├── tickers.rs → models/ticker.rs
  └── tickers_volume.rs → models/ticker_volume.rs

src/trading/model/entity/candles/ → crates/market/src/models/entity/
```

**新增接口定义**:
```rust
// crates/market/src/exchanges/mod.rs
#[async_trait]
pub trait Exchange: Send + Sync {
    async fn get_candles(&self, inst_id: &str, period: &str) -> Result<Vec<Candle>>;
    async fn get_ticker(&self, inst_id: &str) -> Result<Ticker>;
}
```

---

#### **Day 4-5: 迁移 WebSocket 和数据流**

**迁移内容**:
```bash
src/socket/ → crates/market/src/streams/
  └── websocket_service.rs → streams/websocket_stream.rs
```

**重构要点**:
- 抽象 WebSocket 数据流接口
- 使用 tokio::sync::mpsc 通道
- 实现背压控制

---

#### **Day 6-7: 迁移数据持久化**

**迁移内容**:
```bash
src/trading/services/candle_service/ → crates/market/src/repositories/
  ├── candle_service.rs → repositories/candle_repository.rs
  └── persist_worker.rs → repositories/persist_worker.rs
```

**验收标准**:
```bash
cargo check --package rust-quant-market
cargo test --package rust-quant-market
```

**提交代码**:
```bash
git add crates/market
git commit -m "feat: 迁移 market 包"
```

---

### **阶段 3: 迁移指标和策略层（2 周）** 🎯

**目标**: 迁移 indicators 和 strategies 包

#### **Week 1: 迁移 indicators 包**

**Day 1-2: 迁移趋势指标**
```bash
src/trading/indicator/ → crates/indicators/src/
  ├── ema_indicator.rs → trend/ema.rs
  ├── sma.rs → trend/sma.rs
  └── super_trend.rs → trend/super_trend.rs
```

**统一接口**:
```rust
// crates/indicators/src/lib.rs
pub trait Indicator {
    type Input;
    type Output;
    
    fn update(&mut self, input: Self::Input) -> Self::Output;
    fn reset(&mut self);
}
```

**Day 3-4: 迁移动量和波动性指标**
```bash
# 动量指标
src/trading/indicator/rsi_rma_indicator.rs → momentum/rsi.rs
src/trading/indicator/macd_simple_indicator.rs → momentum/macd.rs
src/trading/indicator/kdj_simple_indicator.rs → momentum/kdj.rs

# 波动性指标
src/trading/indicator/atr.rs → volatility/atr.rs
src/trading/indicator/bollings.rs → volatility/bollinger.rs
```

**Day 5: 测试和验证**
```bash
cargo test --package rust-quant-indicators -- --nocapture
```

---

#### **Week 2: 迁移 strategies 包**

**Day 1-2: 迁移策略框架**
```bash
src/trading/strategy/ → crates/strategies/src/
  ├── strategy_trait.rs → framework/strategy_trait.rs
  ├── strategy_registry.rs → framework/strategy_registry.rs
  ├── executor_common.rs → framework/executor_common.rs
  └── strategy_common.rs → framework/strategy_common.rs
```

**Day 3-5: 迁移具体策略**
```bash
src/trading/strategy/ → crates/strategies/src/implementations/
  ├── vegas_executor.rs → vegas/executor.rs
  ├── nwe_executor.rs → nwe/executor.rs
  ├── ut_boot_strategy.rs → ut_boot/mod.rs
  ├── engulfing_strategy.rs → engulfing/mod.rs
  └── squeeze_strategy.rs → squeeze/mod.rs
```

**重要**: 同时迁移策略的指标缓存
```bash
src/trading/strategy/arc/ → crates/strategies/src/implementations/*/cache/
```

**验收标准**:
```bash
cargo check --package rust-quant-strategies
cargo test --package rust-quant-strategies
```

**提交代码**:
```bash
git add crates/indicators crates/strategies
git commit -m "feat: 迁移 indicators 和 strategies 包"
```

---

### **阶段 4: 迁移执行和编排层（1 周）** 🎯

**目标**: 迁移 risk, execution, orchestration 包

#### **Day 1-2: 迁移 risk 包**

**提取风控逻辑**:
```bash
src/job/ → crates/risk/src/
  ├── risk_order_job.rs → order/order_validator.rs
  ├── risk_positon_job.rs → position/position_limiter.rs
  └── risk_banlance_job.rs → account/balance_monitor.rs
```

---

#### **Day 3-4: 迁移 execution 包**

**迁移订单执行**:
```bash
src/trading/services/order_service/ → crates/execution/src/
  └── swap_order_service.rs → execution_engine/market_order.rs

src/trading/services/position_service/ → crates/execution/src/
  └── position_service.rs → position_manager/position_tracker.rs
```

---

#### **Day 5-7: 迁移 orchestration 包**

**迁移任务调度和编排**:
```bash
src/job/ → crates/orchestration/src/scheduler/jobs/
  ├── announcements_job.rs → jobs/announcement_job.rs
  └── task_scheduler.rs → scheduler.rs

src/trading/task/ → crates/orchestration/src/
  └── strategy_runner.rs → strategy_runner/real_time_runner.rs
```

**验收标准**:
```bash
cargo check --package rust-quant-risk
cargo check --package rust-quant-execution
cargo check --package rust-quant-orchestration
```

**提交代码**:
```bash
git add crates/risk crates/execution crates/orchestration
git commit -m "feat: 迁移 risk, execution, orchestration 包"
```

---

### **阶段 5: 迁移主程序和测试（1 周）** 🎯

**目标**: 迁移 CLI 主程序和所有测试

#### **Day 1-3: 迁移主程序**

**迁移启动逻辑**:
```bash
src/main.rs → rust-quant-cli/src/main.rs
src/app/bootstrap.rs → rust-quant-cli/src/bootstrap.rs
src/lib.rs → 重构为各包的集成层
```

**更新导入路径**:
```rust
// 旧导入
use crate::trading::strategy::Strategy;

// 新导入
use rust_quant_strategies::Strategy;
```

---

#### **Day 4-7: 迁移测试**

**测试迁移策略**:
```bash
tests/ → 分散到各个包的 tests/ 目录

# 单元测试
crates/indicators/tests/test_ema.rs
crates/strategies/tests/test_vegas_strategy.rs

# 集成测试（保留在根 tests/）
tests/integration/
  ├── test_full_workflow.rs
  └── test_strategy_execution.rs
```

**验收标准**:
```bash
# 完整编译
cargo build --workspace --release

# 所有测试通过
cargo test --workspace

# Clippy 无警告
cargo clippy --workspace -- -D warnings

# 格式检查
cargo fmt --all -- --check
```

**提交代码**:
```bash
git add rust-quant-cli tests
git commit -m "feat: 迁移主程序和测试"
```

---

### **阶段 6: 清理和优化（1 周）** 🎯

**目标**: 清理旧代码，优化性能，完善文档

#### **Day 1-2: 清理旧代码**

```bash
# 移除旧目录（或移到 deprecated/）
mkdir deprecated
mv src/trading deprecated/
mv src/app_config deprecated/
mv src/socket deprecated/
# ... 其他旧代码
```

---

#### **Day 3-4: 性能优化**

**性能基准测试**:
```bash
# 添加 benchmark
crates/indicators/benches/indicator_bench.rs
crates/strategies/benches/strategy_bench.rs

# 运行 benchmark
cargo bench --workspace
```

**优化目标**:
- 指标计算延迟 < 5ms
- 策略信号生成 < 10ms
- 内存占用减少 30%

---

#### **Day 5-7: 完善文档**

**文档清单**:
- [ ] README.md 更新
- [ ] 各包的 README.md
- [ ] API 文档（cargo doc）
- [ ] 架构设计文档
- [ ] 迁移总结报告

**生成文档**:
```bash
cargo doc --workspace --no-deps --open
```

---

## 🎯 验收标准

### **功能验收**
- [ ] 所有包编译通过
- [ ] 所有测试通过（100%）
- [ ] 核心功能正常运行（实盘测试）

### **性能验收**
- [ ] 指标计算性能不低于迁移前
- [ ] 策略执行延迟 < 50ms
- [ ] 内存占用无明显增加

### **代码质量验收**
- [ ] Clippy 无警告
- [ ] 代码覆盖率 > 70%
- [ ] 文档覆盖率 > 80%

### **架构验收**
- [ ] 包依赖关系清晰（单向依赖）
- [ ] 无循环依赖
- [ ] 接口设计合理

---

## ⚠️ 风险管理

### **高风险项**

| 风险项 | 概率 | 影响 | 缓解措施 |
|-------|------|------|---------|
| 策略逻辑回归 | 🟡 中 | 🔴 高 | 1. 补充单元测试<br>2. 对比迁移前后指标值<br>3. 小范围实盘验证 |
| 性能回退 | 🟢 低 | 🟡 中 | 1. 性能基准测试<br>2. 逐步迁移验证 |
| 依赖关系混乱 | 🟡 中 | 🟡 中 | 1. 使用 cargo tree 检查<br>2. 严格遵守分层原则 |

### **回滚策略**

```bash
# 如果迁移失败，回退到主分支
git checkout main

# 保留迁移分支以备后续调整
git branch -D refactor/workspace-migration  # 删除失败的分支
```

---

## 📊 进度追踪

### **每日站会**
- 昨天完成了什么？
- 今天计划做什么？
- 遇到什么阻碍？

### **每周回顾**
- 本周完成的包
- 编译和测试情况
- 性能对比数据
- 下周计划

### **里程碑**
- [ ] Week 1: common + core 迁移完成
- [ ] Week 2: market 迁移完成
- [ ] Week 3-4: indicators + strategies 迁移完成
- [ ] Week 5: risk + execution + orchestration 迁移完成
- [ ] Week 6: 主程序迁移 + 清理优化

---

## 🎉 迁移完成后的收益

### **短期收益（1-2个月）**
- ✅ 编译时间减少 **60%**（增量编译）
- ✅ 测试运行时间减少 **50%**（包级别测试）
- ✅ 代码职责清晰，维护成本降低 **40%**

### **中期收益（3-6个月）**
- ✅ 新增策略开发时间减少 **70%**
- ✅ Bug 修复时间减少 **50%**
- ✅ 新人上手时间减少 **60%**

### **长期收益（6个月+）**
- ✅ 支持多交易所扩展（统一接口）
- ✅ 支持微服务拆分（清晰的包边界）
- ✅ 支持团队并行开发（包级别隔离）

---

**祝迁移顺利！如有问题，请参考迁移指南或联系团队成员。** 🚀

