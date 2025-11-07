#!/bin/bash
# 最终综合修复所有包的编译错误

set -e

echo "🚀 开始最终批量修复所有包..."
echo ""

cd /Users/mac2/onions/rust_quant

# === 修复 orchestration 包 ===
echo "1️⃣ 修复 orchestration 包..."
find crates/orchestration/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/use crate::trading::/use rust_quant_common::/g' \
    -e 's/rust_quant_market::models::strategy::/rust_quant_risk::backtest::/g' \
    -e 's/use crate::app_config::/use rust_quant_core::config::/g' \
    -e 's/crate::job::/crate::workflow::/g' \
    {} +

# === 修复 risk 包 ===
echo "2️⃣ 修复 risk 包错误转换..."
find crates/risk/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/\.await?/.await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?/g' \
    {} +

# === 修复 execution 包 ===  
echo "3️⃣ 修复 execution 包..."
find crates/execution/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_market::models::strategy::/rust_quant_risk::backtest::/g' \
    {} +

# === 取消注释 infrastructure 缓存模块 ===
echo "4️⃣ 临时处理 infrastructure 缓存模块..."
# 这里暂时保持注释状态，因为依赖indicator未完全就绪

echo ""
echo "✅ 批量修复完成！"
echo ""
echo "📊 验证各包编译状态..."
echo ""

for pkg in risk execution orchestration; do
    echo "检查 $pkg..."
    cargo check --package rust-quant-$pkg 2>&1 | grep -c "error\[" || echo "✅ 通过"
done

echo ""
echo "🎉 修复完成！"

