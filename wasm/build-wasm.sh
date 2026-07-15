#!/usr/bin/env bash

# Rebuild the WASM and copy it into ../docs/ 
set -euo pipefail

cd "$(dirname "$0")"                     # shipdb-wasm/
ROOT=".."
WASM="$ROOT/target/wasm32-unknown-unknown/release/shipdb_wasm.wasm"
DOCS="$ROOT/docs"

want=$(grep -A1 'name = "wasm-bindgen"' "$ROOT/Cargo.lock" | grep -m1 version \
       | sed 's/.*"\(.*\)".*/\1/')

wasm_bindgen=$(command -v wasm-bindgen || echo "${CARGO_HOME:-$HOME/.cargo}/bin/wasm-bindgen")

if [ ! -x "$wasm_bindgen" ]; then
  echo "Try cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi

have=$("$wasm_bindgen" --version | awk '{print $2}')
if [ "$have" != "$want" ]; then
  echo "Try cargo install wasm-bindgen-cli --version $want" >&2
fi

# --- build ----------------------------------------------------------------
echo "==> cargo build (release, wasm32)"
cargo build --release --target wasm32-unknown-unknown

echo "==> wasm-bindgen -> pkg/"
"$wasm_bindgen" "$WASM" --out-dir pkg --target web

echo "==> syncing to $DOCS/"
cp pkg/shipdb_wasm.js pkg/shipdb_wasm_bg.wasm \
   pkg/shipdb_wasm.d.ts pkg/shipdb_wasm_bg.wasm.d.ts "$DOCS/"
echo "==> done. pkg/shipdb_wasm_bg.wasm: $(du -h pkg/shipdb_wasm_bg.wasm | cut -f1)"

