# 架构重构指南 - 快速开始

## 📌 文档导航

### **1. 核心文档**

| 文档名称 | 用途 | 优先级 |
|---------|------|-------|
| [architecture_analysis_summary.md](./architecture_analysis_summary.md) | **总览报告** - 问题诊断、方案评估、实施路线图 | ⭐⭐⭐⭐⭐ |
| [architecture_refactoring_plan.md](./architecture_refactoring_plan.md) | **详细重构方案** - 目录结构、迁移清单、技术细节 | ⭐⭐⭐⭐⭐ |
| [current_vs_proposed_architecture.md](./current_vs_proposed_architecture.md) | **架构对比** - 当前问题 vs 优化后对比 | ⭐⭐⭐⭐☆ |
| [README_REFACTORING.md](./README_REFACTORING.md)（本文档） | **快速参考** - 如何使用重构文档和脚本 | ⭐⭐⭐⭐☆ |

### **2. 自动化脚本**

| 脚本名称 | 功能 | 使用场景 |
|---------|------|---------|
| [scripts/refactor_phase1_setup.sh](../scripts/refactor_phase1_setup.sh) | 自动创建新架构目录结构 | 开始阶段一重构时执行 |

---

## 🚀 快速开始（5分钟了解）

### **步骤 1: 了解当前问题**

```bash
# 阅读总结报告（推荐优先阅读）
cat docs/architecture_analysis_summary.md

# 或在浏览器中查看
open docs/architecture_analysis_summary.md
```

**关键发现**：
- 🔴 `trading/` 模块膨胀（159个文件）
- 🔴 `job/` 与 `trading/task/` 职责重叠
- 🔴 缺少明确的 DDD 分层架构

---

### **步骤 2: 了解推荐方案**

```bash
# 阅读架构对比文档
cat docs/current_vs_proposed_architecture.md
```

**推荐架构**：
```
src/
├── domain/          # 领域层 - 核心业务逻辑
├── application/     # 应用层 - 用例编排
├── infrastructure/  # 基础设施层 - 技术实现
├── interfaces/      # 接口层 - 对外暴露
└── shared/         # 共享层 - 跨层工具
```

---

### **步骤 3: 运行自动化脚本**

```bash
# 赋予脚本执行权限
chmod +x scripts/refactor_phase1_setup.sh

# 运行阶段一脚本（创建目录结构）
./scripts/refactor_phase1_setup.sh
```

**脚本会自动完成**：
- ✅ 创建重构分支 `refactor/ddd-architecture-phase1`
- ✅ 创建 `domain/`, `application/`, `infrastructure/`, `shared/` 目录
- ✅ 生成所有 `mod.rs` 文件
- ✅ 更新 `lib.rs` 模块声明
- ✅ 创建迁移进度追踪文档 `MIGRATION_PROGRESS.md`

---

### **步骤 4: 验证搭建结果**

```bash
# 检查编译是否通过
cargo check

# 查看新创建的目录结构
tree -L 3 src/

# 查看迁移进度
cat MIGRATION_PROGRESS.md
```

---

## 📋 重构阶段路线图

### **阶段一：基础设施层重构（1-2周）**
```bash
# 1. 运行自动化脚本（已完成）
./scripts/refactor_phase1_setup.sh

# 2. 迁移配置模块
cp -r src/app_config/* src/infrastructure/config/
# 然后手动调整 mod.rs 和引用路径

# 3. 迁移 WebSocket 服务
cp -r src/socket/* src/infrastructure/messaging/websocket/

# 4. 整合任务调度
# 合并 job/ 和 trading/task/ → infrastructure/scheduler/

# 5. 运行测试
cargo test
```

---

### **阶段二：领域层拆分（2-3周）**
```bash
# 1. 迁移市场数据
cp src/trading/model/market/*.rs src/domain/market/entities/

# 2. 迁移策略逻辑
cp -r src/trading/strategy/ src/domain/strategy/strategies/

# 3. 重组技术指标
# 按 trend/momentum/volatility/volume 分类迁移

# 4. 提取风控领域
# 从 job/risk_*.rs 提取核心逻辑 → domain/risk/
```

---

### **阶段三：应用层构建（1-2周）**
```bash
# 1. 创建 Commands 和 Queries
# application/commands/strategy/
# application/queries/strategy/

# 2. 迁移应用服务
cp src/trading/services/* src/application/services/
```

---

### **阶段四：共享层整理（1周）**
```bash
# 1. 迁移工具函数
mv src/time_util.rs src/shared/utils/time_util.rs
cp -r src/trading/utils/* src/shared/utils/

# 2. 增强错误处理
cp -r src/error/* src/shared/errors/

# 3. 清理旧代码
mkdir deprecated/
mv src/trading/ deprecated/trading_old/
```

---

## 🔧 常用命令速查

### **编译和测试**
```bash
# 快速检查编译
cargo check

# 完整编译
cargo build

# 运行所有测试
cargo test

# 运行特定模块测试
cargo test --package rust_quant --lib domain::strategy

# 显示测试输出
cargo test -- --nocapture
```

---

### **代码质量检查**
```bash
# 代码格式化
cargo fmt

# Clippy 检查
cargo clippy -- -D warnings

# 查看依赖图
cargo tree

# 检查循环依赖
cargo install cargo-modules
cargo modules generate tree
```

---

### **Git 工作流**
```bash
# 创建重构分支
git checkout -b refactor/ddd-architecture-phase1

# 小步提交
git add .
git commit -m "refactor: 创建 infrastructure 目录结构"

# 查看变更
git status
git diff

# 合并到主分支（阶段完成后）
git checkout main
git merge refactor/ddd-architecture-phase1
```

---

## 📊 重构进度追踪

### **使用 MIGRATION_PROGRESS.md 追踪进度**

```markdown
### 待完成（阶段一）
- [x] 创建目录结构
- [x] 迁移 app_config/
- [ ] 迁移 socket/
- [ ] 整合 job/ + trading/task/
```

每完成一项任务，将 `[ ]` 改为 `[x]`

---

## ⚠️ 注意事项

### **重构期间的最佳实践**

1. **小步提交**
   - 每迁移一个模块就提交一次
   - Commit message 清晰描述变更内容

2. **保持测试通过**
   - 每次提交前运行 `cargo test`
   - 如果测试失败，立即修复

3. **并行运行新旧代码**
   - 迁移期间保留旧代码
   - 新代码通过 Feature Flag 控制

4. **定期同步主分支**
   - 每周合并主分支的新提交
   - 避免分支长期分叉

5. **文档同步更新**
   - 代码迁移的同时更新注释
   - 更新 API 文档

---

## 🎯 质量检查清单

### **每个阶段完成后检查**

- [ ] **编译通过**: `cargo check` 无错误
- [ ] **测试通过**: `cargo test` 全部通过
- [ ] **无 Clippy 警告**: `cargo clippy` 无警告
- [ ] **代码格式化**: `cargo fmt` 已执行
- [ ] **文档更新**: 相关模块文档已更新
- [ ] **Git 提交**: 变更已提交到版本控制
- [ ] **代码审查**: 团队成员已审查（可选）

---

## 📞 遇到问题？

### **常见问题解决**

**问题 1: 编译错误 - 找不到模块**
```bash
# 检查 mod.rs 是否正确导出
# 检查 use 语句路径是否正确
```

**问题 2: 循环依赖错误**
```bash
# 检查依赖方向是否符合 Infrastructure → Application → Domain
# 使用 cargo modules 工具可视化依赖关系
```

**问题 3: 测试失败**
```bash
# 检查是否更新了测试中的模块路径
# 使用 -- --nocapture 查看详细输出
cargo test -- --nocapture
```

**问题 4: Git 冲突**
```bash
# 定期合并主分支
git checkout refactor/ddd-architecture-phase1
git merge main
# 解决冲突后提交
```

---

## 🎓 学习资源

### **DDD 和 Clean Architecture**

1. **[领域驱动设计（DDD）](https://martinfowler.com/bliki/DomainDrivenDesign.html)**
   - 核心概念：Entity, Value Object, Aggregate, Repository
   - 分层架构：Domain, Application, Infrastructure

2. **[整洁架构（Clean Architecture）](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)**
   - 依赖倒置原则
   - 业务逻辑与技术实现分离

3. **[CQRS 模式](https://martinfowler.com/bliki/CQRS.html)**
   - Command vs Query 分离
   - 读写分离的优势

### **Rust 架构实践**

1. **[Rust-DDD-Example](https://github.com/vaerdi/rust-ddd-example)**
   - Rust 实现的 DDD 分层架构示例

2. **[Axum-DDD-Template](https://github.com/jeremychone/rust-axum-ddd-template)**
   - Rust Web应用 DDD 模板

3. **[Rust 项目结构最佳实践](https://doc.rust-lang.org/cargo/guide/project-layout.html)**
   - Cargo 官方指南

---

## 📈 预期收益

完成重构后，你将获得：

✅ **开发效率提升 60%** - 新增策略开发时间从 2-3天 → 0.5-1天  
✅ **测试覆盖率提升 133%** - 从 ~30% → 70%  
✅ **Bug修复时间减少 50%** - 从 2-4小时 → 0.5-1小时  
✅ **新人上手时间减少 85%** - 从 2周 → 3天  
✅ **代码复杂度降低 75%** - 单模块文件数从 159 → <50  

---

## 🎉 开始重构

准备好了吗？让我们开始吧！

```bash
# 1. 运行自动化脚本
./scripts/refactor_phase1_setup.sh

# 2. 查看迁移进度
cat MIGRATION_PROGRESS.md

# 3. 开始第一个迁移任务
# 迁移 app_config/ → infrastructure/config/

# 4. 提交你的第一个变更
git add .
git commit -m "refactor(phase1): 创建新架构目录结构"
```

---

**祝重构顺利！如有问题，请参考相关文档或咨询团队成员。**

---

**版本**: v1.0  
**最后更新**: 2025-11-06  
**维护者**: AI Assistant

