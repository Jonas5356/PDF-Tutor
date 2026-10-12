"use strict";
// Übersetzungen der Oberfläche. Schlüssel ist der englische Text; fehlt eine deutsche
// Übersetzung, erscheint der englische Text. Platzhalter: {name}.
//
// In HTML: data-i18n (Textinhalt), data-i18n-html (Inhalt mit <b> u. Ä.), data-i18n-title, data-i18n-placeholder,
// data-i18n-aria-label. Der englische Text steht direkt im HTML.
(function () {
  const DE = {
    // ----- allgemein
    "Cancel": "Abbrechen",
    "Save": "Speichern",
    "Undo": "Rückgängig",
    "Delete": "Löschen",
    "Open": "Öffnen",
    "Settings": "Einstellungen",
    "Error while loading: {msg}": "Fehler beim Laden: {msg}",
    "Saving failed: {msg}": "Speichern fehlgeschlagen: {msg}",
    "Saved: {path}": "Gespeichert: {path}",
    "Yellow": "Gelb", "Green": "Grün", "Blue": "Blau", "Pink": "Rosa", "Orange": "Orange",
    "Only {color}": "Nur {color}",
    "Page {n}": "Seite {n}",
    "p. {n}": "S. {n}",
    "{n} (printed: {label})": "{n} (gedruckt: {label})",
    "(printed: {label})": "(gedruckt: {label})",

    // ----- Startseite
    "Search documents": "Dokument suchen",
    "Open PDF …": "PDF öffnen …",
    "Add folder …": "Ordner hinzufügen …",
    "Merge PDFs …": "PDFs zusammenfügen …",
    "Merge PDFs, reorder, rotate or remove pages": "PDFs zusammenfügen, Seiten umsortieren, drehen oder entfernen",
    "Update now": "Jetzt aktualisieren",
    "Continue reading": "Weiterlesen",
    "Your library is empty": "Deine Bibliothek ist noch leer",
    "Open a PDF or add a folder, for example the one with your lecture notes. All PDFs in it will then appear here.": "Öffne ein PDF oder füge einen Ordner hinzu, etwa den mit deinen Vorlesungsunterlagen. Alle PDFs darin erscheinen dann hier.",
    "No document matches this search.": "Kein Dokument passt zu dieser Suche.",
    "Language": "Sprache",
    "Notes for Claude": "Hinweise an Claude",
    "What Claude should know about you and how answers should look. Applies to all documents.": "Was Claude über dich wissen soll und wie die Antworten aussehen sollen. Gilt für alle Dokumente.",
    "Full text up to": "Volltext bis",
    "If a document is smaller, Claude receives its whole text with every question. Larger documents are searched. About 30,000 tokens correspond to 50–60 book pages or a long slide deck.": "Ist ein Dokument kleiner, bekommt Claude bei jeder Frage den ganzen Text mit. Größere Dokumente werden durchsucht. Etwa 30.000 Tokens entsprechen 50–60 Buchseiten oder einem langen Foliensatz.",
    "tokens": "Tokens",
    "Your name": "Dein Name",
    "Stored as the author of your highlights and comments in the PDF. Other programs such as Acrobat display it.": "Steht als Autor bei deinen Markierungen und Kommentaren im PDF. Andere Programme wie Acrobat zeigen ihn an.",
    "Backup copy": "Sicherheitskopie",
    "Before PDF-Tutor changes a PDF for the first time, it stores a copy in the data folder.": "Bevor der PDF-Tutor ein PDF zum ersten Mal verändert, legt er eine Kopie im Datenordner ab.",
    "Open data folder": "Datenordner öffnen",
    "Open log file": "Protokoll öffnen",
    "Check for updates": "Nach Updates suchen",
    "today, {time}": "heute, {time}",
    "yesterday, {time}": "gestern, {time}",
    "{n} days ago": "vor {n} Tagen",
    "Could not load the library: {msg}": "Bibliothek konnte nicht geladen werden: {msg}",
    "Show in Explorer": "Im Explorer zeigen",
    "Remove folder": "Ordner entfernen",
    "Remove “{name}” from the library?\n\nThe files and notes remain unchanged.": "„{name}“ aus der Bibliothek entfernen?\n\nDie Dateien und Notizen bleiben unverändert.",
    "This folder no longer exists. Remove it or restore it.": "Dieser Ordner existiert nicht mehr. Entferne ihn oder stelle ihn wieder her.",
    "No PDFs in this folder.": "Keine PDFs in diesem Ordner.",
    "File not found": "Datei nicht gefunden",
    "Page {a} of {b}": "Seite {a} von {b}",
    "{n} pages": "{n} Seiten",
    "1 note": "1 Notiz",
    "{n} notes": "{n} Notizen",
    "Locate …": "Neu suchen …",
    "More actions": "Weitere Aktionen",
    "Open in new window": "In neuem Fenster öffnen",
    "Remove from “Continue reading”": "Aus „Weiterlesen“ entfernen",
    "Hide": "Ausblenden",
    "Could not open the window: {msg}": "Fenster konnte nicht geöffnet werden: {msg}",
    "Opening failed: {msg}": "Öffnen fehlgeschlagen: {msg}",
    "Could not add the folder: {msg}": "Ordner konnte nicht hinzugefügt werden: {msg}",
    "Relinking failed: {msg}": "Neu verknüpfen fehlgeschlagen: {msg}",
    "Could not load the settings: {msg}": "Einstellungen konnten nicht geladen werden: {msg}",
    "There is no log file yet.": "Noch kein Protokoll vorhanden.",
    "Checking for updates …": "Suche nach Updates …",
    "Updates are not configured in this build.": "Die Update-Funktion ist in dieser Version nicht eingerichtet.",
    "You have the latest version.": "Du hast die neueste Version.",
    "Version {v} is available.": "Version {v} ist verfügbar.",
    "A new version ({v}) is available.": "Eine neue Version ({v}) ist verfügbar.",
    "Downloading and installing the update, PDF-Tutor will restart afterwards …": "Update wird geladen und installiert, der PDF-Tutor startet danach neu …",
    "Claude Code was not found. Reading works, but questions will not. Check in PowerShell with “claude --version” whether it is installed.": "Claude Code wurde nicht gefunden. Lesen funktioniert, Fragen aber nicht. Prüfe in der PowerShell mit „claude --version“, ob es installiert ist.",

    // ----- Reader
    "(untitled)": "(ohne Titel)",
    "Add a comment …": "Kommentar hinzufügen …",
    "Add a sticky note: then click the desired spot on the page": "Haftnotiz setzen: danach auf die gewünschte Stelle der Seite klicken",
    "All": "Alle",
    "Annotation": "Anmerkung",
    "Highlight": "Hervorhebung",
    "Underline": "Unterstreichung",
    "Strikeout": "Durchstreichung",
    "Markup": "Markierung",
    "Note": "Notiz",
    "Text box": "Textfeld",
    "Highlight text": "Hervorheben",
    "Underline text": "Unterstreichen",
    "Strike out": "Durchstreichen",
    "Ask Claude": "Claude fragen",
    "Automatic: full text up to {n} tokens, search above that.": "Automatisch: Volltext bis {n} Tokens, darüber Suche.",
    "Change failed: {msg}": "Ändern fehlgeschlagen: {msg}",
    "Claude ({model}) is thinking …": "Claude ({model}) denkt nach …",
    "Close (Esc)": "Schließen (Esc)",
    "Collapse": "Einklappen",
    "Expand": "Aufklappen",
    "Collapse all notes": "Alle Notizen einklappen",
    "Comment": "Kommentar",
    "Save comment": "Kommentieren",
    "Question": "Frage",
    "Contents": "Inhalt",
    "full text": "Volltext",
    "search": "Suche",
    "Could not create the note: {msg}": "Konnte die Notiz nicht anlegen: {msg}",
    "Could not highlight: {msg}": "Konnte nicht markieren: {msg}",
    "Create study sheet …": "Lernzettel erstellen …",
    "Ctrl+Enter · Esc cancels": "Strg+Enter · Esc bricht ab",
    "Ctrl+Enter: ask Claude": "Strg+Enter: Claude fragen",
    "Ctrl+Enter: save": "Strg+Enter: speichern",
    "Delete (Ctrl+Z undoes it)": "Löschen (Strg+Z macht es rückgängig)",
    "Delete highlight with comments": "Markierung samt Kommentaren löschen",
    "Delete note with comments": "Notiz samt Kommentaren löschen",
    "Deleting failed: {msg}": "Löschen fehlgeschlagen: {msg}",
    "Document not found. Go back to the library to relink it.": "Dokument nicht gefunden. Zurück zur Bibliothek, um es neu zu verknüpfen.",
    "Edit": "Bearbeiten",
    "Error: {msg}": "Fehler: {msg}",
    "Fit to width": "Auf Breite einpassen",
    "Width": "Breite",
    "Follow-up question to Claude …": "Nachfrage an Claude …",
    "Your question to Claude …": "Deine Frage an Claude …",
    "Highlight in {color}": "{color} hervorheben",
    "How much of the document Claude receives with a question": "Wie viel vom Dokument Claude bei einer Frage mitbekommt",
    "Input {a} · from cache {b} · written to cache {c} · output {d} tokens": "Eingabe {a} · aus Cache {b} · in Cache geschrieben {c} · Ausgabe {d} Tokens",
    "‹ Library": "‹ Bibliothek",
    "To the library": "Zur Bibliothek",
    "Loading …": "Lade …",
    "Model for new questions": "Modell für neue Fragen",
    "More functions": "Weitere Funktionen",
    "New note": "Neue Notiz",
    "Next match (Enter)": "Nächster Treffer (Enter)",
    "Previous match (Shift+Enter)": "Vorheriger Treffer (Umschalt+Enter)",
    "Next page": "Nächste Seite",
    "Previous page": "Vorherige Seite",
    "No highlights or notes yet.": "Noch keine Markierungen oder Notizen.",
    "No matches": "Keine Treffer",
    "No notes for this filter.": "Keine Notizen für diesen Filter.",
    "Notes": "Notizen",
    "Nothing to undo": "Nichts rückgängig zu machen",
    "Organize / merge pages …": "Seiten organisieren / zusammenfügen …",
    "Page": "Seite",
    "Pages": "Seiten",
    "Preparing document …": "Dokument wird vorbereitet …",
    "Saving it empty deletes the comment": "Leer speichern löscht den Kommentar",
    "Search in document (Ctrl+F)": "Im Dokument suchen (Strg+F)",
    "Search in document …": "Im Dokument suchen …",
    "Searching …": "Suche …",
    "Searching … {p} %": "Suche … {p} %",
    "Select a passage in the text to highlight it, comment on it or <b>ask Claude</b>. With <b>Note</b> you can place a sticky note anywhere.": "Markiere eine Stelle im Text, um sie hervorzuheben, zu kommentieren oder <b>Claude zu fragen</b>. Mit <b>Notiz</b> setzt du eine Haftnotiz an eine beliebige Stelle.",
    "Sidebar: contents, pages, notes": "Seitenleiste: Inhalt, Seiten, Notizen",
    "Sticky notes": "Haftnotizen",
    "Strike out (in the last chosen color)": "Durchstreichen (in der zuletzt gewählten Farbe)",
    "Underline (in the last chosen color)": "Unterstreichen (in der zuletzt gewählten Farbe)",
    "Text of the note …": "Text der Notiz …",
    "The PDF is currently open in another program. The answer will be saved as soon as possible.": "Das PDF ist gerade in einem anderen Programm geöffnet. Die Antwort wird gespeichert, sobald es wieder geht.",
    "not yet saved in the PDF": "noch nicht im PDF gespeichert",
    "The “claude” command was not found. Reading works, but questions will fail.": "Der Befehl „claude“ wurde nicht gefunden. Lesen geht, Fragen werden aber fehlschlagen.",
    "This PDF has no table of contents.<br>You will find thumbnails under <b>Pages</b>.": "Dieses PDF hat kein Inhaltsverzeichnis.<br>Unter <b>Seiten</b> findest du die Miniaturansichten.",
    "Try again": "Erneut versuchen",
    "Undo: {what} (Ctrl+Z)": "Rückgängig: {what} (Strg+Z)",
    "Undone: {what}": "Rückgängig: {what}",
    "Your comment on this passage …": "Dein Kommentar zu dieser Stelle …",
    "Your comment …": "Dein Kommentar …",
    "Your question about this passage …": "Deine Frage zu dieser Stelle …",
    "Zoom in": "Vergrößern",
    "Zoom out": "Verkleinern",
    "highlights only": "nur Markierungen",
    "with Claude": "mit Claude",
    "with comment": "mit Kommentar",
    "images {p} %": "Bilder {p} %",
    "{a} of {b}": "{a} von {b}",
    "{kind} deleted.": "{kind} gelöscht.",
    "{n}k": "{n} Tsd.",
    "{tokens} tokens → {mode}": "{tokens} Tokens → {mode}",
    "“{q}”": "„{q}“",

    // ----- Lernzettel
    "1 entry": "1 Eintrag",
    "{n} entries": "{n} Einträge",
    "Answer from Claude": "Antwort von Claude",
    "As HTML …": "Als HTML …",
    "As Markdown …": "Als Markdown …",
    "As of {date}": "Stand {date}",
    "Document": "Dokument",
    "Highlights without comment": "Markierungen ohne Kommentar",
    "No notes for this selection.": "Keine Notizen für diese Auswahl.",
    "Only these colors (none selected: all)": "Nur diese Farben (keine Auswahl: alle)",
    "Own comments": "Eigene Kommentare",
    "Print / save as PDF …": "Drucken / als PDF …",
    "Questions and answers from Claude": "Fragen und Antworten von Claude",
    "Study sheet": "Lernzettel",
    "Study sheet: {title} – PDF-Tutor": "Lernzettel: {title} – PDF-Tutor",

    // ----- Seiten organisieren
    "Organize pages": "Seiten organisieren",
    "Add PDF …": "PDF hinzufügen …",
    "Rotate selection left": "Auswahl nach links drehen",
    "Rotate selection right (R)": "Auswahl nach rechts drehen (R)",
    "Remove selection (Del)": "Auswahl entfernen (Entf)",
    "Remove": "Entfernen",
    "Rotate": "Drehen",
    "Undo (Ctrl+Z)": "Rückgängig (Strg+Z)",
    "Save as new file …": "Als neue Datei speichern …",
    "Drag pages to reorder · Click selects, with Ctrl or Shift several · The original remains unchanged.": "Seiten ziehen zum Umsortieren · Klick wählt aus, mit Strg oder Umschalt mehrere · Das Original bleibt unverändert.",
    "No pages yet. Add one or more PDFs to merge them or reorder pages.": "Noch keine Seiten. Füge ein oder mehrere PDFs hinzu, um sie zusammenzufügen oder Seiten umzusortieren.",
    "Could not load the PDF: {msg}": "Konnte das PDF nicht laden: {msg}",
    "1 page": "1 Seite",
    "from {n} documents": "aus {n} Dokumenten",
    "{n} selected": "{n} ausgewählt",
    "1 page removed.": "1 Seite entfernt.",
    "{n} pages removed.": "{n} Seiten entfernt.",
    "{name} (edited).pdf": "{name} (bearbeitet).pdf",
    "Merged.pdf": "Zusammengefügt.pdf",
    "Saved: {path} ({n} pages)": "Gespeichert: {path} ({n} Seiten)",
    "Open in reader": "Im Reader öffnen",
    "Organize pages – PDF-Tutor": "Seiten organisieren – PDF-Tutor",
    "Organize pages: {title} – PDF-Tutor": "Seiten organisieren: {title} – PDF-Tutor",

    // ----- Modell und Denktiefe
    "Settings for questions to Claude: context, model, reasoning depth": "Einstellungen für Fragen an Claude: Kontext, Modell, Denktiefe",
    "Context": "Kontext",
    "automatic": "automatisch",
    "automatic ({mode})": "automatisch ({mode})",
    "Model": "Modell",
    "Reasoning depth": "Denktiefe",
    "Default of the model": "Voreinstellung des Modells",
    "Low": "Niedrig",
    "Medium": "Mittel",
    "High": "Hoch",
    "Very high": "Sehr hoch",
    "Maximum": "Maximal",
    "default": "Standard",
    "Stronger models and deeper reasoning often give better answers to hard questions, but take longer and use more of your usage limit. Applies to new questions in this document.": "Stärkere Modelle und mehr Denktiefe geben bei schwierigen Fragen oft bessere Antworten, dauern aber länger und verbrauchen mehr vom Nutzungslimit. Gilt für neue Fragen in diesem Dokument.",
    "depth {level}": "Denktiefe {level}",
    "low": "niedrig",
    "medium": "mittel",
    "high": "hoch",
    "very high": "sehr hoch",
    "maximum": "maximal",
    "Use a model that may cost extra?": "Modell mit möglichen Zusatzkosten verwenden?",
    "This model may be billed via extra usage credits, depending on your plan.": "Dieses Modell wird je nach Abo eventuell über Zusatzguthaben abgerechnet.",
    "{model} may be billed via extra usage credits, depending on your Claude plan, and may not be available on every plan. Questions to {model} could therefore cost real money beyond your subscription.": "{model} wird je nach Claude-Abo eventuell über Zusatzguthaben abgerechnet und ist nicht in jedem Abo verfügbar. Fragen an {model} könnten also echtes Geld über dein Abo hinaus kosten.",
    "Use {model}": "{model} verwenden",
  };

  let LANG = "en";
  const t = (s, vars) => {
    let r = LANG === "de" && Object.prototype.hasOwnProperty.call(DE, s) ? DE[s] : s;
    if (vars) r = r.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
    return r;
  };

  // Englischen Ausgangstext beim ersten Anwenden merken, damit sich die Sprache wechseln lässt
  function apply(root) {
    const r = root || document;
    // feste Texte mit einfacher Auszeichnung (<b>, <br>) aus der eigenen Übersetzungstabelle
    r.querySelectorAll("[data-i18n-html]").forEach((el) => {
      if (el.dataset.i18nHtmlSrc === undefined) el.dataset.i18nHtmlSrc = el.innerHTML.trim();
      el.innerHTML = t(el.dataset.i18nHtmlSrc);
    });
    r.querySelectorAll("[data-i18n]").forEach((el) => {
      if (el.dataset.i18nSrc === undefined) el.dataset.i18nSrc = el.dataset.i18n || el.textContent.trim();
      el.textContent = t(el.dataset.i18nSrc);
    });
    for (const attr of ["title", "placeholder", "aria-label"]) {
      const key = `i18n${attr === "aria-label" ? "AriaLabel" : attr[0].toUpperCase() + attr.slice(1)}`;
      r.querySelectorAll(`[data-i18n-${attr}]`).forEach((el) => {
        if (el.dataset[key + "Src"] === undefined) el.dataset[key + "Src"] = el.dataset[key] || el.getAttribute(attr) || "";
        el.setAttribute(attr, t(el.dataset[key + "Src"]));
      });
    }
  }

  async function init() {
    try {
      const app = await window.PT.api("/api/app");
      LANG = app.lang === "de" ? "de" : "en";
    } catch {
      LANG = "en";
    }
    document.documentElement.lang = LANG;
    apply(document);
    return LANG;
  }

  const locale = () => (LANG === "de" ? "de-AT" : "en-US");
  const fmtNum = (n) => Number(n || 0).toLocaleString(locale());

  window.I18N = { init, apply, t, lang: () => LANG, locale, fmtNum, _de: DE };
  window.t = t;
})();
