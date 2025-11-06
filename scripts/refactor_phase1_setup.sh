#!/bin/bash
# Rust Quant 项目架构重构 - 阶段一：基础设施层搭建脚本
# 版本: v1.0
# 日期: 2025-11-06

set -e  # 遇到错误立即退出

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Rust Quant 架构重构 - 阶段一${NC}"
echo -e "${GREEN}创建基础设施层目录结构${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# 检查是否在 Git 仓库中
if [ ! -d ".git" ]; then
    echo -e "${RED}错误: 当前目录不是 Git 仓库${NC}"
    exit 1
fi

# 检查是否有未提交的更改
if ! git diff-index --quiet HEAD --; then
    echo -e "${YELLOW}警告: 检测到未提交的更改${NC}"
    read -p "是否继续? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# 创建重构分支
BRANCH_NAME="refactor/ddd-architecture-phase1"
echo -e "${YELLOW}创建分支: $BRANCH_NAME${NC}"

if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
    echo -e "${YELLOW}分支已存在，切换到该分支${NC}"
    git checkout "$BRANCH_NAME"
else
    git checkout -b "$BRANCH_NAME"
    echo -e "${GREEN}✓ 分支创建成功${NC}"
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 1: 创建 Infrastructure 层目录${NC}"
echo -e "${YELLOW}========================================${NC}"

# 创建 infrastructure 目录结构
mkdir -p src/infrastructure/{persistence,messaging,cache,config,scheduler,external_api,monitoring}

# Persistence 子目录
mkdir -p src/infrastructure/persistence/{database,repositories,entities}

# Messaging 子目录
mkdir -p src/infrastructure/messaging/{websocket,message_bus}

# Scheduler 子目录
mkdir -p src/infrastructure/scheduler/jobs

# External API 子目录
mkdir -p src/infrastructure/external_api/{okx_client,notification}

echo -e "${GREEN}✓ Infrastructure 目录结构创建完成${NC}"

# 创建 mod.rs 文件
echo -e "${YELLOW}创建 mod.rs 文件...${NC}"

cat > src/infrastructure/mod.rs << 'EOF'
//! # 基础设施层
//! 
//! 提供技术实现细节，包括：
//! - 数据持久化
//! - 消息通信（WebSocket, 消息总线）
//! - 缓存管理
//! - 配置管理
//! - 任务调度
//! - 外部API集成
//! - 监控和可观测性

pub mod persistence;
pub mod messaging;
pub mod cache;
pub mod config;
pub mod scheduler;
pub mod external_api;
pub mod monitoring;
EOF

cat > src/infrastructure/persistence/mod.rs << 'EOF'
//! 数据持久化模块

pub mod database;
pub mod repositories;
pub mod entities;
EOF

cat > src/infrastructure/messaging/mod.rs << 'EOF'
//! 消息通信模块

pub mod websocket;
pub mod message_bus;
EOF

cat > src/infrastructure/scheduler/mod.rs << 'EOF'
//! 任务调度模块
//! 
//! 整合原 job/ 和 trading/task/ 的功能

pub mod jobs;

// 重新导出常用类型
// pub use jobs::*;
EOF

cat > src/infrastructure/config/mod.rs << 'EOF'
//! 配置管理模块
//! 
//! 迁移自 app_config/ 目录

// TODO: 迁移原 app_config/ 模块内容
// pub mod database_config;
// pub mod redis_config;
// pub mod log_config;
// pub mod environment;
EOF

cat > src/infrastructure/cache/mod.rs << 'EOF'
//! 缓存管理模块

// TODO: 迁移 trading/cache/ 内容
EOF

cat > src/infrastructure/external_api/mod.rs << 'EOF'
//! 外部API集成模块

pub mod okx_client;
pub mod notification;
EOF

cat > src/infrastructure/monitoring/mod.rs << 'EOF'
//! 监控和可观测性模块

// pub mod metrics;
// pub mod tracing;
// pub mod health_check;
EOF

echo -e "${GREEN}✓ mod.rs 文件创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 2: 创建 Domain 层目录${NC}"
echo -e "${YELLOW}========================================${NC}"

# 创建 domain 目录结构
mkdir -p src/domain/{market,strategy,risk,order,shared}

# Market 子目录
mkdir -p src/domain/market/{entities,value_objects,repositories,services}

# Strategy 子目录
mkdir -p src/domain/strategy/{entities,value_objects,strategies,indicators,repositories}
mkdir -p src/domain/strategy/indicators/{trend,momentum,volatility,volume}

# Risk 子目录
mkdir -p src/domain/risk/{entities,value_objects,services,policies}

# Order 子目录
mkdir -p src/domain/order/{entities,value_objects,services,repositories}

# Shared 子目录
mkdir -p src/domain/shared/{events,specifications}

echo -e "${GREEN}✓ Domain 目录结构创建完成${NC}"

# 创建 domain mod.rs
cat > src/domain/mod.rs << 'EOF'
//! # 领域层
//! 
//! 核心业务逻辑，不依赖任何外部框架
//! 
//! ## 领域划分
//! - `market`: 市场数据领域
//! - `strategy`: 策略领域
//! - `risk`: 风控领域
//! - `order`: 订单领域
//! - `shared`: 跨领域共享

pub mod market;
pub mod strategy;
pub mod risk;
pub mod order;
pub mod shared;
EOF

cat > src/domain/market/mod.rs << 'EOF'
//! 市场数据领域

pub mod entities;
pub mod value_objects;
pub mod repositories;
pub mod services;
EOF

cat > src/domain/strategy/mod.rs << 'EOF'
//! 策略领域

pub mod entities;
pub mod value_objects;
pub mod strategies;
pub mod indicators;
pub mod repositories;
EOF

cat > src/domain/strategy/indicators/mod.rs << 'EOF'
//! 技术指标模块
//! 
//! 按指标类型分类：
//! - `trend`: 趋势指标（EMA, SMA, SuperTrend）
//! - `momentum`: 动量指标（RSI, MACD, KDJ）
//! - `volatility`: 波动性指标（ATR, Bollinger Bands）
//! - `volume`: 成交量指标

pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
EOF

cat > src/domain/risk/mod.rs << 'EOF'
//! 风控领域

pub mod entities;
pub mod value_objects;
pub mod services;
pub mod policies;
EOF

cat > src/domain/order/mod.rs << 'EOF'
//! 订单领域

pub mod entities;
pub mod value_objects;
pub mod services;
pub mod repositories;
EOF

cat > src/domain/shared/mod.rs << 'EOF'
//! 跨领域共享模块

pub mod events;
pub mod specifications;
EOF

echo -e "${GREEN}✓ Domain mod.rs 文件创建完成${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 3: 创建 Application 层目录${NC}"
echo -e "${YELLOW}========================================${NC}"

# 创建 application 目录结构
mkdir -p src/application/{commands,queries,services,dto,workflows}
mkdir -p src/application/commands/{strategy,order,handlers}
mkdir -p src/application/queries/{strategy,market,handlers}

echo -e "${GREEN}✓ Application 目录结构创建完成${NC}"

cat > src/application/mod.rs << 'EOF'
//! # 应用层
//! 
//! 用例编排，协调领域对象完成业务流程
//! 
//! ## CQRS 模式
//! - `commands`: 命令处理（写操作）
//! - `queries`: 查询处理（读操作）
//! - `services`: 应用服务（编排领域服务）
//! - `dto`: 数据传输对象
//! - `workflows`: 复杂业务流程

pub mod commands;
pub mod queries;
pub mod services;
pub mod dto;
pub mod workflows;
EOF

cat > src/application/commands/mod.rs << 'EOF'
//! 命令处理模块（写操作）

pub mod strategy;
pub mod order;
pub mod handlers;
EOF

cat > src/application/queries/mod.rs << 'EOF'
//! 查询处理模块（读操作）

pub mod strategy;
pub mod market;
pub mod handlers;
EOF

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 4: 创建 Shared 层目录${NC}"
echo -e "${YELLOW}========================================${NC}"

# 创建 shared 目录结构
mkdir -p src/shared/{types,utils,constants,errors}

echo -e "${GREEN}✓ Shared 目录结构创建完成${NC}"

cat > src/shared/mod.rs << 'EOF'
//! # 共享层
//! 
//! 跨层共享的工具和类型
//! 
//! - `types`: 公共类型定义
//! - `utils`: 工具函数
//! - `constants`: 全局常量
//! - `errors`: 统一错误处理

pub mod types;
pub mod utils;
pub mod constants;
pub mod errors;
EOF

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 5: 更新 lib.rs${NC}"
echo -e "${YELLOW}========================================${NC}"

# 备份原 lib.rs
cp src/lib.rs src/lib.rs.backup

# 在 lib.rs 开头添加新模块声明（保留原有模块）
cat > src/lib.rs.new << 'EOF'
// ============================================
// 新架构模块（DDD 分层）
// ============================================
pub mod domain;
pub mod application;
pub mod infrastructure;
// pub mod interfaces;  // 接口层（可选）
pub mod shared;

// ============================================
// 旧架构模块（待迁移）
// ============================================
pub mod app_config;
pub mod enums;
pub mod error;
pub mod job;
pub mod socket;
pub mod time_util;
pub mod trading;
pub mod app;

EOF

# 追加原 lib.rs 的其他内容（从第9行开始）
tail -n +9 src/lib.rs >> src/lib.rs.new
mv src/lib.rs.new src/lib.rs

echo -e "${GREEN}✓ lib.rs 更新完成（原文件已备份为 lib.rs.backup）${NC}"

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}步骤 6: 创建迁移文档${NC}"
echo -e "${YELLOW}========================================${NC}"

cat > MIGRATION_PROGRESS.md << 'EOF'
# 架构重构迁移进度

## 当前阶段：阶段一 - 基础设施层搭建

### 已完成
- ✅ 创建 `infrastructure/` 目录结构
- ✅ 创建 `domain/` 目录结构
- ✅ 创建 `application/` 目录结构
- ✅ 创建 `shared/` 目录结构
- ✅ 更新 `lib.rs` 模块声明

### 待完成（阶段一）
- [ ] 迁移 `app_config/` → `infrastructure/config/`
- [ ] 迁移 `socket/` → `infrastructure/messaging/websocket/`
- [ ] 整合 `job/` + `trading/task/` → `infrastructure/scheduler/`
- [ ] 迁移 `trading/cache/` → `infrastructure/cache/`
- [ ] 更新所有引用路径
- [ ] 回归测试

### 待完成（阶段二）
- [ ] 迁移市场数据模型到 `domain/market/`
- [ ] 迁移策略逻辑到 `domain/strategy/`
- [ ] 重组技术指标为 trend/momentum/volatility/volume
- [ ] 提取风控领域到 `domain/risk/`

### 待完成（阶段三）
- [ ] 创建 CQRS Commands 和 Queries
- [ ] 迁移应用服务到 `application/services/`

### 待完成（阶段四）
- [ ] 迁移工具函数到 `shared/utils/`
- [ ] 增强错误处理
- [ ] 清理旧代码

## 迁移检查清单

### 每次迁移后必做
- [ ] 运行 `cargo check` 确保编译通过
- [ ] 运行 `cargo test` 确保测试通过
- [ ] 更新相关文档
- [ ] Git 提交（描述清晰的 commit message）

### 常用命令
```bash
# 编译检查
cargo check

# 运行所有测试
cargo test

# 运行特定模块测试
cargo test --package rust_quant --lib domain::strategy

# 格式化代码
cargo fmt

# Clippy 检查
cargo clippy -- -D warnings
```

## 注意事项
1. 每次迁移模块前先创建单元测试
2. 保持小步提交，便于回滚
3. 迁移过程中保留旧代码，新旧代码并存
4. 所有测试通过后再删除旧代码

## 参考文档
- [架构重构方案](docs/architecture_refactoring_plan.md)
- [架构对比](docs/current_vs_proposed_architecture.md)
EOF

echo -e "${GREEN}✓ 迁移进度文档创建完成${NC}"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}阶段一目录结构搭建完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# 显示目录结构
echo -e "${YELLOW}新创建的目录结构：${NC}"
tree -L 3 -d src/ || find src/ -type d | sed 's|[^/]*/| |g'

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}下一步操作建议：${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""
echo "1. 验证编译："
echo "   ${GREEN}cargo check${NC}"
echo ""
echo "2. 开始迁移配置模块："
echo "   ${GREEN}cp -r src/app_config/* src/infrastructure/config/${NC}"
echo "   然后修改 infrastructure/config/mod.rs"
echo ""
echo "3. 查看迁移进度："
echo "   ${GREEN}cat MIGRATION_PROGRESS.md${NC}"
echo ""
echo "4. 参考重构文档："
echo "   ${GREEN}cat docs/architecture_refactoring_plan.md${NC}"
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}重构愉快！${NC}"
echo -e "${GREEN}========================================${NC}"

