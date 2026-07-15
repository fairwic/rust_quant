#!/bin/bash
# 1H Momentum Reversal 策略 - 快速启动指南
#
# 本脚本帮助你快速完成探索阶段的数据准备和首次回测

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "============================================"
echo "🚀 1H Momentum Reversal - 探索启动"
echo "============================================"
echo ""

# Step 1: 检查项目结构
echo "📁 检查项目结构..."
cd "$PROJECT_ROOT"

if [ ! -f "Cargo.toml" ]; then
    echo "❌ 错误: 不在 rust_quant 项目根目录"
    exit 1
fi

echo "✅ 项目根目录: $PROJECT_ROOT"
echo ""

# Step 2: 检查测试文件
echo "📝 检查测试文件..."
TEST_FILE="tests/exploration_momentum_reversal_1h.rs"

if [ ! -f "$TEST_FILE" ]; then
    echo "❌ 错误: 测试文件不存在: $TEST_FILE"
    exit 1
fi

echo "✅ 测试文件存在"
echo ""

# Step 3: 检查数据
echo "📊 检查回测数据..."
FIXTURE_DIR="tests/fixtures"
mkdir -p "$FIXTURE_DIR"

DATA_FILES=(
    "btc_1h_3months.csv"
    "btc_1h_2026_q2.csv"
)

DATA_FOUND=false
for file in "${DATA_FILES[@]}"; do
    if [ -f "$FIXTURE_DIR/$file" ]; then
        echo "✅ 找到数据文件: $file"
        DATA_FOUND=true
        break
    fi
done

if [ "$DATA_FOUND" = false ]; then
    echo ""
    echo "⚠️  未找到回测数据，需要准备数据"
    echo ""
    echo "数据要求:"
    echo "  - 币种: BTC-USDT-SWAP"
    echo "  - 周期: 1H"
    echo "  - 时间: 2026-04-01 ~ 2026-06-30（3 个月）"
    echo "  - 格式: CSV (timestamp,open,high,low,close,volume)"
    echo ""
    echo "准备数据后，将文件放到: $FIXTURE_DIR/"
    echo "文件名: btc_1h_3months.csv"
    echo ""

    read -p "是否已准备好数据？(y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "请准备数据后重新运行此脚本"
        exit 0
    fi
fi

echo ""

# Step 4: 编译检查
echo "🔨 编译检查..."
if ! cargo check --tests 2>&1 | tail -5; then
    echo "❌ 编译失败，请检查代码"
    exit 1
fi

echo "✅ 编译通过"
echo ""

# Step 5: 运行测试
echo "============================================"
echo "🧪 运行探索测试"
echo "============================================"
echo ""
echo "注意: 首次运行可能需要一些时间..."
echo ""

# 移除 #[ignore] 标记（如果存在）
if grep -q "#\[ignore\]" "$TEST_FILE"; then
    echo "📝 检测到 #[ignore] 标记，建议移除后运行"
    echo ""
    read -p "是否自动移除 #[ignore]？(y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        # 备份原文件
        cp "$TEST_FILE" "${TEST_FILE}.backup"
        # 移除 #[ignore]
        sed -i.bak '/#\[ignore\]/d' "$TEST_FILE"
        echo "✅ 已移除 #[ignore] 标记（备份: ${TEST_FILE}.backup）"
    else
        echo "请手动移除 #[ignore] 标记后运行:"
        echo "  cargo test test_momentum_reversal_1h_exploration -- --nocapture"
        exit 0
    fi
fi

echo ""
echo "运行测试..."
echo ""

# 运行测试
cargo test test_momentum_reversal_1h_exploration -- --nocapture

TEST_RESULT=$?

echo ""
echo "============================================"
echo "📊 测试完成"
echo "============================================"
echo ""

if [ $TEST_RESULT -eq 0 ]; then
    echo "✅ 测试成功运行"
    echo ""
    echo "下一步:"
    echo "  1. 查看上面的回测结果"
    echo "  2. 根据决策建议判断:"
    echo "     - Win Rate > 55% → 升级到生产模式"
    echo "     - Win Rate 52-55% → 优化参数"
    echo "     - Win Rate < 52% → 记录失败，尝试新想法"
    echo ""
    echo "  3. 更新 TODO: docs/plans/TODO_momentum_reversal_1h.md"
    echo "  4. 更新探索日志: docs/exploration_log.md"
else
    echo "❌ 测试运行失败"
    echo ""
    echo "可能原因:"
    echo "  - 数据文件格式不正确"
    echo "  - 指标计算实现缺失（占位符函数）"
    echo "  - 代码逻辑错误"
    echo ""
    echo "建议:"
    echo "  1. 检查错误信息"
    echo "  2. 验证数据文件格式"
    echo "  3. 实现占位符函数（RSI/EMA/ATR）"
fi

echo ""
echo "相关文件:"
echo "  - 测试代码: $TEST_FILE"
echo "  - TODO 文档: docs/plans/TODO_momentum_reversal_1h.md"
echo "  - 探索日志: docs/exploration_log.md"
echo "  - 方法论: docs/STRATEGY_ITERATION_METHODOLOGY.md"
echo ""
