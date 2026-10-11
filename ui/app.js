import * as pdfjsLib from "./vendor/pdf.min.mjs";

pdfjsLib.GlobalWorkerOptions.workerSrc = "vendor/pdf.worker.min.mjs";

const $ = (id) => document.getElementById(id);
const scroller = $("scroller");
const pagesEl = $("pages");
const notesEl = $("notes");
const askBtn = $("askBtn");

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
  pendingSel: null, // Auswahl, für die der Frage-Knopf angezeigt wird
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

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}

const saveState = debounce((obj) => api("/api/state", obj).catch(() => {}), 800);

function pageLabel(num) {
  const l = S.labels && S.labels[num - 1];
  return l && l !== String(num) ? l : String(num);
}

function modelLabel(id) {
  const m = (S.info?.models || []).find((m) => m.id === id);
  return m ? m.label.replace(/\s*\(.*\)/, "") : id;
}

// Markdown + LaTeX ----------------------------------------------------------

function renderMD(src) {
  const math = [];
  const ph = (t, display) => { math.push({ t, display }); return `MATHPH${math.length - 1}X`; };
  let s = String(src || "");
  s = s.replace(/\$\$([\s\S]+?)\$\$/g, (_, t) => ph(t, true));
  s = s.replace(/\\\[([\s\S]+?)\\\]/g, (_, t) => ph(t, true));
  s = s.replace(/\\\(([\s\S]+?)\\\)/g, (_, t) => ph(t, false));
  s = s.replace(/(^|[^\\$])\$(?!\s)([^$\n]+?)(?<![\s\\])\$(?!\d)/g, (_, pre, t) => pre + ph(t, false));
  let html = window.marked.parse(s, { gfm: true, breaks: false });
  html = window.DOMPurify.sanitize(html, { ADD_ATTR: ["target"], FORBID_TAGS: ["img", "iframe", "form"] });
  html = html.replace(/<a /g, '<a target="_blank" rel="noopener" ');
  return html.replace(/MATHPH(\d+)X/g, (_, i) => {
    const { t, display } = math[+i];
    try {
      return window.katex.renderToString(t, { displayMode: display, throwOnError: false, output: "html" });
    } catch {
      return esc(t);
    }
  });
}

// ---------------------------------------------------------------------------
// PDF laden und Seiten aufbauen
// ---------------------------------------------------------------------------

async function init() {
  S.info = await api("/api/info");
  window.PT.setTitle(`${S.info.title} – PDF-Tutor`);
  $("bookTitle").textContent = S.info.title;
  if (!S.info.claudeFound) {
    showBanner("Der Befehl „claude“ wurde nicht gefunden. Lesen geht, Fragen werden aber fehlschlagen.");
  }

  S.doc = S.info.doc;
  $("modeSelect").value = S.doc.mode || "auto";
  updateDocStatus();
  pollStatus();

  const sel = $("modelSelect");
  for (const m of S.info.models) sel.add(new Option(m.label, m.id));

  const notesDoc = await api("/api/notes");
  S.notes = notesDoc.notes || [];
  S.version = notesDoc.version;
  const st = notesDoc.state || {};
  S.model = st.model || S.info.defaultModel;
  sel.value = S.model;

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
  const text = document.createElement("div");
  text.className = "textLayer";
  div.append(hl, text);
  pagesEl.append(div);
  S.pages.push({
    num: pdfPage.pageNumber, div, hl, text, w0: vp.width, h0: vp.height, vp1: vp, pdfPage,
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
}

// Neuen Stand der Notizen holen; wurde das PDF von außen verändert, neu laden
async function refreshNotes(forceReload = false) {
  const doc = await api("/api/notes");
  const changed = doc.version !== S.version;
  S.version = doc.version;
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
    try { p.pdfPage = await S.pdf.getPage(p.num); } catch { return; } finally { p.loading = false; }
    if (p.renderId !== id) return;
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
}

function unrender(p) {
  p.renderId++;
  if (p.task) { try { p.task.cancel(); } catch {} p.task = null; }
  if (!p.rendered) return;
  p.div.querySelector("canvas")?.remove();
  p.text.replaceChildren();
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
  const l = pageLabel(n);
  $("pageLabel").textContent = l !== String(n) ? `(gedruckt: ${l})` : "";
  saveState({ lastPage: n });
}

let scrollRaf = 0;
scroller.addEventListener("scroll", () => {
  hideAskBtn();
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
$("modelSelect").onchange = (e) => { S.model = e.target.value; saveState({ model: S.model }); };
$("modeSelect").onchange = async (e) => {
  try { S.doc = await api("/api/state", { mode: e.target.value }); updateDocStatus(); } catch {}
};

// Status des Dokuments (Vorbereitung, Umfang, Modus)
const fmtNum = (n) => Number(n || 0).toLocaleString("de-AT");
const modeName = (m) => ({ volltext: "Volltext", suche: "Suche" }[m] || m || "");

function updateDocStatus() {
  const d = S.doc;
  if (!d) return;
  const el = $("docStatus");
  if (!d.textReady) {
    el.textContent = "Dokument wird vorbereitet …";
  } else {
    const tk = d.tokens >= 1000 ? `${fmtNum(Math.round(d.tokens / 1000))} Tsd.` : fmtNum(d.tokens);
    let t = `${tk} Tokens → ${modeName(d.effectiveMode)}`;
    if (d.imagesDone < d.pages) t += ` · Bilder ${Math.floor((100 * d.imagesDone) / d.pages)} %`;
    el.textContent = t;
  }
  if (d.textReady && d.tokens != null) {
    const autoMode = d.tokens <= d.limit ? "Volltext" : "Suche";
    $("modeSelect").options[0].textContent = `Kontext: automatisch (${autoMode})`;
  }
  el.title = `Automatisch: Volltext bis ${fmtNum(d.limit)} Tokens, darüber Suche.` +
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
  for (const n of S.notes) {
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
// Markieren
// ---------------------------------------------------------------------------

pagesEl.addEventListener("mousedown", (e) => {
  const tl = e.target.closest(".textLayer");
  if (tl) tl.classList.add("selecting");
  hideAskBtn();
});

document.addEventListener("mouseup", (e) => {
  document.querySelectorAll(".textLayer.selecting").forEach((t) => t.classList.remove("selecting"));
  if (e.target === askBtn || e.target.closest("#notes")) return;
  setTimeout(() => handleSelection(e), 0);
});

function handleSelection(e) {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
    hitTestHighlight(e.clientX, e.clientY);
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
  askBtn.style.left = `${Math.min(window.innerWidth - 130, last.right + 6)}px`;
  askBtn.style.top = `${Math.min(window.innerHeight - 40, last.bottom + 6)}px`;
  askBtn.hidden = false;
}

function hideAskBtn() {
  askBtn.hidden = true;
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

function hitTestHighlight(x, y) {
  for (const el of document.querySelectorAll(".hl:not(.draft)")) {
    const b = el.getBoundingClientRect();
    if (x >= b.left && x <= b.right && y >= b.top && y <= b.bottom) {
      const note = S.notes.find((n) => n.id === el.dataset.id);
      if (note) openNote(note, true);
      return;
    }
  }
}

askBtn.addEventListener("mousedown", (e) => e.preventDefault()); // Auswahl nicht verlieren
askBtn.addEventListener("click", () => {
  if (!S.pendingSel) return;
  S.draft = S.pendingSel;
  S.pendingSel = null;
  hideAskBtn();
  window.getSelection().removeAllRanges();
  renderHighlights();
  renderAllCards();
  const ta = notesEl.querySelector(".card.draft textarea");
  if (ta) {
    ta.focus({ preventScroll: true });
    requestAnimationFrame(() => requestAnimationFrame(() => {
      ta.closest(".card")?.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
    }));
  }
});

// ---------------------------------------------------------------------------
// Markierungen zeichnen
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
  for (const n of S.notes) for (const r of n.rects) add(r, n.id === S.activeId ? "active" : "", n.id);
  if (S.draft) for (const r of S.draft.rects) add(r, "draft");
}

function setHlClass(id, cls, on) {
  document.querySelectorAll(`.hl[data-id="${id}"]`).forEach((d) => d.classList.toggle(cls, on));
}

// ---------------------------------------------------------------------------
// Notizkarten
// ---------------------------------------------------------------------------

const isPending = (n) => (n.thread || []).some((m) => m.status === "pending");

function anchorTop(item) {
  const r0 = item.rects.filter((r) => r.page === item.page);
  const rs = r0.length ? r0 : item.rects;
  const p = S.pages[(rs[0]?.page || item.page) - 1];
  if (!p) return 0;
  const minY = Math.min(...rs.map((r) => r.y));
  return p.div.offsetTop + minY * p.div.offsetHeight;
}

function renderAllCards() {
  notesEl.querySelectorAll(".card").forEach((c) => c.remove());
  $("notesEmpty").hidden = S.notes.length > 0 || !!S.draft;
  for (const n of S.notes) notesEl.append(buildCard(n));
  if (S.draft) notesEl.append(buildDraftCard());
  layoutNotes();
}

function refreshCard(note) {
  const old = notesEl.querySelector(`.card[data-id="${note.id}"]`);
  const fresh = buildCard(note);
  if (old) old.replaceWith(fresh); else notesEl.append(fresh);
  layoutNotes();
}

function buildCard(n) {
  const card = document.createElement("div");
  card.className = "card" + (n.collapsed ? " collapsed" : "") + (n.id === S.activeId ? " active" : "");
  card.dataset.id = n.id;
  const thread = n.thread || [];
  const kind = { Highlight: "Markierung", Underline: "Unterstreichung", StrikeOut: "Durchstreichung", Squiggly: "Markierung", Text: "Notiz", FreeText: "Textfeld" }[n.subtype] || "Anmerkung";
  const quote = n.text ? (n.text.length > 90 ? n.text.slice(0, 90) + " …" : n.text) : "";

  let body = "";
  if (n.collapsed) {
    const q = thread.find((m) => m.role === "user");
    const a = [...thread].reverse().find((m) => m.role === "assistant" && m.status === "ok");
    body += q ? `<div class="msg q">${esc(q.text)}</div>` : "";
    body += a ? `<div class="preview">${esc(a.text.replace(/[#*$`\\]/g, "").slice(0, 300))}</div>` : "";
  } else {
    for (const m of thread) {
      if (m.role === "user") {
        const who = m.author && n.own === false ? `<span class="who">${esc(m.author)}</span>` : "";
        body += `<div class="msg q">${who}${renderMD(m.text)}</div>`;
      } else if (m.status === "pending") {
        const start = Date.parse(m.time) || Date.now();
        body += `<div class="msg pending"><span class="spinner"></span>Claude (${esc(modelLabel(m.model))}) denkt nach … <span class="elapsed" data-start="${start}"></span></div>`;
      } else if (m.status === "error") {
        body += `<div class="msg err">${esc(m.text)}<button class="btn retry">Erneut versuchen</button></div>`;
      } else {
        body += `<div class="msg a">${renderMD(m.text)}</div>`;
        const u = m.usage;
        const tip = u ? `Eingabe ${fmtNum(u.input_tokens)} · aus Cache ${fmtNum(u.cache_read_input_tokens)} · in Cache geschrieben ${fmtNum(u.cache_creation_input_tokens)} · Ausgabe ${fmtNum(u.output_tokens)} Tokens` : "";
        body += `<div class="meta" title="${esc(tip)}">${esc(modelLabel(m.model))}${m.mode ? ` · ${modeName(m.mode)}` : ""}${m.seconds ? ` · ${m.seconds} s` : ""}${m.unsaved ? ` · <span class="unsaved" title="Das PDF ist gerade in einem anderen Programm geöffnet. Die Antwort wird gespeichert, sobald es wieder geht.">noch nicht im PDF gespeichert</span>` : ""}</div>`;
      }
    }
    if (!isPending(n)) {
      body += `<div class="composer">
        <textarea rows="1" placeholder="${thread.length ? "Nachfrage …" : "Deine Frage …"}"></textarea>
        <div class="row"><span class="hint">Strg+Enter sendet</span><button class="btn primary send">Senden</button></div>
      </div>`;
    }
  }

  card.innerHTML = `
    <div class="card-head">
      <span class="pg">S. ${esc(pageLabel(n.page))}</span>
      <span class="quote" title="${esc(n.text)}">${quote ? `„${esc(quote)}“` : esc(kind)}</span>
      <button class="icon toggle" title="${n.collapsed ? "Aufklappen" : "Einklappen"}">${n.collapsed ? "▸" : "▾"}</button>
      <button class="icon del" title="Notiz löschen">×</button>
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

  const ta = card.querySelector("textarea");
  if (ta) {
    const send = () => { const q = ta.value.trim(); if (q) ask(n, q, false); };
    card.querySelector(".send").addEventListener("click", send);
    ta.addEventListener("keydown", (e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); send(); } });
    autoGrow(ta);
  }
  return card;
}

function buildDraftCard() {
  const d = S.draft;
  const card = document.createElement("div");
  card.className = "card draft active";
  const quote = d.text.length > 90 ? d.text.slice(0, 90) + " …" : d.text;
  card.innerHTML = `
    <div class="card-head"><span class="pg">S. ${esc(pageLabel(d.page))}</span><span class="quote" title="${esc(d.text)}">„${esc(quote)}“</span></div>
    <div class="card-body"><div class="composer">
      <textarea rows="2" placeholder="Deine Frage zu dieser Stelle …"></textarea>
      <div class="row"><span class="hint">Strg+Enter sendet · Esc bricht ab</span>
        <button class="btn cancel">Abbrechen</button><button class="btn primary send">Fragen</button></div>
    </div></div>`;
  const ta = card.querySelector("textarea");
  const cancel = () => { S.draft = null; renderHighlights(); renderAllCards(); };
  const send = async () => {
    const q = ta.value.trim();
    if (!q) return;
    card.querySelectorAll("button, textarea").forEach((el) => (el.disabled = true));
    try {
      const rects = d.rects.filter((r) => r.page === d.page).map(pdfRect);
      const note = prepNote(await api("/api/notes/create", { page: d.page, pdfRects: rects, text: d.text, question: q }));
      S.notes.push(note);
      S.draft = null;
      S.activeId = note.id;
      renderHighlights();
      renderAllCards();
      reloadPdf().catch(() => {});
      ask(note, q, false, true);
    } catch (e) {
      card.querySelectorAll("button, textarea").forEach((el) => (el.disabled = false));
      alert("Konnte die Notiz nicht anlegen: " + e.message);
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
      const card = notesEl.querySelector(`.card[data-id="${n.id}"]`);
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
  if (!confirm("Diese Notiz mit allen Fragen und Antworten löschen?")) return;
  try {
    await api("/api/notes/delete", { id: n.id });
    S.notes = S.notes.filter((x) => x.id !== n.id);
    if (S.activeId === n.id) S.activeId = null;
    renderHighlights();
    renderAllCards();
    await reloadPdf();
  } catch (e) {
    alert("Löschen fehlgeschlagen: " + e.message);
  }
}

// ---------------------------------------------------------------------------
// Fragen an Claude
// ---------------------------------------------------------------------------

async function ask(note, question, retry, saved = false) {
  const model = S.model;
  // sofort anzeigen (der Server macht dasselbe)
  if (retry) {
    const last = note.thread[note.thread.length - 1];
    if (last && last.status === "error") note.thread.pop();
  } else if (!saved) {
    note.thread.push({ role: "user", text: question, time: new Date().toISOString() });
  }
  note.thread.push({ role: "assistant", status: "pending", model, time: new Date().toISOString(), text: "" });
  note.collapsed = false;
  refreshCard(note);
  try {
    const updated = await api("/api/ask", { id: note.id, question, model, retry, saved });
    if (updated.deleted) return;
    replaceNote(prepNote(updated));
  } catch (e) {
    const last = note.thread[note.thread.length - 1];
    Object.assign(last, { status: "error", text: "Fehler: " + e.message });
    refreshCard(note);
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

init().catch((e) => {
  console.error(e);
  showBanner(e.notFound ? `Dokument nicht gefunden. Zurück zur Bibliothek, um es neu zu verknüpfen.` : "Fehler beim Laden: " + e.message);
});
