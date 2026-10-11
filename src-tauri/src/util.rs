//! Kleine Hilfsfunktionen: Zeit, IDs, robustes Schreiben, Protokoll.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Datum im PDF-Format, z. B. D:20261011193000+02'00'
pub fn pdf_date() -> String {
    let n = chrono::Local::now();
    let off = n.format("%z").to_string(); // +0200
    if off.len() == 5 {
        format!("D:{}{}'{}'", n.format("%Y%m%d%H%M%S"), &off[..3], &off[3..])
    } else {
        format!("D:{}", n.format("%Y%m%d%H%M%S"))
    }
}

/// PDF-Datum (D:YYYYMMDDHHmmSS…) in ISO-Form umwandeln, soweit möglich.
pub fn pdf_date_to_iso(s: &str) -> String {
    let d: String = s.trim_start_matches("D:").chars().filter(|c| c.is_ascii_digit()).take(14).collect();
    if d.len() >= 8 {
        let g = |a: usize, b: usize| d.get(a..b).unwrap_or("00").to_string();
        format!("{}-{}-{}T{}:{}:{}", g(0, 4), g(4, 6), g(6, 8), g(8, 10), g(10, 12), g(12, 14))
    } else {
        String::new()
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

pub fn sha1_hex(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).digest().to_string()
}

/// Pfad für Vergleiche normalisieren (unter Windows ohne Groß-/Kleinschreibung).
pub fn norm_path(p: &Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn is_transient(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }
    // Windows: ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33)
    matches!(e.raw_os_error(), Some(32) | Some(33))
}

/// Erst in eine Hilfsdatei schreiben, dann ersetzen. Virenscanner, OneDrive und die
/// Windows-Suche halten Dateien oft kurz offen, daher mehrere Versuche.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name, new_id()));
    std::fs::write(&tmp, data)?;
    let mut last = None;
    for _ in 0..25 {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last.unwrap_or_else(|| std::io::Error::other("Datei konnte nicht ersetzt werden")))
}

/// Ist die Datei gerade von einem anderen Programm gesperrt (z. B. Acrobat)?
pub fn is_locked_error(e: &std::io::Error) -> bool {
    is_transient(e)
}

/// Datei liegt (bei OneDrive & Co.) nur in der Cloud; Lesen würde einen Download auslösen.
#[cfg(windows)]
pub fn is_cloud_only(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    match std::fs::metadata(path) {
        Ok(m) => m.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0,
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn is_cloud_only(_path: &Path) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Protokolldatei

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init_log(path: PathBuf) {
    if let Ok(m) = std::fs::metadata(&path) {
        if m.len() > 2_000_000 {
            let _ = std::fs::rename(&path, path.with_extension("alt.log"));
        }
    }
    let _ = LOG_PATH.set(path);
    log(&format!("===== Start {} =====", now_iso()));
    std::panic::set_hook(Box::new(|info| {
        log(&format!("ABSTURZ: {info}"));
    }));
}

pub fn log(msg: &str) {
    eprintln!("{msg}");
    if let Some(p) = LOG_PATH.get() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{msg}");
        }
    }
}

#[macro_export]
macro_rules! logf {
    ($($t:tt)*) => { $crate::util::log(&format!($($t)*)) };
}
