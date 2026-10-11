# PDF-Tutor

**Ein PDF-Reader zum Durcharbeiten von Fachliteratur: Stelle markieren, Frage stellen, die Antwort erscheint direkt daneben.**

Der PDF-Tutor ist für Lehrbücher, Skripten und Vorlesungsfolien gedacht. Du liest wie in jedem anderen Reader. Wenn dir eine Stelle unklar ist, markierst du sie und stellst eine Frage. Claude sieht dabei die markierte Stelle, die ganze Seite als Bild (Formeln und Abbildungen inklusive) und bei Bedarf den Rest des Dokuments. Die Antwort erscheint in einer Randspalte neben dem Text, mit sauber gesetzten Formeln. Nachfragen sind jederzeit möglich.

*English summary below.*

## Funktionen

- **Fragen zu markierten Stellen.** Claude bekommt die markierte Stelle, ein Bild der Seite und den Kontext des Dokuments. Du kannst auf andere Stellen verweisen, etwa mit „diese und die nächsten 5 Folien“, „Seite 87“, „Abschnitt 3.2“ oder „Wo wurde dieses Symbol eingeführt?“.
- **Alles steht im PDF.** Markierungen, Fragen und Antworten werden als Standard-PDF-Kommentare gespeichert, als Markierung mit Antwort-Thread wie in Acrobat. Sie sind auch in Acrobat, Edge oder auf dem Tablet sichtbar, und Kommentare aus diesen Programmen erscheinen umgekehrt im PDF-Tutor. Gespeichert wird inkrementell: Der ursprüngliche Inhalt der Datei bleibt Byte für Byte erhalten.
- **Volltext- oder Suchmodus.** Kleinere Dokumente bekommt Claude komplett mit. In großen Büchern sucht Claude gezielt nach den relevanten Seiten.
- **Bibliothek.** Unter „Weiterlesen“ findest du zuletzt geöffnete Dokumente samt Lesefortschritt. Zusätzlich lassen sich Ordner einbinden, zum Beispiel deine Vorlesungsunterlagen.
- **Eigene Hinweise an Claude.** Du kannst einstellen, welches Vorwissen vorausgesetzt werden darf und wie ausführlich die Erklärungen sein sollen.

## Voraussetzungen

- Windows 10 oder 11
- [Claude Code](https://docs.claude.com/en/docs/claude-code/overview), installiert und mit deinem eigenen Claude-Konto (Pro oder Max) oder einem eigenen API-Schlüssel angemeldet. Zum Prüfen gibst du in der PowerShell `claude --version` ein.

Der PDF-Tutor ruft das auf deinem Rechner installierte Claude Code auf. Er speichert oder verwendet keine Zugangsdaten. Fragen werden über dein eigenes Konto abgerechnet und zählen zu dessen Nutzungslimits.

## Installation

Unter [Releases](../../releases) die Datei `PDF-Tutor_…_x64-setup.exe` herunterladen und ausführen. Administratorrechte sind nicht nötig.

Windows zeigt eventuell die Meldung „Der Computer wurde durch Windows geschützt“, weil das Programm nicht signiert ist. Klicke dann auf „Weitere Informationen“ und anschließend auf „Trotzdem ausführen“.

## Datenschutz

- Keine Telemetrie, keine eigenen Server.
- Bei einer Frage schickt Claude Code die Frage, die markierte Stelle, Seitenbilder und Seitentexte des Dokuments an Anthropic. Dafür gelten die Bedingungen deines Claude-Kontos.
- Seitenbilder und -texte für Claude, die Bibliothek und die Einstellungen liegen lokal unter `%LOCALAPPDATA%\PDF-Tutor-Daten`.

## Hinweise

- KI-Antworten können falsch sein. Prüfe sie, bevor du dich auf sie verlässt.
- Der PDF-Tutor ist ein unabhängiges Freizeitprojekt. Er steht in keiner Verbindung zu Anthropic und wird von Anthropic weder unterstützt noch geprüft.

## Selbst bauen

Benötigt werden [Rust](https://rustup.rs) (unter Windows inklusive Visual-Studio-Buildtools) und [Node.js](https://nodejs.org).

```
npm ci
npx tauri build
```

Unter Windows erledigt `build_windows.bat` dasselbe per Doppelklick. Der Installer liegt danach in `src-tauri/target/release/bundle/nsis/`. Bei jedem Tag `v*` baut GitHub Actions den Installer automatisch (siehe `.github/workflows/build.yml`).

Aufbau des Codes: siehe [CLAUDE.md](CLAUDE.md).

## Lizenz

Wahlweise unter der [MIT-Lizenz](LICENSE-MIT) oder der [Apache-Lizenz 2.0](LICENSE-APACHE). Die Lizenzen der enthaltenen Fremdkomponenten (PDF.js, PDFium, KaTeX u. a.) stehen in [THIRD_PARTY.md](THIRD_PARTY.md).

---

## English summary

PDF-Tutor is a PDF reader for studying textbooks, lecture notes and slides. Select a passage, ask a question, and Claude's answer appears in a margin column next to the text, with properly typeset math. Claude sees the selected passage, an image of the page and, as needed, the rest of the document.

Highlights, questions and answers are stored **inside the PDF** as standard annotations (a highlight with a reply thread). They show up in Acrobat, Edge and other readers. Saving is incremental: the original bytes of the file are never modified.

Requirements: Windows 10/11 and [Claude Code](https://docs.claude.com/en/docs/claude-code/overview), signed in with your own Claude subscription or API key. PDF-Tutor never handles credentials; usage is billed to your own account. The UI is currently German only.

Not affiliated with or endorsed by Anthropic. Licensed under MIT or Apache-2.0, at your option.
