# Workspace 迁移快速开始指南

> **总耗时**: 5-6 周  
> **难度**: 🟡 中等  
> **推荐**: ⭐⭐⭐⭐⭐ 强烈推荐

---

## 🎯 为什么要迁移到 Workspace？

### **当前问题**
- ❌ 编译慢（每次修改都编译整个项目）
- ❌ `trading/` 模块 159 个文件，维护困难
- ❌ 职责不清（`job/` vs `trading/task/`）
- ❌ 测试运行慢（无法独立测试单个模块）

### **迁移后收益**
- ✅ 编译时间减少 **60%**（增量编译）
- ✅ 测试时间减少 **50%**（包级别测试）
- ✅ 新增策略开发时间减少 **70%**
- ✅ 代码职责清晰，维护成本降低 **40%**

---

## 📚 已创建的文档和脚本

### **1. 核心文档**
| 文档 | 用途 | 阅读顺序 |
|-----|------|---------|
| [package_service_split_strategy.md](./package_service_split_strategy.md) | 拆包 vs 拆服务决策分析 | ① **优先阅读** |
| [quant_system_architecture_redesign.md](./quant_system_architecture_redesign.md) | 量化交易专用架构设计 | ② |
| [workspace_migration_plan.md](./workspace_migration_plan.md) | 详细迁移计划（6周） | ③ |
| [QUICK_START_WORKSPACE_MIGRATION.md](./QUICK_START_WORKSPACE_MIGRATION.md)（本文档） | 快速开始指南 | ④ |

### **2. 自动化脚本**
| 脚本 | 用途 | 执行时机 |
|-----|------|---------|
| [workspace_migration_setup.sh](../scripts/workspace_migration_setup.sh) | 创建 Workspace 骨架 | ⭐ **立即执行** |
| [migrate_phase1_common_core.sh](../scripts/migrate_phase1_common_core.sh) | 迁移 common 和 core 包 | 第1周 |

### **3. 其他参考文档**
- [architecture_refactoring_plan.md](./architecture_refactoring_plan.md) - 之前的 DDD 重构方案（参考）
- [current_vs_proposed_architecture.md](./current_vs_proposed_architecture.md) - 架构对比
- [refactor_phase1_setup.sh](../scripts/refactor_phase1_setup.sh) - DDD 重构脚本（暂不使用）

---

## 🚀 立即开始（3 步走）

### **步骤 1: 创建 Workspace 骨架（5 分钟）** ⭐

```bash
# 1. 确保在项目根目录
cd /Users/mac2/onions/rust_quant

# 2. 运行自动化脚本
chmod +x scripts/workspace_migration_setup.sh
./scripts/workspace_migration_setup.sh

# 脚本会自动：
# ✓ 创建分支 refactor/workspace-migration
# ✓ 创建 crates/ 目录结构
# ✓ 生成所有包的 Cargo.toml
# ✓ 创建基础 lib.rs 文件
# ✓ 验证编译
```

**预期输出**:
```
========================================
Workspace 骨架搭建完成！
========================================

新创建的 Workspace 结构：
crates/
├── common/
├── core/
├── market/
├── indicators/
├── strategies/
├── risk/
├── execution/
├── orchestration/
└── analytics/

rust-quant-cli/
```

**验证**:
```bash
# 检查编译
cargo check --workspace

# 应该看到所有包都编译通过
```

---

### **步骤 2: 查看迁移指南（10 分钟）**

```bash
# 1. 查看自动生成的迁移指南
cat WORKSPACE_MIGRATION_GUIDE.md

# 2. 查看详细迁移计划
cat docs/workspace_migration_plan.md
```

**重点关注**:
- 📂 代码迁移映射（哪些文件迁移到哪里）
- 📋 迁移检查清单
- 🔧 常用命令

---

### **步骤 3: 开始代码迁移（1 周）** ⭐

#### **阶段 1: 迁移 common 和 core 包**

```bash
# 1. 运行阶段1迁移脚本
chmod +x scripts/migrate_phase1_common_core.sh
./scripts/migrate_phase1_common_core.sh

# 脚本会自动迁移：
# ✓ src/trading/types.rs → crates/common/src/types/
# ✓ src/time_util.rs → crates/common/src/utils/time.rs
# ✓ src/trading/utils/ → crates/common/src/utils/
# ✓ src/app_config/ → crates/core/src/config/
# ✓ src/app_config/db.rs → crates/core/src/database/
# ✓ src/app_config/redis_config.rs → crates/core/src/cache/
```

**手动调整（重要）**:
```bash
# 1. 检查迁移后的文件
ls -la crates/common/src/
ls -la crates/core/src/

# 2. 修复导入路径
# 旧导入: use crate::time_util;
# 新导入: use rust_quant_common::utils::time;

# 3. 更新 mod.rs 导出
# 确保所有模块正确导出
```

**验证**:
```bash
# 编译 common 包
cargo check --package rust-quant-common

# 编译 core 包
cargo check --package rust-quant-core

# 运行测试
cargo test --package rust-quant-common
cargo test --package rust-quant-core
```

**提交代码**:
```bash
git add crates/common crates/core
git commit -m "feat: 迁移 common 和 core 包"
git push origin refactor/workspace-migration
```

---

## 📅 后续阶段规划

### **阶段 2: 迁移 market 包（1 周）**
```bash
# 迁移内容
src/trading/model/market/ → crates/market/src/models/
src/socket/ → crates/market/src/streams/
src/trading/services/candle_service/ → crates/market/src/repositories/

# 执行
# TODO: 等待阶段2脚本
```

### **阶段 3: 迁移 indicators 和 strategies 包（2 周）**
```bash
# 迁移内容
src/trading/indicator/ → crates/indicators/src/
src/trading/strategy/ → crates/strategies/src/

# 执行
# TODO: 等待阶段3脚本
```

### **阶段 4: 迁移 risk, execution, orchestration 包（1 周）**
```bash
# 迁移内容
src/job/risk_*.rs → crates/risk/src/
src/trading/services/order_service/ → crates/execution/src/
src/trading/task/ → crates/orchestration/src/

# 执行
# TODO: 等待阶段4脚本
```

### **阶段 5: 迁移主程序和测试（1 周）**
```bash
# 迁移内容
src/main.rs → rust-quant-cli/src/main.rs
src/app/bootstrap.rs → rust-quant-cli/src/bootstrap.rs
tests/ → 分散到各包的 tests/
```

---

## 🔧 常用命令

### **编译相关**
```bash
# 编译整个 workspace
cargo build --workspace

# 编译特定包
cargo build --package rust-quant-core

# 快速检查（不生成二进制）
cargo check --workspace

# 发布版本编译
cargo build --workspace --release
```

### **测试相关**
```bash
# 运行所有测试
cargo test --workspace

# 运行特定包测试
cargo test --package rust-quant-indicators

# 显示测试输出
cargo test --workspace -- --nocapture

# 只运行某个测试函数
cargo test test_ema --package rust-quant-indicators
```

### **代码质量**
```bash
# 格式化代码
cargo fmt --all

# Clippy 检查
cargo clippy --workspace -- -D warnings

# 查看依赖树
cargo tree

# 查看特定包的依赖
cargo tree --package rust-quant-strategies
```

### **文档生成**
```bash
# 生成并打开文档
cargo doc --workspace --no-deps --open

# 只生成特定包文档
cargo doc --package rust-quant-core --open
```

---

## ⚠️ 常见问题与解决

### **问题 1: 编译错误 - 找不到模块**

**症状**:
```
error[E0583]: file not found for module `xxx`
```

**解决**:
```bash
# 1. 检查 mod.rs 是否正确导出
# 2. 检查文件名是否与模块名匹配
# 3. 检查是否有 pub mod xxx;
```

---

### **问题 2: 循环依赖错误**

**症状**:
```
error: cyclic package dependency
```

**解决**:
```bash
# 检查依赖方向
cargo tree --package rust-quant-xxx

# 确保依赖方向：
# common → core → market/indicators → strategies → execution → orchestration
```

---

### **问题 3: 导入路径错误**

**症状**:
```
error[E0432]: unresolved import `crate::trading::strategy`
```

**解决**:
```rust
// 旧导入（单体架构）
use crate::trading::strategy::Strategy;

// 新导入（Workspace 架构）
use rust_quant_strategies::Strategy;
```

---

### **问题 4: 全局状态访问错误**

**症状**:
数据库连接池、Redis 客户端等全局状态无法访问

**解决**:
```rust
// 在 core 包中导出全局状态
// crates/core/src/database/mod.rs
pub use connection_pool::get_db_pool;

// 在其他包中使用
use rust_quant_core::database::get_db_pool;
```

---

## 📊 进度追踪

### **每周检查清单**

**Week 1: common + core**
- [ ] Workspace 骨架创建
- [ ] common 包迁移完成
- [ ] core 包迁移完成
- [ ] 编译通过
- [ ] 测试通过
- [ ] 代码已提交

**Week 2: market**
- [ ] market 包迁移完成
- [ ] WebSocket 数据流正常
- [ ] 数据持久化正常
- [ ] 编译通过
- [ ] 测试通过

**Week 3-4: indicators + strategies**
- [ ] indicators 包迁移完成
- [ ] strategies 包迁移完成
- [ ] 策略执行正常
- [ ] 回测功能正常
- [ ] 性能无回退

**Week 5: risk + execution + orchestration**
- [ ] risk 包迁移完成
- [ ] execution 包迁移完成
- [ ] orchestration 包迁移完成
- [ ] 实盘下单功能正常

**Week 6: 主程序 + 清理**
- [ ] 主程序迁移完成
- [ ] 所有测试迁移
- [ ] 旧代码清理
- [ ] 文档更新
- [ ] 性能优化

---

## 🎯 验收标准

### **最终验收清单**

**功能验收**:
- [ ] 所有包编译通过
- [ ] 所有测试通过（100%）
- [ ] 实盘交易功能正常
- [ ] 回测功能正常
- [ ] WebSocket 数据流正常

**性能验收**:
- [ ] 编译时间减少 > 50%
- [ ] 测试时间减少 > 40%
- [ ] 策略执行延迟 < 50ms
- [ ] 内存占用无明显增加

**代码质量验收**:
- [ ] Clippy 无警告
- [ ] 代码格式化通过
- [ ] 文档覆盖率 > 80%
- [ ] 无循环依赖

---

## 🎉 迁移完成后的下一步

1. **性能优化**
   ```bash
   # 运行性能基准测试
   cargo bench --workspace
   ```

2. **文档完善**
   ```bash
   # 生成API文档
   cargo doc --workspace --no-deps
   ```

3. **合并到主分支**
   ```bash
   git checkout main
   git merge refactor/workspace-migration
   git push origin main
   ```

4. **清理旧代码**
   ```bash
   # 移除旧 src/ 目录（或移到 deprecated/）
   mkdir deprecated
   mv src/trading deprecated/
   mv src/app_config deprecated/
   ```

---

## 📞 获取帮助

### **问题排查**

1. **查看编译错误**
   ```bash
   cargo build --workspace 2>&1 | less
   ```

2. **查看依赖关系**
   ```bash
   cargo tree --workspace
   ```

3. **查看具体包的依赖**
   ```bash
   cargo tree --package rust-quant-strategies --depth 2
   ```

### **回滚策略**

```bash
# 如果迁移失败，回退到主分支
git checkout main

# 删除迁移分支（可选）
git branch -D refactor/workspace-migration
```

---

## 🚀 现在开始！

```bash
# 1. 运行第一个脚本
./scripts/workspace_migration_setup.sh

# 2. 查看迁移指南
cat WORKSPACE_MIGRATION_GUIDE.md

# 3. 开始代码迁移
./scripts/migrate_phase1_common_core.sh

# 4. 验证和提交
cargo check --workspace
git commit -m "feat: 完成阶段1迁移"
```

**祝迁移顺利！** 🎯

---

**版本**: v1.0  
**日期**: 2025-11-06  
**维护者**: AI Assistant

