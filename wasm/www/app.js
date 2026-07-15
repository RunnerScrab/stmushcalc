import init from "../pkg/shipdb_wasm.js";
import { inputs, timingEl } from "./tuning.js";
import { initPanels, refresh, run } from "./panels.js";
import { initShipList, populateAvailable, updateCounts } from "./shiplist.js";
import { initStorage, restoreUploaded } from "./storage.js";
import { applyStateFromUrl, updateUrl } from "./url.js";
import { initDock } from "./dock.js";

await init();

const onSelectionChange = () => {
  run();
  updateUrl();
};

restoreUploaded();
populateAvailable();
updateCounts();

initStorage();
initShipList(onSelectionChange);
initPanels();
initDock();

for (const el of [...inputs, timingEl]) {
  el.addEventListener("change", () => {
    refresh();
    updateUrl();
  });
}

applyStateFromUrl();
run();

