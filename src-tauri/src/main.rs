//! PDF-Tutor – PDF-Reader zum Durcharbeiten von Fachliteratur mit Fragen an Claude.
//! Lizenz: MIT oder Apache-2.0

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod annots;
mod api;
mod cache;
mod claude;
mod doc;
mod i18n;
mod library;
mod pdfium_host;
mod settings;
mod state;
mod util;
mod windows;

use state::Ctx;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::http::Response;
use tauri::Manager;

/// PDF-Datei aus den Startargumenten; relative Pfade gelten relativ zu `cwd`
/// (beim zweiten Programmstart ist das das Arbeitsverzeichnis des neuen Aufrufs).
fn pdf_arg(args: &[String], cwd: Option<&Path>) -> Option<PathBuf> {
    args.iter()
        .skip(1)
        .filter(|a| a.to_lowercase().ends_with(".pdf"))
        .map(|a| match cwd {
            Some(c) if Path::new(a).is_relative() => c.join(a),
            _ => PathBuf::from(a),
        })
        .find(|p| p.is_file())
}

fn main() {
    let paths = settings::Paths::new();
    util::init_log(paths.log.clone());
    logf!("PDF-Tutor {} startet, Daten in {}", api::VERSION, paths.data.display());
    let ctx = Arc::new(Ctx::new(paths));
    {
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));
        let mut st = ctx.settings.write().unwrap();
        settings::resolve_language(&mut st, &ctx.paths.settings, exe_dir.as_deref());
        logf!("Sprache: {}", i18n::lang());
    }
    let ctx_proto = ctx.clone();
    let ctx_single = ctx.clone();
    let ctx_setup = ctx.clone();

    let result = tauri::Builder::default()
        // Zweiter Programmstart: vorhandenes Programm aktivieren bzw. das PDF darin öffnen
        .plugin(tauri_plugin_single_instance::init(move |app, argv, cwd| match pdf_arg(&argv, Some(Path::new(&cwd))) {
            Some(p) => {
                let id = ctx_single.lib.ensure(&p);
                let _ = windows::open(app, &format!("reader.html?id={id}"));
            }
            None => windows::focus_any(app),
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(ctx)
        .register_asynchronous_uri_scheme_protocol("pdftutor", move |_ctx, request, responder| {
            let ctx = ctx_proto.clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let resp = match api::protocol(&ctx, &path) {
                    Ok((data, mime)) => Response::builder()
                        .header("Content-Type", mime)
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Cache-Control", "no-store")
                        .body(data),
                    Err((code, msg)) => Response::builder()
                        .status(code)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(msg.into_bytes()),
                };
                responder.respond(resp.unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![api::api])
        .setup(move |app| {
            let mut dirs = vec![];
            if let Ok(r) = app.path().resource_dir() {
                dirs.push(r.clone());
                dirs.push(r.join("resources"));
            }
            if let Some(p) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
                dirs.push(p);
            }
            dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"));
            pdfium_host::init(&dirs);
            let tpl_name = if i18n::lang() == "de" { "hinweise.vorlage.txt" } else { "hints.template.txt" };
            let tpl = dirs.iter().map(|d| d.join(tpl_name)).find(|p| p.exists());
            settings::ensure_hinweise(&ctx_setup.paths, tpl);
            let args: Vec<String> = std::env::args().collect();
            let url = match pdf_arg(&args, None) {
                Some(p) => format!("reader.html?id={}", ctx_setup.lib.ensure(&p)),
                None => "index.html".into(),
            };
            windows::open(app.handle(), &url)?;
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        logf!("Programmfehler: {e}");
    }
}
