#!/bin/sh
# Builds the folder site/dist, which is ready to publish.
# Needs cargo with the target wasm32-unknown-unknown.
# Needs score-map.svg and score.json, which make_map.py writes. README.md has the commands.
set -eu
cd "$(dirname "$0")"
cargo build --manifest-path wasm/Cargo.toml --release --locked --target wasm32-unknown-unknown
rm -rf dist
mkdir dist
cp index.html site.css site.js score-map.svg score.json dist/
cp wasm/target/wasm32-unknown-unknown/release/lekha_wasm.wasm dist/lekha.wasm
