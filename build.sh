#!/usr/bin/env bash

set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

( cd wasm && wasm-pack build --target web --release )

cp wasm/pkg/shipdb_wasm.js \
   wasm/pkg/shipdb_wasm_bg.wasm \
   wasm/pkg/shipdb_wasm.d.ts \
   wasm/pkg/shipdb_wasm_bg.wasm.d.ts \
   docs/
cp wasm/www/favicon.ico docs/favicon.ico
cp wasm/www/largecombadge.png docs/largecombadge.png

for f in index.html styles.css app.js data.js tuning.js shiplist.js storage.js panels.js url.js dock.js; do
  sed 's#\.\./pkg/#./#' "wasm/www/$f" > "docs/$f"
done

echo "Built wasm and synced docs/"

