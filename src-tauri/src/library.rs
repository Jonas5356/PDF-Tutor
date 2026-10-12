//! Bibliothek: zuletzt geöffnete Dokumente, Bibliotheksordner, Ansichtszustand je Dokument.
//! Gespeichert in bibliothek.json im Datenordner (kompatibel mit Version 3).

use crate::util::{norm_path, now_iso, sha1_hex, write_atomic};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_SCAN_FILES: usize = 3000;
const RECENT_LIMIT: usize = 12;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pages: Option<u32>,
    #[serde(default)]
    pub added: String,
    #[serde(default)]
    pub last_opened: Option<String>,
    #[serde(default)]
    pub last_page: Option<u32>,
    #[serde(default)]
    pub notes: u32,
    #[serde(default)]
    pub ui: UiState,
    #[serde(default)]
    pub notes_imported: bool,
    #[serde(default)]
    pub backup_done: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    docs: BTreeMap<String, Entry>,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    hidden: Vec<String>,
}

pub struct Library {
    path: PathBuf,
    data: Mutex<Data>,
}

pub fn doc_id_for(path: &Path) -> String {
    sha1_hex(norm_path(path).as_bytes())[..12].to_string()
}

fn notes_json_summary(pdf: &Path) -> (u32, Option<u32>) {
    let nj = pdf.with_file_name(format!("{}.notes.json", pdf.file_stem().unwrap_or_default().to_string_lossy()));
    match std::fs::read_to_string(nj).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(v) => (
            v["notes"].as_array().map(|a| a.len() as u32).unwrap_or(0),
            v["state"]["lastPage"].as_u64().map(|x| x as u32),
        ),
        None => (0, None),
    }
}

impl Library {
    pub fn load(path: PathBuf) -> Library {
        let mut data: Data = match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).unwrap_or_else(|e| {
                crate::logf!("bibliothek.json beschädigt ({e}), beginne neu.");
                let _ = std::fs::rename(&path, path.with_extension("defekt.json"));
                Data::default()
            }),
            Err(_) => Data::default(),
        };
        data.version = 2;
        Library { path, data: Mutex::new(data) }
    }

    fn save(&self, d: &Data) {
        if let Err(e) = write_atomic(&self.path, serde_json::to_string_pretty(d).unwrap().as_bytes()) {
            crate::logf!("Bibliothek speichern: {e}");
        }
    }

    fn find_by_path(d: &Data, path: &Path) -> Option<String> {
        let n = norm_path(path);
        d.docs.iter().find(|(_, e)| norm_path(Path::new(&e.path)) == n).map(|(k, _)| k.clone())
    }

    fn new_entry(d: &mut Data, path: &Path) -> String {
        let mut id = doc_id_for(path);
        while d.docs.contains_key(&id) {
            id = sha1_hex(format!("{id}x").as_bytes())[..12].to_string();
        }
        let (notes, last) = notes_json_summary(path);
        d.docs.insert(
            id.clone(),
            Entry {
                path: path.to_string_lossy().to_string(),
                title: None,
                pages: None,
                added: now_iso(),
                last_opened: None,
                last_page: last,
                notes,
                ui: UiState::default(),
                notes_imported: false,
                backup_done: false,
            },
        );
        id
    }

    /// ID zu einem PDF-Pfad, legt bei Bedarf einen Eintrag an.
    pub fn ensure(&self, path: &Path) -> String {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut d = self.data.lock().unwrap();
        let id = match Self::find_by_path(&d, &path) {
            Some(id) => id,
            None => Self::new_entry(&mut d, &path),
        };
        d.hidden.retain(|h| h != &id);
        self.save(&d);
        id
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.data.lock().unwrap().docs.get(id).cloned()
    }

    pub fn update(&self, id: &str, f: impl FnOnce(&mut Entry)) {
        let mut d = self.data.lock().unwrap();
        if let Some(e) = d.docs.get_mut(id) {
            f(e);
            self.save(&d);
        }
    }

    fn in_folder(d: &Data, path: &Path) -> bool {
        let n = norm_path(path);
        d.folders.iter().any(|f| {
            let fnorm = norm_path(Path::new(f));
            n.starts_with(&format!("{}{}", fnorm.trim_end_matches(['/', '\\']), std::path::MAIN_SEPARATOR))
        })
    }

    pub fn remove_recent(&self, id: &str) {
        let mut d = self.data.lock().unwrap();
        let in_folder = d.docs.get(id).map(|e| Self::in_folder(&d, Path::new(&e.path))).unwrap_or(false);
        if in_folder {
            if let Some(e) = d.docs.get_mut(id) {
                e.last_opened = None;
            }
        } else {
            d.docs.remove(id);
        }
        self.save(&d);
    }

    pub fn hide(&self, id: &str) {
        let mut d = self.data.lock().unwrap();
        if !d.hidden.iter().any(|h| h == id) {
            d.hidden.push(id.to_string());
        }
        if let Some(e) = d.docs.get_mut(id) {
            e.last_opened = None;
        }
        self.save(&d);
    }

    pub fn relink(&self, id: &str, new_path: &Path) -> Result<(), String> {
        let new_path = std::path::absolute(new_path).unwrap_or_else(|_| new_path.to_path_buf());
        let mut d = self.data.lock().unwrap();
        if !d.docs.contains_key(id) {
            return Err(crate::i18n::tx("Document not found", "Dokument nicht gefunden"));
        }
        if let Some(other) = Self::find_by_path(&d, &new_path) {
            if other != id {
                d.docs.remove(&other);
            }
        }
        let e = d.docs.get_mut(id).unwrap();
        // alte Notizdatei aus Version 3 mitnehmen, falls vorhanden
        let old = Path::new(&e.path).to_path_buf();
        let stem = |p: &Path| p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let old_nj = old.with_file_name(format!("{}.notes.json", stem(&old)));
        let new_nj = new_path.with_file_name(format!("{}.notes.json", stem(&new_path)));
        if old_nj.exists() && !new_nj.exists() {
            let _ = std::fs::copy(&old_nj, &new_nj);
        }
        e.path = new_path.to_string_lossy().to_string();
        self.save(&d);
        Ok(())
    }

    pub fn add_folder(&self, folder: &Path) {
        let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
        let mut d = self.data.lock().unwrap();
        let n = norm_path(&folder);
        if !d.folders.iter().any(|f| norm_path(Path::new(f)) == n) {
            d.folders.push(folder.to_string_lossy().to_string());
            self.save(&d);
        }
    }

    pub fn remove_folder(&self, folder: &str) {
        let mut d = self.data.lock().unwrap();
        let n = norm_path(Path::new(folder));
        d.folders.retain(|f| norm_path(Path::new(f)) != n);
        self.save(&d);
    }

    fn scan(folder: &Path) -> Vec<PathBuf> {
        let mut out = vec![];
        let walker = walkdir::WalkDir::new(folder).sort_by_file_name().into_iter().filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !(e.file_type().is_dir() && (name.starts_with('.') || name.starts_with('$')))
        });
        for e in walker.flatten() {
            if e.file_type().is_file()
                && e.path().extension().map(|x| x.eq_ignore_ascii_case("pdf")).unwrap_or(false)
            {
                out.push(e.path().to_path_buf());
                if out.len() >= MAX_SCAN_FILES {
                    break;
                }
            }
        }
        out
    }

    fn public(id: &str, e: &Entry) -> Value {
        let p = Path::new(&e.path);
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        json!({
            "id": id, "path": e.path, "name": name,
            "folder": p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
            "title": e.title.clone().filter(|t| !t.is_empty()).unwrap_or(stem),
            "pages": e.pages, "lastOpened": e.last_opened, "lastPage": e.last_page,
            "notes": e.notes, "exists": p.is_file(),
        })
    }

    pub fn listing(&self) -> Value {
        let mut d = self.data.lock().unwrap();
        let mut folders_out = vec![];
        let mut changed = false;
        let folders = d.folders.clone();
        // Pfad → ID einmal aufbauen statt für jede gefundene Datei alle Einträge zu durchsuchen
        let mut index: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if !folders.is_empty() {
            for (id, e) in &d.docs {
                index.entry(norm_path(Path::new(&e.path))).or_insert_with(|| id.clone());
            }
        }
        for folder in &folders {
            let fp = Path::new(folder);
            let mut docs = vec![];
            if fp.is_dir() {
                for pdf in Self::scan(fp) {
                    let key = norm_path(&pdf);
                    let id = match index.get(&key) {
                        Some(id) => id.clone(),
                        None => {
                            changed = true;
                            let id = Self::new_entry(&mut d, &pdf);
                            index.insert(key, id.clone());
                            id
                        }
                    };
                    if d.hidden.contains(&id) {
                        continue;
                    }
                    let mut item = Self::public(&id, &d.docs[&id]);
                    let parent = pdf.parent().unwrap_or(fp);
                    let rel = parent.strip_prefix(fp).map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
                    item["subfolder"] = json!(rel);
                    docs.push(item);
                }
            }
            folders_out.push(json!({
                "path": folder,
                "name": fp.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| folder.clone()),
                "exists": fp.is_dir(), "docs": docs,
            }));
        }
        if changed {
            self.save(&d);
        }
        let mut recent: Vec<(String, Value)> = d
            .docs
            .iter()
            .filter(|(_, e)| e.last_opened.is_some())
            .map(|(id, e)| (e.last_opened.clone().unwrap_or_default(), Self::public(id, e)))
            .collect();
        recent.sort_by(|a, b| b.0.cmp(&a.0));
        let recent: Vec<Value> = recent.into_iter().take(RECENT_LIMIT).map(|(_, v)| v).collect();
        json!({ "recent": recent, "folders": folders_out })
    }
}
