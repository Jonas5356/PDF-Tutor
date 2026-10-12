# PDF-Tutor – notes for development

Tauri 2 app: the interface is HTML/JavaScript without a build step (`ui/`), the backend is written in Rust (`src-tauri/src/`). The interface is available in English and German; code comments and log messages are in German.

## Structure

- `ui/index.html`, `home.js`, `home.css`: start page with library, settings and updates
- `ui/reader.html`, `app.js`, `style.css`: reader (PDF.js for display, margin column with notes, sidebar with contents/pages/notes, search with Ctrl+F)
- `ui/i18n.js`: translations. The key is the English text, `DE` holds the German version; missing entries fall back to English. HTML marks texts with `data-i18n`, `data-i18n-html`, `data-i18n-title`, `data-i18n-placeholder`, `data-i18n-aria-label`; JavaScript uses `t()` (start page) or `tr()` (reader, study sheet, organizer — `t` is often a local variable there). Every page calls `await I18N.init()` before rendering.
- `ui/render.js`: Markdown with formulas (marked, KaTeX, DOMPurify), shared by reader and study sheet
- `ui/export.html`, `export.js`, `export.css`, `sheet.css`: study sheet window. `sheet.css` also defines the look when printing and in the saved HTML file (fonts embedded as data: URLs).
- `ui/organize.html`, `organize.js`, `organize.css`: organize and merge pages (dragging uses mouse events, not HTML5 drag & drop, which Tauri intercepts on Windows). Saving always creates a new file via `pdfium_host::assemble`.
- `ui/bridge.js`: connection to the Rust side. A single command `api(path, body)`, plus the custom protocol `pdftutor://` (PDF data under `doc/<id>/pdf`, thumbnails under `thumb/<id>`).
- `src-tauri/src/api.rs`: dispatches the `api` calls (`/api/library…`, `/api/doc/<id>/…`, `/api/settings`, `/api/organize/…`, `/api/update/…`)
- `i18n.rs`: language for messages from the Rust side, `tx(english, german)`. Language order: language chosen in the installer (written to `installer-language.txt` next to the exe, taken over once at start, not on automatic updates), then the saved setting, then the system language.
- `annots.rs`: reading and writing PDF annotations with lopdf.
  - A note is a root annotation (Highlight/Underline/StrikeOut/Squiggly/Text/FreeText) with replies (`/IRT`).
  - Own extra data lives in the dictionary `/PDFTutor`: highlighted text, Claude session, model, role, token usage.
  - Writing is **always incremental** (`IncrementalDocument`). `Editor::finish` checks that the previous file content is unchanged at the start.
  - Kinds (`Kind`): Highlight, Underline, StrikeOut, Text (sticky note). Each gets its own appearance stream (/AP) so it looks the same in every program. `corners()` determines the reading direction from the page rotation.
  - A comment without replies is the /Contents of the root; further comments and Claude's answers are replies (/IRT). In the interface a user message counts as a "question" if Claude answered it, otherwise as a "comment".
- Undo (Ctrl+Z): since every change is appended, `Doc::undo` truncates the file to its length before the step (only if it is unchanged since). Steps go through `modify_undoable`; a plain `modify` (e.g. a Claude answer) clears the stack.
- `doc.rs`: an open document. Notes come from the PDF. Also the flow of a question (`ask`), saving (`modify`, atomic with retry on a locked file) and the import of `*.notes.json` from version 3.
- `cache.rs`: page cache for Claude (text and JPEG per page, `uebersicht.md`, `volltext.md`) in the data folder. The key is the path. Valid as long as the first 1 MB of the file stays the same, since appended comments don't change it.
- `claude.rs`: calls `claude -p` (prompt via stdin, constant part as `--append-system-prompt-file`, read-only tools `Read,Grep,Glob`) and the prompt texts. Instructions to Claude are in English; Claude answers in the language of the question unless the question asks otherwise. Models (`models()`, aliases that always point to the latest version) with `extra: true` may cost extra usage credits; the interface asks before selecting them. Reasoning depth is passed as `--effort` (empty = model default) and stored per answer in `/PDFTutor` (`Effort`).
- `pdfium_host.rs`: PDFium (page images, text, outline, merging pages). All calls are serialized through a lock.
- `library.rs`, `settings.rs`, `windows.rs`, `util.rs`: library (`bibliothek.json`), settings, windows, helpers

Data folder: `%LOCALAPPDATA%\PDF-Tutor-Daten` (Windows) or `~/.local/share/pdf-tutor`. For tests it can be overridden with `PDFTUTOR_DATA`.

## Principles

- The original PDF is only changed through incremental updates, never rewritten.
- Claude never reads the original PDF, only the page cache. Claude must not execute or write anything.
- Coordinates: the interface converts between screen and PDF coordinates with PDF.js (`convertToPdfPoint` / `convertToViewportRectangle`). The Rust side receives PDF coordinates `[left, bottom, right, top]`.

## Testing

- `cargo test` in `src-tauri` (writing into plain and encrypted PDFs, foreign annotations, deleting, rotated pages, merging). Test files are in `PT_TESTDIR`.
- End-to-end tests run with `tauri-driver` and WebKitWebDriver on Linux/Xvfb. With `PDFTUTOR_TEST_SAVE_DIR` the app saves into that folder without a dialog, and `PDFTUTOR_LANG` (`en`/`de`) fixes the language (tests only).
- Cross-check appearance streams with a second renderer (e.g. PyMuPDF), including rotated pages.
