#!/bin/sh
# Build the browser lesson engine: the arct lessons compiled to
# WebAssembly, with nothing saved and nothing sent.
#
# Writes dist-web/arct_web.wasm, dist-web/arct-web.js (the loader, no
# dependencies) and dist-web/demo.html (a plain test page that runs
# Explorer lesson 1). Needs the wasm32-unknown-unknown target:
#   rustup target add wasm32-unknown-unknown
# Try it: python3 -m http.server -d dist-web 8000, then open
# http://localhost:8000/demo.html (browsers won't load .wasm from file://).
set -eu

cd "$(dirname "$0")/.."

cargo build --locked --profile web -p arct-web --target wasm32-unknown-unknown

mkdir -p dist-web
cp target/wasm32-unknown-unknown/web/arct_web.wasm dist-web/arct_web.wasm
cp crates/arct-web/web/arct-web.js crates/arct-web/web/demo.html dist-web/

# wasm-opt (from binaryen) shrinks it further when it's installed
if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -Oz dist-web/arct_web.wasm -o dist-web/arct_web.wasm
fi

bytes=$(wc -c < dist-web/arct_web.wasm | tr -d ' ')
gzipped=$(gzip -9 -c dist-web/arct_web.wasm | wc -c | tr -d ' ')
echo "dist-web/arct_web.wasm: $bytes bytes ($gzipped gzipped)"
