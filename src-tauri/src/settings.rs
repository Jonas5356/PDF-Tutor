//! Datenordner, Einstellungen und Hinweise an Claude.

use crate::util::write_atomic;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub struct Paths {
    pub data: PathBuf,
    pub work: PathBuf,
    pub marks: PathBuf,
    pub thumbs: PathBuf,
    pub backups: PathBuf,
    pub hinweise: PathBuf,
    pub settings: PathBuf,
    pub library: PathBuf,
    pub log: PathBuf,
}

impl Paths {
    pub fn new() -> Paths {
        let data = if let Ok(d) = std::env::var("PDFTUTOR_DATA") {
            PathBuf::from(d)
        } else if cfg!(windows) {
            let base = std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home().join("AppData").join("Local"));
            // Eigener Datenordner, getrennt vom Programmordner (%LOCALAPPDATA%\PDF-Tutor)
            let data = base.join("PDF-Tutor-Daten");
            migrate_v3(&base.join("PDF-Tutor"), &data);
            data
        } else {
            home().join(".local").join("share").join("pdf-tutor")
        };
        let work = data.join("arbeit");
        let p = Paths {
            marks: work.join("markierungen"),
            thumbs: data.join("vorschau"),
            backups: data.join("sicherungen"),
            hinweise: data.join("hinweise.txt"),
            settings: data.join("einstellungen.json"),
            library: data.join("bibliothek.json"),
            log: data.join("pdf-tutor.log"),
            work,
            data,
        };
        for d in [&p.data, &p.work, &p.marks, &p.thumbs] {
            let _ = std::fs::create_dir_all(d);
        }
        p
    }
}

/// Daten aus Version 3 (lag in %LOCALAPPDATA%\PDF-Tutor) beim ersten Start übernehmen.
fn migrate_v3(old: &Path, new: &Path) {
    if new.exists() || !old.join("bibliothek.json").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(new);
    for f in ["bibliothek.json", "einstellungen.json", "hinweise.txt"] {
        let _ = std::fs::copy(old.join(f), new.join(f));
    }
    for d in ["notizen", "sicherungen"] {
        let src = old.join(d);
        if let Ok(rd) = std::fs::read_dir(&src) {
            let _ = std::fs::create_dir_all(new.join(d));
            for e in rd.flatten() {
                let _ = std::fs::copy(e.path(), new.join(d).join(e.file_name()));
            }
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn default_limit() -> u64 {
    30000
}
fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    #[serde(default = "default_limit")]
    pub volltext_grenze_tokens: u64,
    /// Name, der als Autor von Markierungen und Kommentaren ins PDF geschrieben wird
    #[serde(default)]
    pub autor: String,
    /// Vor der ersten Änderung eines PDFs eine Kopie im Datenordner ablegen
    #[serde(default = "default_true")]
    pub sicherheitskopie: bool,
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let s: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| serde_json::from_str("{}").unwrap());
        if !path.exists() {
            let _ = s.save(path);
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self).unwrap().as_bytes())
    }

    pub fn author(&self) -> String {
        if !self.autor.trim().is_empty() {
            return self.autor.trim().to_string();
        }
        std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Leser".into())
    }
}

pub fn ensure_hinweise(paths: &Paths, template: Option<PathBuf>) {
    if paths.hinweise.exists() {
        return;
    }
    if let Some(t) = template {
        if let Ok(text) = std::fs::read(&t) {
            let _ = write_atomic(&paths.hinweise, &text);
        }
    }
}

pub fn read_hinweise(paths: &Paths) -> String {
    std::fs::read_to_string(&paths.hinweise).map(|s| s.trim().to_string()).unwrap_or_default()
}
