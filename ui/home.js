"use strict";

const $ = (id) => document.getElementById(id);
let APP = null;
let LIB = { recent: [], folders: [] };

const api = window.PT.api;

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function showBanner(text) {
  const b = $("banner");
  b.textContent = text;
  b.hidden = !text;
}

function relTime(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  const now = new Date();
  const day = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate());
  const diff = Math.round((day(now) - day(d)) / 86400000);
  const hm = d.toLocaleTimeString("de-AT", { hour: "2-digit", minute: "2-digit" });
  if (diff <= 0) return `heute, ${hm}`;
  if (diff === 1) return `gestern, ${hm}`;
  if (diff < 7) return `vor ${diff} Tagen`;
  return d.toLocaleDateString("de-AT", { day: "numeric", month: "numeric", year: "numeric" });
}

// ---------------------------------------------------------------------------

async function load() {
  try {
    LIB = await api("/api/library");
  } catch (e) {
    showBanner("Bibliothek konnte nicht geladen werden: " + e.message);
    return;
  }
  render();
}

function render() {
  const q = $("search").value.trim().toLowerCase();
  const match = (d) => !q || d.title.toLowerCase().includes(q) || d.name.toLowerCase().includes(q);
  let shown = 0;

  const recent = LIB.recent.filter(match);
  $("recentSection").hidden = recent.length === 0;
  $("recent").replaceChildren(...recent.map((d) => bookEl(d, "recent")));
  shown += recent.length;

  const wrap = $("folders");
  wrap.replaceChildren();
  for (const f of LIB.folders) {
    const docs = f.docs.filter(match);
    if (q && !docs.length) continue;
    shown += docs.length;
    const sec = document.createElement("section");
    sec.className = "shelf-section folder-section";
    sec.innerHTML = `
      <div class="shelf-head">
        <h2 class="shelf-title">${esc(f.name)}</h2>
        <span class="shelf-path" title="${esc(f.path)}">${esc(f.path)}</span>
        <span class="shelf-actions">
          <button class="link-btn" data-act="show">Im Explorer zeigen</button>
          <button class="link-btn" data-act="remove">Ordner entfernen</button>
        </span>
      </div>`;
    sec.querySelector('[data-act="show"]').onclick = () => api("/api/open-path", { what: "folder", path: f.path }).catch(() => {});
    sec.querySelector('[data-act="remove"]').onclick = async () => {
      if (!confirm(`„${f.name}“ aus der Bibliothek entfernen?\n\nDie Dateien und Notizen bleiben unverändert.`)) return;
      await api("/api/library/remove-folder", { path: f.path });
      load();
    };
    if (!f.exists) {
      const p = document.createElement("p");
      p.className = "book-meta";
      p.textContent = "Dieser Ordner existiert nicht mehr. Entferne ihn oder stelle ihn wieder her.";
      sec.append(p);
    } else if (!docs.length) {
      const p = document.createElement("p");
      p.className = "book-meta";
      p.textContent = "Keine PDFs in diesem Ordner.";
      sec.append(p);
    }
    // nach Unterordnern gruppieren
    const groups = new Map();
    for (const d of docs) {
      if (!groups.has(d.subfolder)) groups.set(d.subfolder, []);
      groups.get(d.subfolder).push(d);
    }
    for (const [sub, items] of groups) {
      if (sub) {
        const h = document.createElement("h3");
        h.className = "subfolder";
        h.textContent = sub.split(/[\\/]/).join(" › ");
        sec.append(h);
      }
      const shelf = document.createElement("div");
      shelf.className = "shelf";
      shelf.append(...items.map((d) => bookEl(d, "folder")));
      sec.append(shelf);
    }
    wrap.append(sec);
  }

  const libraryEmpty = !LIB.recent.length && !LIB.folders.length;
  $("empty").hidden = !libraryEmpty;
  $("noResults").hidden = libraryEmpty || shown > 0 || !q;
}

function bookEl(d, where) {
  const el = document.createElement("article");
  el.className = "book" + (d.exists ? "" : " missing");
  const url = `reader.html?id=${d.id}`;
  let meta;
  if (!d.exists) meta = "Datei nicht gefunden";
  else if (d.lastPage && d.pages) meta = `Seite ${d.lastPage} von ${d.pages}`;
  else if (d.pages) meta = `${d.pages} Seiten`;
  else meta = "";
  const when = where === "recent" ? relTime(d.lastOpened) : "";
  const fileLine = d.title !== d.name.replace(/\.pdf$/i, "") ? d.name : "";

  const progress = d.exists && d.lastPage && d.pages ? Math.min(1, d.lastPage / d.pages) : 0;
  const ribbon = progress > 0
    ? `<span class="ribbon" style="height:calc(${(progress * 100).toFixed(1)}% + ${progress >= 1 ? 12 : 4}px)"></span>` : "";
  const badge = d.notes ? `<span class="badge" title="${d.notes} ${d.notes === 1 ? "Notiz" : "Notizen"}">${d.notes}</span>` : "";

  const cover = d.exists
    ? `<span class="paper"><img src="${window.PT.protoUrl(`thumb/${d.id}`)}" alt="" loading="lazy" onerror="this.replaceWith(Object.assign(document.createElement('span'),{className:'placeholder'}))">${ribbon}${badge}</span>`
    : `<span class="missing-box">Datei nicht gefunden<button class="btn" data-act="relink">Neu suchen …</button></span>`;

  el.innerHTML = `
    <a class="book-link" href="${d.exists ? url : "#"}" title="${esc(d.path)}">
      <div class="cover">${cover}</div>
      <h3 class="book-title">${esc(d.title)}</h3>
      <div class="book-meta">
        ${fileLine ? `<span class="file">${esc(fileLine)}</span>` : ""}
        ${meta ? `<span>${esc(meta)}</span>` : ""}${meta && when ? "<br>" : ""}${when ? `<span>${esc(when)}</span>` : ""}
      </div>
    </a>
    <button class="more" aria-haspopup="menu" aria-expanded="false" title="Weitere Aktionen" aria-label="Weitere Aktionen">⋯</button>`;

  const link = el.querySelector(".book-link");
  link.addEventListener("click", (e) => {
    if (!d.exists) { e.preventDefault(); return; }
    if (e.ctrlKey || e.metaKey || e.shiftKey) { e.preventDefault(); openWindow(url); }
  });
  link.addEventListener("auxclick", (e) => { if (e.button === 1 && d.exists) { e.preventDefault(); openWindow(url); } });
  el.querySelector('[data-act="relink"]')?.addEventListener("click", (e) => { e.preventDefault(); relink(d); });

  const more = el.querySelector(".more");
  more.addEventListener("click", (e) => {
    e.stopPropagation();
    const items = [];
    if (d.exists) {
      items.push(["Öffnen", () => (location.href = url)]);
      items.push(["In neuem Fenster öffnen", () => openWindow(url)]);
      items.push(["Im Explorer zeigen", () => api("/api/open-path", { what: "folder", path: d.folder })]);
    } else {
      items.push(["Neu suchen …", () => relink(d)]);
    }
    if (where === "recent") items.push(["Aus „Weiterlesen“ entfernen", async () => { await api("/api/library/remove", { id: d.id }); load(); }]);
    else items.push(["Ausblenden", async () => { await api("/api/library/hide", { id: d.id }); load(); }]);
    openMenu(more, items);
  });
  return el;
}

// ---------------------------------------------------------------------------
// Menü

function openMenu(anchor, items) {
  const m = $("menu");
  m.replaceChildren(...items.map(([label, fn]) => {
    const b = document.createElement("button");
    b.setAttribute("role", "menuitem");
    b.textContent = label;
    b.onclick = () => { closeMenu(); fn(); };
    return b;
  }));
  m.hidden = false;
  const r = anchor.getBoundingClientRect();
  const w = m.offsetWidth, h = m.offsetHeight;
  m.style.left = `${Math.max(8, Math.min(window.innerWidth - w - 8, r.right - w))}px`;
  m.style.top = `${r.bottom + h + 8 > window.innerHeight ? r.top - h - 4 : r.bottom + 4}px`;
  anchor.setAttribute("aria-expanded", "true");
  m._anchor = anchor;
  m.querySelector("button")?.focus();
}

function closeMenu() {
  const m = $("menu");
  if (m.hidden) return;
  m.hidden = true;
  m._anchor?.setAttribute("aria-expanded", "false");
}
document.addEventListener("click", (e) => { if (!e.target.closest("#menu")) closeMenu(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeMenu(); });
window.addEventListener("scroll", closeMenu, { passive: true });

// ---------------------------------------------------------------------------
// Aktionen

async function openWindow(url) {
  try {
    await api("/api/window/open", { url });
  } catch (e) {
    showBanner("Fenster konnte nicht geöffnet werden: " + e.message);
  }
}

async function openPdf() {
  try {
    const r = await api("/api/library/open", {});
    if (r.id) location.href = `reader.html?id=${r.id}`;
  } catch (e) {
    showBanner("Öffnen fehlgeschlagen: " + e.message);
  }
}

async function addFolder() {
  try {
    const r = await api("/api/library/add-folder", {});
    if (!r.cancelled) load();
  } catch (e) {
    showBanner("Ordner konnte nicht hinzugefügt werden: " + e.message);
  }
}

async function relink(d) {
  try {
    const r = await api("/api/library/relink", { id: d.id });
    if (!r.cancelled) load();
  } catch (e) {
    showBanner("Neu verknüpfen fehlgeschlagen: " + e.message);
  }
}

$("openBtn").onclick = openPdf;
$("folderBtn").onclick = addFolder;
document.querySelector('#empty [data-act="open"]').onclick = openPdf;
document.querySelector('#empty [data-act="folder"]').onclick = addFolder;
$("search").addEventListener("input", render);

// Einstellungen
$("settingsBtn").onclick = async () => {
  try {
    const s = await api("/api/settings");
    $("setHinweise").value = s.hinweise || "";
    $("setGrenze").value = s.volltextGrenze;
    $("setAutor").value = s.autor || "";
    $("setAutor").placeholder = s.autorStandard || "";
    $("setBackup").checked = !!s.sicherheitskopie;
    $("updateInfo").textContent = "";
    $("versionInfo").textContent = `Version ${APP?.version || ""}`;
    $("settings").showModal();
  } catch (e) {
    showBanner("Einstellungen konnten nicht geladen werden: " + e.message);
  }
};
$("settings").addEventListener("close", async () => {
  if ($("settings").returnValue !== "save") return;
  try {
    await api("/api/settings", {
      hinweise: $("setHinweise").value, volltextGrenze: Number($("setGrenze").value) || 0,
      autor: $("setAutor").value, sicherheitskopie: $("setBackup").checked,
    });
  } catch (e) {
    showBanner("Speichern fehlgeschlagen: " + e.message);
  }
});
$("openData").onclick = () => api("/api/open-path", { what: "data" }).catch(() => {});
$("openLog").onclick = () => api("/api/open-path", { what: "log" }).catch(() => showBanner("Noch kein Protokoll vorhanden."));

// Updates
async function checkUpdate(manual) {
  const info = $("updateInfo");
  if (manual) info.textContent = "Suche nach Updates …";
  try {
    const r = await api("/api/update/check");
    if (!r.configured) {
      if (manual) info.textContent = "Die Update-Funktion wird mit der Veröffentlichung auf GitHub aktiviert.";
      return;
    }
    if (!r.available) {
      if (manual) info.textContent = "Du hast die neueste Version.";
      return;
    }
    const msg = `Version ${r.version} ist verfügbar.`;
    if (manual) info.textContent = msg;
    showUpdateBanner(r.version);
  } catch (e) {
    if (manual) info.textContent = e.message;
  }
}

function showUpdateBanner(version) {
  const b = $("updateBanner");
  b.querySelector(".txt").textContent = `Eine neue Version (${version}) ist verfügbar.`;
  b.hidden = false;
}
$("updateNow").onclick = async () => {
  const b = $("updateBanner");
  b.querySelector(".txt").textContent = "Update wird geladen und installiert, der PDF-Tutor startet danach neu …";
  $("updateNow").disabled = true;
  try { await api("/api/update/install"); } catch (e) { b.querySelector(".txt").textContent = e.message; $("updateNow").disabled = false; }
};
$("checkUpdate").onclick = () => checkUpdate(true);

document.addEventListener("visibilitychange", () => { if (!document.hidden) load(); });

(async function init() {
  try {
    APP = await api("/api/app");
  } catch {
    APP = {};
  }
  window.PT.setTitle("PDF-Tutor");
  if (APP.updaterConfigured) {
    let last = 0;
    try { last = +localStorage.getItem("pdftutor-update-check") || 0; } catch {}
    if (Date.now() - last > 6 * 3600 * 1000) {
      try { localStorage.setItem("pdftutor-update-check", String(Date.now())); } catch {}
      checkUpdate(false);
    }
  }
  if (APP.claudeFound === false) {
    showBanner("Claude Code wurde nicht gefunden. Lesen funktioniert, Fragen aber nicht. Prüfe in der PowerShell mit „claude --version“, ob es installiert ist.");
  }
  load();
})();
