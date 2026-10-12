// Lernzettel: alle Notizen eines Dokuments aufbereitet, zum Drucken oder Speichern.
import { esc, renderMD } from "./render.js";

const $ = (id) => document.getElementById(id);
const DOC_ID = new URLSearchParams(location.search).get("id") || "";
const api = (path, body) => window.PT.api(path, body);

const COLORS = [
  { name: "Gelb", hl: [1, 0.85, 0], line: [0.9, 0.68, 0], md: "🟨" },
  { name: "Grün", hl: [0.49, 0.85, 0.39], line: [0.18, 0.62, 0.22], md: "🟩" },
  { name: "Blau", hl: [0.45, 0.75, 1], line: [0.15, 0.45, 0.9], md: "🟦" },
  { name: "Rosa", hl: [1, 0.56, 0.75], line: [0.88, 0.2, 0.5], md: "🟪" },
  { name: "Orange", hl: [1, 0.65, 0.25], line: [0.95, 0.45, 0.05], md: "🟧" },
];
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

const S = { info: null, notes: [], colors: new Set(), entries: [] };

function pageLabel(n) {
  const l = S.info?.labels?.[n - 1];
  return l && l !== String(n) ? `${n} (gedruckt: ${l})` : String(n);
}

// Notiz in Einträge zerlegen: Kommentare und Frage/Antwort-Paare in Thread-Reihenfolge
function entriesOf(n) {
  const th = (n.thread || []).filter((m) => (m.status || "ok") === "ok");
  const out = [];
  for (let i = 0; i < th.length; i++) {
    const m = th[i];
    if (m.role === "user") {
      const next = th[i + 1];
      if (next && next.role === "assistant") { out.push({ type: "qa", q: m.text, a: next.text }); i++; }
      else out.push({ type: "comment", text: m.text });
    } else {
      out.push({ type: "answer", a: m.text }); // Antwort ohne erhaltene Frage
    }
  }
  return out;
}

function selected() {
  const o = { plain: $("optPlain").checked, comments: $("optComments").checked, claude: $("optClaude").checked, sticky: $("optSticky").checked };
  const res = [];
  for (const n of S.notes) {
    if (S.colors.size && !S.colors.has(colorIndex(n.color))) continue;
    const isSticky = n.subtype === "Text" || n.subtype === "FreeText";
    if (isSticky && !o.sticky) continue;
    let ents = entriesOf(n);
    if (!isSticky) ents = ents.filter((e) => (e.type === "comment" ? o.comments : o.claude));
    if (!ents.length && (isSticky || !o.plain || !n.text)) continue;
    res.push({ n, ents, isSticky });
  }
  return res;
}

function buildSheet() {
  const items = selected();
  const title = S.info?.title || "Dokument";
  const date = new Date().toLocaleDateString("de-AT", { day: "numeric", month: "long", year: "numeric" });
  let html = `<h1>${esc(title)}</h1><div class="sub">Lernzettel · ${esc(S.info?.file || "")} · Stand ${esc(date)} · ${items.length} ${items.length === 1 ? "Eintrag" : "Einträge"}</div>`;
  if (!items.length) html += `<p class="empty">Keine Notizen für diese Auswahl.</p>`;
  let page = 0;
  for (const { n, ents, isSticky } of items) {
    if (n.page !== page) {
      if (page) html += `</section>`;
      page = n.page;
      html += `<section><h2>Seite ${esc(pageLabel(n.page))}</h2>`;
    }
    const col = css(n.color || COLORS[0].hl);
    html += `<div class="item" style="--c:${col}">`;
    if (isSticky) {
      html += `<div class="sticky"><span class="lab">Notiz</span>`;
      for (const e of ents) html += e.type === "qa" ? qaHtml(e) : `<div>${renderMD(e.text || e.a)}</div>`;
      html += `</div></div>`;
      continue;
    }
    if (n.text) html += `<blockquote class="quote ${esc(n.subtype)}"><span>${esc(n.text)}</span></blockquote>`;
    for (const e of ents) {
      if (e.type === "comment") html += `<div class="comment"><span class="lab">Kommentar</span>${renderMD(e.text)}</div>`;
      else html += qaHtml(e);
    }
    html += `</div>`;
  }
  if (page) html += `</section>`;
  $("sheet").innerHTML = html;
  S.count = items.length;
}

function qaHtml(e) {
  return `<div class="qa">${e.q ? `<span class="lab">Frage</span><div class="q">${renderMD(e.q)}</div>` : ""}<span class="lab">Antwort von Claude</span><div class="a">${renderMD(e.a)}</div></div>`;
}

// --- Markdown ------------------------------------------------------------------

function buildMarkdown() {
  const items = selected();
  const title = S.info?.title || "Dokument";
  const date = new Date().toLocaleDateString("de-AT", { day: "numeric", month: "long", year: "numeric" });
  const out = [`# ${title}`, "", `*Lernzettel · ${S.info?.file || ""} · Stand ${date}*`, ""];
  const quoteLines = (t) => t.split(/\n/).map((l) => `> ${l}`).join("\n");
  let page = 0;
  for (const { n, ents, isSticky } of items) {
    if (n.page !== page) {
      page = n.page;
      out.push(`## Seite ${pageLabel(n.page)}`, "");
    }
    const mark = COLORS[colorIndex(n.color)].md;
    if (isSticky) {
      out.push(`${mark} **Notiz:** ${ents.map((e) => e.text || e.a).join("\n\n")}`, "");
      continue;
    }
    if (n.text) {
      const t = n.subtype === "StrikeOut" ? `~~${n.text}~~` : n.subtype === "Highlight" ? `==${n.text}==` : n.text;
      out.push(quoteLines(`${mark} ${t}`), "");
    }
    for (const e of ents) {
      if (e.type === "comment") out.push(`**Kommentar:** ${e.text}`, "");
      else {
        if (e.q) out.push(`**Frage:** ${e.q}`, "");
        out.push(`**Antwort von Claude:**`, "", e.a, "");
      }
    }
    out.push("---", "");
  }
  return out.join("\n");
}

// --- eigenständige HTML-Datei (Schriften und Formelsatz eingebettet) ------------------

async function dataUrl(url, type) {
  const blob = await (await fetch(url)).blob();
  return await new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(String(r.result).replace(/^data:[^;]*;/, `data:${type};`));
    r.onerror = rej;
    r.readAsDataURL(blob);
  });
}

async function buildHtmlFile() {
  let katexCss = await (await fetch("vendor/katex/katex.min.css")).text();
  const fonts = [...new Set([...katexCss.matchAll(/url\(fonts\/([^)]+?\.woff2)\)/g)].map((m) => m[1]))];
  const urls = {};
  await Promise.all(fonts.map(async (f) => { urls[f] = await dataUrl(`vendor/katex/fonts/${f}`, "font/woff2"); }));
  // nur woff2 verwenden, die übrigen Formate liegen nicht bei
  katexCss = katexCss.replace(/src:url\(fonts\/([^)]+?\.woff2)\) format\("woff2"\)[^;}]*/g, (_, f) => `src:url(${urls[f]}) format("woff2")`);
  const serif = await dataUrl("vendor/fonts/source-serif-4-latin-opsz-normal.woff2", "font/woff2");
  const serifIt = await dataUrl("vendor/fonts/source-serif-4-latin-opsz-italic.woff2", "font/woff2");
  const sheetCss = await (await fetch("sheet.css")).text();
  const title = `${S.info?.title || "Dokument"} – Lernzettel`;
  return `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="PDF-Tutor">
<title>${esc(title)}</title>
<style>
@font-face { font-family: "Source Serif 4"; src: url(${serif}) format("woff2"); font-weight: 200 900; font-style: normal; }
@font-face { font-family: "Source Serif 4"; src: url(${serifIt}) format("woff2"); font-weight: 200 900; font-style: italic; }
${katexCss}
body { margin: 0; background: #e2e5e9; }
@media screen { .sheet { margin: 24px auto; box-shadow: 0 1px 3px rgba(0,0,0,.12), 0 4px 14px rgba(0,0,0,.06); } }
@media print { body { background: #fff; } }
${sheetCss}
</style>
</head>
<body>
<article class="sheet">
${$("sheet").innerHTML}
</article>
</body>
</html>
`;
}

// --- Speichern, Drucken --------------------------------------------------------

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
  toastTimer = setTimeout(() => (t.hidden = true), action ? 8000 : 4000);
}

const safeName = (s) => String(s || "Lernzettel").replace(/[\\/:*?"<>|]+/g, " ").trim().slice(0, 120);

async function save(kind) {
  const btn = $(kind === "html" ? "htmlBtn" : "mdBtn");
  btn.disabled = true;
  try {
    const content = kind === "html" ? await buildHtmlFile() : buildMarkdown();
    const r = await api("/api/save-file", { docId: DOC_ID, kind, name: `${safeName(S.info?.title)} – Lernzettel.${kind}`, content });
    if (r.cancelled) return;
    showToast(`Gespeichert: ${r.path}`, "Öffnen", () => api("/api/open-path", { what: "file", path: r.path }).catch((e) => showToast(e.message)));
  } catch (e) {
    showToast("Speichern fehlgeschlagen: " + e.message);
  } finally {
    btn.disabled = false;
  }
}

$("printBtn").onclick = () => window.print();
$("htmlBtn").onclick = () => save("html");
$("mdBtn").onclick = () => save("md");
for (const id of ["optPlain", "optComments", "optClaude", "optSticky"]) $(id).addEventListener("change", buildSheet);

COLORS.forEach((c, i) => {
  const b = document.createElement("button");
  b.className = "sw";
  b.title = `Nur ${c.name}`;
  b.style.background = css(c.hl);
  b.addEventListener("click", () => {
    S.colors.has(i) ? S.colors.delete(i) : S.colors.add(i);
    b.classList.toggle("on", S.colors.has(i));
    buildSheet();
  });
  $("optColors").append(b);
});

async function init() {
  S.info = await api(`/api/doc/${DOC_ID}/info`);
  window.PT.setTitle(`Lernzettel: ${S.info.title} – PDF-Tutor`);
  const d = await api(`/api/doc/${DOC_ID}/notes`);
  S.notes = (d.notes || []).slice().sort((a, b) => a.page - b.page || pdfTop(b) - pdfTop(a));
  buildSheet();
}

// Reihenfolge innerhalb der Seite: von oben nach unten (PDF-Koordinaten: größeres y = weiter oben)
const pdfTop = (n) => Math.max(...(n.pdfRects || [[0, 0, 0, 0]]).map((r) => r[3]));

init().catch((e) => {
  const b = $("banner");
  b.textContent = "Fehler beim Laden: " + e.message;
  b.hidden = false;
});
