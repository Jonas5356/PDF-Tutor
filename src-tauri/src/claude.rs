//! Claude Code aufrufen (`claude -p`) und die Prompts zusammenstellen.

use crate::cache::PageCache;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

pub const TOOLS: &str = "Read,Grep,Glob"; // nur lesende Werkzeuge
const TIMEOUT_SECS: u64 = 900;

pub fn models() -> Value {
    json!([
        {"id": "sonnet", "label": "Sonnet (schneller)"},
        {"id": "opus", "label": "Opus (gründlicher)"}
    ])
}
pub const DEFAULT_MODEL: &str = "sonnet";

pub fn find_claude() -> Option<PathBuf> {
    if let Ok(p) = which::which("claude") {
        return Some(p);
    }
    // Programme aus dem Startmenü sehen nicht immer den vollen PATH
    let home = crate::settings::home();
    let mut candidates = vec![home.join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" })];
    if cfg!(windows) {
        let appdata = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| home.join("AppData").join("Roaming"));
        candidates.push(appdata.join("npm").join("claude.cmd"));
        candidates.push(home.join(".claude").join("local").join("claude.exe"));
    } else {
        candidates.push(home.join(".claude").join("local").join("claude"));
    }
    candidates.into_iter().find(|c| c.is_file())
}

pub struct Answer {
    pub text: String,
    pub session: Option<String>,
    pub usage: Value,
}

/// Ruft `claude -p` auf. Der Prompt geht über stdin, der gleichbleibende Teil als
/// System-Prompt-Datei (zwischenspeicherbar).
pub fn run(prompt: &str, model: &str, system_path: &Path, session: Option<&str>, cwd: &Path) -> Result<Answer, String> {
    let exe = find_claude().ok_or_else(|| {
        "Claude Code wurde nicht gefunden. Ist es installiert? (Test: in der PowerShell 'claude --version' eingeben)".to_string()
    })?;
    let mut cmd = Command::new(&exe);
    cmd.args(["-p", "--output-format", "json", "--model", model, "--tools", TOOLS, "--allowedTools", TOOLS])
        .arg("--append-system-prompt-file")
        .arg(system_path);
    if let Some(s) = session {
        cmd.args(["--resume", s]);
    }
    cmd.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("Claude Code konnte nicht gestartet werden: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let input = prompt.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut out_pipe = child.stdout.take().unwrap();
    let mut err_pipe = child.stderr.take().unwrap();
    let out_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out_pipe.read_to_end(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err_pipe.read_to_end(&mut s);
        s
    });
    let status = match child.wait_timeout(Duration::from_secs(TIMEOUT_SECS)).map_err(|e| e.to_string())? {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Claude hat nicht innerhalb von {} Minuten geantwortet.", TIMEOUT_SECS / 60));
        }
    };
    let _ = writer.join();
    let out = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).trim().to_string();
    let err = String::from_utf8_lossy(&err_t.join().unwrap_or_default()).trim().to_string();
    let mut result: Option<Value> = None;
    for line in out.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                result = Some(v);
                break;
            }
        }
    }
    let result = match result.or_else(|| serde_json::from_str(&out).ok()) {
        Some(r) => r,
        None => {
            let detail = if !err.is_empty() { err } else if !out.is_empty() { out } else { format!("Exit-Code {status}") };
            let tail: String = detail.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(format!("Unerwartete Ausgabe von Claude Code:\n{tail}"));
        }
    };
    let ok_subtype = result["subtype"].as_str().map(|s| s == "success").unwrap_or(true);
    if result["is_error"].as_bool().unwrap_or(false) || !ok_subtype {
        let detail = result["result"]
            .as_str()
            .or(result["subtype"].as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| if err.is_empty() { "unbekannter Fehler".into() } else { err.clone() });
        return Err(format!("Claude Code meldet einen Fehler: {detail}"));
    }
    let text = result["result"].as_str().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Err("Claude Code hat eine leere Antwort geliefert.".into());
    }
    let u = &result["usage"];
    let usage = json!({
        "input_tokens": u["input_tokens"].as_u64().unwrap_or(0),
        "cache_read_input_tokens": u["cache_read_input_tokens"].as_u64().unwrap_or(0),
        "cache_creation_input_tokens": u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        "output_tokens": u["output_tokens"].as_u64().unwrap_or(0),
    });
    Ok(Answer { text, session: result["session_id"].as_str().map(|s| s.to_string()), usage })
}

// ---------------------------------------------------------------------------
// Prompts

pub const FORMAT_HINWEIS: &str = "Formatierung deiner Antwort: Markdown. Formeln in LaTeX, inline mit $...$, abgesetzt mit $$...$$. \
Keine Überschrift erster Ebene. Beginne direkt mit der Erklärung, ohne Einleitungsfloskel und ohne \
die Frage zu wiederholen. Nenne die Seiten, auf die du dich stützt, wenn es nicht nur die markierte ist.";

pub fn system_text(c: &PageCache, mode: &str, hinweise: &str) -> String {
    let nn = "N".repeat(c.digits);
    let sep = std::path::MAIN_SEPARATOR;
    let mut parts: Vec<String> = vec![
        "# Rolle".into(),
        "Du hilfst beim Durcharbeiten eines Dokuments (Fachbuch, Skript oder Vorlesungsfolien). Der Leser \
markiert Stellen und stellt Fragen dazu. Deine Antworten erscheinen in einer Randspalte neben dem \
Dokument."
            .into(),
        String::new(),
        "# Zugriff auf das Dokument".into(),
        format!("- Text jeder Seite: {}{sep}seite_{nn}.txt", c.text_dir.display()),
        format!("- Bild jeder Seite: {}{sep}seite_{nn}.jpg", c.img_dir.display()),
        format!("  ({nn} = PDF-Seitenzahl mit führenden Nullen, z. B. seite_{:0w$}.txt)", 1, w = c.digits),
        "- Der Text ist automatisch extrahiert. Formeln, Tabellen, Diagramme und Folienlayouts sind darin oft \
unvollständig oder verstümmelt. Wenn es darauf ankommt, sieh dir das Bild der Seite mit dem \
Read-Werkzeug an."
            .into(),
        "- „Diese Seite“ bzw. „diese Folie“ meint die Seite der Markierung. Angaben wie „die nächsten 5 Folien“, \
„Seite 87“, „Abschnitt 3.2“ oder „Kapitel 4“ löst du über die PDF-Seitenzahlen, die gedruckten \
Seitenzahlen und das Inhaltsverzeichnis in der Übersicht auf und liest dann die betreffenden Seiten."
            .into(),
        "- Gedruckte Seitenzahlen können von den PDF-Seitenzahlen abweichen. Nennt der Leser eine Seitenzahl, \
ist im Zweifel die gedruckte gemeint."
            .into(),
        "- Fehlt ein Seitenbild noch (der Reader erzeugt sie im Hintergrund), nutze den Text.".into(),
        "- Lies das Original-PDF nicht direkt, sondern nur diese Dateien.".into(),
    ];
    if mode == "suche" {
        parts.push(format!(
            "- Um Stellen zu finden (wo ein Begriff oder Symbol eingeführt wird, frühere Beispiele, \
Definitionen), durchsuche den Ordner {} mit dem Grep-Werkzeug. Der Dateiname nennt die Seite.",
            c.text_dir.display()
        ));
        parts.push("- Lies gezielt, was du für die Antwort brauchst, nicht das ganze Dokument.".into());
    } else {
        parts.push(
            "- Der vollständige Text des Dokuments steht unten. Für Formeln und Abbildungen sieh dir trotzdem \
die Seitenbilder an."
                .into(),
        );
    }
    if !hinweise.is_empty() {
        parts.push(String::new());
        parts.push("# Hinweise des Lesers zu sich selbst und zu den Antworten".into());
        parts.push(hinweise.to_string());
    }
    parts.push(String::new());
    parts.push("# Format".into());
    parts.push(FORMAT_HINWEIS.into());
    parts.push(String::new());
    parts.push(c.overview_text());
    if mode == "volltext" {
        parts.push(String::new());
        parts.push("# Vollständiger Text des Dokuments".into());
        parts.push(String::new());
        parts.push(c.fulltext());
    }
    parts.join("\n")
}

pub struct HistoryItem {
    pub role: String,
    pub text: String,
}

pub fn first_prompt(
    c: &PageCache,
    page: u32,
    selected: &str,
    question: &str,
    mode: &str,
    images: &[PathBuf],
    history: &[HistoryItem],
) -> String {
    let mut parts: Vec<String> = vec![
        "Neue Frage zu einer markierten Stelle.".into(),
        format!("Dokument: {}", c.title),
        format!("Stelle: {}", c.page_name(page)),
        String::new(),
        "Markierter Text (automatisch extrahiert, Formeln darin oft verstümmelt):".into(),
        "\"\"\"".into(),
        selected.trim().to_string(),
        "\"\"\"".into(),
        String::new(),
        "Bild der Seite, auf dem die markierte Stelle farbig hinterlegt und rot umrandet ist:".into(),
    ];
    for i in images {
        parts.push(format!("- {}", i.display()));
    }
    parts.push("Sieh dir dieses Bild zuerst an.".into());
    if mode == "suche" {
        let lo = page.saturating_sub(2).max(1);
        let hi = (page + 2).min(c.n);
        parts.push(String::new());
        parts.push(format!("Zur Orientierung der Text der umliegenden Seiten ({lo}–{hi}):"));
        for q in lo..=hi {
            parts.push(format!("===== {} =====", c.page_name(q)));
            parts.push(c.page_text(q).chars().take(5000).collect());
        }
    }
    if !history.is_empty() {
        parts.push(String::new());
        parts.push("Bisheriger Gesprächsverlauf zu dieser Stelle:".into());
        for m in history {
            let who = if m.role == "user" { "Leser" } else { "Du" };
            parts.push(format!("[{who}]: {}", m.text));
        }
    }
    parts.push(String::new());
    parts.push("Frage des Lesers:".into());
    parts.push(question.trim().to_string());
    parts.join("\n")
}

pub fn followup_prompt(question: &str) -> String {
    format!("Nachfrage des Lesers zur selben markierten Stelle:\n{}", question.trim())
}
