import {
  add_ships_from_log,
  export_uploaded,
  import_uploaded,
} from "./shipdb_wasm.js";
import { plur, status } from "./data.js";
import { populateAvailable, updateCounts } from "./shiplist.js";

const STORAGE_KEY = "shipdb.uploaded";
const bytesToB64 = (bytes) => {
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(bin);
};
const b64ToBytes = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

export function persistUploaded() {
  try {
    const bytes = export_uploaded();
    if (bytes.length) localStorage.setItem(STORAGE_KEY, bytesToB64(bytes));
    else localStorage.removeItem(STORAGE_KEY);
  } catch (e) {
    console.warn("Couldn't cache loaded ships", e);
  }
}

export function restoreUploaded() {
  try {
    const b64 = localStorage.getItem(STORAGE_KEY);
    if (b64) import_uploaded(b64ToBytes(b64));
  } catch (e) {
    console.warn("Couldn't restore cached ships", e);
  }
}

async function ingestFiles(files) {
  if (!files.length) return;
  let added = 0;
  for (const file of files) {
    added += add_ships_from_log(file.name, await file.bytes()).length;
  }
  if (added) persistUploaded();
  populateAvailable();
  updateCounts();
  status.textContent = `Loaded ${added} ship${
    plur(added)
  } from ${files.length} file${plur(files.length)}.`;
}

const isFileDrag = (e) =>
  e.dataTransfer && [...e.dataTransfer.types].includes("Files");

export function initStorage() {
  const logfiles = document.getElementById("logfiles");
  document.getElementById("loadbtn").addEventListener(
    "click",
    () => logfiles.click(),
  );
  logfiles.addEventListener("change", (e) => {
    ingestFiles(e.target.files);
    e.target.value = "";
  });
  document.getElementById("clearloaded").addEventListener("click", () => {
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch (e) {}
    location.reload();
  });
  window.addEventListener("dragover", (e) => {
    if (!isFileDrag(e)) return;
    e.preventDefault();
    document.body.classList.add("dragging-file");
  });
  window.addEventListener("dragleave", (e) => {
    if (!e.relatedTarget) document.body.classList.remove("dragging-file");
  });
  window.addEventListener("drop", (e) => {
    if (!isFileDrag(e)) return;
    e.preventDefault();
    document.body.classList.remove("dragging-file");
    ingestFiles(e.dataTransfer.files);
  });
}
