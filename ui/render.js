// Gemeinsame Darstellung: Markdown mit Formeln (KaTeX), abgesichert mit DOMPurify.
// Benötigt window.marked, window.DOMPurify und window.katex.

export function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

export function renderMD(src) {
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
