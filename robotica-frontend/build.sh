#!/bin/sh
set -e
set -x

TOP_DIR="$(dirname "$0")/.."
TOP_DIR="$(realpath "$TOP_DIR")"

DIR="$(dirname "$0")"
DIR="$(realpath "$DIR")"

cd "$TOP_DIR"
cargo build -p robotica-frontend --target wasm32-unknown-unknown

cd "$DIR"
npm install
wasm-bindgen --target bundler --out-dir pkg --omit-default-module-path "$TOP_DIR/target/wasm32-unknown-unknown/debug/robotica_frontend.wasm"
webpack
