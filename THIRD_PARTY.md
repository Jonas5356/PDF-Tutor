# Fremdkomponenten

| Komponente | Zweck | Lizenz | Lizenztext |
|---|---|---|---|
| [PDF.js](https://github.com/mozilla/pdf.js) 4.10 | Anzeige der PDF-Seiten | Apache-2.0 | `ui/vendor/LICENSE-pdfjs.txt` |
| [PDFium](https://pdfium.googlesource.com/pdfium/) (Build von [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), chromium/8076) | Seitenbilder und Text für Claude, Vorschaubilder | BSD-3-Clause / Apache-2.0 (mit enthaltenen Bibliotheken) | `third_party/pdfium/` |
| [KaTeX](https://katex.org) | Formelsatz | MIT | `ui/vendor/LICENSE-katex.txt` |
| [marked](https://marked.js.org) | Markdown | MIT | `ui/vendor/LICENSE-marked.txt` |
| [DOMPurify](https://github.com/cure53/DOMPurify) | Absichern der Antworten | Apache-2.0 oder MPL-2.0 | `ui/vendor/LICENSE-dompurify.txt` |
| [Source Serif 4](https://github.com/adobe-fonts/source-serif) | Schrift | SIL Open Font License 1.1 | `ui/vendor/fonts/LICENSE-source-serif-4.txt` |
| [Tauri](https://tauri.app) | Programmfenster, Installer, Updates | MIT oder Apache-2.0 | – |
| [lopdf](https://github.com/J-F-Liu/lopdf) | Kommentare ins PDF schreiben | MIT | – |
| [pdfium-render](https://github.com/ajrcarey/pdfium-render) | Anbindung an PDFium | MIT oder Apache-2.0 | – |

Die übrigen Rust-Bibliotheken stehen in `src-tauri/Cargo.toml` (bzw. `Cargo.lock`). Sie stehen überwiegend unter MIT und/oder Apache-2.0.
