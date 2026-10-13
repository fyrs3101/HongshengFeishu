#!/usr/bin/env bash
# 鸿升飞书（Tauri）本地打包脚本
# 适用：macOS（需已安装 Rust + Xcode Command Line Tools）
# 用法：
#   ./build.sh            # release 构建，产出 .app 与 .dmg
#   ./build.sh debug      # debug 构建
#   ./build.sh run        # 直接运行（不打包）
set -euo pipefail

cd "$(dirname "$0")/src-tauri"

MODE="${1:-release}"
if ! command -v cargo >/dev/null 2>&1; then
  echo "未找到 cargo，请先安装 Rust：https://rustup.rs"
  exit 1
fi
if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "未找到 cargo-tauri，正在安装 tauri-cli ..."
  cargo install tauri-cli --version "^2" --locked
fi

case "$MODE" in
  debug) cargo tauri build --debug ;;
  run)   cargo tauri dev ;;
  *)     cargo tauri build ;;
esac

echo ""
echo "构建完成，产物位于："
find target -maxdepth 4 \( -name "*.dmg" -o -name "*.app" \) 2>/dev/null | sed 's/^/  /' || true
