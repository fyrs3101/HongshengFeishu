#!/usr/bin/env bash
# 鸿升飞书（Tauri）· 一键推送到 GitHub 并触发 macOS 云构建
# 用法： ./push_to_github.sh <你的GitHub仓库HTTPS地址>  [tag]
# 例：   ./push_to_github.sh https://github.com/你的名/HongshengFeishu.git
set -euo pipefail

REMOTE="${1:?用法: $0 <github仓库地址> [可选tag, 默认 v1.0.0]}"
TAG="${2:-v1.0.0}"

cd "$(dirname "$0")"

git init -q 2>/dev/null || true
git add -A
git commit -q -m "鸿升飞书 Tauri 工程" 2>/dev/null || echo "（无新改动，跳过提交）"
git branch -M main 2>/dev/null || true
git remote remove origin 2>/dev/null || true
git remote add origin "$REMOTE"
git push -u origin main

echo ""
echo "✅ 已推送。现在创建标签并触发 macOS 云构建…"
git tag -f "$TAG" 2>/dev/null || true
git push -f origin "$TAG"

echo ""
echo "================================================================"
echo " 到这里就完成了。接下来去 GitHub："
echo "   仓库页 → Actions → 等「Build macOS」跑完（约 3-5 分钟）"
echo "   → Releases → 下载「鸿升飞书_${TAG}_aarch64.dmg」和「鸿升飞书.app」"
echo "================================================================"
echo "提示：应用是 ad-hoc 签名，别人下载后需『右键 → 打开』。"
echo "      要对外无提示分发，请在仓库 Settings → Secrets 填 Developer ID 证书相关项。"
