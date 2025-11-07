#!/bin/bash
# 批量修复 strategies 包的导入路径

set -e

echo "🔧 开始批量修复 strategies 包导入路径..."

# 修复 indicators 导入
echo "1️⃣ 修复 indicators 导入..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_indicators::kdj_simple_indicator/rust_quant_indicators::momentum::kdj/g' \
    -e 's/rust_quant_indicators::macd_simple_indicator/rust_quant_indicators::momentum::macd/g' \
    -e 's/rust_quant_indicators::rsi_rma_indicator/rust_quant_indicators::momentum::rsi/g' \
    -e 's/rust_quant_indicators::atr_stop_loos/rust_quant_indicators::volatility::atr/g' \
    -e 's/rust_quant_indicators::atr::/rust_quant_indicators::volatility::atr::/g' \
    {} +

# 修复 trading 路径
echo "2️⃣ 修复 crate::trading 路径..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use crate::trading::model::entity::candles::entity::/use rust_quant_common::types::/g' \
    -e 's/use crate::trading::model::/use rust_quant_common::types::/g' \
    -e 's/use crate::trading::services::/use crate::framework::/g' \
    -e 's/use crate::trading::/use rust_quant_common::/g' \
    {} +

# 修复 arc 路径（缓存）
echo "3️⃣ 修复缓存路径..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use crate::arc::indicator_values::/use rust_quant_infrastructure::cache::/g' \
    -e 's/use crate::arc::/use rust_quant_infrastructure::cache::/g' \
    -e 's/use super::arc::/use rust_quant_infrastructure::cache::/g' \
    {} +

# 修复 order 路径
echo "4️⃣ 修复 order 路径..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use crate::order::/use crate::framework::config::/g' \
    {} +

# 修复 CandleItem 导入
echo "5️⃣ 修复 CandleItem 导入..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use crate::CandleItem/use rust_quant_common::CandleItem/g' \
    {} +

# 修复 time_util 导入
echo "6️⃣ 修复 time_util 导入..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use time_util::/use rust_quant_common::utils::time::/g' \
    -e 's/time_util::/rust_quant_common::utils::time::/g' \
    {} +

# 修复 log → tracing
echo "7️⃣ 修复 log → tracing..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use log::/use tracing::/g' \
    -e 's/log::error!/tracing::error!/g' \
    -e 's/log::info!/tracing::info!/g' \
    -e 's/log::warn!/tracing::warn!/g' \
    -e 's/log::debug!/tracing::debug!/g' \
    {} +

echo "✅ 批量修复完成！"
echo "📊 运行 cargo check 验证..."

cd /Users/mac2/onions/rust_quant
cargo check --package rust-quant-strategies 2>&1 | head -100

