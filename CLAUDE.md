# PDF-Tutor – Hinweise für die Weiterentwicklung

Tauri-2-Programm: Die Oberfläche ist HTML/JavaScript ohne Build-Schritt (`ui/`), der Hintergrund ist in Rust geschrieben (`src-tauri/src/`). Sprache der Oberfläche und der Kommentare im Code: Deutsch.

## Aufbau

- `ui/index.html`, `home.js`, `home.css`: Startseite mit Bibliothek, Einstellungen und Updates
- `ui/reader.html`, `app.js`, `style.css`: Reader (PDF.js für die Anzeige, Randspalte mit Notizen, KaTeX/marked für Antworten)
- `ui/bridge.js`: Verbindung zum Rust-Teil. Ein einziger Befehl `api(path, body)`. Dazu kommt das eigene Protokoll `pdftutor://` (PDF-Daten unter `doc/<id>/pdf`, Vorschaubilder unter `thumb/<id>`).
- `src-tauri/src/api.rs`: verteilt die `api`-Aufrufe (`/api/library…`, `/api/doc/<id>/…`, `/api/settings`, `/api/update/…`)
- `annots.rs`: Lesen und Schreiben der PDF-Anmerkungen mit lopdf.
  - Eine Notiz ist eine Wurzel-Anmerkung (Highlight/Underline/StrikeOut/Squiggly/Text/FreeText) mit Antworten (`/IRT`).
  - Eigene Zusatzdaten stehen im Wörterbuch `/PDFTutor`: markierter Text, Claude-Sitzung, Modell, Rolle, Token-Verbrauch.
  - Geschrieben wird **immer inkrementell** (`IncrementalDocument`). `Editor::finish` prüft, dass der bisherige Dateiinhalt unverändert vorne steht.
- `doc.rs`: geöffnetes Dokument. Notizen kommen aus dem PDF. Dazu gehören der Ablauf einer Frage (`ask`), das Speichern (`modify`, atomar mit Wiederholung bei gesperrter Datei) und die Übernahme der `*.notes.json` aus Version 3.
- `cache.rs`: Seiten-Cache für Claude (Text und JPEG je Seite, `uebersicht.md`, `volltext.md`) im Datenordner. Der Schlüssel ist der Pfad. Gültig, solange die ersten 1 MB der Datei gleich bleiben, denn angehängte Kommentare ändern daran nichts.
- `claude.rs`: Aufruf von `claude -p` (Prompt über stdin, gleichbleibender Teil als `--append-system-prompt-file`, nur lesende Werkzeuge `Read,Grep,Glob`) und Prompt-Texte
- `pdfium_host.rs`: PDFium (Seitenbilder, Text, Inhaltsverzeichnis). Alle Aufrufe laufen nacheinander über eine Sperre.
- `library.rs`, `settings.rs`, `windows.rs`, `util.rs`: Bibliothek (`bibliothek.json`), Einstellungen, Fenster, Hilfsfunktionen

Datenordner: `%LOCALAPPDATA%\PDF-Tutor-Daten` (Windows) bzw. `~/.local/share/pdf-tutor`. Für Tests lässt er sich mit `PDFTUTOR_DATA` überschreiben.

## Grundsätze

- Das Original-PDF wird nur durch inkrementelle Updates verändert, nie neu geschrieben.
- Claude liest nie das Original-PDF, nur den Seiten-Cache. Claude darf nichts ausführen oder schreiben.
- Koordinaten: Die Oberfläche rechnet mit PDF.js (`convertToPdfPoint` / `convertToViewportRectangle`) zwischen Bildschirm- und PDF-Koordinaten um. An den Rust-Teil gehen PDF-Koordinaten `[links, unten, rechts, oben]`.

## Testen

- `cargo test` in `src-tauri` (Schreiben in normale und verschlüsselte PDFs, fremde Anmerkungen, Löschen). Die Testdateien liegen in `PT_TESTDIR`.
- Ende-zu-Ende-Tests laufen mit `tauri-driver` und WebKitWebDriver unter Linux/Xvfb.
