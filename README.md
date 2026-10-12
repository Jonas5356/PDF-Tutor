# PDF-Tutor

**A PDF reader for working through technical literature: highlight a passage, ask a question, and the answer appears right next to it.**

PDF-Tutor is made for textbooks, lecture notes and lecture slides. You read as in any other PDF reader. When a passage is unclear, you highlight it and ask a question. Claude sees the highlighted passage, the whole page as an image (including formulas and figures) and, if needed, the rest of the document. The answer appears in a margin column next to the text, with properly typeset math. You can ask follow-up questions at any time.

The interface is available in English and German. *Deutsche Beschreibung weiter unten.*

## Features

- **Questions about highlighted passages.** Claude gets the highlighted passage, an image of the page and the context of the document. You can refer to other places, e.g. "this and the next 5 slides", "page 87", "section 3.2" or "where was this symbol introduced?". Claude answers in the language of your question.
- **Highlight and comment like in Acrobat.** Highlight text in five colors, underline or strike out, write your own comments and place sticky notes anywhere. Clicking a highlight changes its color or kind. Ctrl+Z undoes the last step.
- **Everything is stored in the PDF.** Highlights, comments, questions and answers are saved as standard PDF annotations (a highlight with a reply thread, as in Acrobat). They are visible in Acrobat, Edge or on a tablet, and comments made in those programs show up in PDF-Tutor. Saving is incremental: the original bytes of the file are never modified.
- **Sidebar and search.** Table of contents, page thumbnails and a list of all notes (filterable by color and kind). Ctrl+F searches the whole document, even across words hyphenated at line ends.
- **Study sheets.** All highlights, comments and Claude answers of a document, neatly arranged by page, with formulas. Print or save as PDF, save as a standalone HTML file or as Markdown (e.g. for Obsidian).
- **Organize pages.** Merge PDFs, reorder, rotate or remove pages. The result is saved as a new file; the originals remain unchanged and comments come along.
- **Full-text or search mode.** For smaller documents Claude receives the complete text. In large books Claude searches for the relevant pages.
- **Choice of model and reasoning depth.** Haiku, Sonnet, Opus or Fable, each with an adjustable reasoning depth. Models that may be billed via extra usage credits (currently Fable) are clearly marked and require a confirmation before use.
- **Library.** "Continue reading" lists recently opened documents with your reading progress. You can also add folders, e.g. the one with your lecture notes.
- **Your own notes for Claude.** Tell Claude what prior knowledge it can assume and how detailed explanations should be.

## Requirements

- Windows 10 or 11
- [Claude Code](https://docs.claude.com/en/docs/claude-code/overview), installed and signed in with your own Claude account (Pro or Max) or your own API key. To check, run `claude --version` in PowerShell.

PDF-Tutor calls the Claude Code installed on your computer. It never stores or uses credentials. Questions are billed to your own account and count towards its usage limits.

## Installation

Download `PDF-Tutor_…_x64-setup.exe` from [Releases](../../releases) and run it. No administrator rights are needed. The installer asks for the language (English or German); PDF-Tutor uses the same language, and you can change it later in the settings.

Windows may show "Windows protected your PC" because the program is not signed yet. Click "More info" and then "Run anyway".

To uninstall, use Windows Settings → Apps → PDF-Tutor. Your PDFs and the comments in them stay untouched; the data folder (see below) is kept and can be deleted manually.

## Privacy

- No telemetry, no servers of our own, no user accounts.
- **Questions to Claude** are only sent when you ask one. Claude Code then transfers the question, the highlighted passage, page images and page texts of the document to Anthropic under the terms of your Claude account ([Anthropic Privacy Policy](https://www.anthropic.com/legal/privacy)).
- **Update check:** about every 6 hours (and when you click "Check for updates"), PDF-Tutor downloads a small version file (`latest.json`) from this project's GitHub releases. No information about you, your documents or your notes is sent; GitHub only sees an ordinary download request ([GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)). Updates are only installed when you click "Update now".
- **Links** in Claude's answers open in your web browser only when you click them.
- Page images and texts prepared for Claude, the library and the settings are stored locally in `%LOCALAPPDATA%\PDF-Tutor-Daten`.

## Notes

- AI answers can be wrong. Check them before you rely on them.
- PDF-Tutor is an independent hobby project. It is not affiliated with, endorsed or reviewed by Anthropic.

## Building from source

You need [Rust](https://rustup.rs) (on Windows including the Visual Studio Build Tools) and [Node.js](https://nodejs.org).

```
npm ci
npx tauri build
```

On Windows, `build_windows.bat` does the same with a double click. The installer ends up in `src-tauri/target/release/bundle/nsis/`. GitHub Actions builds the installer on every push. A release is created via Actions → "Build Windows installer" → *Run workflow* with "Create release draft" ticked (see `.github/workflows/build.yml`).

Code structure: see [CLAUDE.md](CLAUDE.md). Translations of the interface are in `ui/i18n.js`.

## License

Licensed under either the [MIT license](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE), at your option. Licenses of the bundled third-party components (PDF.js, PDFium, KaTeX and others) are listed in [THIRD_PARTY.md](THIRD_PARTY.md).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

*Status: the application with SignPath Foundation is in progress. Until it is approved, releases are not signed.*

- Committers and reviewers: [Jonas Reitbauer (@Jonas5356)](https://github.com/Jonas5356)
- Approvers: [Jonas Reitbauer (@Jonas5356)](https://github.com/Jonas5356)

Only binaries built by GitHub Actions from the source code in this repository are signed ([build workflow](.github/workflows/build.yml)). Every signing request is approved manually.

**Privacy policy:** This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it, with the exception of the update check described under [Privacy](#privacy), which only downloads a version file and sends no information about the user. Questions to Claude are sent through Claude Code to Anthropic only when the user asks them ([Anthropic Privacy Policy](https://www.anthropic.com/legal/privacy)).

**System changes:** The installer installs PDF-Tutor for the current user (no administrator rights) and adds PDF-Tutor to the "Open with" list for PDF files. It does not change the default PDF program. The uninstaller removes the program and these entries.

---

## Deutsch

Der PDF-Tutor ist ein PDF-Reader zum Durcharbeiten von Fachliteratur: Stelle markieren, Frage stellen, die Antwort von Claude erscheint direkt daneben, mit sauber gesetzten Formeln. Markierungen, Kommentare, Haftnotizen und Claude-Gespräche werden als normale PDF-Kommentare in der Datei gespeichert und sind auch in Acrobat oder Edge sichtbar. Dazu kommen eine Seitenleiste mit Inhaltsverzeichnis, Seitenvorschau und Notizliste, die Suche mit Strg+F, Lernzettel (Drucken/PDF, HTML, Markdown) und das Zusammenfügen und Umsortieren von PDFs.

Voraussetzung ist Windows 10 oder 11 und ein installiertes, angemeldetes [Claude Code](https://docs.claude.com/en/docs/claude-code/overview) mit eigenem Claude-Konto. Den Installer gibt es unter [Releases](../../releases); er fragt nach der Sprache, die auch das Programm verwendet. Claude antwortet in der Sprache der Frage.

Der PDF-Tutor ist ein unabhängiges Freizeitprojekt und steht in keiner Verbindung zu Anthropic. KI-Antworten können falsch sein.
