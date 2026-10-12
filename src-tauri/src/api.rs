//! Schnittstelle für die Oberfläche: ein Befehl `api(path, body)` mit denselben Pfaden
//! wie in Version 3, damit die Oberfläche weitgehend gleich bleiben konnte.

use crate::i18n::tx;
use crate::annots::Kind;
use crate::claude;
use crate::state::{ApiError, Ctx};
use crate::util::write_atomic;
use crate::windows;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn bad(s: impl Into<String>) -> ApiError {
    ApiError::Bad(s.into())
}

const DEFAULT_COLOR: [f32; 3] = [1.0, 0.85, 0.0];

/// Farbe als [r, g, b] mit Werten von 0 bis 1.
fn color_of(body: &Value) -> Option<[f32; 3]> {
    let a = body["color"].as_array().filter(|a| a.len() == 3)?;
    let v: Vec<f32> = a.iter().filter_map(|x| x.as_f64()).map(|x| x.clamp(0.0, 1.0) as f32).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn s<'a>(body: &'a Value, key: &str) -> &'a str {
    body[key].as_str().unwrap_or("")
}

#[tauri::command]
pub async fn api(app: AppHandle, window: WebviewWindow, path: String, body: Option<Value>) -> Result<Value, Value> {
    let ctx = app.state::<Arc<Ctx>>().inner().clone();
    let body = body.unwrap_or(Value::Null);
    let res = if path.starts_with("/api/update/") {
        update(&app, &path).await
    } else {
        let app2 = app.clone();
        tauri::async_runtime::spawn_blocking(move || dispatch(&app2, &window, &ctx, &path, &body))
            .await
            .unwrap_or_else(|e| Err(bad(tx(&format!("Internal error: {e}"), &format!("Interner Fehler: {e}")))))
    };
    res.map_err(|e| match e {
        ApiError::NotFound(m) => json!({"error": m, "notFound": true}),
        ApiError::Bad(m) => json!({"error": m}),
    })
}

fn pick_pdf(app: &AppHandle) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_title(tx("Open document", "Dokument öffnen"))
        .add_filter(tx("PDF files", "PDF-Dateien"), &["pdf"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
}

/// Dateiname ohne Zeichen, die Windows nicht erlaubt.
fn clean_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let n = n.trim().trim_end_matches('.').to_string();
    if n.is_empty() { "PDF-Tutor".into() } else { n }
}

/// „Speichern unter“-Dialog. Für automatische Tests lässt er sich mit PDFTUTOR_TEST_SAVE_DIR umgehen.
pub fn pick_save(app: &AppHandle, label: &str, ext: &str, name: &str, dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PDFTUTOR_TEST_SAVE_DIR") {
        return Some(PathBuf::from(d).join(clean_name(name)));
    }
    let mut dlg = app.dialog().file().set_title(tx("Save as", "Speichern unter")).add_filter(label, &[ext]).set_file_name(clean_name(name));
    if let Some(dir) = dir.filter(|d| d.is_dir()) {
        dlg = dlg.set_directory(dir);
    }
    dlg.blocking_save_file().and_then(|f| f.into_path().ok())
}

fn pick_folder(app: &AppHandle) -> Option<PathBuf> {
    app.dialog().file().set_title(tx("Choose library folder", "Bibliotheksordner wählen")).blocking_pick_folder().and_then(|f| f.into_path().ok())
}

fn dispatch(app: &AppHandle, window: &WebviewWindow, ctx: &Arc<Ctx>, path: &str, body: &Value) -> Result<Value, ApiError> {
    // --- Dokumentbezogen: /api/doc/<id>/<aktion>
    if let Some(rest) = path.strip_prefix("/api/doc/") {
        let (id, action) = rest.split_once('/').ok_or_else(|| bad(tx("invalid address", "ungültige Adresse")))?;
        let doc = ctx.get_doc(id)?;
        return match action {
            "info" => {
                ctx.lib.update(id, |e| e.last_opened = Some(crate::util::now_iso()));
                Ok(json!({
                    "id": id, "title": doc.cache.title,
                    "file": doc.path.file_name().unwrap_or_default().to_string_lossy(),
                    "path": doc.path.to_string_lossy(), "version": VERSION,
                    "models": claude::models(), "defaultModel": claude::DEFAULT_MODEL,
                    "claudeFound": claude::find_claude().is_some(), "doc": doc.status(ctx), "shell": "tauri",
                    "pdfUrl": format!("doc/{id}/pdf"),
                    "labels": doc.cache.labels,
                }))
            }
            "status" => Ok(doc.status(ctx)),
            "notes" => Ok(doc.notes_json(ctx)?),
            "notes/create" => {
                let page = body["page"].as_u64().ok_or_else(|| bad(tx("page missing", "Seite fehlt")))? as u32;
                let rects: Vec<[f32; 4]> = body["pdfRects"]
                    .as_array()
                    .ok_or_else(|| bad(tx("highlight missing", "Markierung fehlt")))?
                    .iter()
                    .filter_map(|r| {
                        let a = r.as_array()?;
                        if a.len() != 4 {
                            return None;
                        }
                        Some([a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32, a[3].as_f64()? as f32])
                    })
                    .collect();
                let kind = match body["kind"].as_str() {
                    None => Kind::Highlight,
                    Some(k) => Kind::from_name(k).filter(|k| *k != Kind::Note).ok_or_else(|| bad(tx("Unknown kind", "Unbekannte Art")))?,
                };
                let color = color_of(body).unwrap_or(DEFAULT_COLOR);
                Ok(doc.create_note(ctx, page, kind, rects, color, s(body, "text"), s(body, "comment"), s(body, "question"))?)
            }
            "notes/sticky" => {
                let page = body["page"].as_u64().ok_or_else(|| bad(tx("page missing", "Seite fehlt")))? as u32;
                let p = body["point"].as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(tx("position missing", "Position fehlt")))?;
                let at = (p[0].as_f64().unwrap_or(0.0) as f32, p[1].as_f64().unwrap_or(0.0) as f32);
                Ok(doc.create_sticky(ctx, page, at, color_of(body).unwrap_or(DEFAULT_COLOR), s(body, "text"))?)
            }
            "notes/style" => {
                let kind = match body["kind"].as_str() {
                    None => None,
                    Some(k) => Some(Kind::from_name(k).ok_or_else(|| bad(tx("Unknown kind", "Unbekannte Art")))?),
                };
                Ok(doc.restyle(ctx, s(body, "id"), kind, color_of(body))?)
            }
            "notes/comment" => Ok(doc.comment(ctx, s(body, "id"), s(body, "text"))?),
            "notes/edit" => Ok(doc.edit_message(ctx, s(body, "id"), s(body, "msg"), s(body, "text"))?),
            "undo" => Ok(doc.undo(ctx)?),
            "notes/delete" => {
                doc.delete_note(ctx, s(body, "id"))?;
                Ok(json!({"ok": true}))
            }
            "notes/update" => {
                let nid = s(body, "id").to_string();
                if let Some(c) = body["collapsed"].as_bool() {
                    ctx.lib.update(id, |e| {
                        e.ui.collapsed.retain(|x| x != &nid);
                        if c {
                            e.ui.collapsed.push(nid.clone());
                        }
                    });
                }
                Ok(json!({"ok": true}))
            }
            "state" => {
                ctx.lib.update(id, |e| {
                    if let Some(p) = body["lastPage"].as_u64() {
                        e.last_page = Some(p as u32);
                    }
                    if let Some(x) = body["scale"].as_f64() {
                        e.ui.scale = Some(x);
                    }
                    if let Some(m) = body["model"].as_str() {
                        e.ui.model = Some(m.to_string());
                    }
                    if let Some(x) = body["effort"].as_str() {
                        e.ui.effort = crate::claude::valid_effort(x);
                    }
                    if let Some(m) = body["mode"].as_str() {
                        if ["auto", "volltext", "suche"].contains(&m) {
                            e.ui.mode = Some(m.to_string());
                        }
                    }
                });
                if let Some(p) = body["lastPage"].as_u64() {
                    doc.cache.focus.store(p as u32, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(doc.status(ctx))
            }
            "ask" => Ok(doc.ask(
                ctx,
                s(body, "id"),
                body["question"].as_str().map(|x| x.to_string()),
                s(body, "model"),
                s(body, "effort"),
                body["retry"].as_bool().unwrap_or(false),
                body["saved"].as_bool().unwrap_or(false),
            )?),
            _ => Err(ApiError::NotFound("unknown action".into())),
        };
    }

    match path {
        "/api/app" => Ok(json!({
            "version": VERSION, "shell": "tauri",
            "claudeFound": claude::find_claude().is_some(),
            "dataDir": ctx.paths.data.to_string_lossy(),
            "updaterConfigured": updater_configured(app),
            "lang": crate::i18n::lang(),
            "autoUpdate": ctx.settings.read().unwrap().auto_update,
        })),
        "/api/library" => Ok(ctx.lib.listing()),
        "/api/library/open" => {
            let p = match body["path"].as_str() {
                Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
                _ => pick_pdf(app),
            };
            let Some(p) = p else { return Ok(json!({"cancelled": true})) };
            if !p.is_file() {
                return Err(bad(tx(&format!("File not found: {}", p.display()), &format!("Datei nicht gefunden: {}", p.display()))));
            }
            Ok(json!({"id": ctx.lib.ensure(&p)}))
        }
        "/api/library/add-folder" => {
            let Some(p) = pick_folder(app) else { return Ok(json!({"cancelled": true})) };
            ctx.lib.add_folder(&p);
            Ok(json!({"ok": true}))
        }
        "/api/library/remove-folder" => {
            ctx.lib.remove_folder(s(body, "path"));
            Ok(json!({"ok": true}))
        }
        "/api/library/remove" => {
            ctx.lib.remove_recent(s(body, "id"));
            Ok(json!({"ok": true}))
        }
        "/api/library/hide" => {
            ctx.lib.hide(s(body, "id"));
            Ok(json!({"ok": true}))
        }
        "/api/library/relink" => {
            let Some(p) = pick_pdf(app) else { return Ok(json!({"cancelled": true})) };
            ctx.lib.relink(s(body, "id"), &p)?;
            ctx.docs.lock().unwrap().remove(s(body, "id"));
            Ok(json!({"ok": true}))
        }
        "/api/settings" if body.is_null() => {
            let st = ctx.settings.read().unwrap().clone();
            Ok(json!({
                "hinweise": crate::settings::read_hinweise(&ctx.paths),
                "volltextGrenze": st.volltext_grenze_tokens,
                "autor": st.autor, "autorStandard": st.author(),
                "sicherheitskopie": st.sicherheitskopie,
                "sprache": crate::i18n::lang(),
                "autoUpdate": st.auto_update,
                "dataDir": ctx.paths.data.to_string_lossy(), "logFile": ctx.paths.log.to_string_lossy(),
            }))
        }
        "/api/settings" => {
            if let Some(h) = body["hinweise"].as_str() {
                write_atomic(&ctx.paths.hinweise, format!("{}\n", h.trim()).as_bytes()).map_err(|e| bad(e.to_string()))?;
            }
            let mut st = ctx.settings.write().unwrap();
            if let Some(v) = body["volltextGrenze"].as_u64() {
                st.volltext_grenze_tokens = v;
            }
            if let Some(a) = body["autor"].as_str() {
                st.autor = a.trim().to_string();
            }
            if let Some(b) = body["sicherheitskopie"].as_bool() {
                st.sicherheitskopie = b;
            }
            if let Some(b) = body["autoUpdate"].as_bool() {
                st.auto_update = b;
            }
            let mut lang_changed = false;
            if let Some(l) = body["sprache"].as_str().and_then(crate::i18n::normalize) {
                lang_changed = l != crate::i18n::lang();
                crate::i18n::set(&l);
                st.sprache = Some(l);
            }
            st.save(&ctx.paths.settings).map_err(|e| bad(e.to_string()))?;
            if lang_changed {
                // alle Fenster in der neuen Sprache neu laden
                for (_, w) in app.webview_windows() {
                    let _ = w.eval("setTimeout(() => location.reload(), 50)");
                }
            }
            Ok(json!({"ok": true, "reload": lang_changed}))
        }
        "/api/open-path" => {
            let target: PathBuf = match s(body, "what") {
                "data" => ctx.paths.data.clone(),
                "log" => ctx.paths.log.clone(),
                "folder" => {
                    let p = PathBuf::from(s(body, "path"));
                    if !p.is_dir() {
                        return Err(bad(tx("not a folder", "kein Ordner")));
                    }
                    p
                }
                // nur Dateien, die der PDF-Tutor selbst erzeugt (Lernzettel, neue PDFs)
                "file" => {
                    let p = PathBuf::from(s(body, "path"));
                    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if !p.is_file() || !["html", "md", "pdf"].contains(&ext.as_str()) {
                        return Err(bad(tx("This file cannot be opened.", "Diese Datei kann nicht geöffnet werden.")));
                    }
                    p
                }
                _ => return Err(bad(tx("unknown", "unbekannt"))),
            };
            if !target.exists() {
                return Err(bad(tx("does not exist", "nicht vorhanden")));
            }
            app.opener().open_path(target.to_string_lossy(), None::<&str>).map_err(|e| bad(e.to_string()))?;
            Ok(json!({"ok": true}))
        }
        "/api/open-url" => {
            let url = s(body, "url");
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(bad(tx("Only web addresses can be opened", "Nur Webadressen können geöffnet werden")));
            }
            app.opener().open_url(url, None::<&str>).map_err(|e| bad(e.to_string()))?;
            Ok(json!({"ok": true}))
        }
        // --- Seiten organisieren
        "/api/organize/add" => {
            let paths: Vec<PathBuf> = match body["paths"].as_array() {
                Some(a) => a.iter().filter_map(|x| x.as_str()).map(PathBuf::from).collect(),
                None => app
                    .dialog()
                    .file()
                    .set_title(tx("Add PDFs", "PDFs hinzufügen"))
                    .add_filter(tx("PDF files", "PDF-Dateien"), &["pdf"])
                    .blocking_pick_files()
                    .map(|v| v.into_iter().filter_map(|f| f.into_path().ok()).collect())
                    .unwrap_or_default(),
            };
            let docs: Vec<Value> = paths
                .iter()
                .filter(|p| p.is_file())
                .map(|p| {
                    json!({"id": ctx.lib.ensure(p), "name": p.file_name().unwrap_or_default().to_string_lossy()})
                })
                .collect();
            Ok(json!({"docs": docs}))
        }
        "/api/organize/save" => {
            let ids: Vec<String> = body["sources"].as_array().ok_or_else(|| bad(tx("Sources missing", "Quellen fehlen")))?.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            let mut src_paths = vec![];
            for id in &ids {
                let e = ctx.lib.get(id).ok_or_else(|| bad(tx("A source document is no longer in the library.", "Ein Quelldokument ist nicht mehr in der Bibliothek.")))?;
                src_paths.push(PathBuf::from(e.path));
            }
            let spec: Vec<crate::pdfium_host::PageSpec> = body["pages"]
                .as_array()
                .ok_or_else(|| bad(tx("Pages missing", "Seiten fehlen")))?
                .iter()
                .map(|p| crate::pdfium_host::PageSpec {
                    src: p["src"].as_u64().unwrap_or(0) as usize,
                    page: p["page"].as_u64().unwrap_or(0) as u32,
                    rotate: p["rotate"].as_i64().unwrap_or(0) as i32,
                })
                .collect();
            if spec.iter().any(|p| p.src >= src_paths.len()) {
                return Err(bad(tx("invalid page", "ungültige Seitenangabe")));
            }
            let dir = src_paths.first().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let Some(mut path) = pick_save(app, &tx("PDF document", "PDF-Dokument"), "pdf", s(body, "name"), dir) else {
                return Ok(json!({"cancelled": true}));
            };
            if path.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some("pdf".into()) {
                path.set_extension("pdf");
            }
            let target = crate::util::norm_path(&std::path::absolute(&path).unwrap_or(path.clone()));
            if src_paths.iter().any(|p| crate::util::norm_path(&std::path::absolute(p).unwrap_or(p.clone())) == target) {
                return Err(bad(tx("Please choose a new file name: the original file is not overwritten.", "Bitte einen neuen Dateinamen wählen: Die Originaldatei wird nicht überschrieben.")));
            }
            let sources: Vec<Vec<u8>> = src_paths
                .iter()
                .map(|p| std::fs::read(p).map_err(|e| bad(tx(&format!("{} cannot be read: {e}", p.display()), &format!("{} kann nicht gelesen werden: {e}", p.display())))))
                .collect::<Result<_, _>>()?;
            let out = crate::pdfium_host::assemble(&sources, &spec).map_err(bad)?;
            write_atomic(&path, &out).map_err(|e| bad(tx(&format!("Saving failed: {e}"), &format!("Speichern fehlgeschlagen: {e}"))))?;
            let id = ctx.lib.ensure(&path);
            Ok(json!({"path": path.to_string_lossy(), "id": id, "pages": spec.len()}))
        }
        "/api/save-file" => {
            let (label, ext) = match s(body, "kind") {
                "html" => (tx("HTML file", "HTML-Datei"), "html"),
                "md" => ("Markdown".to_string(), "md"),
                _ => return Err(bad(tx("unknown format", "unbekanntes Format"))),
            };
            let dir = ctx.lib.get(s(body, "docId")).and_then(|e| PathBuf::from(e.path).parent().map(|p| p.to_path_buf()));
            let Some(mut path) = pick_save(app, &label, ext, s(body, "name"), dir) else {
                return Ok(json!({"cancelled": true}));
            };
            if path.extension().map(|e| e.to_string_lossy().to_lowercase()) != Some(ext.to_string()) {
                path.set_extension(ext);
            }
            write_atomic(&path, s(body, "content").as_bytes()).map_err(|e| bad(tx(&format!("Saving failed: {e}"), &format!("Speichern fehlgeschlagen: {e}"))))?;
            Ok(json!({"path": path.to_string_lossy()}))
        }
        "/api/window/open" => {
            windows::open(app, s(body, "url")).map_err(bad)?;
            Ok(json!({"ok": true}))
        }
        "/api/window/title" => {
            let _ = window.set_title(s(body, "title"));
            Ok(json!({"ok": true}))
        }
        _ => Err(ApiError::NotFound(format!("unknown: {path}"))),
    }
}

// ---------------------------------------------------------------------------
// Updates über GitHub Releases

fn updater_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .map(|u| !u["pubkey"].as_str().unwrap_or("").is_empty() && u["endpoints"].as_array().map(|a| !a.is_empty()).unwrap_or(false))
        .unwrap_or(false)
}

async fn update(app: &AppHandle, path: &str) -> Result<Value, ApiError> {
    use tauri_plugin_updater::UpdaterExt;
    if !updater_configured(app) {
        return Ok(json!({"configured": false}));
    }
    let updater = app.updater().map_err(|e| bad(e.to_string()))?;
    let upd = updater.check().await.map_err(|e| bad(tx(&format!("Checking for updates failed: {e}"), &format!("Suche nach Updates fehlgeschlagen: {e}"))))?;
    match (path, upd) {
        ("/api/update/check", Some(u)) => Ok(json!({"configured": true, "available": true, "version": u.version, "notes": u.body})),
        ("/api/update/check", None) => Ok(json!({"configured": true, "available": false})),
        ("/api/update/install", Some(u)) => {
            u.download_and_install(|_, _| {}, || {}).await.map_err(|e| bad(tx(&format!("Update failed: {e}"), &format!("Update fehlgeschlagen: {e}"))))?;
            app.restart();
        }
        _ => Ok(json!({"configured": true, "available": false})),
    }
}

/// Inhalt für das eigene Protokoll pdftutor://…: das PDF und Vorschaubilder.
pub fn protocol(ctx: &Arc<Ctx>, path: &str) -> Result<(Vec<u8>, &'static str), (u16, String)> {
    let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy().to_string();
    let path = path.trim_start_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["doc", id, "pdf"] => {
            let e = ctx.lib.get(id).ok_or((404, "unknown".to_string()))?;
            let data = std::fs::read(&e.path).map_err(|e| (404, e.to_string()))?;
            Ok((data, "application/pdf"))
        }
        ["thumb", id] => thumbnail(ctx, id).map(|d| (d, "image/jpeg")),
        _ => Err((404, "not found".into())),
    }
}

fn thumbnail(ctx: &Arc<Ctx>, id: &str) -> Result<Vec<u8>, (u16, String)> {
    let e = ctx.lib.get(id).ok_or((404, "unknown".to_string()))?;
    let pdf = Path::new(&e.path);
    let meta = std::fs::metadata(pdf).map_err(|_| (404, "file missing".to_string()))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out = ctx.paths.thumbs.join(format!("{id}_{}_{mtime}.jpg", meta.len()));
    if let Ok(d) = std::fs::read(&out) {
        return Ok(d);
    }
    if crate::util::is_cloud_only(pdf) {
        return Err((404, "file is cloud-only".into()));
    }
    for old in std::fs::read_dir(&ctx.paths.thumbs).into_iter().flatten().flatten() {
        if old.file_name().to_string_lossy().starts_with(&format!("{id}_")) {
            let _ = std::fs::remove_file(old.path());
        }
    }
    let bytes = std::fs::read(pdf).map_err(|e| (500, e.to_string()))?;
    let (jpg, pages, title) = crate::pdfium_host::thumbnail(&bytes, 360).map_err(|e| (500, e))?;
    let _ = write_atomic(&out, &jpg);
    let title = crate::cache::nice_title(&title, pdf);
    ctx.lib.update(id, |e| {
        e.title = Some(title);
        e.pages = Some(pages);
    });
    Ok(jpg)
}
