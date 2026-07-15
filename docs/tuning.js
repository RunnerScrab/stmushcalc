import { ship_table } from "./shipdb_wasm.js";
import { selected } from "./data.js";

export const TUNING = ["eng", "tac", "helm", "oper", "sci", "dam", "wis"];
export const inputs = TUNING.map((id) => document.getElementById(id));
export const timingEl = document.getElementById("timing");
export const currentArgs = () => inputs.map((el) => Number(el.value));
export const currentTiming = () => timingEl.value;
export const withTiming = () => [...currentArgs(), currentTiming()];
export const currentNames = () =>
  Array.from(selected.options).map((o) => o.value);
export const parseTable = (name) => {
  const div = document.createElement("div");
  div.innerHTML = ship_table(name, ...currentArgs());
  return div;
};
