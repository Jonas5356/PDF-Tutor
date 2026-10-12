import * as pdfjsLib from "./vendor/pdf.min.mjs";
import { esc, renderMD } from "./render.js";

const tr = window.I18N.t;

pdfjsLib.GlobalWorkerOptions.workerSrc = "vendor/pdf.worker.min.mjs";

const $ = (id) => document.getElementById(id);
const scroller = $("scroller");
const pagesEl = $("pages");
const notesEl = $("notes");

const S = {
  info: null,
  pdf: null,
  pages: [],        // {num, div, hl, text, w0, h0, pdfPage, rendered, renderId, task, visible}
  labels: null,
  scale: 1.3,
  model: "sonnet",
  notes: [],
  draft: null,      // {page, rects, text}
  activeId: null,
  pendingSel: null, // Auswahl, für die die Leiste angezeigt wird
  menuId: null,     // Notiz, deren Menü offen ist
  compose: null,    // {id, mode}: Eingabefeld einer Notiz für Kommentar ("comment") oder Frage ("ask")
  tool: null,       // "sticky": nächster Klick setzt eine Haftnotiz
  undo: null,       // Beschreibung des Schritts, den Strg+Z zurücknimmt
  effort: "",       // Denktiefe für neue Fragen ("" = Voreinstellung des Modells)
  find: { q: "", matches: [], cur: -1, texts: new Map(), run: 0 },
  sidebar: { open: false, tab: "toc", outline: null, thumbsBuilt: false },
};

// ---------------------------------------------------------------------------
// Hilfsfunktionen
// ---------------------------------------------------------------------------

const DOC_ID = new URLSearchParams(location.search).get("id") || "";
const DOC_BASE = `/api/doc/${DOC_ID}`;

function api(path, body) {
  if (path.startsWith("/api/")) path = DOC_BASE + path.slice(4);
  return window.PT.api(path, body);
}

const pdfUrl = () => `${window.PT.protoUrl(`doc/${DOC_ID}/pdf`)}?v=${Date.now()}`;

function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}

const saveState = debounce((obj) => api("/api/state", obj).catch(() => {}), 800);

function pageLabel(num) {
  const l = S.labels && S.labels[num - 1];
  return l && l !== String(num) ? l : String(num);
}

// Kurzname des Modells für Anzeigen ("Opus (gründlicher)" -> "Opus")
function modelLabel(id) {
  const m = (S.info?.models || []).find((m) => m.id === id);
  return m ? m.label.split(/\s[(–]/)[0] : id;
}

const EFFORT_NAMES = { low: "low", medium: "medium", high: "high", xhigh: "very high", max: "maximum" };
// Werte aus dem PDF (z. B. „Effort“) nur über eigene Schlüssel nachschlagen
const own = (obj, k) => (typeof k === "string" && Object.prototype.hasOwnProperty.call(obj, k) ? obj[k] : undefined);
const effortLabel = (e) => (own(EFFORT_NAMES, e) ? tr("depth {level}", { level: tr(EFFORT_NAMES[e]) }) : "");
const effortShort = (e) => (own(EFFORT_NAMES, e) ? tr(EFFORT_NAMES[e]) : tr("default"));
const isExtraModel = (id) => !!(S.info?.models || []).find((m) => m.id === id)?.extra;

// Modelle, die Zusatzguthaben kosten können, farbig kennzeichnen
function markModelSelect() {
  const sel = $("modelSelect");
  const extra = isExtraModel(S.model);
  sel.classList.toggle("extra", extra);
  sel.title = extra
    ? tr("This model may be billed via extra usage credits, depending on your plan.")
    : tr("Model for new questions");
  // Knopf in der Werkzeugleiste: Modell · Denktiefe
  const btn = $("claudeBtn");
  $("claudeBtnLbl").textContent = `${extra ? "⚠ " : ""}${modelLabel(S.model)} · ${effortShort(S.effort)}`;
  btn.classList.toggle("extra", extra);
  if (extra) btn.title = tr("This model may be billed via extra usage credits, depending on your plan.");
  else btn.title = tr("Settings for questions to Claude: context, model, reasoning depth");
}

// Einstellungen für Fragen (Kontext, Modell, Denktiefe) in einem kleinen Fenster
const claudePanel = $("claudePanel");
$("claudeBtn").onclick = (e) => {
  e.stopPropagation();
  if (!claudePanel.hidden) { claudePanel.hidden = true; return; }
  claudePanel.hidden = false;
  const r = $("claudeBtn").getBoundingClientRect();
  claudePanel.style.top = `${r.bottom + 6}px`;
  claudePanel.style.right = "auto";
  claudePanel.style.left = `${Math.max(8, Math.min(window.innerWidth - claudePanel.offsetWidth - 8, r.right - claudePanel.offsetWidth))}px`;
};
document.addEventListener("click", (e) => {
  if (!e.target.closest("#claudePanel") && !e.target.closest("#extraDlg") && !$("extraDlg").open) claudePanel.hidden = true;
});

// ---------------------------------------------------------------------------
// PDF laden und Seiten aufbauen
// ---------------------------------------------------------------------------

async function init() {
  await window.I18N.init();
  // Farbpunkte entstehen vor dem Laden der Sprache: Titel jetzt übersetzen
  document.querySelectorAll("#selBar .sw, #markBar .sw").forEach((b) => (b.title = tr("Highlight in {color}", { color: tr(COLORS[+b.dataset.i].name) })));
  S.info = await api("/api/info");
  window.PT.setTitle(`${S.info.title} – PDF-Tutor`);
  $("bookTitle").textContent = S.info.title;
  if (!S.info.claudeFound) {
    showBanner(tr("The “claude” command was not found. Reading works, but questions will fail."));
  }

  S.doc = S.info.doc;
  $("modeSelect").value = S.doc.mode || "auto";
  updateDocStatus();
  pollStatus();

  const sel = $("modelSelect");
  for (const m of S.info.models) {
    const o = new Option(m.extra ? `⚠ ${m.label}` : m.label, m.id);
    if (m.extra) o.className = "extra";
    sel.add(o);
  }

  const notesDoc = await api("/api/notes");
  S.notes = notesDoc.notes || [];
  S.version = notesDoc.version;
  S.undo = notesDoc.undo || null;
  updateUndo();
  const st = notesDoc.state || {};
  S.model = st.model && S.info.models.some((m) => m.id === st.model) ? st.model : S.info.defaultModel;
  sel.value = S.model;
  markModelSelect();
  S.effort = st.effort || "";
  $("effortSelect").value = S.effort;
  markModelSelect();

  S.pdf = await pdfjsLib.getDocument({ url: pdfUrl() }).promise;
  try { S.labels = await S.pdf.getPageLabels(); } catch { S.labels = null; }
  $("pageCount").textContent = `/ ${S.pdf.numPages}`;

  // Seitengrößen ermitteln (alle Seiten, damit Positionen stimmen)
  const n = S.pdf.numPages;
  const batch = 24;
  for (let i = 1; i <= n; i += batch) {
    const ps = await Promise.all(
      Array.from({ length: Math.min(batch, n - i + 1) }, (_, k) => S.pdf.getPage(i + k)),
    );
    for (const p of ps) addPage(p);
  }

  // Bezugsbreite: die häufigste Seitenbreite. Alle Seiten werden gleich breit angezeigt,
  // damit einzelne größere Seiten (z. B. A4 zwischen Folien) das Layout nicht sprengen.
  const widths = new Map();
  for (const p of S.pages) {
    const w = Math.round(p.w0);
    widths.set(w, (widths.get(w) || 0) + 1);
  }
  S.refW = [...widths.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] || 600;

  S.scale = st.scale || fitScale();
  S.notes.forEach(prepNote);
  applyScale(false);
  renderHighlights();
  renderAllCards();
  initSidebar();

  const observer = new IntersectionObserver(onIntersect, { root: scroller, rootMargin: "1500px 0px" });
  S.pages.forEach((p) => observer.observe(p.div));

  if (st.lastPage && st.lastPage > 1) scrollToPage(st.lastPage, false);
  updateCurrentPage();
  pollPending();
}

function addPage(pdfPage) {
  const vp = pdfPage.getViewport({ scale: 1 });
  const div = document.createElement("div");
  div.className = "page";
  div.dataset.num = pdfPage.pageNumber;
  const hl = document.createElement("div");
  hl.className = "hl-layer";
  const find = document.createElement("div");
  find.className = "find-layer";
  const text = document.createElement("div");
  text.className = "textLayer";
  div.append(hl, find, text);
  pagesEl.append(div);
  S.pages.push({
    num: pdfPage.pageNumber, div, hl, find, text, w0: vp.width, h0: vp.height, vp1: vp, pdfPage,
    rendered: false, renderId: 0, task: null, visible: false, loading: false,
  });
}

// PDF-Koordinaten ([links, unten, rechts, oben]) <-> normierte Koordinaten der angezeigten Seite
function normRect(pageNum, r) {
  const p = S.pages[pageNum - 1];
  if (!p) return null;
  const [x1, y1, x2, y2] = p.vp1.convertToViewportRectangle(r);
  const W = p.vp1.width, H = p.vp1.height;
  return { page: pageNum, x: Math.min(x1, x2) / W, y: Math.min(y1, y2) / H, w: Math.abs(x2 - x1) / W, h: Math.abs(y2 - y1) / H };
}

function pdfRect(r) {
  const p = S.pages[r.page - 1];
  const W = p.vp1.width, H = p.vp1.height;
  const [ax, ay] = p.vp1.convertToPdfPoint(r.x * W, r.y * H);
  const [bx, by] = p.vp1.convertToPdfPoint((r.x + r.w) * W, (r.y + r.h) * H);
  return [Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by)].map((v) => +v.toFixed(3));
}

function prepNote(n) {
  n.rects = (n.pdfRects || []).map((r) => normRect(n.page, r)).filter(Boolean);
  if (!n.rects.length) n.rects = [{ page: n.page, x: 0.02, y: 0.02, w: 0.03, h: 0.03 }];
  n.text = n.text || "";
  n.thread = n.thread || [];
  return n;
}

// Nach einer Änderung im PDF: neu laden und sichtbare Seiten neu zeichnen (ohne Flackern)
async function reloadPdf() {
  const pdf = await pdfjsLib.getDocument({ url: pdfUrl() }).promise;
  const old = S.pdf;
  S.pdf = pdf;
  for (const p of S.pages) {
    p.pdfPage = null;
    const wasShown = p.rendered || p.task || p.visible;
    if (p.task) { try { p.task.cancel(); } catch {} p.task = null; }
    p.renderId++;
    p.rendered = false;
    if (wasShown) render(p);
  }
  setTimeout(() => old?.destroy(), 2000);
  invalidateThumbs();
}

// Neuen Stand der Notizen holen; wurde das PDF von außen verändert, neu laden
async function refreshNotes(forceReload = false) {
  const doc = await api("/api/notes");
  const changed = doc.version !== S.version;
  S.version = doc.version;
  S.undo = doc.undo || null;
  updateUndo();
  S.notes = (doc.notes || []).map(prepNote);
  if (changed || forceReload) await reloadPdf();
  renderHighlights();
  renderAllCards();
}

function fitScale() {
  const avail = scroller.clientWidth - 420 - 28 - 48 - 20;
  return Math.max(0.5, Math.min(2.2, Math.round((avail / (S.refW || 600)) * 20) / 20));
}

// Darstellungsmaßstab einer Seite: alle Seiten bekommen dieselbe Breite
function pageScale(p) {
  return S.scale * (S.refW || p.w0) / p.w0;
}

function applyScale(keepPosition = true) {
  let anchorPage = null, frac = 0;
  if (keepPosition) {
    anchorPage = currentPage();
    const p = S.pages[anchorPage - 1];
    frac = (scroller.scrollTop - p.div.offsetTop) / p.div.offsetHeight;
  }
  for (const p of S.pages) {
    const ps = pageScale(p);
    p.div.style.width = `${Math.floor(p.w0 * ps)}px`;
    p.div.style.height = `${Math.floor(p.h0 * ps)}px`;
    p.div.style.setProperty("--scale-factor", ps);
    if (p.rendered || p.task) unrender(p);
  }
  $("zoomVal").textContent = `${Math.round(S.scale * 100)} %`;
  if (keepPosition && anchorPage) {
    const p = S.pages[anchorPage - 1];
    scroller.scrollTop = p.div.offsetTop + frac * p.div.offsetHeight;
  }
  for (const p of S.pages) if (p.visible) render(p);
  layoutNotes();
}

function setScale(s) {
  S.scale = Math.max(0.4, Math.min(4, Math.round(s * 100) / 100));
  applyScale(true);
  saveState({ scale: S.scale });
}

function onIntersect(entries) {
  for (const e of entries) {
    const p = S.pages[+e.target.dataset.num - 1];
    p.visible = e.isIntersecting;
    if (p.visible) render(p);
    else unrender(p);
  }
}

async function render(p) {
  if (p.rendered || p.task || p.loading) return;
  const id = ++p.renderId;
  if (!p.pdfPage) {
    p.loading = true;
    const doc = S.pdf;
    let page;
    try { page = await doc.getPage(p.num); } catch { p.loading = false; return; }
    p.loading = false;
    // Inzwischen neu geladen oder aus dem Bild gescrollt: Seite gehört evtl. zum alten Dokument
    if (doc !== S.pdf || p.renderId !== id) {
      if (p.visible && !p.rendered && !p.task) render(p);
      return;
    }
    p.pdfPage = page;
  }
  const vp = p.pdfPage.getViewport({ scale: pageScale(p) });
  const dpr = window.devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(vp.width * dpr);
  canvas.height = Math.floor(vp.height * dpr);
  canvas.style.width = `${Math.floor(vp.width)}px`;
  canvas.style.height = `${Math.floor(vp.height)}px`;
  const task = p.pdfPage.render({
    canvasContext: canvas.getContext("2d"),
    viewport: vp,
    transform: dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : null,
  });
  p.task = task;
  try {
    await task.promise;
  } catch {
    if (p.renderId === id) p.task = null;
    return; // abgebrochen
  }
  if (p.renderId !== id) return;
  p.div.querySelector("canvas")?.remove();
  p.div.prepend(canvas);

  p.text.replaceChildren();
  try {
    const tl = new pdfjsLib.TextLayer({
      textContentSource: p.pdfPage.streamTextContent(), container: p.text, viewport: vp,
    });
    await tl.render();
  } catch (e) {
    console.warn("Textebene", p.num, e);
  }
  if (p.renderId !== id) return;
  const end = document.createElement("div");
  end.className = "endOfContent";
  p.text.append(end);
  p.task = null;
  p.rendered = true;
  if (S.find.q) highlightFindOnPage(p);
}

function unrender(p) {
  p.renderId++;
  if (p.task) { try { p.task.cancel(); } catch {} p.task = null; }
  // auch halb fertige Seiten aufräumen (Bild schon da, Textebene noch nicht)
  p.div.querySelector("canvas")?.remove();
  p.text.replaceChildren();
  p.find.replaceChildren();
  p.rendered = false;
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

function currentPage() {
  const y = scroller.scrollTop + scroller.clientHeight / 3;
  let lo = 0, hi = S.pages.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (S.pages[mid].div.offsetTop <= y) lo = mid; else hi = mid - 1;
  }
  return lo + 1;
}

function scrollToPage(n, smooth = true) {
  n = Math.max(1, Math.min(S.pages.length, n));
  scroller.scrollTo({ top: S.pages[n - 1].div.offsetTop - 12, behavior: smooth ? "smooth" : "auto" });
}

let lastShownPage = 0;
function updateCurrentPage() {
  const n = currentPage();
  if (n === lastShownPage) return;
  lastShownPage = n;
  if (document.activeElement !== $("pageInput")) $("pageInput").value = n;
  markCurrentThumb(n);
  const l = pageLabel(n);
  $("pageLabel").textContent = l !== String(n) ? tr("(printed: {label})", { label: l }) : "";
  saveState({ lastPage: n });
}

let scrollRaf = 0;
scroller.addEventListener("scroll", () => {
  hideBars();
  if (scrollRaf) return;
  scrollRaf = requestAnimationFrame(() => { scrollRaf = 0; updateCurrentPage(); });
});

$("pageInput").addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  const v = e.target.value.trim();
  let n = parseInt(v, 10);
  // gedruckte Seitenzahl (z. B. "xii") erlauben
  if (S.labels) {
    const idx = S.labels.indexOf(v);
    if (idx >= 0 && (isNaN(n) || String(n) !== v)) n = idx + 1;
  }
  if (!isNaN(n)) scrollToPage(n);
  e.target.blur();
});
$("pageInput").addEventListener("blur", () => { lastShownPage = 0; updateCurrentPage(); });
$("prevPage").onclick = () => scrollToPage(currentPage() - 1);
$("nextPage").onclick = () => scrollToPage(currentPage() + 1);
$("zoomIn").onclick = () => setScale(S.scale * 1.15);
$("zoomOut").onclick = () => setScale(S.scale / 1.15);
$("zoomFit").onclick = () => setScale(fitScale());
$("modelSelect").onchange = async (e) => {
  const id = e.target.value;
  if (isExtraModel(id) && !(await confirmExtra(id))) {
    e.target.value = S.model;
    return;
  }
  S.model = id;
  markModelSelect();
  saveState({ model: S.model });
};
$("effortSelect").onchange = (e) => { S.effort = e.target.value; markModelSelect(); saveState({ effort: S.effort }); };

// Rückfrage, bevor ein Modell mit möglichen Zusatzkosten verwendet wird
function confirmExtra(id) {
  const dlg = $("extraDlg");
  const name = modelLabel(id);
  $("extraText").textContent = tr("{model} may be billed via extra usage credits, depending on your Claude plan, and may not be available on every plan. Questions to {model} could therefore cost real money beyond your subscription.", { model: name });
  $("extraOk").textContent = tr("Use {model}", { model: name });
  return new Promise((resolve) => {
    dlg.addEventListener("close", () => resolve(dlg.returnValue === "ok"), { once: true });
    dlg.returnValue = "";
    dlg.showModal();
  });
}
$("modeSelect").onchange = async (e) => {
  try { S.doc = await api("/api/state", { mode: e.target.value }); updateDocStatus(); } catch {}
};

// Status des Dokuments (Vorbereitung, Umfang, Modus)
const fmtNum = (n) => window.I18N.fmtNum(n);
const modeName = (m) => (m === "volltext" ? tr("full text") : m === "suche" ? tr("search") : String(m || ""));

function updateDocStatus() {
  const d = S.doc;
  if (!d) return;
  const el = $("docStatus");
  if (!d.textReady) {
    el.textContent = tr("Preparing document …");
  } else {
    const tk = d.tokens >= 1000 ? tr("{n}k", { n: fmtNum(Math.round(d.tokens / 1000)) }) : fmtNum(d.tokens);
    let txt = tr("{tokens} tokens → {mode}", { tokens: tk, mode: modeName(d.effectiveMode) });
    if (d.imagesDone < d.pages) txt += " · " + tr("images {p} %", { p: Math.floor((100 * d.imagesDone) / d.pages) });
    el.textContent = txt;
  }
  if (d.textReady && d.tokens != null) {
    const autoMode = d.tokens <= d.limit ? tr("full text") : tr("search");
    $("modeSelect").options[0].textContent = tr("automatic ({mode})", { mode: autoMode });
  }
  el.title = tr("Automatic: full text up to {n} tokens, search above that.", { n: fmtNum(d.limit) }) +
    (d.error ? `\n${d.error}` : "");
}

function pollStatus() {
  const d = S.doc;
  if (d && d.textReady && d.imagesDone >= d.pages) return;
  setTimeout(async () => {
    try { S.doc = await api("/api/status"); updateDocStatus(); } catch {}
    pollStatus();
  }, 2000);
}
$("collapseAll").onclick = () => {
  for (const n of S.notes.filter(hasCard)) {
    if (!n.collapsed && !isPending(n)) { n.collapsed = true; api("/api/notes/update", { id: n.id, collapsed: true }).catch(() => {}); }
  }
  renderAllCards();
};

function showBanner(text) {
  const b = $("banner");
  b.textContent = text;
  b.hidden = false;
}

// ---------------------------------------------------------------------------
// Farben und Arten
// ---------------------------------------------------------------------------

// Hervorhebungen in hellen Tönen, Linien (Unter-/Durchstreichen) kräftiger
const COLORS = [
  { name: "Yellow", hl: [1, 0.85, 0],      line: [0.9, 0.68, 0] },
  { name: "Green",   hl: [0.49, 0.85, 0.39], line: [0.18, 0.62, 0.22] },
  { name: "Blue",    hl: [0.45, 0.75, 1],    line: [0.15, 0.45, 0.9] },
  { name: "Pink",    hl: [1, 0.56, 0.75],    line: [0.88, 0.2, 0.5] },
  { name: "Orange", hl: [1, 0.65, 0.25],    line: [0.95, 0.45, 0.05] },
];
const KIND_NAMES = { Highlight: "Highlight", Underline: "Underline", StrikeOut: "Strikeout", Squiggly: "Markup", Text: "Note", FreeText: "Text box" };
const kindName = (k) => tr(own(KIND_NAMES, k) || "Annotation");
const colorFor = (i, kind) => (kind === "Highlight" || kind === "Text" ? COLORS[i].hl : COLORS[i].line);
const css = (c) => `rgb(${c.map((v) => Math.round(v * 255)).join(",")})`;

function colorIndex(c) {
  if (!c) return 0;
  let best = 0, bd = Infinity;
  COLORS.forEach((col, i) => {
    for (const v of [col.hl, col.line]) {
      const d = v.reduce((s, x, k) => s + (x - c[k]) ** 2, 0);
      if (d < bd) { bd = d; best = i; }
    }
  });
  return best;
}

let lastColor = 0;
try { lastColor = Math.min(4, Math.max(0, +localStorage.getItem("pdftutor-color") || 0)); } catch {}
function rememberColor(i) {
  lastColor = i;
  try { localStorage.setItem("pdftutor-color", String(i)); } catch {}
}

// Farbpunkte in einer Leiste aufbauen
function buildSwatches(el, onPick) {
  el.replaceChildren();
  COLORS.forEach((c, i) => {
    const b = document.createElement("button");
    b.className = "sw";
    b.dataset.i = i;
    b.title = tr("Highlight in {color}", { color: tr(c.name) });
    b.style.background = css(c.hl);
    b.addEventListener("click", () => onPick(i));
    el.append(b);
  });
}

// ---------------------------------------------------------------------------
// Markieren: Leiste an der Auswahl
// ---------------------------------------------------------------------------

const selBar = $("selBar");
const markBar = $("markBar");
for (const bar of [selBar, markBar]) bar.addEventListener("mousedown", (e) => e.preventDefault()); // Auswahl behalten

buildSwatches(selBar.querySelector(".swatches"), (i) => { rememberColor(i); markSelection("Highlight", i); });
selBar.querySelectorAll("[data-kind]").forEach((b) => b.addEventListener("click", () => markSelection(b.dataset.kind, lastColor)));
selBar.querySelector('[data-act="comment"]').addEventListener("click", () => startDraft("comment"));
selBar.querySelector('[data-act="ask"]').addEventListener("click", () => startDraft("ask"));

pagesEl.addEventListener("mousedown", (e) => {
  if (S.tool === "sticky") return;
  const tl = e.target.closest(".textLayer");
  if (tl) tl.classList.add("selecting");
  hideBars();
});

document.addEventListener("mouseup", (e) => {
  document.querySelectorAll(".textLayer.selecting").forEach((t) => t.classList.remove("selecting"));
  if (S.tool === "sticky") return;
  if (e.target.closest(".floatbar") || e.target.closest("#notes") || e.target.closest("#toolbar") || e.target.closest("#toast")) return;
  setTimeout(() => handleSelection(e), 0);
});

function handleSelection(e) {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
    if (e.target.closest("#pages")) hitTestNote(e.clientX, e.clientY);
    return;
  }
  const node = sel.anchorNode && (sel.anchorNode.nodeType === 1 ? sel.anchorNode : sel.anchorNode.parentElement);
  if (!node || !node.closest(".textLayer")) return;
  const range = sel.getRangeAt(0);
  const rects = selectionRects(range);
  const text = sel.toString().replace(/\s+/g, " ").trim();
  if (!rects.length || !text) return;
  S.pendingSel = { rects, text, page: rects[0].page };
  const last = [...range.getClientRects()].filter((r) => r.width > 0).pop() || range.getBoundingClientRect();
  showBar(selBar, last.right, last.bottom + 8);
}

function showBar(bar, x, y) {
  hideBars();
  bar.hidden = false;
  const w = bar.offsetWidth, h = bar.offsetHeight;
  bar.style.left = `${Math.max(8, Math.min(window.innerWidth - w - 8, x - w / 2))}px`;
  bar.style.top = `${y + h > window.innerHeight - 8 ? Math.max(8, y - h - 40) : y}px`;
}

function hideBars() {
  selBar.hidden = true;
  markBar.hidden = true;
  S.menuId = null;
}

function selectionRects(range) {
  const out = [];
  const pageBoxes = S.pages.filter((p) => p.rendered).map((p) => ({ p, b: p.div.getBoundingClientRect() }));
  for (const r of range.getClientRects()) {
    if (r.width < 1 || r.height < 1) continue;
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const hit = pageBoxes.find(({ b }) => cx >= b.left && cx <= b.right && cy >= b.top && cy <= b.bottom);
    if (!hit) continue;
    const { p, b } = hit;
    if (r.height > b.height * 0.25 || r.width > b.width * 0.98) continue; // Artefakte (endOfContent etc.)
    out.push({
      page: p.num,
      x: (r.left - b.left) / b.width, y: (r.top - b.top) / b.height,
      w: r.width / b.width, h: r.height / b.height,
    });
  }
  // Rechtecke derselben Zeile zusammenfassen
  out.sort((a, b) => a.page - b.page || a.y - b.y || a.x - b.x);
  const merged = [];
  for (const r of out) {
    const m = merged[merged.length - 1];
    if (m && m.page === r.page) {
      const overlap = Math.min(m.y + m.h, r.y + r.h) - Math.max(m.y, r.y);
      const gap = r.x - (m.x + m.w);
      if (overlap > 0.5 * Math.min(m.h, r.h) && gap < 0.02) {
        const x0 = Math.min(m.x, r.x), y0 = Math.min(m.y, r.y);
        const x1 = Math.max(m.x + m.w, r.x + r.w), y1 = Math.max(m.y + m.h, r.y + r.h);
        Object.assign(m, { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
        continue;
      }
    }
    merged.push({ ...r });
  }
  return merged.map((r) => ({ ...r, x: +r.x.toFixed(5), y: +r.y.toFixed(5), w: +r.w.toFixed(5), h: +r.h.toFixed(5) }));
}

// Auswahl sofort markieren (Hervorheben, Unterstreichen, Durchstreichen)
async function markSelection(kind, ci) {
  const sel = S.pendingSel;
  if (!sel) return;
  S.pendingSel = null;
  hideBars();
  window.getSelection().removeAllRanges();
  try {
    const rects = sel.rects.filter((r) => r.page === sel.page).map(pdfRect);
    await api("/api/notes/create", { page: sel.page, kind, color: colorFor(ci, kind), pdfRects: rects, text: sel.text });
    await afterChange();
  } catch (e) {
    showToast(tr("Could not highlight: {msg}", { msg: e.message }));
  }
}

// Kommentar oder Frage zur Auswahl: zuerst eine Entwurfskarte
function startDraft(mode) {
  if (!S.pendingSel) return;
  S.draft = { ...S.pendingSel, mode, color: lastColor };
  S.pendingSel = null;
  hideBars();
  window.getSelection().removeAllRanges();
  showDraft();
}

function showDraft() {
  renderHighlights();
  renderAllCards();
  const ta = notesEl.querySelector(".card.draft textarea");
  if (ta) {
    ta.focus({ preventScroll: true });
    requestAnimationFrame(() => requestAnimationFrame(() => {
      ta.closest(".card")?.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
    }));
  }
}

// ---------------------------------------------------------------------------
// Vorhandene Markierung anklicken: Menü
// ---------------------------------------------------------------------------

buildSwatches(markBar.querySelector(".swatches"), (i) => {
  const n = S.notes.find((x) => x.id === S.menuId);
  if (!n) return;
  rememberColor(i);
  restyle(n, null, i);
});
markBar.querySelectorAll("[data-kind]").forEach((b) => b.addEventListener("click", () => {
  const n = S.notes.find((x) => x.id === S.menuId);
  if (n && b.dataset.kind !== n.subtype) restyle(n, b.dataset.kind, colorIndex(n.color));
}));
markBar.querySelector('[data-act="comment"]').addEventListener("click", () => composeOn(S.menuId, "comment"));
markBar.querySelector('[data-act="ask"]').addEventListener("click", () => composeOn(S.menuId, "ask"));
markBar.querySelector('[data-act="delete"]').addEventListener("click", () => {
  const n = S.notes.find((x) => x.id === S.menuId);
  hideBars();
  if (n) deleteNote(n);
});

function hitTestNote(x, y) {
  // Haftnotizen liegen oben, daher zuerst
  const els = [...document.querySelectorAll(".hl:not(.draft)")].sort((a, b) => (b.classList.contains("sticky") ? 1 : 0) - (a.classList.contains("sticky") ? 1 : 0));
  for (const el of els) {
    const b = el.getBoundingClientRect();
    if (x >= b.left - 2 && x <= b.right + 2 && y >= b.top - 2 && y <= b.bottom + 2) {
      const note = S.notes.find((n) => n.id === el.dataset.id);
      if (note) openMenu(note, x, y);
      return;
    }
  }
  hideBars();
}

function openMenu(n, x, y) {
  const editable = own(KIND_NAMES, n.subtype) && n.subtype !== "Squiggly" && n.subtype !== "FreeText";
  markBar.querySelectorAll("[data-kind]").forEach((b) => {
    b.hidden = n.subtype === "Text" || !editable;
    b.classList.toggle("on", b.dataset.kind === n.subtype);
  });
  markBar.querySelector(".swatches").hidden = !editable;
  markBar.querySelectorAll(".sw").forEach((b) => b.classList.toggle("on", +b.dataset.i === colorIndex(n.color)));
  // nicht scrollen: das würde das Menü gleich wieder schließen
  if (hasCard(n)) openNote(n, false);
  else { S.activeId = n.id; renderHighlights(); }
  showBar(markBar, x, y + 14);
  S.menuId = n.id;
}

async function restyle(n, kind, ci) {
  const newKind = kind || n.subtype;
  try {
    await api("/api/notes/style", { id: n.id, kind: kind || undefined, color: colorFor(ci, newKind) });
    hideBars();
    await afterChange();
  } catch (e) {
    showToast(tr("Change failed: {msg}", { msg: e.message }));
  }
}

// Karte einer Notiz öffnen und Eingabefeld für Kommentar oder Frage bereitstellen
function composeOn(id, mode) {
  const n = S.notes.find((x) => x.id === id);
  hideBars();
  if (!n) return;
  S.compose = { id, mode };
  n.collapsed = false;
  openNote(n, true);
  requestAnimationFrame(() => notesEl.querySelector(`.card[data-id="${CSS.escape(id)}"] .composer textarea`)?.focus({ preventScroll: true }));
}

// ---------------------------------------------------------------------------
// Haftnotizen
// ---------------------------------------------------------------------------

$("stickyTool").onclick = () => setTool(S.tool === "sticky" ? null : "sticky");

function setTool(t) {
  S.tool = t;
  $("stickyTool").classList.toggle("on", t === "sticky");
  pagesEl.classList.toggle("placing", t === "sticky");
  hideBars();
}

pagesEl.addEventListener("click", (e) => {
  if (S.tool !== "sticky") return;
  const div = e.target.closest(".page");
  if (!div) return;
  const p = S.pages[+div.dataset.num - 1];
  const b = div.getBoundingClientRect();
  const nx = (e.clientX - b.left) / b.width, ny = (e.clientY - b.top) / b.height;
  const [px, py] = p.vp1.convertToPdfPoint(nx * p.vp1.width, ny * p.vp1.height);
  const half = 10;
  const r = normRect(p.num, [px - half, py - half, px + half, py + half]);
  setTool(null);
  S.draft = { mode: "sticky", page: p.num, rects: [r], text: "", point: [+px.toFixed(2), +py.toFixed(2)], color: lastColor };
  showDraft();
});

// ---------------------------------------------------------------------------
// Markierungen zeichnen (nur Klickflächen; die Markierungen selbst stehen im PDF)
// ---------------------------------------------------------------------------

function renderHighlights() {
  for (const p of S.pages) p.hl.replaceChildren();
  const add = (r, cls, id) => {
    const p = S.pages[r.page - 1];
    if (!p) return;
    const d = document.createElement("div");
    d.className = `hl ${cls}`;
    if (id) d.dataset.id = id;
    Object.assign(d.style, {
      left: `${r.x * 100}%`, top: `${r.y * 100}%`, width: `${r.w * 100}%`, height: `${r.h * 100}%`,
    });
    p.hl.append(d);
  };
  for (const n of S.notes) {
    const cls = [n.id === S.activeId ? "active" : "", n.subtype === "Text" ? "sticky" : ""].join(" ");
    for (const r of n.rects) add(r, cls, n.id);
  }
  if (S.draft) {
    if (S.draft.mode === "sticky") add(S.draft.rects[0], "draft sticky-draft");
    else for (const r of S.draft.rects) add(r, "draft");
  }
}

function setHlClass(id, cls, on) {
  document.querySelectorAll(`.hl[data-id="${CSS.escape(id)}"]`).forEach((d) => d.classList.toggle(cls, on));
}

// ---------------------------------------------------------------------------
// Notizkarten (nur für Notizen mit Text; reine Markierungen bekommen keine Karte)
// ---------------------------------------------------------------------------

const isPending = (n) => (n.thread || []).some((m) => m.status === "pending");
const hasCard = (n) => (n.thread || []).length > 0 || n.subtype === "Text" || n.subtype === "FreeText" || S.compose?.id === n.id;

function anchorTop(item) {
  const r0 = item.rects.filter((r) => r.page === item.page);
  const rs = r0.length ? r0 : item.rects;
  const p = S.pages[(rs[0]?.page || item.page) - 1];
  if (!p) return 0;
  const minY = Math.min(...rs.map((r) => r.y));
  return p.div.offsetTop + minY * p.div.offsetHeight;
}

// Begonnene Eingaben über ein Neuzeichnen der Karten hinweg erhalten
function saveInputs() {
  const out = {};
  notesEl.querySelectorAll(".card textarea").forEach((ta) => {
    const key = (ta.closest(".card").dataset.id || "draft") + "|" + (ta.dataset.msg || "composer");
    out[key] = { v: ta.value, focus: document.activeElement === ta, s: ta.selectionStart, e: ta.selectionEnd };
  });
  return out;
}

function restoreInputs(saved) {
  notesEl.querySelectorAll(".card textarea").forEach((ta) => {
    const key = (ta.closest(".card").dataset.id || "draft") + "|" + (ta.dataset.msg || "composer");
    const x = saved[key];
    if (!x || (!x.v && !x.focus)) return;
    ta.value = x.v;
    ta.dispatchEvent(new Event("input"));
    if (x.focus) { ta.focus({ preventScroll: true }); try { ta.setSelectionRange(x.s, x.e); } catch {} }
  });
}

function renderAllCards() {
  const saved = saveInputs();
  notesEl.querySelectorAll(".card").forEach((c) => c.remove());
  const withCard = S.notes.filter(hasCard);
  $("notesEmpty").hidden = withCard.length > 0 || !!S.draft;
  for (const n of withCard) notesEl.append(buildCard(n));
  if (S.draft) notesEl.append(buildDraftCard());
  restoreInputs(saved);
  layoutNotes();
  renderSidebarNotes();
}

function refreshCard(note) {
  const saved = saveInputs();
  const old = notesEl.querySelector(`.card[data-id="${CSS.escape(note.id)}"]`);
  if (!hasCard(note)) { old?.remove(); layoutNotes(); return; }
  const fresh = buildCard(note);
  if (old) old.replaceWith(fresh); else notesEl.append(fresh);
  restoreInputs(saved);
  $("notesEmpty").hidden = true;
  layoutNotes();
}

function buildCard(n) {
  const card = document.createElement("div");
  card.className = "card" + (n.collapsed ? " collapsed" : "") + (n.id === S.activeId ? " active" : "");
  card.dataset.id = n.id;
  card.style.setProperty("--note-color", css(n.color || COLORS[0].hl));
  const thread = n.thread || [];
  const kind = kindName(n.subtype);
  const quote = n.text ? (n.text.length > 90 ? n.text.slice(0, 90) + " …" : n.text) : "";
  // Nutzer-Nachricht, auf die Claude geantwortet hat (oder antwortet), ist eine Frage, sonst ein Kommentar
  const isQuestion = (i) => thread[i + 1] && thread[i + 1].role === "assistant";

  let body = "";
  if (n.collapsed) {
    const q = thread.find((m) => m.role === "user");
    const a = [...thread].reverse().find((m) => m.role === "assistant" && m.status === "ok");
    body += q ? `<div class="msg ${isQuestion(thread.indexOf(q)) ? "q" : "c"}" data-label="${esc(isQuestion(thread.indexOf(q)) ? tr("Question") : tr("Comment"))}">${esc(q.text)}</div>` : "";
    body += a ? `<div class="preview">${esc(a.text.replace(/[#*$`\\]/g, "").slice(0, 300))}</div>` : "";
  } else {
    thread.forEach((m, i) => {
      if (m.role === "user") {
        const who = m.author && n.own === false ? `<span class="who">${esc(m.author)}</span>` : "";
        const edit = m.id && !isPending(n) ? `<button class="icon edit" data-msg="${esc(m.id)}" title="${esc(tr("Edit"))}">✎</button>` : "";
        body += `<div class="msg ${isQuestion(i) ? "q" : "c"}" data-label="${esc(isQuestion(i) ? tr("Question") : tr("Comment"))}" data-msg="${esc(m.id || "")}">${edit}${who}${renderMD(m.text, { breaks: true })}</div>`;
      } else if (m.status === "pending") {
        const start = Date.parse(m.time) || Date.now();
        body += `<div class="msg pending"><span class="spinner"></span>${esc(tr("Claude ({model}) is thinking …", { model: modelLabel(m.model) }))} <span class="elapsed" data-start="${start}"></span></div>`;
      } else if (m.status === "error") {
        body += `<div class="msg err">${esc(m.text)}<button class="btn retry">${esc(tr("Try again"))}</button></div>`;
      } else {
        body += `<div class="msg a">${renderMD(m.text)}</div>`;
        const u = m.usage;
        const tip = u ? tr("Input {a} · from cache {b} · written to cache {c} · output {d} tokens", { a: fmtNum(u.input_tokens), b: fmtNum(u.cache_read_input_tokens), c: fmtNum(u.cache_creation_input_tokens), d: fmtNum(u.output_tokens) }) : "";
        body += `<div class="meta" title="${esc(tip)}">${esc(modelLabel(m.model))}${m.effort ? ` · ${esc(effortLabel(m.effort))}` : ""}${m.mode ? ` · ${esc(modeName(m.mode))}` : ""}${m.seconds ? ` · ${esc(m.seconds)} s` : ""}${m.unsaved ? ` · <span class="unsaved" title="${esc(tr("The PDF is currently open in another program. The answer will be saved as soon as possible."))}">${esc(tr("not yet saved in the PDF"))}</span>` : ""}</div>`;
      }
    });
    if (!isPending(n)) {
      const mode = S.compose?.id === n.id ? S.compose.mode : thread.some((m) => m.role === "assistant") ? "ask" : "comment";
      const ph = tr(mode === "ask" ? (thread.length ? "Follow-up question to Claude …" : "Your question to Claude …")
        : thread.length ? "Add a comment …" : n.subtype === "Text" ? "Text of the note …" : "Your comment …");
      body += `<div class="composer" data-mode="${mode}">
        <textarea rows="1" placeholder="${ph}"></textarea>
        <div class="row"><span class="hint">${esc(mode === "ask" ? tr("Ctrl+Enter: ask Claude") : tr("Ctrl+Enter: save"))}</span>
          <button class="btn ${mode === "comment" ? "primary" : ""} save-c">${esc(tr("Save comment"))}</button>
          <button class="btn ${mode === "ask" ? "primary" : ""} send">${esc(tr("Ask Claude"))}</button></div>
      </div>`;
    }
  }

  card.innerHTML = `
    <div class="card-head">
      <span class="pg">${esc(tr("p. {n}", { n: pageLabel(n.page) }))}</span>
      <span class="quote" title="${esc(n.text)}">${quote ? esc(tr("“{q}”", { q: quote })) : esc(kind)}</span>
      <button class="icon toggle" title="${esc(n.collapsed ? tr("Expand") : tr("Collapse"))}">${n.collapsed ? "▸" : "▾"}</button>
      <button class="icon del" title="${esc(n.subtype === "Text" ? tr("Delete note with comments") : tr("Delete highlight with comments"))}">×</button>
    </div>
    <div class="card-body">${body}</div>`;

  card.querySelector(".card-head").addEventListener("click", (e) => {
    if (e.target.closest(".del")) return;
    if (e.target.closest(".toggle")) { toggleCollapsed(n); return; }
    openNote(n, false);
  });
  card.querySelector(".del").addEventListener("click", () => deleteNote(n));
  if (n.collapsed) card.querySelector(".card-body").addEventListener("click", () => openNote(n, false));
  card.addEventListener("mouseenter", () => setHlClass(n.id, "hover", true));
  card.addEventListener("mouseleave", () => setHlClass(n.id, "hover", false));
  card.querySelector(".retry")?.addEventListener("click", () => ask(n, null, true));
  card.querySelectorAll(".edit").forEach((b) => b.addEventListener("click", (e) => { e.stopPropagation(); editMessage(card, n, b.dataset.msg); }));

  const comp = card.querySelector(".composer");
  if (comp) {
    const ta = comp.querySelector("textarea");
    const sendAsk = () => { const q = ta.value.trim(); if (q) { if (S.compose?.id === n.id) S.compose = null; ask(n, q, false); } };
    const sendComment = async () => {
      const t = ta.value.trim();
      if (!t) return;
      comp.querySelectorAll("button, textarea").forEach((el) => (el.disabled = true));
      try {
        await api("/api/notes/comment", { id: n.id, text: t });
        ta.value = "";
        if (S.compose?.id === n.id) S.compose = null;
        await afterChange();
      } catch (e) {
        comp.querySelectorAll("button, textarea").forEach((el) => (el.disabled = false));
        showToast(tr("Saving failed: {msg}", { msg: e.message }));
      }
    };
    comp.querySelector(".send").addEventListener("click", sendAsk);
    comp.querySelector(".save-c").addEventListener("click", sendComment);
    ta.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); comp.dataset.mode === "ask" ? sendAsk() : sendComment(); }
      if (e.key === "Escape" && S.compose?.id === n.id && !ta.value.trim()) { S.compose = null; renderAllCards(); }
    });
    autoGrow(ta);
  }
  return card;
}

// Eigene Nachricht bearbeiten (leer speichern löscht sie)
function editMessage(card, n, msgId) {
  const m = n.thread.find((x) => x.id === msgId);
  const el = card.querySelector(`.msg[data-msg="${CSS.escape(msgId)}"]`);
  if (!m || !el) return;
  el.classList.add("editing");
  el.innerHTML = `<textarea rows="2" data-msg="${esc(msgId)}"></textarea>
    <div class="row"><span class="hint">${esc(tr("Saving it empty deletes the comment"))}</span>
      <button class="btn cancel">${esc(tr("Cancel"))}</button><button class="btn primary save">${esc(tr("Save"))}</button></div>`;
  const ta = el.querySelector("textarea");
  ta.value = m.text;
  autoGrow(ta);
  ta.dispatchEvent(new Event("input"));
  ta.focus();
  const save = async () => {
    el.querySelectorAll("button, textarea").forEach((x) => (x.disabled = true));
    try {
      await api("/api/notes/edit", { id: n.id, msg: msgId, text: ta.value });
      await afterChange();
    } catch (e) {
      el.querySelectorAll("button, textarea").forEach((x) => (x.disabled = false));
      showToast(tr("Saving failed: {msg}", { msg: e.message }));
    }
  };
  el.querySelector(".cancel").addEventListener("click", () => refreshCard(n));
  el.querySelector(".save").addEventListener("click", save);
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); }
    if (e.key === "Escape") refreshCard(n);
  });
  layoutNotes();
}

function buildDraftCard() {
  const d = S.draft;
  const card = document.createElement("div");
  card.className = "card draft active";
  card.style.setProperty("--note-color", css(COLORS[d.color].hl));
  const quote = d.text.length > 90 ? d.text.slice(0, 90) + " …" : d.text;
  const cfg = {
    ask: { ph: tr("Your question about this passage …"), btn: tr("Ask Claude") },
    comment: { ph: tr("Your comment on this passage …"), btn: tr("Save") },
    sticky: { ph: tr("Text of the note …"), btn: tr("Save") },
  }[d.mode];
  card.innerHTML = `
    <div class="card-head"><span class="pg">${esc(tr("p. {n}", { n: pageLabel(d.page) }))}</span><span class="quote" title="${esc(d.text)}">${quote ? esc(tr("“{q}”", { q: quote })) : esc(tr("New note"))}</span></div>
    <div class="card-body"><div class="composer">
      <textarea rows="2" placeholder="${cfg.ph}"></textarea>
      <div class="row"><span class="hint">${esc(tr("Ctrl+Enter · Esc cancels"))}</span>
        <button class="btn cancel">${esc(tr("Cancel"))}</button><button class="btn primary send">${esc(cfg.btn)}</button></div>
    </div></div>`;
  const ta = card.querySelector("textarea");
  const cancel = () => { S.draft = null; renderHighlights(); renderAllCards(); };
  const send = async () => {
    const t = ta.value.trim();
    if (!t && d.mode !== "sticky") return;
    card.querySelectorAll("button, textarea").forEach((el) => (el.disabled = true));
    try {
      if (d.mode === "sticky") {
        await api("/api/notes/sticky", { page: d.page, point: d.point, color: COLORS[d.color].hl, text: t });
        S.draft = null;
        await afterChange();
        return;
      }
      const rects = d.rects.filter((r) => r.page === d.page).map(pdfRect);
      const body = { page: d.page, kind: "Highlight", color: COLORS[d.color].hl, pdfRects: rects, text: d.text };
      if (d.mode === "ask") body.question = t; else body.comment = t;
      const note = prepNote(await api("/api/notes/create", body));
      S.draft = null;
      S.activeId = note.id;
      if (d.mode === "ask") {
        S.notes.push(note);
        renderHighlights();
        renderAllCards();
        reloadPdf().catch(() => {});
        ask(note, t, false, true);
      } else {
        await afterChange();
      }
    } catch (e) {
      card.querySelectorAll("button, textarea").forEach((el) => (el.disabled = false));
      showToast(tr("Could not create the note: {msg}", { msg: e.message }));
    }
  };
  card.querySelector(".cancel").addEventListener("click", cancel);
  card.querySelector(".send").addEventListener("click", send);
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); send(); }
    if (e.key === "Escape") cancel();
  });
  autoGrow(ta);
  return card;
}

function autoGrow(ta) {
  const fit = () => { ta.style.height = "auto"; ta.style.height = `${Math.min(220, ta.scrollHeight + 2)}px`; layoutNotes(); };
  ta.addEventListener("input", fit);
}

// Abgesetzte Formeln, die breiter als die Randspalte sind, verkleinern (bis 70 %),
// erst danach bleibt ein waagrechter Rollbalken
let fontsWaiting = false;
function fitMath(root) {
  // Erst messen, wenn die KaTeX-Schriften geladen sind, sonst stimmen die Breiten nicht
  if (document.fonts && document.fonts.status !== "loaded") {
    if (!fontsWaiting) {
      fontsWaiting = true;
      document.fonts.ready.then(() => { fontsWaiting = false; layoutNotes(); });
    }
    return;
  }
  root.querySelectorAll(".msg .katex-display:not([data-fit])").forEach((el) => {
    const avail = el.clientWidth;
    if (!avail) return; // noch nicht gelayoutet
    el.dataset.fit = "1";
    let f = 1;
    for (let i = 0; i < 4 && el.scrollWidth > el.clientWidth + 1 && f > 0.7; i++) {
      f = Math.max(0.7, f * (el.clientWidth - 2) / el.scrollWidth);
      el.style.fontSize = `${f.toFixed(3)}em`;
    }
  });
}

// Karten vertikal an ihre Stelle setzen, ohne Überlappung
let layoutRaf = 0;
function layoutNotes() {
  if (layoutRaf) return;
  layoutRaf = requestAnimationFrame(() => {
    layoutRaf = 0;
    fitMath(notesEl);
    const items = [];
    for (const card of notesEl.querySelectorAll(".card")) {
      const src = card.classList.contains("draft") ? S.draft : S.notes.find((n) => n.id === card.dataset.id);
      if (src) items.push({ card, top: anchorTop(src), active: card.classList.contains("active") });
    }
    items.sort((a, b) => a.top - b.top);
    let cursor = 0;
    for (const it of items) {
      const top = Math.max(it.top, cursor);
      it.card.style.top = `${top}px`;
      cursor = top + it.card.offsetHeight + 10;
    }
    notesEl.style.minHeight = `${Math.max(pagesEl.offsetHeight, cursor)}px`;
  });
}
new ResizeObserver(() => layoutNotes()).observe(notesEl);

function openNote(n, scrollToCard) {
  S.activeId = n.id;
  if (n.collapsed) {
    n.collapsed = false;
    api("/api/notes/update", { id: n.id, collapsed: false }).catch(() => {});
  }
  renderHighlights();
  renderAllCards();
  if (scrollToCard) {
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const card = notesEl.querySelector(`.card[data-id="${CSS.escape(n.id)}"]`);
      if (!card) return;
      const top = card.offsetTop + notesEl.offsetTop;
      const bottom = top + card.offsetHeight;
      if (top < scroller.scrollTop || bottom > scroller.scrollTop + scroller.clientHeight) {
        scroller.scrollTo({ top: top - 60, behavior: "smooth" });
      }
    }));
  }
}

function toggleCollapsed(n) {
  if (isPending(n)) return;
  n.collapsed = !n.collapsed;
  api("/api/notes/update", { id: n.id, collapsed: n.collapsed }).catch(() => {});
  refreshCard(n);
}

async function deleteNote(n) {
  try {
    await api("/api/notes/delete", { id: n.id });
    if (S.activeId === n.id) S.activeId = null;
    if (S.compose?.id === n.id) S.compose = null;
    await afterChange();
    showToast(tr("{kind} deleted.", { kind: kindName(n.subtype) }), tr("Undo"), doUndo);
  } catch (e) {
    showToast(tr("Deleting failed: {msg}", { msg: e.message }));
  }
}

// ---------------------------------------------------------------------------
// Nach Änderungen, Rückgängig, Hinweise
// ---------------------------------------------------------------------------

async function afterChange() {
  await refreshNotes(true);
}

function updateUndo() {
  const b = $("undoBtn");
  b.disabled = !S.undo;
  b.title = S.undo ? tr("Undo: {what} (Ctrl+Z)", { what: S.undo }) : tr("Nothing to undo");
}

async function doUndo() {
  hideBars();
  try {
    const r = await api("/api/undo");
    await afterChange();
    showToast(tr("Undone: {what}", { what: r.undone }));
  } catch (e) {
    showToast(e.message);
    refreshNotes().catch(() => {});
  }
}
$("undoBtn").onclick = doUndo;

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
  toastTimer = setTimeout(() => (t.hidden = true), action ? 7000 : 4000);
}

document.addEventListener("keydown", (e) => {
  const typing = e.target.closest && e.target.closest("textarea, input, select");
  if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "f") {
    e.preventDefault();
    openFind();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "z" && !typing) {
    e.preventDefault();
    if (S.undo) doUndo();
  } else if (e.key === "Escape" && !typing) {
    hideBars();
    if (S.tool) setTool(null);
  }
});

// ---------------------------------------------------------------------------
// Fragen an Claude
// ---------------------------------------------------------------------------

async function ask(note, question, retry, saved = false) {
  const model = S.model;
  const effort = S.effort || "";
  // sofort anzeigen (der Server macht dasselbe)
  if (retry) {
    const last = note.thread[note.thread.length - 1];
    if (last && last.status === "error") note.thread.pop();
  } else if (!saved) {
    note.thread.push({ role: "user", text: question, time: new Date().toISOString() });
  }
  note.thread.push({ role: "assistant", status: "pending", model, effort, time: new Date().toISOString(), text: "" });
  note.collapsed = false;
  refreshCard(note);
  try {
    const updated = await api("/api/ask", { id: note.id, question, model, effort, retry, saved });
    if (updated.deleted) return;
    replaceNote(prepNote(updated));
    api("/api/notes").then((d) => { S.undo = d.undo || null; S.version = d.version; updateUndo(); }).catch(() => {});
  } catch (e) {
    // Notizliste kann inzwischen neu geladen worden sein: aktuelles Objekt verwenden
    const cur = S.notes.find((x) => x.id === note.id) || note;
    const last = cur.thread[cur.thread.length - 1];
    if (last && last.role === "assistant" && last.status === "pending") {
      Object.assign(last, { status: "error", text: tr("Error: {msg}", { msg: e.message }) });
    } else {
      cur.thread.push({ role: "assistant", status: "error", text: tr("Error: {msg}", { msg: e.message }), time: new Date().toISOString() });
    }
    refreshCard(cur);
  }
}

function replaceNote(updated) {
  const i = S.notes.findIndex((x) => x.id === updated.id);
  if (i < 0) return;
  S.notes[i] = updated;
  refreshCard(updated);
}

// Falls die Seite neu geladen wurde, während Antworten laufen
async function pollPending() {
  if (!S.notes.some(isPending)) return;
  setTimeout(async () => {
    try {
      const doc = await api("/api/notes");
      for (const n of doc.notes) {
        const mine = S.notes.find((x) => x.id === n.id);
        if (mine && isPending(mine) && !isPending(n)) replaceNote(prepNote(n));
      }
    } catch {}
    pollPending();
  }, 3000);
}

// Laufzeit-Anzeige bei wartenden Antworten
setInterval(() => {
  document.querySelectorAll(".elapsed").forEach((el) => {
    el.textContent = `${Math.max(0, Math.round((Date.now() - +el.dataset.start) / 1000))} s`;
  });
}, 1000);

window.addEventListener("resize", debounce(() => layoutNotes(), 150));

// Wurde das PDF in einem anderen Programm verändert (z. B. in Edge kommentiert)?
window.addEventListener("focus", () => {
  if (!S.pdf || S.notes.some(isPending)) return;
  refreshNotes().catch(() => {});
});


// ---------------------------------------------------------------------------
// Seitenleiste: Inhalt, Seiten, Notizen
// ---------------------------------------------------------------------------

const sidebar = $("sidebar");

function initSidebar() {
  let st = {};
  try { st = JSON.parse(localStorage.getItem("pdftutor-sidebar") || "{}"); } catch {}
  S.sidebar.tab = ["toc", "thumbs", "notes"].includes(st.tab) ? st.tab : "toc";
  sidebar.querySelectorAll(".sb-tab").forEach((b) => b.addEventListener("click", () => showTab(b.dataset.tab)));
  $("sidebarBtn").onclick = () => toggleSidebar(!S.sidebar.open);
  buildNoteFilters();
  toggleSidebar(!!st.open, false);
}

function saveSidebar() {
  try { localStorage.setItem("pdftutor-sidebar", JSON.stringify({ open: S.sidebar.open, tab: S.sidebar.tab })); } catch {}
}

function toggleSidebar(open, save = true) {
  S.sidebar.open = open;
  sidebar.hidden = !open;
  $("sidebarBtn").classList.toggle("on", open);
  if (open) showTab(S.sidebar.tab, false);
  if (save) saveSidebar();
  layoutNotes();
}

function showTab(tab, save = true) {
  S.sidebar.tab = tab;
  sidebar.querySelectorAll(".sb-tab").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  sidebar.querySelectorAll(".sb-pane").forEach((p) => (p.hidden = p.dataset.tab !== tab));
  if (tab === "toc") buildOutline();
  if (tab === "thumbs") buildThumbs();
  if (tab === "notes") renderSidebarNotes();
  if (save) saveSidebar();
}

// --- Inhaltsverzeichnis ------------------------------------------------------

async function resolveDest(dest) {
  try {
    const d = typeof dest === "string" ? await S.pdf.getDestination(dest) : dest;
    if (!Array.isArray(d) || !d[0]) return null;
    const idx = typeof d[0] === "object" ? await S.pdf.getPageIndex(d[0]) : d[0];
    const page = idx + 1;
    let y = null;
    const p = S.pages[page - 1];
    // XYZ: [ref, XYZ, links, oben, zoom]; FitH/FitBH: [ref, FitH, oben]
    const name = d[1]?.name;
    const top = name === "XYZ" ? d[3] : name === "FitH" || name === "FitBH" ? d[2] : null;
    if (p && typeof top === "number") {
      const [, vy] = p.vp1.convertToViewportPoint(typeof d[2] === "number" && name === "XYZ" ? d[2] : 0, top);
      y = Math.max(0, Math.min(1, vy / p.vp1.height));
    }
    return { page, y };
  } catch {
    return null;
  }
}

async function buildOutline() {
  const pane = $("sbToc");
  if (S.sidebar.outline) return;
  S.sidebar.outline = "loading";
  let outline = null;
  try { outline = await S.pdf.getOutline(); } catch {}
  if (!outline || !outline.length) {
    pane.innerHTML = `<div class="sb-empty">${tr("This PDF has no table of contents.<br>You will find thumbnails under <b>Pages</b>.")}</div>`;
    S.sidebar.outline = "none";
    return;
  }
  const build = (items, depth) => {
    const ul = document.createElement("ul");
    ul.className = "toc";
    for (const it of items) {
      const li = document.createElement("li");
      const row = document.createElement("div");
      row.className = "toc-row";
      row.style.paddingLeft = `${6 + depth * 14}px`;
      const tg = document.createElement("button");
      tg.className = "toc-tg";
      const hasKids = it.items && it.items.length;
      tg.textContent = hasKids ? (depth === 0 ? "▾" : "▸") : "";
      tg.disabled = !hasKids;
      const a = document.createElement("a");
      a.className = "toc-link";
      a.textContent = it.title || tr("(untitled)");
      a.title = it.title || "";
      a.href = "#";
      a.addEventListener("click", async (e) => {
        e.preventDefault();
        if (!it.dest) return;
        const t = await resolveDest(it.dest);
        if (t) scrollToSpot(t.page, t.y);
      });
      row.append(tg, a);
      li.append(row);
      if (hasKids) {
        const sub = build(it.items, depth + 1);
        sub.hidden = depth > 0;
        li.append(sub);
        tg.addEventListener("click", () => { sub.hidden = !sub.hidden; tg.textContent = sub.hidden ? "▸" : "▾"; });
      }
      ul.append(li);
    }
    return ul;
  };
  pane.replaceChildren(build(outline, 0));
  S.sidebar.outline = "done";
}

function scrollToSpot(page, y) {
  const p = S.pages[page - 1];
  if (!p) return;
  const top = p.div.offsetTop + (y != null ? y * p.div.offsetHeight - 40 : -12);
  scroller.scrollTo({ top: Math.max(0, top), behavior: "auto" });
}

// --- Miniaturansichten ---------------------------------------------------------

let thumbObserver = null;
const THUMB_W = 150;

function buildThumbs() {
  const pane = $("sbThumbs");
  if (S.sidebar.thumbsBuilt) { markCurrentThumb(currentPage()); return; }
  S.sidebar.thumbsBuilt = true;
  thumbObserver = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) renderThumb(+e.target.dataset.num);
  }, { root: pane, rootMargin: "300px 0px" });
  for (const p of S.pages) {
    const t = document.createElement("div");
    t.className = "thumb";
    t.dataset.num = p.num;
    const box = document.createElement("div");
    box.className = "thumb-img";
    box.style.width = `${THUMB_W}px`;
    box.style.height = `${Math.round((THUMB_W * p.vp1.height) / p.vp1.width)}px`;
    const lab = document.createElement("div");
    lab.className = "thumb-lab";
    lab.textContent = pageLabel(p.num);
    t.append(box, lab);
    t.addEventListener("click", () => scrollToPage(p.num, false));
    pane.append(t);
    thumbObserver.observe(t);
  }
  markCurrentThumb(currentPage());
}

const thumbState = new Map(); // Seite -> "busy" | "done"
async function renderThumb(num) {
  if (thumbState.get(num)) return;
  thumbState.set(num, "busy");
  const el = $("sbThumbs").querySelector(`.thumb[data-num="${num}"] .thumb-img`);
  try {
    const page = await S.pdf.getPage(num);
    const vp = page.getViewport({ scale: (THUMB_W * 2) / page.getViewport({ scale: 1 }).width });
    const c = document.createElement("canvas");
    c.width = Math.floor(vp.width);
    c.height = Math.floor(vp.height);
    await page.render({ canvasContext: c.getContext("2d"), viewport: vp }).promise;
    el.replaceChildren(c);
    thumbState.set(num, "done");
  } catch {
    thumbState.delete(num);
  }
}

function invalidateThumbs() {
  if (!S.sidebar.thumbsBuilt) return;
  thumbState.clear();
  const pane = $("sbThumbs");
  const pr = pane.getBoundingClientRect();
  pane.querySelectorAll(".thumb").forEach((t) => {
    const b = t.getBoundingClientRect();
    if (b.bottom > pr.top - 300 && b.top < pr.bottom + 300) renderThumb(+t.dataset.num);
  });
}

function markCurrentThumb(n) {
  if (!S.sidebar.thumbsBuilt) return;
  const pane = $("sbThumbs");
  pane.querySelector(".thumb.cur")?.classList.remove("cur");
  const t = pane.querySelector(`.thumb[data-num="${n}"]`);
  if (!t) return;
  t.classList.add("cur");
  if (!pane.hidden) {
    const pr = pane.getBoundingClientRect(), b = t.getBoundingClientRect();
    if (b.top < pr.top || b.bottom > pr.bottom) pane.scrollTop += b.top - pr.top - pr.height / 3;
  }
}

// --- Notizliste ----------------------------------------------------------------

const noteFilter = { colors: new Set(), kind: "all" };

function buildNoteFilters() {
  const box = $("sbNoteColors");
  COLORS.forEach((c, i) => {
    const b = document.createElement("button");
    b.className = "sw small";
    b.title = tr("Only {color}", { color: tr(c.name) });
    b.style.background = css(c.hl);
    b.addEventListener("click", () => {
      noteFilter.colors.has(i) ? noteFilter.colors.delete(i) : noteFilter.colors.add(i);
      b.classList.toggle("on", noteFilter.colors.has(i));
      renderSidebarNotes();
    });
    box.append(b);
  });
  $("sbNoteKind").onchange = (e) => { noteFilter.kind = e.target.value; renderSidebarNotes(); };
}

function noteMatches(n) {
  if (noteFilter.colors.size && !noteFilter.colors.has(colorIndex(n.color))) return false;
  const th = n.thread || [];
  switch (noteFilter.kind) {
    case "comment": return th.some((m, i) => m.role === "user" && !(th[i + 1] && th[i + 1].role === "assistant"));
    case "claude": return th.some((m) => m.role === "assistant");
    case "plain": return th.length === 0 && n.subtype !== "Text";
    case "sticky": return n.subtype === "Text";
    default: return true;
  }
}

function renderSidebarNotes() {
  if (!S.sidebar.open || S.sidebar.tab !== "notes") return;
  const list = $("sbNoteList");
  const sorted = [...S.notes].sort((a, b) => a.page - b.page || (a.rects[0]?.y || 0) - (b.rects[0]?.y || 0));
  const shown = sorted.filter(noteMatches);
  $("sbNoteCount").textContent = shown.length === S.notes.length
    ? (S.notes.length === 1 ? tr("1 note") : tr("{n} notes", { n: S.notes.length }))
    : tr("{a} of {b}", { a: shown.length, b: S.notes.length });
  list.replaceChildren();
  if (!shown.length) {
    list.innerHTML = `<div class="sb-empty">${esc(S.notes.length ? tr("No notes for this filter.") : tr("No highlights or notes yet."))}</div>`;
    return;
  }
  let lastPage = 0;
  for (const n of shown) {
    if (n.page !== lastPage) {
      lastPage = n.page;
      const h = document.createElement("div");
      h.className = "sbn-page";
      h.textContent = tr("Page {n}", { n: pageLabel(n.page) });
      list.append(h);
    }
    const th = n.thread || [];
    const firstUser = th.find((m) => m.role === "user");
    const claude = th.some((m) => m.role === "assistant");
    const icon = { Highlight: `<span class="k-hl" style="background:${css(n.color)}">ab</span>`, Underline: `<u style="text-decoration-color:${css(n.color)}">U</u>`, StrikeOut: `<s style="text-decoration-color:${css(n.color)}">S</s>`, Text: `<span class="k-note" style="background:${css(n.color)}"></span>` }[n.subtype] || "•";
    const el = document.createElement("div");
    el.className = "sbn" + (n.id === S.activeId ? " on" : "");
    el.dataset.id = n.id;
    el.innerHTML = `<div class="sbn-k">${icon}</div><div class="sbn-b">
        ${n.text ? `<div class="sbn-q">${esc(n.text.length > 140 ? n.text.slice(0, 140) + " …" : n.text)}</div>` : ""}
        ${firstUser ? `<div class="sbn-c">${esc(firstUser.text.replace(/[#*$`\\]/g, "").slice(0, 120))}</div>` : ""}
        ${claude ? `<span class="sbn-badge">Claude</span>` : ""}
        ${!n.text && !firstUser ? `<div class="sbn-c muted">${esc(kindName(n.subtype))}</div>` : ""}
      </div>`;
    el.addEventListener("click", () => focusNote(n));
    list.append(el);
  }
}

function focusNote(n) {
  const r = n.rects[0];
  if (r) scrollToSpot(r.page, r.y);
  S.activeId = n.id;
  if (hasCard(n)) openNote(n, false);
  else { renderHighlights(); renderSidebarNotes(); }
  setTimeout(() => {
    document.querySelectorAll(`.hl[data-id="${CSS.escape(n.id)}"]`).forEach((d) => {
      d.classList.remove("flash");
      void d.offsetWidth;
      d.classList.add("flash");
    });
  }, 60);
}

// ---------------------------------------------------------------------------
// Suche (Strg+F)
// ---------------------------------------------------------------------------

const findBar = $("findBar");
const findInput = $("findInput");

// Text für die Suche aufbauen: Leerraum zusammenfassen, Kleinbuchstaben, Trennstrich am
// Zeilenende entfernen. `pieces`: [{text, node}] oder {eol: true}. Liefert Text und
// zu jedem Zeichen die Herkunft (Textknoten und Position), falls vorhanden.
function buildSearchText(pieces) {
  let str = "";
  const map = [];
  const push = (ch, node, off) => {
    const isWs = /\s/.test(ch);
    if (isWs) {
      if (!str.length || str[str.length - 1] === " ") return;
      ch = " ";
    } else {
      const l = ch.toLowerCase();
      if (l.length === 1) ch = l;
    }
    str += ch;
    map.push(node ? [node, off] : null);
  };
  for (const pc of pieces) {
    if (pc.eol) {
      // Silbentrennung „Stichproben-“ + Zeilenende: Strich entfernen, nichts einfügen
      if (str.length > 1 && str[str.length - 1] === "-" && /[a-zäöüß]/.test(str[str.length - 2])) {
        str = str.slice(0, -1);
        map.pop();
      } else push(" ", null, 0);
      continue;
    }
    const t = pc.text || "";
    for (let i = 0; i < t.length; i++) push(t[i], pc.node, i);
  }
  return { str, map };
}

const normQuery = (q) => q.replace(/\s+/g, " ").trim().toLowerCase();

function findAll(str, q) {
  const out = [];
  if (!q) return out;
  let i = str.indexOf(q);
  while (i >= 0) { out.push(i); i = str.indexOf(q, i + Math.max(1, q.length)); }
  return out;
}

async function pageSearchText(num) {
  if (S.find.texts.has(num)) return S.find.texts.get(num);
  const page = await S.pdf.getPage(num);
  const tc = await page.getTextContent();
  const pieces = [];
  for (const it of tc.items) {
    if (it.str === undefined) continue;
    pieces.push({ text: it.str });
    if (it.hasEOL) pieces.push({ eol: true });
  }
  const t = buildSearchText(pieces).str;
  S.find.texts.set(num, t);
  return t;
}

function openFind() {
  findBar.hidden = false;
  findInput.focus();
  findInput.select();
}

function closeFind() {
  findBar.hidden = true;
  S.find.q = "";
  S.find.matches = [];
  S.find.cur = -1;
  S.find.run++;
  for (const p of S.pages) p.find.replaceChildren();
  $("findCount").textContent = "";
}

const runFind = debounce(() => startFind(findInput.value), 250);
findInput.addEventListener("input", runFind);
findInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") { e.preventDefault(); if (normQuery(findInput.value) !== S.find.q) startFind(findInput.value, e.shiftKey ? -1 : 1); else stepFind(e.shiftKey ? -1 : 1); }
  if (e.key === "Escape") { e.preventDefault(); closeFind(); }
});
$("findPrev").onclick = () => stepFind(-1);
$("findNext").onclick = () => stepFind(1);
$("findClose").onclick = closeFind;
// Menü „Weitere Funktionen“
const moreMenu = $("moreMenu");
$("moreBtn").onclick = (e) => { e.stopPropagation(); moreMenu.hidden = !moreMenu.hidden; };
document.addEventListener("click", (e) => { if (!e.target.closest("#moreMenu")) moreMenu.hidden = true; });
moreMenu.querySelectorAll("button").forEach((b) => b.addEventListener("click", () => {
  moreMenu.hidden = true;
  if (b.dataset.act === "collapse") { $("collapseAll").onclick(); return; }
  const url = b.dataset.act === "export" ? `export.html?id=${DOC_ID}` : `organize.html?id=${DOC_ID}`;
  window.PT.api("/api/window/open", { url }).catch((e) => showToast(e.message));
}));

$("findBtn").onclick = () => (findBar.hidden ? openFind() : closeFind());

async function startFind(raw, dir = 1) {
  const q = normQuery(raw);
  const run = ++S.find.run;
  S.find.q = q;
  S.find.matches = [];
  S.find.cur = -1;
  for (const p of S.pages) p.find.replaceChildren();
  const cnt = $("findCount");
  if (!q) { cnt.textContent = ""; return; }
  cnt.textContent = tr("Searching …");
  // ab der aktuellen Seite suchen, damit der erste Treffer nahe liegt
  const start = currentPage();
  let first = -1;
  for (let k = 0; k < S.pages.length; k++) {
    if (run !== S.find.run) return;
    const num = k + 1;
    const t = await pageSearchText(num);
    if (run !== S.find.run) return;
    findAll(t, q).forEach((_, i) => S.find.matches.push({ page: num, k: i }));
    if (first < 0 && num >= start && S.find.matches.some((m) => m.page === num)) first = S.find.matches.findIndex((m) => m.page === num);
    if (k % 20 === 19) cnt.textContent = tr("Searching … {p} %", { p: Math.round((100 * num) / S.pages.length) });
  }
  if (!S.find.matches.length) { cnt.textContent = tr("No matches"); findBar.classList.add("none"); return; }
  findBar.classList.remove("none");
  for (const p of S.pages) if (p.rendered) highlightFindOnPage(p);
  goToMatch(first >= 0 ? first : 0);
}

function stepFind(dir) {
  const n = S.find.matches.length;
  if (!n) return;
  goToMatch((S.find.cur + dir + n) % n);
}

async function goToMatch(i) {
  S.find.cur = i;
  const m = S.find.matches[i];
  $("findCount").textContent = tr("{a} of {b}", { a: i + 1, b: S.find.matches.length });
  const p = S.pages[m.page - 1];
  const run = S.find.run;
  // Seite in den Blick bringen und warten, bis sie samt Textebene gezeichnet ist
  const vis = scroller.scrollTop, vh = scroller.clientHeight;
  if (p.div.offsetTop > vis + vh || p.div.offsetTop + p.div.offsetHeight < vis) scrollToPage(m.page, false);
  for (let t = 0; t < 60 && !p.rendered; t++) await new Promise((r) => setTimeout(r, 50));
  if (run !== S.find.run) return;
  for (const q of S.pages) q.find.querySelectorAll(".find.cur").forEach((d) => d.classList.remove("cur"));
  highlightFindOnPage(p);
  const cur = p.find.querySelector(`.find[data-k="${m.k}"]`);
  if (cur) {
    const top = p.div.offsetTop + cur.offsetTop;
    if (top < scroller.scrollTop + 60 || top > scroller.scrollTop + scroller.clientHeight - 80) {
      scroller.scrollTo({ top: Math.max(0, top - scroller.clientHeight / 3) });
    }
  }
}

// Treffer auf einer gezeichneten Seite markieren (über die Textebene)
function highlightFindOnPage(p) {
  p.find.replaceChildren();
  const q = S.find.q;
  if (!q || !p.rendered) return;
  const pieces = [];
  const walker = document.createTreeWalker(p.text, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (n.nodeType === 3) pieces.push({ text: n.data, node: n });
    else if (n.nodeName === "BR") pieces.push({ eol: true });
  }
  const { str, map } = buildSearchText(pieces);
  const hits = findAll(str, q);
  const pb = p.div.getBoundingClientRect();
  const cur = S.find.matches[S.find.cur];
  hits.forEach((idx, k) => {
    let a = idx, b = idx + q.length - 1;
    while (a <= b && !map[a]) a++;
    while (b >= a && !map[b]) b--;
    if (a > b) return;
    const r = document.createRange();
    try {
      r.setStart(map[a][0], map[a][1]);
      r.setEnd(map[b][0], map[b][1] + 1);
    } catch { return; }
    for (const cr of r.getClientRects()) {
      if (cr.width < 1) continue;
      const d = document.createElement("div");
      d.className = "find" + (cur && cur.page === p.num && cur.k === k ? " cur" : "");
      d.dataset.k = k;
      Object.assign(d.style, {
        left: `${((cr.left - pb.left) / pb.width) * 100}%`, top: `${((cr.top - pb.top) / pb.height) * 100}%`,
        width: `${(cr.width / pb.width) * 100}%`, height: `${(cr.height / pb.height) * 100}%`,
      });
      p.find.append(d);
    }
  });
}

init().catch((e) => {
  console.error(e);
  showBanner(e.notFound ? tr("Document not found. Go back to the library to relink it.") : tr("Error while loading: {msg}", { msg: e.message }));
});
