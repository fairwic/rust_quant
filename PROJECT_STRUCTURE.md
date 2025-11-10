# 🏗️ 项目结构说明

**更新时间**: 2025-11-10  
**架构版本**: DDD Workspace 架构

---

## ✅ 当前项目结构

```
rust_quant/                    # Workspace 根目录
├── Cargo.toml                 # ⭐ Workspace 配置（无 [package]）
├── .env                       # 环境变量配置
├── docker-compose.yml         # Docker 编排
├── create_table.sql           # 数据库初始化脚本
│
├── crates/                    # 所有代码包
│   ├── common/                # 通用工具
│   ├── core/                  # 核心基础设施
│   ├── domain/                # 领域模型
│   ├── infrastructure/        # 基础设施实现
│   ├── services/              # 应用服务层
│   ├── market/                # 市场数据
│   ├── indicators/            # 技术指标
│   ├── strategies/            # 策略引擎
│   ├── risk/                  # 风险管理
│   ├── execution/             # 订单执行
│   ├── orchestration/         # 任务调度
│   ├── analytics/             # 分析报告
│   ├── ai-analysis/           # AI 分析
│   └── rust-quant-cli/        # ⭐ CLI 可执行程序
│       ├── Cargo.toml         # 包含 [[bin]] 配置
│       └── src/
│           ├── main.rs        # ✅ 程序入口
│           ├── lib.rs         # ✅ CLI 库
│           └── app/
│               └── bootstrap.rs # ✅ 启动逻辑
│
├── docs/                      # 文档
├── examples/                  # 示例代码
├── tests/                     # 集成测试
├── scripts/                   # 脚本工具
└── target/                    # 编译输出
    └── release/
        └── rust-quant         # ✅ 可执行文件（10MB）
```

---

## 🎯 为什么不需要根目录的 `src/`

### Rust Workspace 标准实践

在 Workspace 项目中，有两种组织方式：

#### 方式 1: Virtual Workspace（推荐）✅
```toml
# Cargo.toml
[workspace]
members = ["crates/*"]
# 没有 [package] section
```

**特点**:
- ✅ 根目录没有 `src/`
- ✅ 所有代码在 `crates/` 子包中
- ✅ 清晰的模块边界
- ✅ 独立编译和测试

#### 方式 2: Package with Workspace（不推荐）❌
```toml
# Cargo.toml
[workspace]
members = ["crates/*"]

[package]  # ❌ 根目录也是一个包
name = "rust-quant"
# ...
```

**缺点**:
- ❌ 根目录需要 `src/`
- ❌ 容易产生循环依赖
- ❌ 模块职责不清晰
- ❌ 编译缓存不友好

---

## 📦 可执行程序配置

### `crates/rust-quant-cli/Cargo.toml`

```toml
[package]
name = "rust-quant-cli"
version.workspace = true
edition.workspace = true

# ⭐ 定义二进制可执行文件
[[bin]]
name = "rust-quant"           # 可执行文件名
path = "src/main.rs"          # 入口文件路径

[dependencies]
# 引入所有需要的 workspace 包
rust-quant-common.workspace = true
rust-quant-core.workspace = true
rust-quant-orchestration.workspace = true
# ...
```

### 编译和运行

```bash
# 编译（在项目根目录）
cargo build --release --bin rust-quant

# 运行
./target/release/rust-quant

# 或直接运行
cargo run --release --bin rust-quant
```

---

## 🚀 项目的多入口支持

### 当前入口

**主程序**:
```bash
# 编译
cargo build --release --bin rust-quant

# 运行
./target/release/rust-quant
```

### 可以添加更多入口（如需要）

**例如: 数据导入工具**:
```toml
# crates/rust-quant-cli/Cargo.toml
[[bin]]
name = "data-importer"
path = "src/bin/data_importer.rs"

[[bin]]
name = "backtest-analyzer"
path = "src/bin/backtest_analyzer.rs"
```

**运行**:
```bash
cargo run --bin data-importer
cargo run --bin backtest-analyzer
```

---

## 📊 编译产物说明

### Library 文件（.rlib）

所有 crate 都会编译为 library:
```
librust_quant_common.rlib      (294 KB)
librust_quant_core.rlib        (1.0 MB)
librust_quant_domain.rlib      (1.3 MB)
librust_quant_strategies.rlib  (2.1 MB)
librust_quant_cli.rlib         (648 KB)  ← CLI 的库部分
...
```

### Binary 可执行文件

只有声明了 `[[bin]]` 的包会生成可执行文件:
```
rust-quant                     (10 MB)  ← 最终可执行文件
```

**为什么只有一个可执行文件？**
- `rust-quant-cli` 是唯一配置了 `[[bin]]` 的包
- 其他包都是 library（供 CLI 调用）

---

## 🎯 迁移前后对比

### 迁移前（混乱）❌

```
rust_quant/
├── src/
│   ├── main.rs              # 入口
│   ├── lib.rs               # 根库
│   └── trading/             # 159 个文件混杂
│       ├── indicator/
│       ├── strategy/
│       ├── task/
│       ├── model/
│       └── services/
├── Cargo.toml               # Package + Workspace 混合
└── crates/                  # 部分模块
    └── ...
```

**问题**:
- ❌ 根 package 和 workspace 混合
- ❌ `src/` 和 `crates/` 职责不清
- ❌ 依赖关系混乱
- ❌ 难以维护

### 迁移后（清晰）✅

```
rust_quant/
├── Cargo.toml               # ⭐ 纯 Workspace 配置
├── crates/                  # ⭐ 所有代码
│   ├── common/              # 14 个独立包
│   ├── core/
│   ├── domain/
│   ├── ...
│   └── rust-quant-cli/      # ⭐ 唯一的可执行程序包
│       └── src/
│           └── main.rs      # ✅ 程序入口
└── target/
    └── release/
        └── rust-quant       # ✅ 可执行文件
```

**优势**:
- ✅ 纯 workspace 架构
- ✅ 职责边界清晰
- ✅ 依赖关系单向
- ✅ 易于维护和扩展

---

## 🎯 入口点说明

### 1. 可执行程序入口

**文件**: `crates/rust-quant-cli/src/main.rs`
```rust
#[tokio::main]
async fn main() -> Result<()> {
    rust_quant_cli::app_init().await?;
    rust_quant_cli::run().await
}
```

**作用**: 程序启动入口

### 2. CLI 库入口

**文件**: `crates/rust-quant-cli/src/lib.rs`
```rust
pub async fn app_init() -> Result<()> {
    // 初始化日志、数据库、Redis
}

pub async fn run() -> Result<()> {
    // 运行主程序逻辑
}
```

**作用**: 对外暴露的 API

### 3. 启动逻辑

**文件**: `crates/rust-quant-cli/src/app/bootstrap.rs`
```rust
pub async fn run_modes() -> Result<()> {
    // 根据环境变量运行不同模式
}

pub async fn run() -> Result<()> {
    // 完整的启动流程
}
```

**作用**: 应用启动编排

---

## 📝 常见问题

### Q1: 为什么删除了 `src/`？

**A**: 采用 Virtual Workspace 架构，根目录只做 workspace 配置，不包含代码。

### Q2: 程序入口在哪里？

**A**: `crates/rust-quant-cli/src/main.rs`

### Q3: 如何运行程序？

**A**: 
```bash
# 方式 1: 编译后运行
cargo build --release --bin rust-quant
./target/release/rust-quant

# 方式 2: 直接运行
cargo run --release --bin rust-quant

# 方式 3: 指定包运行
cargo run --release -p rust-quant-cli
```

### Q4: 如何添加新的可执行程序？

**A**: 在 `crates/rust-quant-cli/Cargo.toml` 添加 `[[bin]]`:
```toml
[[bin]]
name = "your-tool"
path = "src/bin/your_tool.rs"
```

### Q5: 旧代码在哪里？

**A**: 已备份到 `src_backup_20251110_140646.tar.gz`（221KB）

---

## 🎊 架构优势

### 1. 清晰的模块边界
- 每个 crate 职责单一
- 依赖关系明确
- 易于理解和维护

### 2. 更好的编译性能
- 增量编译更快
- 并行编译 14 个包
- 改动影响范围小

### 3. 更好的测试支持
- 每个包可以独立测试
- 测试依赖隔离
- 测试覆盖率清晰

### 4. 更好的发布管理
- 可以独立发布库包
- 版本管理更灵活
- 依赖升级更安全

---

## 🔧 开发工作流

### 开发新功能
```bash
# 1. 确定功能属于哪个包
# 例如：新增一个技术指标

# 2. 进入对应包目录
cd crates/indicators

# 3. 编写代码
# src/trend/my_indicator.rs

# 4. 测试
cargo test -p rust-quant-indicators

# 5. 在 CLI 中使用
# crates/rust-quant-cli 会自动引入
```

### 构建整个项目
```bash
# 构建所有包
cargo build --workspace

# 构建 release 版本
cargo build --workspace --release

# 只构建可执行程序
cargo build --release --bin rust-quant
```

### 运行程序
```bash
# 开发模式
cargo run

# Release 模式
cargo run --release

# 指定包运行（等价）
cargo run -p rust-quant-cli --release
```

---

## 📊 当前目录结构总览

```
rust_quant/                    # 项目根目录
├── Cargo.toml                 # ⭐ Workspace 配置
├── Cargo.lock                 # 依赖锁定
├── .env                       # 环境变量
├── .gitignore                 # Git 忽略规则
│
├── crates/                    # ⭐ 所有代码在这里
│   ├── common/                # [lib] 通用工具
│   ├── core/                  # [lib] 核心基础设施
│   ├── domain/                # [lib] 领域模型
│   ├── infrastructure/        # [lib] 基础设施实现
│   ├── services/              # [lib] 应用服务
│   ├── market/                # [lib] 市场数据
│   ├── indicators/            # [lib] 技术指标
│   ├── strategies/            # [lib] 策略引擎
│   ├── risk/                  # [lib] 风险管理
│   ├── execution/             # [lib] 订单执行
│   ├── orchestration/         # [lib] 任务调度
│   ├── analytics/             # [lib] 分析报告
│   ├── ai-analysis/           # [lib] AI 分析
│   └── rust-quant-cli/        # [lib + bin] ⭐ 可执行程序
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs        # ✅ 程序入口
│           ├── lib.rs         # CLI 库（供其他程序调用）
│           └── app/
│               ├── mod.rs
│               └── bootstrap.rs
│
├── docs/                      # 文档
│   ├── MIGRATION_PROGRESS_REPORT.md
│   ├── BUSINESS_LOGIC_COMPARISON.md
│   ├── WORK_COMPLETION_SUMMARY.md
│   └── STARTUP_GUIDE.md
│
├── examples/                  # 使用示例
├── tests/                     # 集成测试
├── scripts/                   # 工具脚本
├── config/                    # 配置文件
├── log_files/                 # 日志文件
│
├── src_backup_*.tar.gz        # 📦 旧代码备份
└── target/                    # 编译输出
    ├── debug/
    │   └── rust-quant         # Debug 可执行文件
    └── release/
        └── rust-quant         # ✅ Release 可执行文件
```

---

## 🔑 关键变化

### ❌ 删除的内容
```
src/                           # ❌ 删除（已迁移到 crates/rust-quant-cli）
├── main.rs                    # → crates/rust-quant-cli/src/main.rs
├── lib.rs                     # → crates/rust-quant-cli/src/lib.rs
├── app/                       # → crates/rust-quant-cli/src/app/
├── trading/                   # → 分散到各个 crate
│   ├── indicator/             # → crates/indicators/
│   ├── strategy/              # → crates/strategies/
│   ├── task/                  # → crates/orchestration/
│   ├── model/                 # → crates/common/
│   └── services/              # → crates/services/
├── app_config/                # → crates/core/config/
├── job/                       # → crates/orchestration/
└── error/                     # → crates/core/error/
```

### ✅ 新的组织
```
crates/
├── rust-quant-cli/            # ⭐ 唯一的可执行程序包
│   └── src/main.rs            # 程序入口
├── common/                    # 通用功能
├── core/                      # 基础设施
├── domain/                    # 业务领域
├── strategies/                # 策略逻辑
└── ...                        # 其他业务包
```

---

## 🎯 编译和运行命令

### 开发阶段

```bash
# 快速编译和运行（Debug 模式）
cargo run

# 指定包运行
cargo run -p rust-quant-cli

# 带环境变量
IS_BACK_TEST=true cargo run
```

### 生产部署

```bash
# 编译 Release 版本
cargo build --release --bin rust-quant

# 运行
./target/release/rust-quant

# 或带环境变量
APP_ENV=prod ./target/release/rust-quant
```

### 测试

```bash
# 测试所有包
cargo test --workspace

# 测试特定包
cargo test -p rust-quant-strategies

# 运行单个测试
cargo test test_vegas_strategy
```

---

## 📦 包依赖图

```
rust-quant (可执行文件)
  └─ rust-quant-cli [bin + lib]
      ├─ rust-quant-orchestration [lib]
      │   ├─ rust-quant-strategies [lib]
      │   │   ├─ rust-quant-indicators [lib]
      │   │   ├─ rust-quant-domain [lib]
      │   │   └─ rust-quant-infrastructure [lib]
      │   ├─ rust-quant-risk [lib]
      │   ├─ rust-quant-execution [lib]
      │   └─ rust-quant-services [lib]
      ├─ rust-quant-core [lib]
      └─ rust-quant-common [lib]
```

---

## ✅ 验证架构正确性

### 检查 workspace 配置
```bash
cargo metadata --no-deps | jq '.workspace_members'
```

### 检查二进制目标
```bash
cargo metadata --no-deps | jq '.packages[] | select(.name == "rust-quant-cli") | .targets[] | select(.kind[] == "bin")'
```

### 检查编译产物
```bash
ls -lh target/release/rust-quant
# -rwxr-xr-x  10M  rust-quant  ✅
```

---

## 🎊 总结

### ✅ 当前状态

1. **纯 Workspace 架构**: 根目录无 `src/`
2. **可执行程序**: `crates/rust-quant-cli`
3. **编译成功**: Release 版本 10MB
4. **架构清晰**: 14 个独立包

### ✅ 优势

1. **清晰的模块边界**: 每个包职责单一
2. **更好的编译性能**: 增量编译和并行编译
3. **更好的测试支持**: 独立测试每个包
4. **符合 Rust 最佳实践**: Virtual Workspace

### 📝 下一步

启动服务需要：
1. 启动 MySQL
2. 启动 Redis
3. 配置 `.env`
4. 运行 `./target/release/rust-quant`

---

**项目现在是标准的 Rust Workspace 架构，不再需要根目录的 `src/`！** ✅

