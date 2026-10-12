// Gemeinsame Darstellung: Markdown mit Formeln (KaTeX), abgesichert mit DOMPurify.
// Benötigt window.marked, window.DOMPurify und window.katex.

export function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

// Ergebnisse merken: Karten werden oft neu aufgebaut, der Formelsatz ist vergleichsweise teuer
const memo = new Map();

// `breaks`: einfache Zeilenumbrüche erhalten (für selbst geschriebene Kommentare und Fragen)
export function renderMD(src, { breaks = false } = {}) {
  const text = String(src || "");
  const mkey = (breaks ? "1" : "0") + text;
  const hit = memo.get(mkey);
  if (hit !== undefined) return hit;
  const html = render(text, breaks);
  memo.set(mkey, html);
  if (memo.size > 200) memo.delete(memo.keys().next().value);
  return html;
}

function render(text, breaks) {
  // Formeln vor dem Markdown durch Platzhalter ersetzen, damit * und _ darin erhalten bleiben.
  // Der Platzhalter enthält einen Zufallsteil, damit gleich aussehender Text im Original nichts auslöst.
  const key = "MATHPH" + Math.random().toString(36).slice(2, 8).replace(/\d/g, "q") + "Z";
  const math = [];
  const ph = (raw, t, display) => { math.push({ raw, t, display }); return `${key}${math.length - 1}E`; };
  let s = text;
  s = s.replace(/\$\$([\s\S]+?)\$\$/g, (m, t) => ph(m, t, true));
  s = s.replace(/\\\[([\s\S]+?)\\\]/g, (m, t) => ph(m, t, true));
  s = s.replace(/\\\(([\s\S]+?)\\\)/g, (m, t) => ph(m, t, false));
  s = s.replace(/(^|[^\\$])\$(?!\s)([^$\n]+?)(?<![\s\\])\$(?!\d)/g, (m, pre, t) => pre + ph(m.slice(pre.length), t, false));
  const clean = window.DOMPurify.sanitize(window.marked.parse(s, { gfm: true, breaks }), { FORBID_TAGS: ["img", "iframe", "form"] });

  const box = document.createElement("template");
  box.innerHTML = clean;
  const root = box.content;
  const re = new RegExp(`${key}(\\d+)E`, "g");
  const raw = (i) => (math[+i] ? math[+i].raw : "");

  // Links: Webadressen öffnen im Browser; andere Ziele (Dateinamen, relative Adressen) würden
  // das Programmfenster verlassen und bleiben deshalb normaler Text.
  for (const a of root.querySelectorAll("a")) {
    const href = (a.getAttribute("href") || "").replace(re, (_, i) => raw(i));
    if (/^https?:\/\//i.test(href)) {
      a.setAttribute("href", href);
      a.setAttribute("target", "_blank");
      a.setAttribute("rel", "noopener noreferrer");
    } else {
      a.removeAttribute("href");
    }
  }
  // Platzhalter in übrigen Attributen (z. B. Linktitel): ursprünglicher Text
  for (const el of root.querySelectorAll("*")) {
    for (const at of [...el.attributes]) {
      if (at.value.includes(key)) el.setAttribute(at.name, at.value.replace(re, (_, i) => raw(i)));
    }
  }
  // Platzhalter im Text: Formel setzen, in Code unverändert lassen
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) if (n.data.includes(key)) nodes.push(n);
  for (const n of nodes) {
    const inCode = !!(n.parentElement && n.parentElement.closest("code, pre"));
    const frag = document.createDocumentFragment();
    let last = 0;
    for (const m of n.data.matchAll(re)) {
      frag.append(n.data.slice(last, m.index));
      const item = math[+m[1]];
      if (item && !inCode) frag.append(formula(item));
      else frag.append(item ? item.raw : "");
      last = m.index + m[0].length;
    }
    frag.append(n.data.slice(last));
    n.replaceWith(frag);
  }
  return box.innerHTML;
}

function formula({ t, raw, display }) {
  const span = document.createElement("span");
  try {
    span.innerHTML = window.katex.renderToString(t, { displayMode: display, throwOnError: false, output: "html" });
  } catch {
    span.textContent = raw;
  }
  const frag = document.createDocumentFragment();
  frag.append(...span.childNodes);
  return frag;
}
