//! Gemeinsamer Programmzustand.

use crate::doc::Doc;
use crate::library::Library;
use crate::settings::{Paths, Settings};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

pub struct Ctx {
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub lib: Library,
    pub docs: Mutex<HashMap<String, Arc<Doc>>>,
    /// Verhindert, dass zwei gleichzeitige Anfragen dasselbe Dokument doppelt öffnen
    /// (sonst würden z. B. alte Notizen zweimal übernommen).
    open_lock: Mutex<()>,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    Bad(String),
}

impl From<String> for ApiError {
    fn from(s: String) -> Self {
        ApiError::Bad(s)
    }
}

impl Ctx {
    pub fn new(paths: Paths) -> Ctx {
        let settings = Settings::load(&paths.settings);
        let lib = Library::load(paths.library.clone());
        Ctx { paths, settings: RwLock::new(settings), lib, docs: Mutex::new(HashMap::new()), open_lock: Mutex::new(()) }
    }

    pub fn get_doc(self: &Arc<Self>, id: &str) -> Result<Arc<Doc>, ApiError> {
        let entry = self.lib.get(id).ok_or_else(|| ApiError::NotFound(crate::i18n::tx("Document not in the library", "Dokument nicht in der Bibliothek")))?;
        let path = Path::new(&entry.path).to_path_buf();
        if !path.is_file() {
            return Err(ApiError::NotFound(crate::i18n::tx(&format!("File not found: {}", path.display()), &format!("Datei nicht gefunden: {}", path.display()))));
        }
        let cached = |ctx: &Ctx| -> Option<Arc<Doc>> {
            ctx.docs.lock().unwrap().get(id).filter(|d| d.path == path).cloned()
        };
        if let Some(d) = cached(self) {
            return Ok(d);
        }
        // Öffnen außerhalb der Dokumentliste (kann etwas dauern), aber nur einmal zur Zeit
        let _guard = self.open_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(d) = cached(self) {
            return Ok(d);
        }
        let d = Doc::open(self, id, &path)?;
        // ersetzt auch einen Eintrag mit altem Pfad (nach "Datei suchen")
        self.docs.lock().unwrap().insert(id.to_string(), d.clone());
        Ok(d)
    }
}
