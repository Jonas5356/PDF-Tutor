//! Claude Code aufrufen (`claude -p`) und die Prompts zusammenstellen.

use crate::cache::PageCache;
use crate::i18n::tx;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

pub const TOOLS: &str = "Read,Grep,Glob"; // nur lesende Werkzeuge
const TIMEOUT_SECS: u64 = 900;

/// Modelle für Fragen. Die Kurznamen zeigen in Claude Code immer auf die neueste Version.
/// `extra`: kann je nach Abo über Zusatzguthaben abgerechnet werden (vor der Auswahl nachfragen).
pub fn models() -> Value {
    json!([
        {"id": "haiku", "label": tx("Haiku (fastest)", "Haiku (am schnellsten)")},
        {"id": "sonnet", "label": tx("Sonnet (balanced)", "Sonnet (ausgewogen)")},
        {"id": "opus", "label": tx("Opus (more thorough)", "Opus (gründlicher)")},
        {"id": "fable", "label": tx("Fable (strongest) – may cost extra", "Fable (am stärksten) – evtl. Zusatzkosten"), "extra": true}
    ])
}

/// Denktiefe (Claude Code: --effort). Leer: Voreinstellung des Modells.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn valid_effort(e: &str) -> Option<String> {
    EFFORTS.contains(&e).then(|| e.to_string())
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
pub fn run(
    prompt: &str,
    model: &str,
    effort: Option<&str>,
    system_path: &Path,
    session: Option<&str>,
    cwd: &Path,
) -> Result<Answer, String> {
    let exe = find_claude().ok_or_else(|| {
        tx(
            "Claude Code was not found. Is it installed? (Test: run 'claude --version' in PowerShell)",
            "Claude Code wurde nicht gefunden. Ist es installiert? (Test: in der PowerShell 'claude --version' eingeben)",
        )
    })?;
    let mut cmd = Command::new(&exe);
    cmd.args(["-p", "--output-format", "json", "--model", model, "--tools", TOOLS, "--allowedTools", TOOLS])
        .arg("--append-system-prompt-file")
        .arg(system_path);
    if let Some(e) = effort.filter(|e| EFFORTS.contains(e)) {
        cmd.args(["--effort", e]);
    }
    if let Some(s) = session {
        cmd.args(["--resume", s]);
    }
    cmd.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| tx(&format!("Claude Code could not be started: {e}"), &format!("Claude Code konnte nicht gestartet werden: {e}")))?;
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
            let m = TIMEOUT_SECS / 60;
            return Err(tx(&format!("Claude did not answer within {m} minutes."), &format!("Claude hat nicht innerhalb von {m} Minuten geantwortet.")));
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
            let detail = if !err.is_empty() { err } else if !out.is_empty() { out } else { format!("exit code {status}") };
            let tail: String = detail.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(tx(&format!("Unexpected output from Claude Code:\n{tail}"), &format!("Unerwartete Ausgabe von Claude Code:\n{tail}")));
        }
    };
    let ok_subtype = result["subtype"].as_str().map(|s| s == "success").unwrap_or(true);
    if result["is_error"].as_bool().unwrap_or(false) || !ok_subtype {
        let detail = result["result"]
            .as_str()
            .or(result["subtype"].as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| if err.is_empty() { tx("unknown error", "unbekannter Fehler") } else { err.clone() });
        return Err(tx(&format!("Claude Code reports an error: {detail}"), &format!("Claude Code meldet einen Fehler: {detail}")));
    }
    let text = result["result"].as_str().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Err(tx("Claude Code returned an empty answer.", "Claude Code hat eine leere Antwort geliefert."));
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

// Die Anweisungen an Claude sind englisch; geantwortet wird in der Sprache der Frage.

pub const FORMAT_HINT: &str = "Format of your answer: Markdown. Math in LaTeX, inline with $...$, displayed with $$...$$. \
No first-level heading. Start directly with the explanation, without an introductory phrase and without \
repeating the question. Mention the pages you rely on if they are not just the highlighted one.";

pub const LANGUAGE_RULE: &str = "Answer in the language in which the reader asks the question, regardless of the \
language of the document or of these instructions. Only if the question explicitly asks for a different language \
(e.g. \"answer in English\" / \"antworte auf Deutsch\"), use that language. Keep technical terms from the \
document where helpful.";

pub fn system_text(c: &PageCache, mode: &str, hints: &str) -> String {
    let nn = "N".repeat(c.digits);
    let sep = std::path::MAIN_SEPARATOR;
    let mut parts: Vec<String> = vec![
        "# Role".into(),
        "You help the reader work through a document (textbook, lecture notes or lecture slides). The reader \
highlights passages and asks questions about them. Your answers appear in a margin column next to the \
document."
            .into(),
        String::new(),
        "# Language".into(),
        LANGUAGE_RULE.into(),
        String::new(),
        "# Access to the document".into(),
        format!("- Text of each page: {}{sep}seite_{nn}.txt", c.text_dir.display()),
        format!("- Image of each page: {}{sep}seite_{nn}.jpg", c.img_dir.display()),
        format!("  ({nn} = PDF page number with leading zeros, e.g. seite_{:0w$}.txt)", 1, w = c.digits),
        "- The text is extracted automatically. Formulas, tables, diagrams and slide layouts are often \
incomplete or garbled in it. When they matter, look at the image of the page with the Read tool."
            .into(),
        "- \"This page\" or \"this slide\" refers to the page of the highlight. References such as \"the next \
5 slides\", \"page 87\", \"section 3.2\" or \"chapter 4\" are resolved via the PDF page numbers, the printed \
page numbers and the table of contents in the overview; then read the pages concerned."
            .into(),
        "- Printed page numbers can differ from PDF page numbers. If the reader names a page number, the \
printed one is meant when in doubt."
            .into(),
        "- If a page image does not exist yet (the reader creates them in the background), use the text.".into(),
        "- Do not read the original PDF directly, only these files.".into(),
    ];
    if mode == "suche" {
        parts.push(format!(
            "- To find passages (where a term or symbol is introduced, earlier examples, definitions), search \
the folder {} with the Grep tool. The file name gives the page.",
            c.text_dir.display()
        ));
        parts.push("- Read selectively what you need for the answer, not the whole document.".into());
    } else {
        parts.push(
            "- The complete text of the document is below. For formulas and figures, still look at the page \
images."
                .into(),
        );
    }
    if !hints.is_empty() {
        parts.push(String::new());
        parts.push("# Notes from the reader about themselves and the answers".into());
        parts.push(hints.to_string());
    }
    parts.push(String::new());
    parts.push("# Format".into());
    parts.push(FORMAT_HINT.into());
    parts.push(String::new());
    parts.push(c.overview_text());
    if mode == "volltext" {
        parts.push(String::new());
        parts.push("# Complete text of the document".into());
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
        "New question about a highlighted passage.".into(),
        format!("Document: {}", c.title),
        format!("Location: {}", c.page_name(page)),
        String::new(),
        "Highlighted text (extracted automatically, formulas in it are often garbled):".into(),
        "\"\"\"".into(),
        selected.trim().to_string(),
        "\"\"\"".into(),
        String::new(),
        "Image of the page, with the highlighted passage shaded and outlined in red:".into(),
    ];
    for i in images {
        parts.push(format!("- {}", i.display()));
    }
    parts.push("Look at this image first.".into());
    if mode == "suche" {
        let lo = page.saturating_sub(2).max(1);
        let hi = (page + 2).min(c.n);
        parts.push(String::new());
        parts.push(format!("For orientation, the text of the surrounding pages ({lo}–{hi}):"));
        for q in lo..=hi {
            parts.push(format!("===== {} =====", c.page_name(q)));
            parts.push(c.page_text(q).chars().take(5000).collect());
        }
    }
    if !history.is_empty() {
        parts.push(String::new());
        parts.push("Conversation so far about this passage:".into());
        for m in history {
            let who = if m.role == "user" { "Reader" } else { "You" };
            parts.push(format!("[{who}]: {}", m.text));
        }
    }
    parts.push(String::new());
    parts.push("Question from the reader (answer in its language):".into());
    parts.push(question.trim().to_string());
    parts.join("\n")
}

pub fn followup_prompt(question: &str) -> String {
    format!(
        "Follow-up question from the reader about the same highlighted passage (answer in its language):\n{}",
        question.trim()
    )
}
