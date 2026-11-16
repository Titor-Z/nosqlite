#!/usr/bin/env bash
# 构建 nodblite-studio 单二进制。
#
# 顺序不能颠倒:wasm 引擎 -> 前端 bundle -> 内嵌两者的二进制。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "[1/4] 构建 WASM 引擎 (wasm32-wasip1)"
cargo build --release --target wasm32-wasip1 --lib
mkdir -p studio-ui/public
cp target/wasm32-wasip1/release/nodblite.wasm studio-ui/public/nodblite.wasm

echo "[2/4] 安装前端依赖"
(cd studio-ui && bun install --frozen-lockfile)

echo "[3/4] 构建前端 (Svelte + Tailwind + Vite)"
(cd studio-ui && bun run build)

echo "[4/4] 编译单二进制"
cargo build --release --features studio --bin nodblite-studio

echo
echo "完成: target/release/nodblite-studio"
ls -lh target/release/nodblite-studio
