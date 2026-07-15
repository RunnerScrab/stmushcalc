import { list_ships } from "../pkg/shipdb_wasm.js";
import {
  currentArgs,
  currentNames,
  currentTiming,
  inputs,
  timingEl,
  TUNING,
} from "./tuning.js";
import { addShip } from "./shiplist.js";

// Load state from URL string
export function applyStateFromUrl() {
  const p = new URLSearchParams(location.search);
  TUNING.forEach((id, i) => {
    if (p.has(id)) inputs[i].value = p.get(id);
  });
  if (
    p.has("turning") &&
    [...timingEl.options].some((o) => o.value === p.get("turning"))
  ) {
    timingEl.value = p.get("turning");
  }
  const validNames = new Set(list_ships());
  for (const name of p.getAll("s")) {
    if (validNames.has(name)) addShip(name);
  }
}

// Reflect the current state into the URL
export function updateUrl() {
  const p = new URLSearchParams();
  for (const name of currentNames()) p.append("s", name);
  currentArgs().forEach((v, i) => p.set(TUNING[i], v));
  p.set("turning", currentTiming());
  history.replaceState(null, "", location.pathname + "?" + p.toString());
}
