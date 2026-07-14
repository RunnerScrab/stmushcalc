#!/usr/bin/env bash

set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

( cd shipdb-wasm && wasm-pack build --target web --release )

cp shipdb-wasm/pkg/shipdb_wasm.js \
   shipdb-wasm/pkg/shipdb_wasm_bg.wasm \
   shipdb-wasm/pkg/shipdb_wasm.d.ts \
   shipdb-wasm/pkg/shipdb_wasm_bg.wasm.d.ts \
   docs/
cp shipdb-wasm/www/favicon.ico docs/favicon.ico

sed 's#\.\./pkg/#./#' shipdb-wasm/www/index.html > docs/index.html

echo "Built wasm and synced docs/"

