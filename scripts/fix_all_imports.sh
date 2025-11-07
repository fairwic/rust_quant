#!/bin/bash
# fix_all_imports.sh - 批量修复所有包的导入路径

set -e  # 遇到错误立即退出

cd /Users/mac2/onions/rust_quant

echo "🔧 开始批量修复导入路径..."

# 1. 修复 trading 相关的导入
echo "📦 修复 trading 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::trading::model::entity::candles::entity::CandlesEntity|rust_quant_market::models::CandlesEntity|g' \
    -e 's|crate::trading::model::entity::candles::dto::SelectCandleReqDto|rust_quant_market::models::SelectCandleReqDto|g' \
    -e 's|crate::trading::model::entity::candles::enums::|rust_quant_market::models::|g' \
    -e 's|crate::trading::model::market::candles::CandlesModel|rust_quant_market::models::CandlesModel|g' \
    -e 's|crate::trading::model::order::|rust_quant_risk::order::|g' \
    -e 's|crate::trading::indicator::|rust_quant_indicators::|g' \
    -e 's|crate::trading::strategy::|rust_quant_strategies::|g' \
    -e 's|crate::trading::services::position_service|rust_quant_risk::position|g' \
    -e 's|crate::trading::services::order_service|rust_quant_execution::order_manager|g' \
    -e 's|crate::trading::services::candle_service|rust_quant_market::repositories|g' \
    -e 's|crate::trading::services::strategy_metrics|rust_quant_strategies::framework|g' \
    -e 's|crate::trading::task::|rust_quant_orchestration::workflow::|g' \
    {} +

# 2. 修复 app_config 相关的导入
echo "⚙️  修复 app_config 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::app_config::db::get_db_client|rust_quant_core::database::get_db_pool|g' \
    -e 's|crate::app_config::db|rust_quant_core::database|g' \
    -e 's|crate::app_config::redis_config|rust_quant_core::cache|g' \
    -e 's|crate::app_config::shutdown_manager|rust_quant_core::shutdown|g' \
    -e 's|crate::app_config::|rust_quant_core::config::|g' \
    {} +

# 3. 修复 time_util 相关的导入
echo "⏰ 修复 time_util 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::time_util|rust_quant_common::utils::time|g' \
    {} +

# 4. 修复 error 相关的导入
echo "🚨 修复 error 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::error::|rust_quant_core::error::|g' \
    {} +

# 5. 修复 enums 相关的导入
echo "📋 修复 enums 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::enums::|rust_quant_common::types::enums::|g' \
    {} +

# 6. 修复 socket 相关的导入
echo "🔌 修复 socket 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::socket::|rust_quant_market::streams::|g' \
    {} +

# 7. 修复 job 相关的导入
echo "📅 修复 job 模块导入..."
find crates/ -name "*.rs" -type f -exec sed -i '' \
    -e 's|crate::job::|rust_quant_orchestration::workflow::|g' \
    {} +

echo "✅ 导入路径修复完成！"
echo ""
echo "🔍 验证编译..."
cargo check --workspace --quiet 2>&1 | head -50

