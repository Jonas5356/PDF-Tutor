//! Schnittstelle für die Oberfläche: ein Befehl `api(path, body)` mit denselben Pfaden
//! wie in Version 3, damit die Oberfläche weitgehend gleich bleiben konnte.

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
            .unwrap_or_else(|e| Err(bad(format!("Interner Fehler: {e}"))))
    };
    res.map_err(|e| match e {
        ApiError::NotFound(m) => json!({"error": m, "notFound": true}),
        ApiError::Bad(m) => json!({"error": m}),
    })
}

fn pick_pdf(app: &AppHandle) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_title("Dokument öffnen")
        .add_filter("PDF-Dateien", &["pdf"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
}

fn pick_folder(app: &AppHandle) -> Option<PathBuf> {
    app.dialog().file().set_title("Bibliotheksordner wählen").blocking_pick_folder().and_then(|f| f.into_path().ok())
}

fn dispatch(app: &AppHandle, window: &WebviewWindow, ctx: &Arc<Ctx>, path: &str, body: &Value) -> Result<Value, ApiError> {
    // --- Dokumentbezogen: /api/doc/<id>/<aktion>
    if let Some(rest) = path.strip_prefix("/api/doc/") {
        let (id, action) = rest.split_once('/').ok_or_else(|| bad("ungültige Adresse"))?;
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
                }))
            }
            "status" => Ok(doc.status(ctx)),
            "notes" => Ok(doc.notes_json(ctx)?),
            "notes/create" => {
                let page = body["page"].as_u64().ok_or_else(|| bad("Seite fehlt"))? as u32;
                let rects: Vec<[f32; 4]> = body["pdfRects"]
                    .as_array()
                    .ok_or_else(|| bad("Markierung fehlt"))?
                    .iter()
                    .filter_map(|r| {
                        let a = r.as_array()?;
                        if a.len() != 4 {
                            return None;
                        }
                        Some([a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32, a[3].as_f64()? as f32])
                    })
                    .collect();
                Ok(doc.create_note(ctx, page, rects, s(body, "text"), s(body, "question"))?)
            }
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
                body["retry"].as_bool().unwrap_or(false),
                body["saved"].as_bool().unwrap_or(false),
            )?),
            _ => Err(ApiError::NotFound("unbekannte Aktion".into())),
        };
    }

    match path {
        "/api/app" => Ok(json!({
            "version": VERSION, "shell": "tauri",
            "claudeFound": claude::find_claude().is_some(),
            "dataDir": ctx.paths.data.to_string_lossy(),
            "updaterConfigured": updater_configured(app),
        })),
        "/api/library" => Ok(ctx.lib.listing()),
        "/api/library/open" => {
            let p = match body["path"].as_str() {
                Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
                _ => pick_pdf(app),
            };
            let Some(p) = p else { return Ok(json!({"cancelled": true})) };
            if !p.is_file() {
                return Err(bad(format!("Datei nicht gefunden: {}", p.display())));
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
            st.save(&ctx.paths.settings).map_err(|e| bad(e.to_string()))?;
            Ok(json!({"ok": true}))
        }
        "/api/open-path" => {
            let target: PathBuf = match s(body, "what") {
                "data" => ctx.paths.data.clone(),
                "log" => ctx.paths.log.clone(),
                "folder" => {
                    let p = PathBuf::from(s(body, "path"));
                    if !p.is_dir() {
                        return Err(bad("kein Ordner"));
                    }
                    p
                }
                _ => return Err(bad("unbekannt")),
            };
            if !target.exists() {
                return Err(bad("nicht vorhanden"));
            }
            app.opener().open_path(target.to_string_lossy(), None::<&str>).map_err(|e| bad(e.to_string()))?;
            Ok(json!({"ok": true}))
        }
        "/api/open-url" => {
            let url = s(body, "url");
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(bad("nur Webadressen"));
            }
            app.opener().open_url(url, None::<&str>).map_err(|e| bad(e.to_string()))?;
            Ok(json!({"ok": true}))
        }
        "/api/window/open" => {
            windows::open(app, s(body, "url")).map_err(bad)?;
            Ok(json!({"ok": true}))
        }
        "/api/window/title" => {
            let _ = window.set_title(s(body, "title"));
            Ok(json!({"ok": true}))
        }
        _ => Err(ApiError::NotFound(format!("unbekannt: {path}"))),
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
    let upd = updater.check().await.map_err(|e| bad(format!("Suche nach Updates fehlgeschlagen: {e}")))?;
    match (path, upd) {
        ("/api/update/check", Some(u)) => Ok(json!({"configured": true, "available": true, "version": u.version, "notes": u.body})),
        ("/api/update/check", None) => Ok(json!({"configured": true, "available": false})),
        ("/api/update/install", Some(u)) => {
            u.download_and_install(|_, _| {}, || {}).await.map_err(|e| bad(format!("Update fehlgeschlagen: {e}")))?;
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
            let e = ctx.lib.get(id).ok_or((404, "unbekannt".to_string()))?;
            let data = std::fs::read(&e.path).map_err(|e| (404, e.to_string()))?;
            Ok((data, "application/pdf"))
        }
        ["thumb", id] => thumbnail(ctx, id).map(|d| (d, "image/jpeg")),
        _ => Err((404, "nicht gefunden".into())),
    }
}

fn thumbnail(ctx: &Arc<Ctx>, id: &str) -> Result<Vec<u8>, (u16, String)> {
    let e = ctx.lib.get(id).ok_or((404, "unbekannt".to_string()))?;
    let pdf = Path::new(&e.path);
    let meta = std::fs::metadata(pdf).map_err(|_| (404, "Datei fehlt".to_string()))?;
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
        return Err((404, "Datei liegt nur in der Cloud".into()));
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
