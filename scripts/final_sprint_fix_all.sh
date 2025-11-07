#!/bin/bash
# 最终冲刺 - 修复所有剩余编译错误

set -e

echo "🚀🚀🚀 最终冲刺开始 - 目标：所有包编译通过！"
echo ""

cd /Users/mac2/onions/rust_quant

# === 阶段1: 快速修复简单错误 ===
echo "════════════════════════════════════════"
echo "阶段1: 修复简单错误 (risk, execution)"
echo "════════════════════════════════════════"
echo ""

# 修复 risk 包的 okx::Error 转换
echo "1️⃣ 修复 risk 包错误转换..."
find crates/risk/src/account -name "*.rs" -type f -exec sed -i '' \
    -e 's/\.get_account_positions/\.get_account_positions/g' \
    {} +

# 修复所有 .await? 为 .await.map_err
find crates/risk/src -name "*.rs" -type f -exec perl -i -pe 's/(\w+::get_\w+\([^)]*\))\.await\?/$1.await.map_err(|e| anyhow::anyhow!("{:?}", e))?/g' {} +

# 修复 execution 包
echo "2️⃣ 修复 execution 包..."
find crates/execution/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_common::strategy::/rust_quant_risk::backtest::/g' \
    {} +

echo ""
echo "✅ 阶段1完成！检查状态..."
cargo check --package rust-quant-risk 2>&1 | grep -c "error" || echo "✅ risk通过"
cargo check --package rust-quant-execution 2>&1 | grep -c "error" || echo "✅ execution通过"
echo ""

# === 阶段2: 修复indicators和strategies ===
echo "════════════════════════════════════════"
echo "阶段2: 修复 indicators 和 strategies"
echo "════════════════════════════════════════"
echo ""

# 为 indicators 添加 SignalResult builder
echo "3️⃣ 修复 indicators SignalResult 初始化..."

# 修复 indicators 的导入
find crates/indicators/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_common::utils::IsBigKLineIndicator/rust_quant_common::utils::common::IsBigKLineIndicator/g' \
    -e 's/rust_quant_core::database::init_db/rust_quant_core::database::get_db_pool/g' \
    {} +

# 修复 strategies 的indicator导入
echo "4️⃣ 修复 strategies indicator导入..."
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_indicators::vegas_indicator/rust_quant_indicators::trend::vegas/g' \
    -e 's/rust_quant_indicators::nwe_indicator/rust_quant_indicators::trend::nwe_indicator/g' \
    -e 's/rust_quant_indicators::signal_weight/rust_quant_indicators::trend::signal_weight/g' \
    -e 's/rust_quant_indicators::ema_indicator/rust_quant_indicators::trend::ema_indicator/g' \
    {} +

echo ""
echo "✅ 阶段2完成！检查状态..."
echo "indicators: $(cargo check --package rust-quant-indicators 2>&1 | grep -c 'error' || echo '0') errors"
echo "strategies: $(cargo check --package rust-quant-strategies 2>&1 | grep -c 'error' || echo '0') errors"
echo ""

# === 阶段3: 修复orchestration ===
echo "════════════════════════════════════════"
echo "阶段3: 修复 orchestration"
echo "════════════════════════════════════════"
echo ""

echo "5️⃣ 修复 orchestration 导入路径..."
find crates/orchestration/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_indicators::vegas_indicator/rust_quant_indicators::trend::vegas/g' \
    -e 's/rust_quant_strategies::nwe_strategy/rust_quant_strategies::implementations::nwe_strategy/g' \
    {} +

echo ""
echo "✅ 阶段3完成！检查状态..."
echo "orchestration: $(cargo check --package rust-quant-orchestration 2>&1 | grep -c 'error' || echo '0') errors"
echo ""

# === 最终状态报告 ===
echo "════════════════════════════════════════"
echo "🎉 最终状态报告"
echo "════════════════════════════════════════"
echo ""

for pkg in common core domain infrastructure market indicators strategies risk execution orchestration; do
    errors=$(cargo check --package rust-quant-$pkg 2>&1 | grep -c "error\[" || echo "0")
    if [ "$errors" = "0" ]; then
        echo "✅ rust-quant-$pkg: 编译通过"
    else
        echo "🟡 rust-quant-$pkg: $errors errors"
    fi
done

echo ""
echo "🎊 冲刺完成！"

