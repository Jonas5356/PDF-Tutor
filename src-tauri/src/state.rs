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
        Ctx { paths, settings: RwLock::new(settings), lib, docs: Mutex::new(HashMap::new()) }
    }

    pub fn get_doc(self: &Arc<Self>, id: &str) -> Result<Arc<Doc>, ApiError> {
        let entry = self.lib.get(id).ok_or_else(|| ApiError::NotFound(crate::i18n::tx("Document not in the library", "Dokument nicht in der Bibliothek")))?;
        let path = Path::new(&entry.path).to_path_buf();
        if !path.is_file() {
            return Err(ApiError::NotFound(crate::i18n::tx(&format!("File not found: {}", path.display()), &format!("Datei nicht gefunden: {}", path.display()))));
        }
        {
            let docs = self.docs.lock().unwrap();
            if let Some(d) = docs.get(id) {
                if d.path == path {
                    return Ok(d.clone());
                }
            }
        }
        // Öffnen außerhalb der Sperre (kann etwas dauern)
        let d = Doc::open(self, id, &path)?;
        let mut docs = self.docs.lock().unwrap();
        let d = docs.entry(id.to_string()).or_insert(d).clone();
        Ok(d)
    }
}
