//! Markierungen, Kommentare und Antwort-Threads als Standard-PDF-Anmerkungen.
//!
//! Lesen: alle Anmerkungen aller Seiten, egal aus welchem Programm sie stammen.
//! Schreiben: inkrementell (Änderungen werden ans Dateiende angehängt, der bisherige
//! Inhalt bleibt Byte für Byte erhalten), wie es auch Acrobat macht.

use crate::i18n::tx;
use crate::util::{pdf_date, pdf_date_to_iso};
use lopdf::{decode_text_string, dictionary, text_string, Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const MARKUP: [&[u8]; 4] = [b"Highlight", b"Underline", b"StrikeOut", b"Squiggly"];
pub const META_KEY: &str = "PDFTutor";
pub const CLAUDE_AUTHOR: &str = "Claude";

#[derive(Clone, Debug)]
pub struct Msg {
    /// Kennung der Anmerkung, in der die Nachricht steht (bei der Wurzel: Kennung der Notiz)
    pub id: String,
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
                let mut v = json!({"id": m.id, "role": m.role, "author": m.author, "text": m.text, "time": m.time, "status": "ok"});
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
                id: note_id(&r.nm, r.id),
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
        for (k, out) in [("Model", "model"), ("Effort", "effort"), ("Mode", "mode"), ("Seconds", "seconds")] {
            if !r.meta[k].is_null() {
                meta[out] = r.meta[k].clone();
            }
        }
        if let Value::Object(u) = &r.meta["Usage"] {
            meta["usage"] = Value::Object(u.clone());
        }
        notes[ni].thread.push(Msg {
            id: note_id(&r.nm, r.id),
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
    let doc = Document::load_mem(bytes).map_err(|e| tx(&format!("The PDF could not be read: {e}"), &format!("PDF konnte nicht gelesen werden: {e}")))?;
    if doc.is_encrypted() {
        return Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password("")).map_err(|_| {
            tx("This PDF is password-protected; comments cannot be saved.", "Dieses PDF ist mit einem Passwort geschützt; Kommentare können nicht gespeichert werden.")
        });
    }
    Ok(doc)
}

/// Art einer Notiz, die der PDF-Tutor anlegen kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Highlight,
    Underline,
    StrikeOut,
    /// Haftnotiz (/Text)
    Note,
}

impl Kind {
    pub fn from_name(s: &str) -> Option<Kind> {
        match s {
            "Highlight" => Some(Kind::Highlight),
            "Underline" => Some(Kind::Underline),
            "StrikeOut" => Some(Kind::StrikeOut),
            "Text" | "Note" => Some(Kind::Note),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Highlight => "Highlight",
            Kind::Underline => "Underline",
            Kind::StrikeOut => "StrikeOut",
            Kind::Note => "Text",
        }
    }
}

pub const STICKY_SIZE: f32 = 20.0;

/// Ecken eines Textrechtecks in Leserichtung (oben links, oben rechts, unten links, unten rechts),
/// abhängig von der Seitendrehung. Reihenfolge wie in /QuadPoints üblich.
pub fn corners(r: [f32; 4], rot: i64) -> [(f32, f32); 4] {
    let [l, b, rr, t] = r;
    match rot {
        90 => [(l, b), (l, t), (rr, b), (rr, t)],
        180 => [(rr, b), (l, b), (rr, t), (l, t)],
        270 => [(rr, t), (rr, b), (l, t), (l, b)],
        _ => [(l, t), (rr, t), (l, b), (rr, b)],
    }
}

fn lerp(a: (f32, f32), b: (f32, f32), f: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
}

/// Erscheinungsbild einer Text-Markierung; liefert Form-XObject und Begrenzungsrechteck.
fn markup_appearance(kind: Kind, quads: &[[(f32, f32); 4]], color: [f32; 3]) -> (Stream, [f32; 4]) {
    let [c0, c1, c2] = color;
    let mut content = String::new();
    let mut pts: Vec<(f32, f32)> = vec![];
    match kind {
        Kind::Highlight => {
            content.push_str(&format!("/GS0 gs\n{c0} {c1} {c2} rg\n"));
            for q in quads {
                let [ul, ur, ll, lr] = *q;
                content.push_str(&format!(
                    "{:.3} {:.3} m {:.3} {:.3} l {:.3} {:.3} l {:.3} {:.3} l h f\n",
                    ul.0, ul.1, ur.0, ur.1, lr.0, lr.1, ll.0, ll.1
                ));
                pts.extend(q);
            }
        }
        _ => {
            // Linie in Leserichtung: knapp unter der Grundlinie bzw. auf Höhe der Kleinbuchstaben
            let pos = if kind == Kind::Underline { 0.1 } else { 0.42 };
            content.push_str(&format!("{c0} {c1} {c2} RG 0 J\n"));
            for q in quads {
                let [ul, ur, ll, lr] = *q;
                let h = ((ul.0 - ll.0).powi(2) + (ul.1 - ll.1).powi(2)).sqrt();
                let w = (h * 0.07).clamp(0.6, 2.5);
                let a = lerp(ll, ul, pos);
                let b = lerp(lr, ur, pos);
                content.push_str(&format!("{w:.2} w {:.3} {:.3} m {:.3} {:.3} l S\n", a.0, a.1, b.0, b.1));
                pts.extend(q);
            }
        }
    }
    let bb = [
        pts.iter().map(|p| p.0).fold(f32::MAX, f32::min) - 1.0,
        pts.iter().map(|p| p.1).fold(f32::MAX, f32::min) - 1.0,
        pts.iter().map(|p| p.0).fold(f32::MIN, f32::max) + 1.0,
        pts.iter().map(|p| p.1).fold(f32::MIN, f32::max) + 1.0,
    ];
    let mut res = Dictionary::new();
    if kind == Kind::Highlight {
        res.set(
            "ExtGState",
            dictionary! { "GS0" => dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "CA" => 1.0, "ca" => 1.0 } },
        );
    }
    let ap = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => rect_obj(bb), "Resources" => res },
        content.into_bytes(),
    );
    (ap, bb)
}

/// Sprechblasen-Symbol einer Haftnotiz (20 × 20 Punkte).
fn sticky_appearance(color: [f32; 3]) -> Stream {
    let [c0, c1, c2] = color;
    let content = format!(
        "{c0} {c1} {c2} rg 0.3 0.3 0.3 RG 0.8 w 1 j\n\
         2 6.5 m 2 18 l 18 18 l 18 6.5 l 10 6.5 l 5 2.5 l 6 6.5 l h B\n\
         0.3 0.3 0.3 RG 1 w 5 14.5 m 15 14.5 l 5 11.8 m 15 11.8 l 5 9.1 m 12 9.1 l S\n"
    );
    Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), STICKY_SIZE.into(), STICKY_SIZE.into()] },
        content.into_bytes(),
    )
}

/// Anmerkung anhand ihrer Kennung (NM oder Objektnummer) finden: (Objekt, Seite).
pub fn find_annot(doc: &Document, id: &str) -> Option<(ObjectId, u32)> {
    for (page_no, page_id) in doc.get_pages() {
        for aid in page_annot_ids(doc, page_id) {
            let nm = doc.get_dictionary(aid).map(|d| get_text(doc, d, b"NM")).unwrap_or_default();
            if note_id(&nm, aid) == id {
                return Some((aid, page_no));
            }
        }
    }
    None
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
        self.doc().get_pages().get(&page_no).copied().ok_or_else(|| tx(&format!("Page {page_no} not found"), &format!("Seite {page_no} nicht gefunden")))
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

    /// Markierung (Hervorhebung, Unterstreichung oder Durchstreichung) mit Erscheinungsbild
    /// anlegen. `rects` in PDF-Koordinaten.
    #[allow(clippy::too_many_arguments)]
    pub fn add_markup(
        &mut self,
        page_no: u32,
        kind: Kind,
        rects: &[[f32; 4]],
        color: [f32; 3],
        contents: &str,
        author: &str,
        nm: &str,
        meta: &Value,
    ) -> Result<ObjectId, String> {
        if rects.is_empty() {
            return Err(tx("No highlight given", "Keine Markierung übergeben"));
        }
        if kind == Kind::Note {
            return Err("Falsche Art".into());
        }
        let page_id = self.page_id(page_no)?;
        let (_, rot) = page_view(self.doc(), page_id);
        let quads: Vec<[(f32, f32); 4]> = rects.iter().map(|r| corners(*r, rot)).collect();
        let (ap, bb) = markup_appearance(kind, &quads, color);
        let ap_id = self.inc.new_document.add_object(ap);
        let now = pdf_date();
        let qp: Vec<Object> = quads.iter().flat_map(|q| q.iter().flat_map(|(x, y)| [Object::Real(*x), Object::Real(*y)])).collect();
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => kind.name(),
            "Rect" => rect_obj(bb), "QuadPoints" => qp, "C" => color_obj(color), "CA" => 1.0,
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

    /// Gelbe Hervorhebung (Kurzform, z. B. für die Übernahme aus Version 3).
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
        self.add_markup(page_no, Kind::Highlight, rects, color, contents, author, nm, meta)
    }

    /// Haftnotiz (Sprechblasen-Symbol) an einem Punkt der Seite (PDF-Koordinaten).
    #[allow(clippy::too_many_arguments)]
    pub fn add_sticky(
        &mut self,
        page_no: u32,
        at: (f32, f32),
        color: [f32; 3],
        contents: &str,
        author: &str,
        nm: &str,
        meta: &Value,
    ) -> Result<ObjectId, String> {
        let page_id = self.page_id(page_no)?;
        let s = STICKY_SIZE;
        let rect = [at.0 - s / 2.0, at.1 - s / 2.0, at.0 + s / 2.0, at.1 + s / 2.0];
        let ap_id = self.inc.new_document.add_object(sticky_appearance(color));
        let now = pdf_date();
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Text", "Name" => "Comment", "Open" => false,
            "Rect" => rect_obj(rect), "C" => color_obj(color),
            "F" => 28, "P" => page_id,
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

    /// Art und/oder Farbe einer Notiz ändern; das Erscheinungsbild wird neu erzeugt.
    pub fn restyle(&mut self, root: &Note, kind: Option<Kind>, color: Option<[f32; 3]>) -> Result<(), String> {
        let e = |x: lopdf::Error| x.to_string();
        let current = Kind::from_name(&root.subtype);
        let is_note = matches!(current, Some(Kind::Note)) || root.subtype == "FreeText";
        if root.subtype == "FreeText" {
            return Err(tx("This kind of note cannot be changed here.", "Diese Art von Notiz kann hier nicht geändert werden."));
        }
        let Some(current) = current else {
            return Err(tx("This kind of highlight cannot be changed here.", "Diese Art von Markierung kann hier nicht geändert werden."));
        };
        let kind = kind.unwrap_or(current);
        if is_note != (kind == Kind::Note) {
            return Err(tx("A sticky note cannot be turned into a highlight.", "Eine Haftnotiz kann nicht in eine Markierung umgewandelt werden."));
        }
        let color = color.unwrap_or(root.color);
        let page_id = self.page_id(root.page)?;
        let (_, rot) = page_view(self.doc(), page_id);
        let ap = if kind == Kind::Note {
            sticky_appearance(color)
        } else {
            let d = self.doc().get_dictionary(root.obj).map_err(e)?;
            let q = nums(self.doc(), d, b"QuadPoints");
            let quads: Vec<[(f32, f32); 4]> = if q.len() >= 8 {
                q.chunks(8).filter(|c| c.len() == 8).map(|c| [(c[0], c[1]), (c[2], c[3]), (c[4], c[5]), (c[6], c[7])]).collect()
            } else {
                root.rects.iter().map(|r| corners(*r, rot)).collect()
            };
            markup_appearance(kind, &quads, color).0
        };
        let ap_id = self.inc.new_document.add_object(ap);
        self.inc.opt_clone_object_to_new_document(root.obj).map_err(e)?;
        let d = self.inc.new_document.get_dictionary_mut(root.obj).map_err(e)?;
        d.set("Subtype", kind.name());
        d.set("C", color_obj(color));
        d.set("AP", dictionary! { "N" => ap_id });
        d.set("M", text_string(&pdf_date()));
        Ok(())
    }

    /// Text einer Anmerkung (/Contents) ersetzen; leerer Text entfernt ihn.
    pub fn set_contents(&mut self, obj: ObjectId, text: &str) -> Result<(), String> {
        let e = |x: lopdf::Error| x.to_string();
        self.inc.opt_clone_object_to_new_document(obj).map_err(e)?;
        let d = self.inc.new_document.get_dictionary_mut(obj).map_err(e)?;
        if text.is_empty() {
            d.remove(b"Contents");
        } else {
            d.set("Contents", text_string(text));
        }
        d.set("M", text_string(&pdf_date()));
        Ok(())
    }

    /// Einzelne Anmerkung (z. B. eine Antwort) samt zugehörigem Popup von der Seite nehmen.
    pub fn remove_annot(&mut self, page_no: u32, obj: ObjectId) -> Result<(), String> {
        let page_id = self.page_id(page_no)?;
        let doc = self.doc();
        let mut remove = std::collections::HashSet::from([obj]);
        for id in page_annot_ids(doc, page_id) {
            let parent = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|o| o.as_reference().ok());
            if parent == Some(obj) {
                remove.insert(id);
            }
        }
        self.annots_edit(page_id, |a| a.retain(|o| o.as_reference().map(|r| !remove.contains(&r)).unwrap_or(true)))
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
        self.inc.save_to(&mut out).map_err(|e| tx(&format!("Saving failed: {e}"), &format!("Speichern fehlgeschlagen: {e}")))?;
        if out.len() < self.prev_len || out[..self.prev_len] != self.inc.get_prev_documents_bytes()[..] {
            return Err(tx("Internal error: the existing file content would have been changed", "Interner Fehler: der bisherige Dateiinhalt wäre verändert worden"));
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

    #[test]
    fn writes_all_kinds_on_rotated_pages_and_edits() {
        let path = format!("{}/rotup.pdf", dir());
        let bytes = std::fs::read(&path).unwrap();
        let rects: Vec<[f32; 4]> = serde_json::from_str(&std::fs::read_to_string(format!("{}/rotup_rects.json", dir())).unwrap()).unwrap();
        let mut ed = Editor::open(bytes.clone()).unwrap();
        let meta = json!({"App": "PDF-Tutor"});
        for (i, r) in rects.iter().enumerate() {
            let p = i as u32 + 1;
            let r = *r;
            // drei Abschnitte der ersten Zeile: hervorheben, unterstreichen, durchstreichen
            let w = (r[2] - r[0]).max(r[3] - r[1]) / 3.0;
            let part = |k: usize| -> [f32; 4] {
                let k = k as f32;
                match i {
                    0 => [r[0] + k * w, r[1], r[0] + (k + 1.0) * w, r[3]],
                    1 => [r[0], r[1] + k * w, r[2], r[1] + (k + 1.0) * w],
                    2 => [r[2] - (k + 1.0) * w, r[1], r[2] - k * w, r[3]],
                    _ => [r[0], r[3] - (k + 1.0) * w, r[2], r[3] - k * w],
                }
            };
            ed.add_markup(p, Kind::Highlight, &[part(0)], [1.0, 0.85, 0.0], "", "T", &format!("h{p}"), &meta).unwrap();
            ed.add_markup(p, Kind::Underline, &[part(1)], [0.15, 0.45, 0.9], "", "T", &format!("u{p}"), &meta).unwrap();
            ed.add_markup(p, Kind::StrikeOut, &[part(2)], [0.9, 0.25, 0.55], "Kommentar", "T", &format!("s{p}"), &meta).unwrap();
            ed.add_sticky(p, (r[0] + 5.0, r[1] - 40.0), [1.0, 0.85, 0.0], "Haftnotiz", "T", &format!("n{p}"), &meta).unwrap();
        }
        let out = ed.finish().unwrap();
        assert_eq!(&out[..bytes.len()], &bytes[..]);
        std::fs::write(format!("{}/out_rotup.pdf", dir()), &out).unwrap();
        let notes = read_notes(&load(&out).unwrap());
        assert_eq!(notes.len(), 16);
        let s1 = notes.iter().find(|n| n.id == "s1").unwrap();
        assert_eq!(s1.subtype, "StrikeOut");
        assert_eq!(s1.thread[0].text, "Kommentar");
        assert_eq!(s1.thread[0].id, "s1");
        assert!(notes.iter().any(|n| n.id == "n3" && n.subtype == "Text" && n.thread[0].text == "Haftnotiz"));

        // ändern: Unterstreichung -> grüne Hervorhebung, Kommentar bearbeiten, Haftnotiz umfärben
        let mut ed = Editor::open(out.clone()).unwrap();
        let u2 = notes.iter().find(|n| n.id == "u2").unwrap();
        ed.restyle(u2, Some(Kind::Highlight), Some([0.49, 0.85, 0.39])).unwrap();
        let n1 = notes.iter().find(|n| n.id == "n1").unwrap();
        ed.restyle(n1, None, Some([0.45, 0.75, 1.0])).unwrap();
        assert!(ed.restyle(n1, Some(Kind::Underline), None).is_err());
        let (obj, _) = find_annot(ed.doc(), "s1").unwrap();
        ed.set_contents(obj, "geändert").unwrap();
        let h4 = notes.iter().find(|n| n.id == "h4").unwrap();
        ed.add_reply(h4, "eigener Kommentar", "T", "r4", &json!({"Role": "user"})).unwrap();
        let out2 = ed.finish().unwrap();
        assert_eq!(&out2[..out.len()], &out[..]);
        std::fs::write(format!("{}/out_rotup_edit.pdf", dir()), &out2).unwrap();
        let notes2 = read_notes(&load(&out2).unwrap());
        let u2b = notes2.iter().find(|n| n.id == "u2").unwrap();
        assert_eq!(u2b.subtype, "Highlight");
        assert_eq!(u2b.color, [0.49, 0.85, 0.39]);
        assert_eq!(notes2.iter().find(|n| n.id == "s1").unwrap().thread[0].text, "geändert");
        let h4b = notes2.iter().find(|n| n.id == "h4").unwrap();
        assert_eq!(h4b.thread.len(), 1);
        assert_eq!(h4b.thread[0].id, "r4");

        // Antwort entfernen, Kommentar leeren
        let mut ed = Editor::open(out2.clone()).unwrap();
        let (r4, pg) = find_annot(ed.doc(), "r4").unwrap();
        ed.remove_annot(pg, r4).unwrap();
        let (s1o, _) = find_annot(ed.doc(), "s1").unwrap();
        ed.set_contents(s1o, "").unwrap();
        let out3 = ed.finish().unwrap();
        let notes3 = read_notes(&load(&out3).unwrap());
        assert!(notes3.iter().find(|n| n.id == "h4").unwrap().thread.is_empty());
        assert!(notes3.iter().find(|n| n.id == "s1").unwrap().thread.is_empty());
        assert_eq!(notes3.len(), 16);
    }
}
