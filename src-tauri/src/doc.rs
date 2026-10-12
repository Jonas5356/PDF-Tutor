//! Ein geöffnetes Dokument: Notizen (aus dem PDF), Fragen an Claude, Schreiben ins PDF.

use crate::i18n::tx;
use crate::annots::{self, Editor, Kind, Note, CLAUDE_AUTHOR};
use crate::cache::PageCache;
use crate::claude::{self, HistoryItem};
use crate::pdfium_host;
use crate::settings::read_hinweise;
use crate::state::Ctx;
use crate::util::{is_locked_error, new_id, now_iso, write_atomic};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

pub fn locked_msg() -> String {
    tx(
        "The PDF is currently open in another program and cannot be saved. Please close it there.",
        "Das PDF ist gerade in einem anderen Programm geöffnet und kann nicht gespeichert werden. Bitte dort schließen.",
    )
}

fn saving_failed(e: impl std::fmt::Display) -> String {
    tx(&format!("Saving failed: {e}"), &format!("Speichern fehlgeschlagen: {e}"))
}

fn cannot_read(e: impl std::fmt::Display) -> String {
    tx(&format!("The PDF cannot be read: {e}"), &format!("PDF kann nicht gelesen werden: {e}"))
}

/// Ein Schritt, der sich rückgängig machen lässt: Da jede Änderung ans Dateiende angehängt
/// wird, genügt es, die Datei wieder auf die alte Länge zu kürzen.
struct UndoStep {
    prev_len: usize,
    new_len: usize,
    tail: String,
    label: String,
}

fn tail_sig(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(65536);
    crate::util::sha1_hex(&bytes[start..])
}

const UNDO_MAX: usize = 50;

struct Unsaved {
    note_id: String,
    text: String,
    meta: Value,
    session: Option<String>,
}

pub struct Doc {
    pub id: String,
    pub path: PathBuf,
    pub cache: Arc<PageCache>,
    write: Mutex<()>,
    notes_cache: Mutex<Option<(String, Arc<Vec<Note>>)>>,
    /// Nachrichten, die (noch) nicht im PDF stehen: wartende Antworten, Fehler
    extra: Mutex<HashMap<String, Vec<Value>>>,
    unsaved: Mutex<Vec<Unsaved>>,
    retry_running: Mutex<bool>,
    undo: Mutex<Vec<UndoStep>>,
}

fn file_sig(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("{}-{}", m.len(), t)
        }
        Err(_) => String::new(),
    }
}

fn kind_label(k: Kind) -> String {
    match k {
        Kind::Highlight => tx("create highlight", "Hervorhebung anlegen"),
        Kind::Underline => tx("create underline", "Unterstreichung anlegen"),
        Kind::StrikeOut => tx("create strikeout", "Durchstreichung anlegen"),
        Kind::Note => tx("create sticky note", "Haftnotiz anlegen"),
    }
}

fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

impl Doc {
    pub fn open(ctx: &Arc<Ctx>, id: &str, path: &Path) -> Result<Arc<Doc>, String> {
        let cache = PageCache::open(path, &ctx.paths.work)?;
        let doc = Arc::new(Doc {
            id: id.to_string(),
            path: path.to_path_buf(),
            cache,
            write: Mutex::new(()),
            notes_cache: Mutex::new(None),
            extra: Mutex::new(HashMap::new()),
            unsaved: Mutex::new(vec![]),
            retry_running: Mutex::new(false),
            undo: Mutex::new(vec![]),
        });
        let (title, n) = (doc.cache.title.clone(), doc.cache.n);
        ctx.lib.update(id, |e| {
            e.title = Some(title);
            e.pages = Some(n);
        });
        if let Some(e) = ctx.lib.get(id) {
            if e.last_page.is_some() {
                doc.cache.focus.store(e.last_page.unwrap().max(1), std::sync::atomic::Ordering::Relaxed);
            }
            if !e.notes_imported {
                if let Err(err) = doc.import_v3_notes(ctx) {
                    crate::logf!("Übernahme der alten Notizen von {}: {err}", path.display());
                }
            }
        }
        Ok(doc)
    }

    // -----------------------------------------------------------------------
    // Lesen

    pub fn notes(&self) -> Result<Arc<Vec<Note>>, String> {
        let sig = file_sig(&self.path);
        if let Some((s, n)) = self.notes_cache.lock().unwrap().as_ref() {
            if *s == sig {
                return Ok(n.clone());
            }
        }
        let bytes = std::fs::read(&self.path).map_err(cannot_read)?;
        let doc = annots::load(&bytes)?;
        let mut notes = annots::read_notes(&doc);
        // markierten Text für Markierungen aus anderen Programmen ermitteln
        let missing: Vec<usize> = notes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.text.is_empty() && n.subtype != "Text" && n.subtype != "FreeText")
            .map(|(i, _)| i)
            .collect();
        if !missing.is_empty() {
            let items: Vec<(u32, Vec<[f32; 4]>)> = missing.iter().map(|&i| (notes[i].page - 1, notes[i].rects.clone())).collect();
            if let Ok(texts) = pdfium_host::texts_in_rects(&bytes, &items) {
                for (k, &i) in missing.iter().enumerate() {
                    notes[i].text = texts[k].clone();
                }
            }
        }
        let arc = Arc::new(notes);
        *self.notes_cache.lock().unwrap() = Some((sig, arc.clone()));
        Ok(arc)
    }

    fn find(&self, id: &str) -> Result<Note, String> {
        self.notes()?.iter().find(|n| n.id == id).cloned().ok_or_else(|| tx("Note not found", "Notiz nicht gefunden"))
    }

    fn note_json(&self, ctx: &Ctx, n: &Note) -> Value {
        let mut v = n.to_json();
        let collapsed = ctx.lib.get(&self.id).map(|e| e.ui.collapsed.contains(&n.id)).unwrap_or(false);
        v["collapsed"] = json!(collapsed);
        if let Some(extra) = self.extra.lock().unwrap().get(&n.id) {
            if let Some(arr) = v["thread"].as_array_mut() {
                arr.extend(extra.iter().cloned());
            }
        }
        for u in self.unsaved.lock().unwrap().iter().filter(|u| u.note_id == n.id) {
            let mut m = json!({"role": "assistant", "author": CLAUDE_AUTHOR, "text": u.text, "status": "ok", "unsaved": true});
            for (k, out) in [("Model", "model"), ("Mode", "mode"), ("Seconds", "seconds")] {
                if !u.meta[k].is_null() {
                    m[out] = u.meta[k].clone();
                }
            }
            v["thread"].as_array_mut().map(|a| a.push(m));
        }
        v
    }

    pub fn notes_json(&self, ctx: &Ctx) -> Result<Value, String> {
        let notes = self.notes()?;
        let e = ctx.lib.get(&self.id);
        let state = json!({
            "lastPage": e.as_ref().and_then(|e| e.last_page),
            "scale": e.as_ref().and_then(|e| e.ui.scale),
            "model": e.as_ref().and_then(|e| e.ui.model.clone()),
            "mode": e.as_ref().and_then(|e| e.ui.mode.clone()),
        });
        Ok(json!({
            "notes": notes.iter().map(|n| self.note_json(ctx, n)).collect::<Vec<_>>(),
            "state": state,
            "version": file_sig(&self.path),
            "unsavedCount": self.unsaved.lock().unwrap().len(),
            "undo": self.undo.lock().unwrap().last().map(|u| u.label.clone()),
        }))
    }

    pub fn chosen_mode(&self, ctx: &Ctx) -> String {
        ctx.lib
            .get(&self.id)
            .and_then(|e| e.ui.mode)
            .filter(|m| ["auto", "volltext", "suche"].contains(&m.as_str()))
            .unwrap_or_else(|| "auto".into())
    }

    pub fn effective_mode(&self, ctx: &Ctx) -> String {
        let m = self.chosen_mode(ctx);
        if m != "auto" {
            return m;
        }
        let limit = ctx.settings.read().unwrap().volltext_grenze_tokens;
        match *self.cache.tokens.lock().unwrap() {
            Some(t) if t <= limit => "volltext".into(),
            _ => "suche".into(),
        }
    }

    pub fn status(&self, ctx: &Ctx) -> Value {
        let mut st = self.cache.status();
        st["mode"] = json!(self.chosen_mode(ctx));
        st["effectiveMode"] = if self.cache.text_ready() { json!(self.effective_mode(ctx)) } else { Value::Null };
        st["limit"] = json!(ctx.settings.read().unwrap().volltext_grenze_tokens);
        st
    }

    // -----------------------------------------------------------------------
    // Schreiben

    /// Änderung ins PDF schreiben (inkrementell, atomar ersetzt). Nicht rückgängig zu machen;
    /// frühere Rückgängig-Schritte verfallen dabei.
    pub fn modify<R>(&self, ctx: &Ctx, f: impl FnOnce(&mut Editor) -> Result<R, String>) -> Result<R, String> {
        self.modify_ex(ctx, None, f)
    }

    /// Wie `modify`, aber als Schritt, den Strg+Z wieder zurücknimmt.
    pub fn modify_undoable<R>(&self, ctx: &Ctx, label: &str, f: impl FnOnce(&mut Editor) -> Result<R, String>) -> Result<R, String> {
        self.modify_ex(ctx, Some(label), f)
    }

    fn modify_ex<R>(&self, ctx: &Ctx, label: Option<&str>, f: impl FnOnce(&mut Editor) -> Result<R, String>) -> Result<R, String> {
        let _g = self.write.lock().unwrap();
        let bytes = std::fs::read(&self.path).map_err(cannot_read)?;
        self.backup_once(ctx, &bytes);
        let prev_len = bytes.len();
        let mut ed = Editor::open(bytes)?;
        let r = f(&mut ed)?;
        let out = ed.finish()?;
        if let Err(e) = write_atomic(&self.path, &out) {
            return Err(if is_locked_error(&e) { locked_msg() } else { saving_failed(e) });
        }
        {
            let mut undo = self.undo.lock().unwrap();
            match label {
                Some(l) => {
                    undo.push(UndoStep { prev_len, new_len: out.len(), tail: tail_sig(&out), label: l.to_string() });
                    if undo.len() > UNDO_MAX {
                        undo.remove(0);
                    }
                }
                None => undo.clear(),
            }
        }
        self.after_write(ctx);
        Ok(r)
    }

    fn after_write(&self, ctx: &Ctx) {
        *self.notes_cache.lock().unwrap() = None;
        let count = self.notes().map(|n| n.len() as u32).unwrap_or(0);
        ctx.lib.update(&self.id, |e| e.notes = count);
    }

    /// Letzten Schritt zurücknehmen: Datei auf den Stand davor kürzen.
    pub fn undo(&self, ctx: &Ctx) -> Result<Value, String> {
        let _g = self.write.lock().unwrap();
        let Some(step) = self.undo.lock().unwrap().pop() else {
            return Err(tx("There is nothing to undo.", "Es gibt nichts rückgängig zu machen."));
        };
        let bytes = std::fs::read(&self.path).map_err(cannot_read)?;
        if bytes.len() != step.new_len || tail_sig(&bytes) != step.tail || step.prev_len > bytes.len() {
            self.undo.lock().unwrap().clear();
            return Err(tx("Undo is no longer possible because the PDF has been changed elsewhere in the meantime.", "Rückgängig ist nicht mehr möglich, weil das PDF inzwischen anderweitig verändert wurde."));
        }
        // Der alte Stand muss sich wieder lesen lassen
        annots::load(&bytes[..step.prev_len])?;
        if let Err(e) = write_atomic(&self.path, &bytes[..step.prev_len]) {
            let msg = if is_locked_error(&e) { locked_msg() } else { saving_failed(e) };
            self.undo.lock().unwrap().push(step);
            return Err(msg);
        }
        self.after_write(ctx);
        Ok(json!({"ok": true, "undone": step.label}))
    }

    fn backup_once(&self, ctx: &Ctx, bytes: &[u8]) {
        if !ctx.settings.read().unwrap().sicherheitskopie {
            return;
        }
        if ctx.lib.get(&self.id).map(|e| e.backup_done).unwrap_or(true) {
            return;
        }
        let _ = std::fs::create_dir_all(&ctx.paths.backups);
        let stem = self.path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let target = ctx.paths.backups.join(format!("{stem}_{}.pdf", self.id));
        if !target.exists() {
            if let Err(e) = write_atomic(&target, bytes) {
                crate::logf!("Sicherheitskopie {}: {e}", target.display());
                return;
            }
        }
        ctx.lib.update(&self.id, |e| e.backup_done = true);
    }

    /// Neue Markierung. Mit `question` folgt gleich eine Frage an Claude (dann nicht rückgängig zu machen).
    #[allow(clippy::too_many_arguments)]
    pub fn create_note(
        &self,
        ctx: &Ctx,
        page: u32,
        kind: Kind,
        rects: Vec<[f32; 4]>,
        color: [f32; 3],
        text: &str,
        comment: &str,
        question: &str,
    ) -> Result<Value, String> {
        let nm = format!("pdftutor-{}", new_id());
        let author = ctx.settings.read().unwrap().author();
        let meta = json!({"App": "PDF-Tutor", "Text": text.chars().take(5000).collect::<String>()});
        let contents = if question.trim().is_empty() { comment.trim() } else { question.trim() };
        let write = |ed: &mut Editor| ed.add_markup(page, kind, &rects, color, contents, &author, &nm, &meta);
        if question.trim().is_empty() {
            self.modify_undoable(ctx, &kind_label(kind), write)?;
        } else {
            self.modify(ctx, write)?;
        }
        let n = self.find(&nm)?;
        Ok(self.note_json(ctx, &n))
    }

    /// Neue Haftnotiz an einem Punkt (PDF-Koordinaten).
    pub fn create_sticky(&self, ctx: &Ctx, page: u32, at: (f32, f32), color: [f32; 3], text: &str) -> Result<Value, String> {
        let nm = format!("pdftutor-{}", new_id());
        let author = ctx.settings.read().unwrap().author();
        let meta = json!({"App": "PDF-Tutor"});
        self.modify_undoable(ctx, &tx("create sticky note", "Haftnotiz anlegen"), |ed| ed.add_sticky(page, at, color, text.trim(), &author, &nm, &meta))?;
        let n = self.find(&nm)?;
        Ok(self.note_json(ctx, &n))
    }

    /// Art und/oder Farbe ändern.
    pub fn restyle(&self, ctx: &Ctx, id: &str, kind: Option<Kind>, color: Option<[f32; 3]>) -> Result<Value, String> {
        let n = self.find(id)?;
        let label = if kind.is_some() && kind != Kind::from_name(&n.subtype) { tx("change kind", "Art ändern") } else { tx("change color", "Farbe ändern") };
        self.modify_undoable(ctx, &label, |ed| ed.restyle(&n, kind, color))?;
        let n = self.find(id)?;
        Ok(self.note_json(ctx, &n))
    }

    /// Eigenen Kommentar ohne Claude an eine Notiz anhängen.
    pub fn comment(&self, ctx: &Ctx, id: &str, text: &str) -> Result<Value, String> {
        if text.trim().is_empty() {
            return Err(tx("Empty comment.", "Leerer Kommentar."));
        }
        let n = self.find(id)?;
        let author = ctx.settings.read().unwrap().author();
        let nm = format!("pdftutor-{}", new_id());
        self.modify_undoable(ctx, &tx("add comment", "Kommentar hinzufügen"), |ed| {
            if n.thread.is_empty() {
                // erster Text einer Notiz: direkt in die Markierung bzw. Haftnotiz
                ed.set_contents(n.obj, text.trim())
            } else {
                ed.add_reply(&n, text.trim(), &author, &nm, &json!({"Role": "user"})).map(|_| ())
            }
        })?;
        let n = self.find(id)?;
        Ok(self.note_json(ctx, &n))
    }

    /// Text einer Nachricht (Kommentar oder Frage) ändern; leerer Text löscht die Nachricht.
    pub fn edit_message(&self, ctx: &Ctx, note_id: &str, msg_id: &str, text: &str) -> Result<Value, String> {
        let n = self.find(note_id)?;
        let msg = n.thread.iter().find(|m| m.id == msg_id).ok_or_else(|| tx("Message not found", "Nachricht nicht gefunden"))?;
        if msg.role != "user" {
            return Err(tx("Answers from Claude cannot be edited.", "Antworten von Claude lassen sich nicht bearbeiten."));
        }
        let text = text.trim().to_string();
        let label = if text.is_empty() { tx("delete comment", "Kommentar löschen") } else { tx("edit comment", "Kommentar bearbeiten") };
        let is_root = msg_id == n.id;
        self.modify_undoable(ctx, &label, |ed| {
            let (obj, page) = annots::find_annot(ed.doc(), msg_id).ok_or_else(|| tx("Message not found", "Nachricht nicht gefunden"))?;
            if text.is_empty() && !is_root {
                ed.remove_annot(page, obj)
            } else {
                ed.set_contents(obj, &text)
            }
        })?;
        let n = self.find(note_id)?;
        Ok(self.note_json(ctx, &n))
    }

    pub fn delete_note(&self, ctx: &Ctx, id: &str) -> Result<(), String> {
        let n = self.find(id)?;
        let label = if n.subtype == "Text" { tx("delete sticky note", "Haftnotiz löschen") } else { tx("delete highlight", "Markierung löschen") };
        self.modify_undoable(ctx, &label, |ed| ed.delete_note(&n))?;
        self.extra.lock().unwrap().remove(id);
        self.unsaved.lock().unwrap().retain(|u| u.note_id != id);
        for img in std::fs::read_dir(&ctx.paths.marks).into_iter().flatten().flatten() {
            if img.file_name().to_string_lossy().starts_with(&format!("markierung_{}", safe(id))) {
                let _ = std::fs::remove_file(img.path());
            }
        }
        Ok(())
    }

    fn mark_image(&self, ctx: &Ctx, n: &Note) -> Result<PathBuf, String> {
        let out = ctx.paths.marks.join(format!("markierung_{}.png", safe(&n.id)));
        let bytes = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        pdfium_host::render_mark_png(&bytes, n.page - 1, &n.rects, &out)?;
        Ok(out)
    }

    fn system_file(&self, ctx: &Ctx, mode: &str) -> Result<PathBuf, String> {
        let text = claude::system_text(&self.cache, mode, &read_hinweise(&ctx.paths));
        let path = self.cache.dir.join(format!("system_{mode}.md"));
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if old != text {
            write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(path)
    }

    /// Frage stellen. `saved`: die Frage steht schon im PDF (Kommentar der neuen Markierung).
    pub fn ask(
        self: &Arc<Self>,
        ctx: &Arc<Ctx>,
        id: &str,
        question: Option<String>,
        model: &str,
        retry: bool,
        saved: bool,
    ) -> Result<Value, String> {
        let model = if claude::models().as_array().unwrap().iter().any(|m| m["id"] == model) {
            model.to_string()
        } else {
            claude::DEFAULT_MODEL.to_string()
        };
        if !self.cache.wait_text(300) {
            return Err(tx("The document is still being prepared. Please try again in a moment.", "Das Dokument wird noch vorbereitet. Bitte gleich nochmal versuchen."));
        }
        let mode = self.effective_mode(ctx);

        // Frage bestimmen und (bei Nachfragen) zuerst ins PDF schreiben
        if retry {
            if let Some(v) = self.extra.lock().unwrap().get_mut(id) {
                v.retain(|m| m["status"] != "error");
            }
        } else if !saved {
            let q = question.clone().unwrap_or_default();
            if q.trim().is_empty() {
                return Err(tx("Empty question.", "Leere Frage."));
            }
            let n = self.find(id)?;
            let author = ctx.settings.read().unwrap().author();
            let nm = format!("pdftutor-{}", new_id());
            self.modify(ctx, |ed| ed.add_reply(&n, q.trim(), &author, &nm, &json!({"Role": "user"})))?;
        }
        let note = self.find(id)?;
        let last_user = note.thread.iter().rposition(|m| m.role == "user");
        let Some(qi) = last_user else { return Err(tx("There is no question.", "Keine Frage vorhanden.")) };
        let q = note.thread[qi].text.clone();
        let history: Vec<HistoryItem> =
            note.thread[..qi].iter().map(|m| HistoryItem { role: m.role.clone(), text: m.text.clone() }).collect();

        let started = Instant::now();
        let pending = json!({"role": "assistant", "status": "pending", "model": model, "mode": mode, "time": now_iso(), "text": ""});
        self.extra.lock().unwrap().insert(id.to_string(), vec![pending]);
        ctx.lib.update(&self.id, |e| e.ui.collapsed.retain(|c| c != id));

        let result = (|| -> Result<claude::Answer, String> {
            self.cache.focus.store(note.page, std::sync::atomic::Ordering::Relaxed);
            self.cache.ensure_images((note.page.saturating_sub(2))..=(note.page + 7));
            let sys = self.system_file(ctx, &mode)?;
            if let (Some(sess), false) = (note.session.as_deref(), history.is_empty()) {
                match claude::run(&claude::followup_prompt(&q), &model, &sys, Some(sess), &ctx.paths.work) {
                    Ok(a) => return Ok(a),
                    Err(e) => crate::logf!("Fortsetzen der Sitzung fehlgeschlagen ({e}); starte neu mit Verlauf."),
                }
            }
            let img = self.mark_image(ctx, &note)?;
            let prompt = claude::first_prompt(&self.cache, note.page, &note.text, &q, &mode, &[img], &history);
            claude::run(&prompt, &model, &sys, None, &ctx.paths.work)
        })();
        let seconds = started.elapsed().as_secs();

        match result {
            Ok(ans) => {
                self.extra.lock().unwrap().remove(id);
                let meta = json!({"Role": "assistant", "Model": model, "Mode": mode, "Seconds": seconds, "Usage": ans.usage});
                if let Err(e) = self.save_answer(ctx, id, &ans.text, &meta, ans.session.clone()) {
                    crate::logf!("Antwort konnte nicht gespeichert werden: {e}");
                    self.unsaved.lock().unwrap().push(Unsaved {
                        note_id: id.to_string(),
                        text: ans.text,
                        meta,
                        session: ans.session,
                    });
                    self.start_retry(ctx.clone());
                }
            }
            Err(e) => {
                let err = json!({"role": "assistant", "status": "error", "text": e, "model": model, "time": now_iso()});
                self.extra.lock().unwrap().insert(id.to_string(), vec![err]);
            }
        }
        match self.find(id) {
            Ok(n) => Ok(self.note_json(ctx, &n)),
            Err(_) => Ok(json!({"deleted": true})),
        }
    }

    fn save_answer(&self, ctx: &Ctx, id: &str, text: &str, meta: &Value, session: Option<String>) -> Result<(), String> {
        let n = self.find(id)?;
        let nm = format!("pdftutor-{}", new_id());
        self.modify(ctx, |ed| {
            ed.add_reply(&n, text, CLAUDE_AUTHOR, &nm, meta)?;
            if let Some(s) = &session {
                ed.set_meta(n.obj, &json!({"Session": s, "Model": meta["Model"]}))?;
            }
            Ok(())
        })
    }

    /// Antworten, die wegen einer Sperre nicht gespeichert werden konnten, später nachtragen.
    fn start_retry(self: &Arc<Self>, ctx: Arc<Ctx>) {
        let mut running = self.retry_running.lock().unwrap();
        if *running {
            return;
        }
        *running = true;
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let items: Vec<Unsaved> = std::mem::take(&mut *me.unsaved.lock().unwrap());
            let mut failed = vec![];
            for u in items {
                if let Err(e) = me.save_answer(&ctx, &u.note_id, &u.text, &u.meta, u.session.clone()) {
                    if e != locked_msg() && !e.starts_with(&saving_failed("")) {
                        crate::logf!("Nachtragen aufgegeben: {e}");
                        continue;
                    }
                    failed.push(u);
                }
            }
            let mut un = me.unsaved.lock().unwrap();
            un.extend(failed);
            if un.is_empty() {
                *me.retry_running.lock().unwrap() = false;
                return;
            }
        });
    }

    // -----------------------------------------------------------------------
    // Übernahme der Notizen aus Version 3 (<Name>.notes.json neben dem PDF)

    fn import_v3_notes(&self, ctx: &Ctx) -> Result<(), String> {
        let stem = self.path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let nj = self.path.with_file_name(format!("{stem}.notes.json"));
        let mut fallback = None;
        for e in std::fs::read_dir(ctx.paths.data.join("notizen")).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{stem}_")) && name.ends_with(".notes.json") {
                fallback = Some(e.path());
            }
        }
        let src = if nj.exists() { Some(nj) } else { fallback };
        let Some(src) = src else {
            ctx.lib.update(&self.id, |e| e.notes_imported = true);
            return Ok(());
        };
        let data: Value = serde_json::from_str(&std::fs::read_to_string(&src).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", src.display()))?;
        let st = &data["state"];
        ctx.lib.update(&self.id, |e| {
            if e.last_page.is_none() {
                e.last_page = st["lastPage"].as_u64().map(|x| x as u32);
            }
            if e.ui.scale.is_none() {
                e.ui.scale = st["scale"].as_f64();
            }
            if e.ui.model.is_none() {
                e.ui.model = st["model"].as_str().map(|s| s.to_string());
            }
            if e.ui.mode.is_none() {
                e.ui.mode = st["mode"].as_str().map(|s| s.to_string());
            }
        });
        let old_notes = data["notes"].as_array().cloned().unwrap_or_default();
        if old_notes.is_empty() {
            ctx.lib.update(&self.id, |e| e.notes_imported = true);
            return Ok(());
        }
        let author = ctx.settings.read().unwrap().author();
        let existing: Vec<String> = self.notes()?.iter().map(|n| n.id.clone()).collect();
        let mut collapsed = vec![];
        let imported = self.modify(ctx, |ed| {
            let mut count = 0;
            for on in &old_notes {
                let nm = format!("pdftutor-{}", on["id"].as_str().unwrap_or(""));
                if existing.contains(&nm) {
                    continue;
                }
                let page = on["page"].as_u64().unwrap_or(1) as u32;
                let Ok(page_id) = ed.page_id(page) else { continue };
                let (bx, rot) = annots::page_view(ed.doc(), page_id);
                let rects: Vec<[f32; 4]> = on["rects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|r| r["page"].as_u64() == Some(page as u64))
                    .map(|r| {
                        let (x, y, w, h) = (
                            r["x"].as_f64().unwrap_or(0.0) as f32,
                            r["y"].as_f64().unwrap_or(0.0) as f32,
                            r["w"].as_f64().unwrap_or(0.0) as f32,
                            r["h"].as_f64().unwrap_or(0.0) as f32,
                        );
                        let (ax, ay) = annots::norm_to_pdf(bx, rot, x, y);
                        let (bx2, by2) = annots::norm_to_pdf(bx, rot, x + w, y + h);
                        [ax.min(bx2), ay.min(by2), ax.max(bx2), ay.max(by2)]
                    })
                    .collect();
                if rects.is_empty() {
                    continue;
                }
                let thread: Vec<&Value> = on["thread"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["status"].as_str().unwrap_or("ok") == "ok")
                    .collect();
                let first_q = thread.first().filter(|m| m["role"] == "user").map(|m| m["text"].as_str().unwrap_or("").to_string());
                let meta = json!({"App": "PDF-Tutor", "Text": on["text"].as_str().unwrap_or(""),
                                  "Session": on["session"].as_str().unwrap_or("")});
                let root_id = ed.add_highlight(page, &rects, [1.0, 0.85, 0.0], first_q.as_deref().unwrap_or(""), &author, &nm, &meta)?;
                let root = Note {
                    id: nm.clone(),
                    obj: root_id,
                    page,
                    subtype: "Highlight".into(),
                    rects: rects.clone(),
                    color: [1.0, 0.85, 0.0],
                    author: author.clone(),
                    created: String::new(),
                    text: String::new(),
                    thread: vec![],
                    session: None,
                    own: true,
                };
                for (i, m) in thread.iter().enumerate() {
                    if i == 0 && first_q.is_some() {
                        continue;
                    }
                    let text = m["text"].as_str().unwrap_or("");
                    if text.trim().is_empty() {
                        continue;
                    }
                    let rnm = format!("pdftutor-{}", new_id());
                    if m["role"] == "assistant" {
                        let mut meta = json!({"Role": "assistant"});
                        if let Some(x) = m["model"].as_str() { meta["Model"] = json!(x); }
                        if let Some(x) = m["mode"].as_str() { meta["Mode"] = json!(x); }
                        if let Some(x) = m["seconds"].as_u64() { meta["Seconds"] = json!(x); }
                        if m["usage"].is_object() { meta["Usage"] = m["usage"].clone(); }
                        ed.add_reply(&root, text, CLAUDE_AUTHOR, &rnm, &meta)?;
                    } else {
                        ed.add_reply(&root, text, &author, &rnm, &json!({"Role": "user"}))?;
                    }
                }
                if on["collapsed"].as_bool().unwrap_or(false) {
                    collapsed.push(nm.clone());
                }
                count += 1;
            }
            Ok(count)
        })?;
        ctx.lib.update(&self.id, |e| {
            e.notes_imported = true;
            e.ui.collapsed.extend(collapsed);
        });
        crate::logf!("{imported} Notizen aus {} ins PDF übernommen", src.display());
        Ok(())
    }
}
