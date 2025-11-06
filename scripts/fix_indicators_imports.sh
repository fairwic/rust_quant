#!/bin/bash
# 自动修复 indicators 包的导入路径

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PROJECT_ROOT="/Users/mac2/onions/rust_quant"
cd "$PROJECT_ROOT"

echo -e "${YELLOW}开始修复 indicators 包导入路径...${NC}"

# 批量替换导入路径
find crates/indicators/src/ -name "*.rs" -type f -exec sed -i '' \
  -e 's/use crate::CandleItem/use rust_quant_common::types::CandleItem/g' \
  -e 's/use crate::trading::indicator::rma/use crate::momentum::rsi/g' \
  -e 's/use crate::trading::indicator::ema/use crate::trend::ema/g' \
  -e 's/use crate::trading::indicator::sma/use crate::trend::sma/g' \
  {} +

echo -e "${GREEN}✓ 导入路径已批量替换${NC}"

echo -e "${YELLOW}验证编译...${NC}"
if cargo check --package rust-quant-indicators; then
    echo -e "${GREEN}✓ indicators 包编译成功！${NC}"
else
    echo -e "${YELLOW}⚠️ 仍有编译错误，需要手动调整${NC}"
    echo -e "${YELLOW}运行以下命令查看错误：${NC}"
    echo -e "cargo check --package rust-quant-indicators 2>&1 | less"
fi

