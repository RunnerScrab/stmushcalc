import {
  render_cumulative,
  render_instantaneous,
  ship_art,
  simulate_summary,
} from "./shipdb_wasm.js";
import { plots, plur, status } from "./data.js";
import { currentNames, parseTable, withTiming } from "./tuning.js";

function createPanel(title, wide = false) {
  const panel = document.createElement("div");
  panel.className = wide ? "panel panel-wide" : "panel";
  panel.draggable = true;

  const header = document.createElement("button");
  header.type = "button";
  header.className = "panel-header";
  header.setAttribute("aria-expanded", "true");
  const caret = document.createElement("span");
  caret.className = "caret";
  caret.setAttribute("aria-hidden", "true");
  caret.textContent = "▾";
  header.appendChild(caret);
  header.appendChild(document.createTextNode(" " + title));
  header.addEventListener("click", () => {
    const collapsed = panel.classList.toggle("collapsed");
    header.setAttribute("aria-expanded", String(!collapsed));
  });
  panel.appendChild(header);

  const body = document.createElement("div");
  body.className = "panel-body";
  panel.appendChild(body);

  panel.addEventListener("dragstart", () => panel.classList.add("dragging"));
  panel.addEventListener("dragend", () => panel.classList.remove("dragging"));

  return { panel, body };
}

const plotObserver = new IntersectionObserver((entries) => {
  for (const e of entries) {
    e.target._visible = e.isIntersecting;
    if (e.isIntersecting && e.target._dirty) e.target._draw();
  }
}, { rootMargin: "300px" });

function addResizablePlot(parent, render, label, variant) {
  const area = document.createElement("div");
  area.className = variant ? `plot-area ${variant}` : "plot-area";
  if (label) {
    area.setAttribute("role", "img");
    area.setAttribute("aria-label", label);
  }
  parent.appendChild(area);

  let lastW = -1, lastH = -1;
  area._dirty = true;
  area._visible = false;
  area._draw = () => {
    if (!area.isConnected || area.clientWidth === 0) return;
    const w = Math.max(200, Math.floor(area.clientWidth));
    const h = Math.max(150, Math.floor(area.clientHeight));
    lastW = w;
    lastH = h;
    area._dirty = false;
    const bg = getComputedStyle(area.closest(".panel")).backgroundColor;
    area.innerHTML = render(w, h, bg);
  };
  area._redraw = () => {
    area._dirty = true;
    if (area._visible) area._draw();
  };
  new ResizeObserver(() => {
    if (!area.isConnected || area.clientWidth === 0) return;
    const w = Math.max(200, Math.floor(area.clientWidth));
    const h = Math.max(150, Math.floor(area.clientHeight));
    if (w === lastW && h === lastH) return;
    area._dirty = true;
    if (area._visible) area._draw();
  }).observe(area);
  plotObserver.observe(area);
}

function closestPanel(x, y) {
  let best = null;
  let bestDist = Infinity;
  for (const el of plots.querySelectorAll(".panel:not(.dragging)")) {
    const box = el.getBoundingClientRect();
    const cx = box.left + box.width / 2;
    const cy = box.top + box.height / 2;
    const dist = (x - cx) ** 2 + (y - cy) ** 2;
    if (dist < bestDist) {
      bestDist = dist;
      best = { el, before: x < cx };
    }
  }
  return best;
}

let dropTarget = null;

export function initPanels() {
  plots.addEventListener("dragover", (e) => {
    e.preventDefault();
    if (dropTarget) {
      dropTarget.el.classList.remove("panel-drop-before", "panel-drop-after");
    }
    dropTarget = closestPanel(e.clientX, e.clientY);
    if (dropTarget) {
      dropTarget.el.classList.add(
        dropTarget.before ? "panel-drop-before" : "panel-drop-after",
      );
    }
  });
  plots.addEventListener("dragleave", (e) => {
    if (!plots.contains(e.relatedTarget) && dropTarget) {
      dropTarget.el.classList.remove("panel-drop-before", "panel-drop-after");
      dropTarget = null;
    }
  });
  plots.addEventListener("drop", (e) => {
    e.preventDefault();
    const dragging = plots.querySelector(".panel.dragging");
    if (dragging && dropTarget) {
      dropTarget.el.classList.remove("panel-drop-before", "panel-drop-after");
      plots.insertBefore(
        dragging,
        dropTarget.before ? dropTarget.el : dropTarget.el.nextSibling,
      );
    }
    dropTarget = null;
  });
}

function buildShipBody(body, name) {
  const art = ship_art(name);
  if (art) {
    const pre = document.createElement("pre");
    pre.className = "art";
    pre.textContent = art;
    pre.setAttribute("aria-hidden", "true");
    pre.setAttribute("role", "img");
    pre.setAttribute("aria-label", "");
    body.appendChild(pre);
  }

  const parsed = parseTable(name);

  addTables(
    body,
    collapsible("General", parsed.querySelector(".ship-general")),
  );
  addTables(body, parsed.querySelector(".weapons-summary"));
  addResizablePlot(
    body,
    (w, h, bg) => render_instantaneous(name, ...withTiming(), bg, w, h),
    `Damage-per-second chart for ${name}. Data in the tables below.`,
    "plot-inst",
  );

  const summary = document.createElement("p");
  summary.className = "summary";
  summary.innerHTML = simulate_summary(name, ...withTiming());
  body.appendChild(summary);

  addTables(
    body,
    collapsible(
      "Beams and Missiles",
      parsed.querySelector(".weapons-cols"),
      false,
    ),
  );
}

// Full rebuild: use when the set of selected ships changes
export function run() {
  const names = currentNames();
  plots.innerHTML = "";
  if (names.length === 0) {
    status.textContent = "No ships selected.";
    return;
  }

  for (const name of names) {
    const { panel, body } = createPanel(name);
    panel.dataset.ship = name;
    plots.appendChild(panel);
    buildShipBody(body, name);
  }

  if (names.length > 1) {
    const { panel, body } = createPanel("Cumulative Damage Outputs", true);
    plots.appendChild(panel);
    addResizablePlot(
      body,
      (w, h, bg) =>
        render_cumulative(currentNames(), ...withTiming(), bg, w, h),
      "Cumulative damage chart. Data in the tables below.",
      "plot-cum",
    );
  }

  colorRelative();
  status.textContent = `Showing ${names.length} ship${plur(names.length)}: ${
    names.join(", ")
  }.`;
}

// In-place update: use when only tuning/timing changes, so the user's dragged
// panel order, sizes, and collapse state survive
export function refresh() {
  const parts = [];
  for (const panel of plots.querySelectorAll(".panel[data-ship]")) {
    const name = panel.dataset.ship;
    panel.querySelector(".summary").innerHTML = simulate_summary(
      name,
      ...withTiming(),
    );
    const parsed = parseTable(name);
    for (const sel of [".ship-general", ".weapons-summary", ".weapons-cols"]) {
      panel.querySelector(sel).replaceWith(parsed.querySelector(sel));
    }
    const dpsEl = panel.querySelector('[data-stat="sustained_dps"]');
    if (dpsEl) parts.push(`${name} ${Math.round(dpsEl.dataset.val)}`);
  }
  for (const area of plots.querySelectorAll(".plot-area")) {
    area._redraw?.();
  }
  colorRelative();
  if (parts.length) status.textContent = `DPS: ${parts.join(", ")}.`;
}

function collapsible(title, contentEl, open = true) {
  const d = document.createElement("details");
  d.open = open;
  d.className = "sec";
  const s = document.createElement("summary");
  s.textContent = title;
  d.append(s, contentEl);
  return d;
}

function addTables(body, content) {
  const wrap = document.createElement("div");
  wrap.className = "ship-tables";
  wrap.appendChild(content);
  body.appendChild(wrap);
}

function colorRelative() {
  const byStat = {};
  for (const el of plots.querySelectorAll("[data-stat]")) {
    const val = parseFloat(el.dataset.val);
    if (!isFinite(val)) continue;
    (byStat[el.dataset.stat] ||= []).push({ el, val });
  }
  for (const key in byStat) {
    const items = byStat[key];
    if (items.length < 2) continue;
    const avg = items.reduce((s, x) => s + x.val, 0) / items.length;
    const eps = 1e-9 * Math.max(1, Math.abs(avg));
    for (const { el, val } of items) {
      el.classList.remove("stat-above", "stat-below");
      el.querySelectorAll(".cmp-marker").forEach((m) => m.remove());
      if (Math.abs(val - avg) <= eps) continue;
      const better = el.dataset.invert === "1" ? val < avg : val > avg;
      el.classList.add(better ? "stat-above" : "stat-below");
      const glyph = document.createElement("span");
      glyph.className = "cmp-marker";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = better ? " ▲" : " ▼";
      const sr = document.createElement("span");
      sr.className = "cmp-marker sr-only";
      sr.textContent = better ? " better" : " worse";
      el.append(glyph, sr);
    }
  }
}
