import { list_ships, ship_counts } from "../pkg/shipdb_wasm.js";
import { available, selected, shipCounts } from "./data.js";

export function populateAvailable(names = list_ships()) {
  available.innerHTML = "";
  for (const name of names) {
    const opt = document.createElement("option");
    opt.textContent = name;
    available.appendChild(opt);
  }
}

export function updateCounts() {
  const [builtin, local] = ship_counts();
  shipCounts.textContent =
    `${builtin} ships built-in - ${local} ships cached locally`;
}

export function addShip(name) {
  if (Array.from(selected.options).some((o) => o.value === name)) return;
  const opt = document.createElement("option");
  opt.textContent = name;
  selected.appendChild(opt);
}

export function initShipList(onChange) {
  const addSelected = () => {
    for (const opt of Array.from(available.selectedOptions)) addShip(opt.value);
    onChange();
  };
  const removeSelected = () => {
    for (const opt of Array.from(selected.selectedOptions)) opt.remove();
    onChange();
  };
  available.addEventListener("dblclick", addSelected);
  selected.addEventListener("dblclick", removeSelected);
  available.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addSelected();
    }
  });
  selected.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      removeSelected();
    }
  });
  document.getElementById("add").addEventListener("click", addSelected);
  document.getElementById("remove").addEventListener("click", removeSelected);
}
