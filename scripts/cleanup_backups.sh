#!/bin/bash
# 清理备份文件（可选）

echo "=== 清理备份文件 ==="
echo ""
echo "警告：此操作将删除所有 .bak* 备份文件"
echo "如果不确定，请先备份整个项目"
echo ""
read -p "确定要继续吗? (y/N) " -n 1 -r
echo ""

if [[ $REPLY =~ ^[Yy]$ ]]
then
    echo "正在清理..."
    find crates -name "*.bak" -type f -delete
    find crates -name "*.bak2" -type f -delete
    find crates -name "*.bak3" -type f -delete
    echo "✅ 清理完成！"
else
    echo "❌ 已取消"
fi
