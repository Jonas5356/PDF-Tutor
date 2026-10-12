//! Sprache des Programms (Englisch oder Deutsch) für Meldungen aus dem Rust-Teil.

use std::sync::atomic::{AtomicBool, Ordering};

static GERMAN: AtomicBool = AtomicBool::new(false);

pub fn set(lang: &str) {
    GERMAN.store(lang == "de", Ordering::Relaxed);
}

pub fn lang() -> &'static str {
    if GERMAN.load(Ordering::Relaxed) {
        "de"
    } else {
        "en"
    }
}

/// Text in der eingestellten Sprache.
pub fn tx(en: &str, de: &str) -> String {
    if GERMAN.load(Ordering::Relaxed) { de } else { en }.to_string()
}

/// "de" oder "en", wie angegeben; alles andere wird None.
pub fn normalize(s: &str) -> Option<String> {
    match s.trim().to_lowercase().as_str() {
        "de" | "deutsch" | "german" => Some("de".into()),
        "en" | "english" | "englisch" => Some("en".into()),
        _ => None,
    }
}

/// Sprache des Systems: Deutsch, falls die Systemsprache Deutsch ist, sonst Englisch.
pub fn system_lang() -> String {
    match sys_locale::get_locale() {
        Some(l) if l.to_lowercase().starts_with("de") => "de".into(),
        _ => "en".into(),
    }
}
