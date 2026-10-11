//! Markierungen, Kommentare und Antwort-Threads als Standard-PDF-Anmerkungen.
//!
//! Lesen: alle Anmerkungen aller Seiten, egal aus welchem Programm sie stammen.
//! Schreiben: inkrementell (Änderungen werden ans Dateiende angehängt, der bisherige
//! Inhalt bleibt Byte für Byte erhalten), wie es auch Acrobat macht.

use crate::util::{pdf_date, pdf_date_to_iso};
use lopdf::{decode_text_string, dictionary, text_string, Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const MARKUP: [&[u8]; 4] = [b"Highlight", b"Underline", b"StrikeOut", b"Squiggly"];
pub const META_KEY: &str = "PDFTutor";
pub const CLAUDE_AUTHOR: &str = "Claude";

#[derive(Clone, Debug)]
pub struct Msg {
    pub role: String, // "user" | "assistant"
    pub author: String,
    pub text: String,
    pub time: String,
    pub meta: Value,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub id: String,
    pub obj: ObjectId,
    pub page: u32,
    pub subtype: String,
    pub rects: Vec<[f32; 4]>,
    pub color: [f32; 3],
    pub author: String,
    pub created: String,
    pub text: String,
    pub thread: Vec<Msg>,
    pub session: Option<String>,
    pub own: bool,
}

impl Note {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "page": self.page, "subtype": self.subtype,
            "pdfRects": self.rects, "color": self.color, "author": self.author,
            "created": self.created, "text": self.text, "own": self.own,
            "thread": self.thread.iter().map(|m| {
                let mut v = json!({"role": m.role, "author": m.author, "text": m.text, "time": m.time, "status": "ok"});
                if let Value::Object(meta) = &m.meta {
                    for (k, x) in meta { v[k] = x.clone(); }
                }
                v
            }).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------
// Hilfen zum Lesen

fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(f) => Some(*f),
        _ => None,
    }
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn get_text(doc: &Document, d: &Dictionary, key: &[u8]) -> String {
    d.get(key).ok().map(|o| resolve(doc, o)).and_then(|o| decode_text_string(o).ok()).unwrap_or_default()
}

fn get_name(d: &Dictionary, key: &[u8]) -> Option<Vec<u8>> {
    d.get(key).ok().and_then(|o| o.as_name().ok()).map(|n| n.to_vec())
}

fn nums(doc: &Document, d: &Dictionary, key: &[u8]) -> Vec<f32> {
    match d.get(key).ok().map(|o| resolve(doc, o)) {
        Some(Object::Array(a)) => a.iter().filter_map(|x| num(resolve(doc, x))).collect(),
        _ => vec![],
    }
}

fn meta_to_json(doc: &Document, d: &Dictionary) -> Value {
    let mut out = serde_json::Map::new();
    for (k, v) in d.iter() {
        let key = String::from_utf8_lossy(k).to_string();
        let val = match resolve(doc, v) {
            Object::Integer(i) => json!(i),
            Object::Real(f) => json!(f),
            Object::Name(n) => json!(String::from_utf8_lossy(n)),
            Object::Boolean(b) => json!(b),
            o @ Object::String(..) => json!(decode_text_string(o).unwrap_or_default()),
            Object::Dictionary(dd) => meta_to_json(doc, dd),
            _ => Value::Null,
        };
        out.insert(key, val);
    }
    Value::Object(out)
}

/// Annotation-Referenzen einer Seite (Array direkt oder als Referenz).
fn page_annot_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let Ok(page) = doc.get_dictionary(page_id) else { return vec![] };
    match page.get(b"Annots").ok().map(|o| resolve(doc, o)) {
        Some(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
        _ => vec![],
    }
}

fn bbox(v: &[f32]) -> [f32; 4] {
    let xs = [v[0], v[2], v[4], v[6]];
    let ys = [v[1], v[3], v[5], v[7]];
    let f = |a: &[f32; 4], m: fn(f32, f32) -> f32| a.iter().copied().fold(a[0], m);
    [f(&xs, f32::min), f(&ys, f32::min), f(&xs, f32::max), f(&ys, f32::max)]
}

pub fn note_id(nm: &str, obj: ObjectId) -> String {
    if !nm.is_empty() {
        nm.to_string()
    } else {
        format!("o{}_{}", obj.0, obj.1)
    }
}

struct Raw {
    id: ObjectId,
    page: u32,
    subtype: Vec<u8>,
    irt: Option<ObjectId>,
    nm: String,
    contents: String,
    author: String,
    date: String,
    rects: Vec<[f32; 4]>,
    color: [f32; 3],
    meta: Value,
}

/// Alle Notizen (Markierungen und Haftnotizen samt Antworten) eines Dokuments.
pub fn read_notes(doc: &Document) -> Vec<Note> {
    let mut raws: Vec<Raw> = vec![];
    for (page_no, page_id) in doc.get_pages() {
        for aid in page_annot_ids(doc, page_id) {
            let Ok(d) = doc.get_dictionary(aid) else { continue };
            let Some(subtype) = get_name(d, b"Subtype") else { continue };
            let quads = nums(doc, d, b"QuadPoints");
            let rects: Vec<[f32; 4]> = if quads.len() >= 8 {
                quads.chunks(8).filter(|c| c.len() == 8).map(bbox).collect()
            } else {
                let r = nums(doc, d, b"Rect");
                if r.len() == 4 {
                    vec![[r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]]
                } else {
                    vec![]
                }
            };
            let c = nums(doc, d, b"C");
            let color = if c.len() == 3 { [c[0], c[1], c[2]] } else { [1.0, 0.85, 0.0] };
            let mut date = get_text(doc, d, b"M");
            if date.is_empty() {
                date = get_text(doc, d, b"CreationDate");
            }
            let meta = match d.get(META_KEY.as_bytes()).ok().map(|o| resolve(doc, o)) {
                Some(Object::Dictionary(m)) => meta_to_json(doc, m),
                _ => Value::Null,
            };
            raws.push(Raw {
                id: aid,
                page: page_no,
                irt: d.get(b"IRT").ok().and_then(|o| o.as_reference().ok()),
                nm: get_text(doc, d, b"NM"),
                contents: get_text(doc, d, b"Contents"),
                author: get_text(doc, d, b"T"),
                date: pdf_date_to_iso(&date),
                subtype,
                rects,
                color,
                meta,
            });
        }
    }
    let by_id: HashMap<ObjectId, usize> = raws.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let is_root = |r: &Raw| {
        r.irt.is_none() && (MARKUP.contains(&r.subtype.as_slice()) || r.subtype == b"Text" || r.subtype == b"FreeText")
    };
    let mut notes: Vec<Note> = vec![];
    let mut root_index: HashMap<ObjectId, usize> = HashMap::new();
    for r in raws.iter().filter(|r| is_root(r)) {
        let mut thread = vec![];
        if !r.contents.trim().is_empty() {
            thread.push(Msg {
                role: "user".into(),
                author: r.author.clone(),
                text: r.contents.clone(),
                time: r.date.clone(),
                meta: Value::Null,
            });
        }
        root_index.insert(r.id, notes.len());
        notes.push(Note {
            id: note_id(&r.nm, r.id),
            obj: r.id,
            page: r.page,
            subtype: String::from_utf8_lossy(&r.subtype).to_string(),
            rects: r.rects.clone(),
            color: r.color,
            author: r.author.clone(),
            created: r.date.clone(),
            text: r.meta["Text"].as_str().unwrap_or("").to_string(),
            thread,
            session: r.meta["Session"].as_str().map(|s| s.to_string()).filter(|s| !s.is_empty()),
            own: !r.meta.is_null(),
        });
    }
    // Antworten ihrer Wurzel zuordnen (auch Antworten auf Antworten)
    let mut replies: Vec<(usize, &Raw)> = vec![];
    for r in raws.iter().filter(|r| r.irt.is_some()) {
        let mut cur = r.irt;
        let mut guard = 0;
        while let Some(id) = cur {
            if let Some(&ni) = root_index.get(&id) {
                replies.push((ni, r));
                break;
            }
            cur = by_id.get(&id).and_then(|&i| raws[i].irt);
            guard += 1;
            if guard > 50 {
                break;
            }
        }
    }
    replies.sort_by(|a, b| (a.1.date.as_str(), a.1.id.0).cmp(&(b.1.date.as_str(), b.1.id.0)));
    for (ni, r) in replies {
        if r.contents.trim().is_empty() {
            continue;
        }
        let role = if r.meta["Role"].as_str() == Some("assistant") || r.author == CLAUDE_AUTHOR {
            "assistant"
        } else {
            "user"
        };
        let mut meta = json!({});
        for (k, out) in [("Model", "model"), ("Mode", "mode"), ("Seconds", "seconds")] {
            if !r.meta[k].is_null() {
                meta[out] = r.meta[k].clone();
            }
        }
        if let Value::Object(u) = &r.meta["Usage"] {
            meta["usage"] = Value::Object(u.clone());
        }
        notes[ni].thread.push(Msg {
            role: role.into(),
            author: r.author.clone(),
            text: r.contents.clone(),
            time: r.date.clone(),
            meta,
        });
    }
    notes.sort_by(|a, b| (a.page, a.created.as_str()).cmp(&(b.page, b.created.as_str())));
    notes
}

/// Sichtbarer Seitenausschnitt (CropBox, sonst MediaBox, auch geerbt) und Drehung.
pub fn page_view(doc: &Document, page_id: ObjectId) -> ([f32; 4], i64) {
    let mut boxv: Option<[f32; 4]> = None;
    let mut media: Option<[f32; 4]> = None;
    let mut rot: Option<i64> = None;
    let mut cur = Some(page_id);
    let mut guard = 0;
    while let Some(id) = cur {
        let Ok(d) = doc.get_dictionary(id) else { break };
        let arr = |k: &[u8]| {
            let v = nums(doc, d, k);
            if v.len() == 4 {
                Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
            } else {
                None
            }
        };
        if boxv.is_none() {
            boxv = arr(b"CropBox");
        }
        if media.is_none() {
            media = arr(b"MediaBox");
        }
        if rot.is_none() {
            rot = d.get(b"Rotate").ok().and_then(|o| num(resolve(doc, o))).map(|f| f as i64);
        }
        cur = d.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        guard += 1;
        if guard > 30 {
            break;
        }
    }
    let b = boxv.or(media).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    (b, rot.unwrap_or(0).rem_euclid(360))
}

/// Normierter Punkt der angezeigten Seite (0..1, Ursprung oben links) in PDF-Koordinaten.
pub fn norm_to_pdf(b: [f32; 4], rot: i64, x: f32, y: f32) -> (f32, f32) {
    let (x0, y0, x1, y1) = (b[0], b[1], b[2], b[3]);
    let (w, h) = (x1 - x0, y1 - y0);
    match rot {
        90 => (x0 + y * w, y0 + x * h),
        180 => (x1 - x * w, y0 + y * h),
        270 => (x1 - y * w, y1 - x * h),
        _ => (x0 + x * w, y1 - y * h),
    }
}

// ---------------------------------------------------------------------------
// Schreiben

pub fn load(bytes: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("PDF konnte nicht gelesen werden: {e}"))?;
    if doc.is_encrypted() {
        return Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password("")).map_err(|_| {
            "Dieses PDF ist mit einem Passwort geschützt; Kommentare können nicht gespeichert werden.".to_string()
        });
    }
    Ok(doc)
}

pub struct Editor {
    inc: IncrementalDocument,
    prev_len: usize,
}

fn rect_obj(r: [f32; 4]) -> Vec<Object> {
    r.iter().map(|v| Object::Real(*v)).collect()
}

fn color_obj(c: [f32; 3]) -> Vec<Object> {
    c.iter().map(|v| Object::Real(*v)).collect()
}

pub fn meta_dict(v: &Value) -> Dictionary {
    let mut d = Dictionary::new();
    if let Value::Object(m) = v {
        for (k, x) in m {
            let o = match x {
                Value::String(s) => text_string(s),
                Value::Number(n) if n.is_i64() => Object::Integer(n.as_i64().unwrap()),
                Value::Number(n) => Object::Real(n.as_f64().unwrap_or(0.0) as f32),
                Value::Bool(b) => Object::Boolean(*b),
                Value::Object(_) => Object::Dictionary(meta_dict(x)),
                _ => continue,
            };
            d.set(k.as_bytes().to_vec(), o);
        }
    }
    d
}

impl Editor {
    pub fn open(bytes: Vec<u8>) -> Result<Editor, String> {
        let doc = load(&bytes)?;
        let prev_len = bytes.len();
        Ok(Editor { inc: IncrementalDocument::create_from(bytes, doc), prev_len })
    }

    pub fn doc(&self) -> &Document {
        self.inc.get_prev_documents()
    }

    pub fn page_id(&self, page_no: u32) -> Result<ObjectId, String> {
        self.doc().get_pages().get(&page_no).copied().ok_or_else(|| format!("Seite {page_no} nicht gefunden"))
    }

    fn annots_edit(&mut self, page_id: ObjectId, f: impl FnOnce(&mut Vec<Object>)) -> Result<(), String> {
        let e = |x: lopdf::Error| x.to_string();
        self.inc.opt_clone_object_to_new_document(page_id).map_err(e)?;
        let annots = self.inc.new_document.get_dictionary(page_id).map_err(e)?.get(b"Annots").ok().cloned();
        match annots {
            Some(Object::Reference(arr_id)) => {
                self.inc.opt_clone_object_to_new_document(arr_id).map_err(e)?;
                let arr = self.inc.new_document.get_object_mut(arr_id).map_err(e)?.as_array_mut().map_err(e)?;
                f(arr);
            }
            Some(Object::Array(mut a)) => {
                f(&mut a);
                self.inc.new_document.get_dictionary_mut(page_id).map_err(e)?.set("Annots", a);
            }
            _ => {
                let mut a = vec![];
                f(&mut a);
                self.inc.new_document.get_dictionary_mut(page_id).map_err(e)?.set("Annots", a);
            }
        }
        Ok(())
    }

    /// Markierung (Highlight) mit Erscheinungsbild anlegen. `rects` in PDF-Koordinaten.
    #[allow(clippy::too_many_arguments)]
    pub fn add_highlight(
        &mut self,
        page_no: u32,
        rects: &[[f32; 4]],
        color: [f32; 3],
        contents: &str,
        author: &str,
        nm: &str,
        meta: &Value,
    ) -> Result<ObjectId, String> {
        if rects.is_empty() {
            return Err("Keine Markierung übergeben".into());
        }
        let page_id = self.page_id(page_no)?;
        let mut quads = vec![];
        let (mut ux0, mut uy0, mut ux1, mut uy1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut content = format!("/GS0 gs\n{} {} {} rg\n", color[0], color[1], color[2]);
        for r in rects {
            let [l, b, rr, t] = *r;
            quads.extend([l, t, rr, t, l, b, rr, b].map(Object::Real));
            content.push_str(&format!("{l:.3} {b:.3} {:.3} {:.3} re f\n", rr - l, t - b));
            ux0 = ux0.min(l);
            uy0 = uy0.min(b);
            ux1 = ux1.max(rr);
            uy1 = uy1.max(t);
        }
        let bb = [ux0, uy0, ux1, uy1];
        let ap = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => rect_obj(bb),
                "Resources" => dictionary! {
                    "ExtGState" => dictionary! { "GS0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "CA" => 1.0, "ca" => 1.0 } }
                },
            },
            content.into_bytes(),
        );
        let ap_id = self.inc.new_document.add_object(ap);
        let now = pdf_date();
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Highlight",
            "Rect" => rect_obj(bb), "QuadPoints" => quads, "C" => color_obj(color), "CA" => 1.0,
            "F" => 4, "P" => page_id,
            "NM" => text_string(nm), "T" => text_string(author),
            "M" => text_string(&now), "CreationDate" => text_string(&now),
            "AP" => dictionary! { "N" => ap_id },
        };
        if !contents.is_empty() {
            d.set("Contents", text_string(contents));
        }
        d.set(META_KEY, meta_dict(meta));
        let id = self.inc.new_document.add_object(d);
        self.annots_edit(page_id, |a| a.push(id.into()))?;
        Ok(id)
    }

    /// Antwort an eine Notiz anhängen (Kommentar-Thread).
    pub fn add_reply(&mut self, root: &Note, contents: &str, author: &str, nm: &str, meta: &Value) -> Result<ObjectId, String> {
        let page_id = self.page_id(root.page)?;
        let rect = root.rects.first().copied().unwrap_or([0.0, 0.0, 10.0, 10.0]);
        // leeres Erscheinungsbild: Antworten erscheinen nur im Thread, nicht als eigenes Symbol
        let empty_ap = self.inc.new_document.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 0.into(), 0.into()] },
            Vec::new(),
        ));
        let now = pdf_date();
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Text",
            "Rect" => rect_obj(rect), "F" => 28, "P" => page_id,
            "IRT" => root.obj, "RT" => "R", "Name" => "Comment",
            "NM" => text_string(nm), "T" => text_string(author),
            "M" => text_string(&now), "CreationDate" => text_string(&now),
            "Contents" => text_string(contents),
            "AP" => dictionary! { "N" => empty_ap },
        };
        d.set(META_KEY, meta_dict(meta));
        let id = self.inc.new_document.add_object(d);
        self.annots_edit(page_id, |a| a.push(id.into()))?;
        Ok(id)
    }

    /// Einträge im PDF-Tutor-Zusatzwörterbuch einer Anmerkung setzen.
    pub fn set_meta(&mut self, obj: ObjectId, entries: &Value) -> Result<(), String> {
        let existing = self
            .doc()
            .get_dictionary(obj)
            .ok()
            .and_then(|d| d.get(META_KEY.as_bytes()).ok())
            .map(|o| resolve(self.doc(), o).clone());
        self.inc.opt_clone_object_to_new_document(obj).map_err(|e| e.to_string())?;
        let mut meta = match existing {
            Some(Object::Dictionary(d)) => d,
            _ => Dictionary::new(),
        };
        for (k, v) in meta_dict(entries).iter() {
            meta.set(k.clone(), v.clone());
        }
        self.inc.new_document.get_dictionary_mut(obj).map_err(|e| e.to_string())?.set(META_KEY, meta);
        Ok(())
    }

    /// Notiz samt allen Antworten (und zugehörigen Popups) von der Seite entfernen.
    pub fn delete_note(&mut self, root: &Note) -> Result<(), String> {
        let page_id = self.page_id(root.page)?;
        let doc = self.doc();
        let ids = page_annot_ids(doc, page_id);
        let irt = |id: ObjectId| doc.get_dictionary(id).ok().and_then(|d| d.get(b"IRT").ok()).and_then(|o| o.as_reference().ok());
        let parent = |id: ObjectId| doc.get_dictionary(id).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|o| o.as_reference().ok());
        let mut remove = std::collections::HashSet::from([root.obj]);
        loop {
            let before = remove.len();
            for &id in &ids {
                if irt(id).map(|t| remove.contains(&t)).unwrap_or(false) {
                    remove.insert(id);
                }
            }
            if remove.len() == before {
                break;
            }
        }
        for &id in &ids {
            if parent(id).map(|p| remove.contains(&p)).unwrap_or(false) {
                remove.insert(id);
            }
        }
        self.annots_edit(page_id, |a| a.retain(|o| o.as_reference().map(|r| !remove.contains(&r)).unwrap_or(true)))
    }

    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(self.prev_len + 8192);
        self.inc.save_to(&mut out).map_err(|e| format!("Speichern fehlgeschlagen: {e}"))?;
        if out.len() < self.prev_len || out[..self.prev_len] != self.inc.get_prev_documents_bytes()[..] {
            return Err("Interner Fehler: der bisherige Dateiinhalt wäre verändert worden".into());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir() -> String {
        std::env::var("PT_TESTDIR").unwrap_or_else(|_| "/tmp/claude-0/special".into())
    }

    fn roundtrip(name: &str) -> Vec<Note> {
        let path = format!("{}/{}", dir(), name);
        let bytes = std::fs::read(&path).unwrap();
        let mut ed = Editor::open(bytes.clone()).unwrap();
        let id = ed
            .add_highlight(1, &[[100.0, 700.0, 300.0, 712.0]], [1.0, 0.85, 0.0], "Frage äöü", "Tester", "pdftutor-test1", &json!({"App": "PDF-Tutor", "Text": "markiert"}))
            .unwrap();
        let root = Note { id: "pdftutor-test1".into(), obj: id, page: 1, subtype: "Highlight".into(), rects: vec![[100.0, 700.0, 300.0, 712.0]],
            color: [1.0, 0.85, 0.0], author: "Tester".into(), created: String::new(), text: String::new(), thread: vec![], session: None, own: true };
        ed.add_reply(&root, "Antwort mit $x^2$", CLAUDE_AUTHOR, "pdftutor-test2", &json!({"Role": "assistant", "Model": "sonnet"})).unwrap();
        let out = ed.finish().unwrap();
        assert_eq!(&out[..bytes.len()], &bytes[..]);
        let outpath = format!("{}/out_{}", dir(), name);
        std::fs::write(&outpath, &out).unwrap();
        read_notes(&load(&out).unwrap())
    }

    #[test]
    fn writes_into_plain_pdf() {
        let notes = roundtrip("plain.pdf");
        let n = notes.iter().find(|n| n.id == "pdftutor-test1").unwrap();
        assert_eq!(n.thread.len(), 2);
        assert_eq!(n.thread[0].text, "Frage äöü");
        assert_eq!(n.thread[1].role, "assistant");
        assert_eq!(n.text, "markiert");
    }

    #[test]
    fn writes_into_encrypted_pdfs() {
        for f in ["enc_owner.pdf", "enc_rc4.pdf"] {
            let notes = roundtrip(f);
            let n = notes.iter().find(|n| n.id == "pdftutor-test1").expect(f);
            assert_eq!(n.thread.len(), 2, "{f}");
            assert_eq!(n.thread[1].text, "Antwort mit $x^2$", "{f}");
        }
    }

    #[test]
    fn reads_foreign_annotations_and_deletes() {
        let path = format!("{}/foreign.pdf", dir());
        let bytes = std::fs::read(&path).unwrap();
        let notes = read_notes(&load(&bytes).unwrap());
        let hl = notes.iter().find(|n| n.subtype == "Highlight").expect("Highlight");
        assert_eq!(hl.author, "Acrobat-Nutzer");
        assert_eq!(hl.thread[0].text, "Kommentar aus einem anderen Programm");
        assert!(!hl.own);
        assert!(notes.iter().any(|n| n.subtype == "Text" && n.thread[0].text == "Haftnotiz aus Edge"));
        // Antwort an fremde Markierung anhängen, dann Notiz löschen
        let mut ed = Editor::open(bytes.clone()).unwrap();
        ed.add_reply(hl, "Claude antwortet", CLAUDE_AUTHOR, "pdftutor-r", &json!({"Role": "assistant"})).unwrap();
        let out = ed.finish().unwrap();
        let notes2 = read_notes(&load(&out).unwrap());
        let hl2 = notes2.iter().find(|n| n.id == hl.id).unwrap();
        assert_eq!(hl2.thread.len(), 2);
        let mut ed = Editor::open(out.clone()).unwrap();
        ed.delete_note(hl2).unwrap();
        let out2 = ed.finish().unwrap();
        std::fs::write(format!("{}/out_foreign_deleted.pdf", dir()), &out2).unwrap();
        let notes3 = read_notes(&load(&out2).unwrap());
        assert!(notes3.iter().all(|n| n.id != hl.id));
        assert_eq!(notes3.len(), 1);
    }
}
