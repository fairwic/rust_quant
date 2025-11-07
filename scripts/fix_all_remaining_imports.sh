#!/bin/bash
# 综合修复所有剩余的导入路径问题

set -e

echo "🔧 开始综合修复所有包的导入路径..."

# === 修复 indicators 包 ===
echo "1️⃣ 修复 indicators 包..."

find crates/indicators/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use super::atr::/use crate::volatility::atr::/g' \
    -e 's/use super::bollings/use crate::volatility::bollinger/g' \
    -e 's/use super::k_line_engulfing_indicator/use crate::pattern::engulfing/g' \
    -e 's/use super::k_line_hammer_indicator/use crate::pattern::hammer/g' \
    -e 's/use super::rsi_rma_indicator/use crate::momentum::rsi/g' \
    -e 's/use super::ema_indicator/use crate::trend::ema_indicator/g' \
    -e 's/use super::vegas_indicator/use crate::trend::vegas/g' \
    {} +

# === 修复 strategies 包 ===  
echo "2️⃣ 修复 strategies 包..."

find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_infrastructure::cache::arc_vegas_indicator_values/rust_quant_indicators::trend::vegas/g' \
    -e 's/rust_quant_infrastructure::cache::arc_nwe_indicator_values/rust_quant_indicators::trend::nwe_indicator/g' \
    -e 's/rust_quant_common::types::CandlesEntity/rust_quant_market::models::CandleEntity/g' \
    -e 's/rust_quant_common::domain_service::/rust_quant_market::repositories::/g' \
    -e 's/rust_quant_common::types::market::/rust_quant_market::models::/g' \
    -e 's/rust_quant_common::types::big_data::/rust_quant_market::models::/g' \
    -e 's/use crate::time_util/use rust_quant_common::utils::time/g' \
    -e 's/crate::time_util::/rust_quant_common::utils::time::/g' \
    -e 's/use crate::CandleItem/use rust_quant_common::CandleItem/g' \
    -e 's/crate::CandleItem/rust_quant_common::CandleItem/g' \
    -e 's/use crate::SCHEDULER/\/\/ use crate::SCHEDULER \/\/ TODO: 定义全局SCHEDULER/g' \
    {} +

echo "✅ 综合修复完成！"
echo ""
echo "📊 编译状态检查..."
echo ""

cd /Users/mac2/onions/rust_quant

echo "检查 domain..."
cargo check --package rust-quant-domain 2>&1 | tail -3

echo ""
echo "检查 infrastructure..."
cargo check --package rust-quant-infrastructure 2>&1 | tail -3

echo ""
echo "检查 indicators..."
cargo check --package rust-quant-indicators 2>&1 | grep -c "error" || echo "0"

echo ""
echo "检查 strategies..."
cargo check --package rust-quant-strategies 2>&1 | grep -c "error" || echo "0"

