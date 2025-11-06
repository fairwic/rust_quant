#!/bin/bash
# Workspace 迁移 - 阶段1: common 和 core 包
# 迁移公共类型、工具函数和核心基础设施

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}阶段1: 迁移 common 和 core 包${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# ============================================
# Part 1: 迁移 common 包
# ============================================
echo -e "${YELLOW}Part 1: 迁移 common 包${NC}"

# 1.1 迁移公共类型
echo -e "${BLUE}迁移公共类型...${NC}"
if [ -f "src/trading/types.rs" ]; then
    cp src/trading/types.rs crates/common/src/types/candle_types.rs
    echo -e "${GREEN}✓ types.rs → common/src/types/candle_types.rs${NC}"
fi

# 1.2 迁移工具函数
echo -e "${BLUE}迁移工具函数...${NC}"

# 时间工具
if [ -f "src/time_util.rs" ]; then
    cp src/time_util.rs crates/common/src/utils/time.rs
    echo -e "${GREEN}✓ time_util.rs → common/src/utils/time.rs${NC}"
fi

# 其他工具函数
if [ -d "src/trading/utils" ]; then
    for file in src/trading/utils/*.rs; do
        if [ -f "$file" ]; then
            filename=$(basename "$file")
            cp "$file" "crates/common/src/utils/$filename"
            echo -e "${GREEN}✓ trading/utils/$filename → common/src/utils/$filename${NC}"
        fi
    done
fi

# 1.3 迁移常量
echo -e "${BLUE}迁移常量定义...${NC}"
if [ -d "src/trading/constants" ]; then
    for file in src/trading/constants/*.rs; do
        if [ -f "$file" ]; then
            filename=$(basename "$file")
            cp "$file" "crates/common/src/constants/$filename"
            echo -e "${GREEN}✓ trading/constants/$filename → common/src/constants/$filename${NC}"
        fi
    done
fi

# 1.4 迁移枚举定义
if [ -d "src/enums" ]; then
    mkdir -p crates/common/src/types/enums
    for file in src/enums/*.rs; do
        if [ -f "$file" ]; then
            filename=$(basename "$file")
            cp "$file" "crates/common/src/types/enums/$filename"
            echo -e "${GREEN}✓ enums/$filename → common/src/types/enums/$filename${NC}"
        fi
    done
fi

# 更新 common/src/types/mod.rs
cat > crates/common/src/types/mod.rs << 'EOF'
//! 公共类型定义

pub mod enums;

// 如果有 candle_types.rs，导出它
#[cfg(feature = "candle_types")]
pub mod candle_types;
EOF

# 更新 common/src/utils/mod.rs
cat > crates/common/src/utils/mod.rs << 'EOF'
//! 工具函数模块

pub mod time;

// 导出其他工具模块
// pub mod math;
// pub mod fibonacci;
// pub mod validation;
EOF

# 更新 common/src/constants/mod.rs
cat > crates/common/src/constants/mod.rs << 'EOF'
//! 常量定义

// 导出所有常量模块
// pub mod timeframes;
// pub mod exchanges;
EOF

echo -e "${GREEN}✓ common 包迁移完成${NC}"

# ============================================
# Part 2: 迁移 core 包
# ============================================
echo -e "${YELLOW}Part 2: 迁移 core 包${NC}"

# 2.1 迁移配置模块
echo -e "${BLUE}迁移配置模块...${NC}"
if [ -d "src/app_config" ]; then
    for file in src/app_config/*.rs; do
        if [ -f "$file" ]; then
            filename=$(basename "$file")
            # 特殊处理 mod.rs
            if [ "$filename" = "mod.rs" ]; then
                cp "$file" "crates/core/src/config/app_config.rs"
                echo -e "${GREEN}✓ app_config/mod.rs → core/src/config/app_config.rs${NC}"
            else
                # 根据文件名分类
                case "$filename" in
                    db.rs)
                        cp "$file" "crates/core/src/database/connection_pool.rs"
                        echo -e "${GREEN}✓ app_config/db.rs → core/src/database/connection_pool.rs${NC}"
                        ;;
                    redis_config.rs)
                        cp "$file" "crates/core/src/cache/redis_client.rs"
                        echo -e "${GREEN}✓ app_config/redis_config.rs → core/src/cache/redis_client.rs${NC}"
                        ;;
                    log.rs)
                        cp "$file" "crates/core/src/logger/setup.rs"
                        echo -e "${GREEN}✓ app_config/log.rs → core/src/logger/setup.rs${NC}"
                        ;;
                    env.rs)
                        cp "$file" "crates/core/src/config/environment.rs"
                        echo -e "${GREEN}✓ app_config/env.rs → core/src/config/environment.rs${NC}"
                        ;;
                    *)
                        cp "$file" "crates/core/src/config/$filename"
                        echo -e "${GREEN}✓ app_config/$filename → core/src/config/$filename${NC}"
                        ;;
                esac
            fi
        fi
    done
fi

# 更新 core/src/config/mod.rs
cat > crates/core/src/config/mod.rs << 'EOF'
//! 配置管理模块

pub mod app_config;
pub mod environment;

// 重新导出常用类型
pub use app_config::AppConfig;
EOF

# 更新 core/src/database/mod.rs
cat > crates/core/src/database/mod.rs << 'EOF'
//! 数据库连接管理

pub mod connection_pool;

// 重新导出
pub use connection_pool::DbPool;
EOF

# 更新 core/src/cache/mod.rs
cat > crates/core/src/cache/mod.rs << 'EOF'
//! 缓存管理

pub mod redis_client;

// 重新导出
pub use redis_client::RedisClient;
EOF

# 更新 core/src/logger/mod.rs
cat > crates/core/src/logger/mod.rs << 'EOF'
//! 日志系统

pub mod setup;

// 重新导出
pub use setup::setup_logging;
EOF

# 更新 core/src/time/mod.rs (如果 common 中也有，这里可以重导出)
cat > crates/core/src/time/mod.rs << 'EOF'
//! 时间工具

// 重新导出 common 中的时间工具
pub use rust_quant_common::utils::time::*;
EOF

echo -e "${GREEN}✓ core 包迁移完成${NC}"

# ============================================
# Part 3: 编译验证
# ============================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}编译验证${NC}"
echo -e "${YELLOW}========================================${NC}"

echo -e "${BLUE}编译 common 包...${NC}"
if cargo check --package rust-quant-common; then
    echo -e "${GREEN}✓ common 包编译成功${NC}"
else
    echo -e "${RED}✗ common 包编译失败，请检查错误${NC}"
    exit 1
fi

echo -e "${BLUE}编译 core 包...${NC}"
if cargo check --package rust-quant-core; then
    echo -e "${GREEN}✓ core 包编译成功${NC}"
else
    echo -e "${RED}✗ core 包编译失败，请检查错误${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}阶段1迁移完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "下一步："
echo "1. 检查编译警告并修复"
echo "2. 运行测试: cargo test --package rust-quant-common"
echo "3. 提交代码: git add . && git commit -m \"feat: 迁移 common 和 core 包\""
echo "4. 执行阶段2: ./scripts/migrate_phase2_market.sh"

