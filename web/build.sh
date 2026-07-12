#!/usr/bin/env bash
# ichigojam-web を wasm へビルドし、ES モジュール (--target web) のグルーを
# pkg/ へ生成する。生成物は demo/index.html や React ラッパからそのまま import
# できる。
#
# 必要ツール:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.121   # Cargo.lock と同一版
set -euo pipefail

cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"
WASM="$ROOT/target/wasm32-unknown-unknown/release/ichigojam_web.wasm"

cargo build --release --target wasm32-unknown-unknown -p ichigojam-web
wasm-bindgen "$WASM" --target web --out-dir pkg --no-typescript

# 任意: wasm-opt があればサイズ最適化 (なければスキップ)。
if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz pkg/ichigojam_web_bg.wasm -o pkg/ichigojam_web_bg.wasm
fi

echo "built -> $(cd pkg && pwd)"
