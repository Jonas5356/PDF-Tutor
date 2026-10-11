"use strict";
// Verbindung zwischen Oberfläche und Rust-Teil (Tauri)
(function () {
  const T = window.__TAURI__;

  async function api(path, body) {
    try {
      return await T.core.invoke("api", { path, body: body === undefined ? null : body });
    } catch (e) {
      const msg = (e && e.error) || (typeof e === "string" ? e : JSON.stringify(e));
      const err = new Error(msg);
      if (e && e.notFound) err.notFound = true;
      throw err;
    }
  }

  // Adresse für das eigene Protokoll (PDF-Daten, Vorschaubilder)
  function protoUrl(p) {
    return T.core.convertFileSrc(p, "pdftutor");
  }

  function setTitle(t) {
    document.title = t;
    api("/api/window/title", { title: t }).catch(() => {});
  }

  // Links (z. B. in Claude-Antworten) im normalen Browser öffnen
  document.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href]");
    if (!a) return;
    const href = a.getAttribute("href") || "";
    if (/^https?:\/\//i.test(href)) {
      e.preventDefault();
      api("/api/open-url", { url: href }).catch(() => {});
    }
  });

  window.PT = { api, protoUrl, setTitle };
})();
