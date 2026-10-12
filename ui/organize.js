// Seiten organisieren: umsortieren, drehen, entfernen, PDFs zusammenfügen.
// Gespeichert wird immer als neue Datei; die Quelldateien bleiben unverändert.
import * as pdfjsLib from "./vendor/pdf.min.mjs";

pdfjsLib.GlobalWorkerOptions.workerSrc = "vendor/pdf.worker.min.mjs";

const $ = (id) => document.getElementById(id);
const api = (path, body) => window.PT.api(path, body);
const grid = $("grid");
const DOC_COLORS = ["#2f5d8a", "#b3342b", "#2e7d4f", "#8a5a2f", "#6a4c9c", "#1f7a8c"];

const S = {
  docs: [],        // {id, name, pdf, color}
  tiles: [],       // {key, doc, page (ab 0), rotate (zusätzlich, Grad)}
  sel: new Set(),  // keys
  anchor: null,
  history: [],
  nextKey: 1,
};

// ---------------------------------------------------------------------------
// Dokumente laden

async function addDocs(list) {
  for (const d of list) {
    let idx = S.docs.findIndex((x) => x.id === d.id);
    if (idx < 0) {
      const pdf = await pdfjsLib.getDocument({ url: `${window.PT.protoUrl(`doc/${d.id}/pdf`)}?v=${Date.now()}` }).promise;
      S.docs.push({ id: d.id, name: d.name, pdf, color: DOC_COLORS[S.docs.length % DOC_COLORS.length] });
      idx = S.docs.length - 1;
    }
    const n = S.docs[idx].pdf.numPages;
    for (let p = 0; p < n; p++) S.tiles.push({ key: S.nextKey++, doc: idx, page: p, rotate: 0 });
  }
  render();
}

async function pickAndAdd() {
  try {
    const r = await api("/api/organize/add");
    if (!r.docs?.length) return;
    snapshot();
    await addDocs(r.docs);
  } catch (e) {
    showToast("Konnte das PDF nicht laden: " + e.message);
  }
}

// ---------------------------------------------------------------------------
// Darstellung

const thumbCache = new Map(); // "doc:page:rot" -> canvas
let observer = null;

function render() {
  const keep = grid.scrollTop;
  grid.querySelectorAll(".tile").forEach((t) => t.remove());
  $("empty").hidden = S.tiles.length > 0;
  observer?.disconnect();
  observer = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) drawThumb(e.target);
  }, { root: grid, rootMargin: "400px 0px" });
  const multi = new Set(S.tiles.map((t) => t.doc)).size > 1 || S.docs.length > 1;
  S.tiles.forEach((t, i) => {
    const d = S.docs[t.doc];
    const el = document.createElement("div");
    el.className = "tile" + (S.sel.has(t.key) ? " sel" : "");
    el.dataset.key = t.key;
    el.style.setProperty("--dc", d.color);
    el.innerHTML = `<span class="num">${i + 1}</span>
      <div class="tools"><button class="rot" title="Drehen">↻</button><button class="del" title="Entfernen">×</button></div>
      <div class="box"><div class="ph"></div></div>
      <div class="lab">${multi ? `<b>${escapeHtml(shortName(d.name))}</b> · ` : ""}S. ${t.page + 1}${t.rotate ? ` · ${t.rotate}°` : ""}</div>`;
    el.querySelector(".rot").addEventListener("mousedown", (e) => e.stopPropagation());
    el.querySelector(".del").addEventListener("mousedown", (e) => e.stopPropagation());
    el.querySelector(".rot").addEventListener("click", (e) => { e.stopPropagation(); rotate([t.key], 90); });
    el.querySelector(".del").addEventListener("click", (e) => { e.stopPropagation(); removeTiles([t.key]); });
    grid.append(el);
    observer.observe(el);
  });
  grid.scrollTop = keep;
  updateBar();
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
const shortName = (n) => String(n).replace(/\.pdf$/i, "").slice(0, 24);

async function drawThumb(el) {
  const t = S.tiles.find((x) => x.key === +el.dataset.key);
  if (!t || el.dataset.drawn === "1") return;
  el.dataset.drawn = "1";
  const d = S.docs[t.doc];
  const ck = `${t.doc}:${t.page}:${t.rotate}`;
  let canvas = thumbCache.get(ck);
  if (!canvas) {
    try {
      const page = await d.pdf.getPage(t.page + 1);
      const rot = (page.rotate + t.rotate) % 360;
      const vp1 = page.getViewport({ scale: 1, rotation: rot });
      const scale = 300 / Math.max(vp1.width, vp1.height);
      const vp = page.getViewport({ scale, rotation: rot });
      canvas = document.createElement("canvas");
      canvas.width = Math.floor(vp.width);
      canvas.height = Math.floor(vp.height);
      await page.render({ canvasContext: canvas.getContext("2d"), viewport: vp }).promise;
      thumbCache.set(ck, canvas);
    } catch {
      el.dataset.drawn = "";
      return;
    }
  }
  const box = el.querySelector(".box");
  if (!box) return;
  const c = document.createElement("canvas");
  c.width = canvas.width;
  c.height = canvas.height;
  c.getContext("2d").drawImage(canvas, 0, 0);
  box.replaceChildren(c);
}

function updateBar() {
  const n = S.tiles.length;
  const docs = new Set(S.tiles.map((t) => t.doc)).size;
  $("ogInfo").textContent = n ? `${n} ${n === 1 ? "Seite" : "Seiten"}${docs > 1 ? ` aus ${docs} Dokumenten` : ""}${S.sel.size ? ` · ${S.sel.size} ausgewählt` : ""}` : "";
  const has = S.sel.size > 0;
  for (const id of ["rotLeftBtn", "rotRightBtn", "delBtn"]) $(id).disabled = !has;
  $("undoBtn").disabled = !S.history.length;
  $("saveBtn").disabled = !n;
}

// ---------------------------------------------------------------------------
// Änderungen (mit Rückgängig)

function snapshot() {
  S.history.push(S.tiles.map((t) => ({ ...t })));
  if (S.history.length > 100) S.history.shift();
}

function undo() {
  const prev = S.history.pop();
  if (!prev) return;
  $("toast").hidden = true;
  S.tiles = prev;
  S.sel = new Set([...S.sel].filter((k) => S.tiles.some((t) => t.key === k)));
  render();
}

function rotate(keys, deg) {
  if (!keys.length) return;
  snapshot();
  for (const t of S.tiles) if (keys.includes(t.key)) t.rotate = (t.rotate + deg + 360) % 360;
  render();
}

function removeTiles(keys) {
  if (!keys.length) return;
  snapshot();
  S.tiles = S.tiles.filter((t) => !keys.includes(t.key));
  keys.forEach((k) => S.sel.delete(k));
  render();
  showToast(`${keys.length} ${keys.length === 1 ? "Seite" : "Seiten"} entfernt.`, "Rückgängig", undo);
}

// ausgewählte Kacheln an Position `index` (Index vor dem Verschieben) verschieben
function moveTo(keys, index) {
  const moving = S.tiles.filter((t) => keys.includes(t.key));
  if (!moving.length) return;
  const before = S.tiles.slice(0, index).filter((t) => !keys.includes(t.key)).length;
  const rest = S.tiles.filter((t) => !keys.includes(t.key));
  const next = [...rest.slice(0, before), ...moving, ...rest.slice(before)];
  if (next.every((t, i) => t === S.tiles[i])) return;
  snapshot();
  S.tiles = next;
  render();
}

// ---------------------------------------------------------------------------
// Auswahl und Ziehen (mit Mausereignissen, unabhängig vom HTML5-Drag&Drop)

let drag = null; // {key, x, y, active, ghost, index}

grid.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  const tile = e.target.closest(".tile");
  if (!tile) return;
  e.preventDefault();
  drag = { key: +tile.dataset.key, x: e.clientX, y: e.clientY, active: false, index: null, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey };
});

document.addEventListener("mousemove", (e) => {
  if (!drag) return;
  if (!drag.active) {
    if (Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 6) return;
    drag.active = true;
    if (!S.sel.has(drag.key)) { S.sel = new Set([drag.key]); S.anchor = drag.key; }
    grid.querySelectorAll(".tile").forEach((t) => { if (S.sel.has(+t.dataset.key)) t.classList.add("dragging", "sel"); else t.classList.remove("sel"); });
    const src = grid.querySelector(`.tile[data-key="${drag.key}"] canvas`);
    const g = document.createElement("div");
    g.className = "ghost";
    if (src) { const c = src.cloneNode(); c.getContext("2d").drawImage(src, 0, 0); c.style.maxWidth = "110px"; c.style.maxHeight = "110px"; g.append(c); }
    if (S.sel.size > 1) { const b = document.createElement("span"); b.className = "count"; b.textContent = S.sel.size; g.append(b); }
    document.body.append(g);
    drag.ghost = g;
  }
  drag.ghost.style.left = `${e.clientX + 8}px`;
  drag.ghost.style.top = `${e.clientY + 8}px`;
  showMarker(e.clientX, e.clientY);
  autoScroll(e.clientY);
});

document.addEventListener("mouseup", (e) => {
  if (!drag) return;
  const d = drag;
  drag = null;
  $("marker").hidden = true;
  d.ghost?.remove();
  if (d.active) {
    grid.querySelectorAll(".tile.dragging").forEach((t) => t.classList.remove("dragging"));
    if (d.index != null) moveTo([...S.sel], d.index);
    else render();
    return;
  }
  // Klick: Auswahl
  if (d.shift && S.anchor != null) {
    const a = S.tiles.findIndex((t) => t.key === S.anchor), b = S.tiles.findIndex((t) => t.key === d.key);
    const [lo, hi] = a < b ? [a, b] : [b, a];
    if (!d.ctrl) S.sel.clear();
    S.tiles.slice(lo, hi + 1).forEach((t) => S.sel.add(t.key));
  } else if (d.ctrl) {
    S.sel.has(d.key) ? S.sel.delete(d.key) : S.sel.add(d.key);
    S.anchor = d.key;
  } else {
    S.sel = new Set([d.key]);
    S.anchor = d.key;
  }
  grid.querySelectorAll(".tile").forEach((t) => t.classList.toggle("sel", S.sel.has(+t.dataset.key)));
  updateBar();
});

// Klick auf freie Fläche hebt die Auswahl auf
grid.addEventListener("click", (e) => {
  if (e.target === grid) { S.sel.clear(); grid.querySelectorAll(".tile.sel").forEach((t) => t.classList.remove("sel")); updateBar(); }
});

// Einfügeposition zur Mausposition bestimmen und anzeigen
function showMarker(x, y) {
  const tiles = [...grid.querySelectorAll(".tile")];
  const m = $("marker");
  if (!tiles.length) { m.hidden = true; drag.index = null; return; }
  let best = null, bd = Infinity;
  tiles.forEach((t, i) => {
    const r = t.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const dist = Math.abs(cy - y) * 3 + Math.abs(cx - x);
    if (dist < bd) { bd = dist; best = { i, r, after: x > cx }; }
  });
  drag.index = best.i + (best.after ? 1 : 0);
  const gx = best.after ? best.r.right + 7 : best.r.left - 11;
  Object.assign(m.style, { left: `${gx}px`, top: `${best.r.top + 6}px`, height: `${best.r.height - 12}px` });
  m.hidden = false;
}

let scrollTimer = 0;
function autoScroll(y) {
  const r = grid.getBoundingClientRect();
  clearInterval(scrollTimer);
  const dir = y < r.top + 50 ? -1 : y > r.bottom - 50 ? 1 : 0;
  if (dir) scrollTimer = setInterval(() => { if (!drag) return clearInterval(scrollTimer); grid.scrollTop += dir * 18; }, 30);
}

// ---------------------------------------------------------------------------
// Speichern

async function save() {
  if (!S.tiles.length) return;
  const used = [...new Set(S.tiles.map((t) => t.doc))];
  const sources = used.map((i) => S.docs[i].id);
  const pages = S.tiles.map((t) => ({ src: used.indexOf(t.doc), page: t.page, rotate: t.rotate }));
  const base = used.length === 1 ? `${S.docs[used[0]].name.replace(/\.pdf$/i, "")} (bearbeitet).pdf` : "Zusammengefügt.pdf";
  $("saveBtn").disabled = true;
  try {
    const r = await api("/api/organize/save", { sources, pages, name: base });
    if (r.cancelled) return;
    showToast(`Gespeichert: ${r.path} (${r.pages} Seiten)`, "Im Reader öffnen", () => api("/api/window/open", { url: `reader.html?id=${r.id}` }));
  } catch (e) {
    showToast(e.message);
  } finally {
    updateBar();
  }
}

// ---------------------------------------------------------------------------
// Bedienelemente

let toastTimer = 0;
function showToast(text, action, fn) {
  const t = $("toast");
  t.querySelector(".txt").textContent = text;
  const b = t.querySelector(".act");
  b.hidden = !action;
  b.textContent = action || "";
  b.onclick = () => { t.hidden = true; fn && fn(); };
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), action ? 9000 : 5000);
}

$("addBtn").onclick = pickAndAdd;
$("addBtn2").onclick = pickAndAdd;
$("rotLeftBtn").onclick = () => rotate([...S.sel], -90);
$("rotRightBtn").onclick = () => rotate([...S.sel], 90);
$("delBtn").onclick = () => removeTiles([...S.sel]);
$("undoBtn").onclick = undo;
$("saveBtn").onclick = save;

document.addEventListener("keydown", (e) => {
  if (e.target.closest && e.target.closest("input, textarea")) return;
  const k = e.key.toLowerCase();
  if ((e.ctrlKey || e.metaKey) && k === "z") { e.preventDefault(); undo(); }
  else if ((e.ctrlKey || e.metaKey) && k === "a") { e.preventDefault(); S.sel = new Set(S.tiles.map((t) => t.key)); render(); }
  else if (k === "delete" || k === "backspace") { e.preventDefault(); removeTiles([...S.sel]); }
  else if (k === "r" && !e.ctrlKey) rotate([...S.sel], e.shiftKey ? -90 : 90);
  else if (k === "escape") { S.sel.clear(); render(); }
});

async function init() {
  window.PT.setTitle("Seiten organisieren – PDF-Tutor");
  const id = new URLSearchParams(location.search).get("id");
  if (id) {
    const info = await api(`/api/doc/${id}/info`);
    window.PT.setTitle(`Seiten organisieren: ${info.title} – PDF-Tutor`);
    await addDocs([{ id, name: info.file }]);
  } else {
    render();
  }
}

init().catch((e) => showToast("Fehler beim Laden: " + e.message));

// für automatische Tests (ohne Dateidialog)
window.__organizer = { S, add: async (paths) => addDocs((await api("/api/organize/add", { paths })).docs) };
