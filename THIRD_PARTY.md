# Third-party components

| Component | Purpose | License | License text |
|---|---|---|---|
| [PDF.js](https://github.com/mozilla/pdf.js) 4.10 | Displaying PDF pages | Apache-2.0 | `ui/vendor/LICENSE-pdfjs.txt` |
| [PDFium](https://pdfium.googlesource.com/pdfium/) (Build von [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), chromium/8076) | Page images and text for Claude, thumbnails, merging pages | BSD-3-Clause / Apache-2.0 (with bundled libraries) | `third_party/pdfium/` |
| [KaTeX](https://katex.org) | Math typesetting | MIT | `ui/vendor/LICENSE-katex.txt` |
| [marked](https://marked.js.org) | Markdown | MIT | `ui/vendor/LICENSE-marked.txt` |
| [DOMPurify](https://github.com/cure53/DOMPurify) | Sanitizing answers | Apache-2.0 or MPL-2.0 | `ui/vendor/LICENSE-dompurify.txt` |
| [Source Serif 4](https://github.com/adobe-fonts/source-serif) | Font | SIL Open Font License 1.1 | `ui/vendor/fonts/LICENSE-source-serif-4.txt` |
| [Tauri](https://tauri.app) | App windows, installer, updates | MIT or Apache-2.0 | – |
| [lopdf](https://github.com/J-F-Liu/lopdf) | Writing comments into PDFs | MIT | – |
| [pdfium-render](https://github.com/ajrcarey/pdfium-render) | Binding to PDFium | MIT or Apache-2.0 | – |

The other Rust libraries are listed in `src-tauri/Cargo.toml` (and `Cargo.lock`). They are mostly licensed under MIT and/or Apache-2.0.
