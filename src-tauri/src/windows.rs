//! Programmfenster öffnen und aktivieren.

use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Erlaubte Seiten: Startseite und Reader mit Dokument-ID.
fn valid(url: &str) -> bool {
    if url.is_empty() || url == "index.html" {
        return true;
    }
    match url.strip_prefix("reader.html?id=") {
        Some(id) => id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

pub fn open(app: &AppHandle, url: &str) -> Result<(), String> {
    if !valid(url) {
        return Err("ungültige Adresse".into());
    }
    let url = if url.is_empty() { "index.html" } else { url };
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let label = if n == 0 && app.get_webview_window("main").is_none() { "main".to_string() } else { format!("fenster{n}") };
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("PDF-Tutor")
        .inner_size(1440.0, 920.0)
        .min_inner_size(900.0, 600.0)
        .maximized(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Vorhandenes Fenster nach vorne holen (beim zweiten Programmstart).
pub fn focus_any(app: &AppHandle) {
    if let Some((_, w)) = app.webview_windows().into_iter().next() {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    } else {
        let _ = open(app, "index.html");
    }
}
