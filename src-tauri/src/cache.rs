//! Seiten-Cache: Text und Bild jeder Seite, Übersicht und Volltext für Claude.
//! Claude liest diese Dateien statt des Original-PDFs. Der Cache bleibt gültig, wenn
//! Anmerkungen ins PDF geschrieben werden (die werden nur angehängt).

use crate::pdfium_host;
use crate::util::{norm_path, sha1_hex, write_atomic};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const CACHE_VERSION: u64 = 3;
const MAX_EDGE: i32 = 1600;
const CHARS_PER_TOKEN: f64 = 3.3;
const CHUNK: usize = 6;

pub struct PageCache {
    pub pdf_path: PathBuf,
    pub dir: PathBuf,
    pub text_dir: PathBuf,
    pub img_dir: PathBuf,
    pub n: u32,
    pub title: String,
    pub labels: Vec<String>,
    pub toc: Vec<(u32, String, u32)>,
    pub digits: usize,
    pub focus: AtomicU32,
    pub tokens: Mutex<Option<u64>>,
    pub error: Mutex<Option<String>>,
    text_ready: (Mutex<bool>, Condvar),
    images_done: AtomicU32,
}

fn fingerprint(bytes: &[u8]) -> String {
    let n = bytes.len().min(1 << 20);
    sha1_hex(&bytes[..n])
}

pub fn nice_title(meta: &str, path: &Path) -> String {
    let t = meta.trim();
    let lower = t.to_lowercase();
    let bad_ext = [".ppt", ".pptx", ".doc", ".docx", ".tex", ".dvi", ".odp", ".odt", ".pdf", ".key"]
        .iter()
        .any(|e| lower.ends_with(e));
    let bad = t.chars().count() < 3
        || ["untitled", "unbenannt", "title", "titel"].contains(&lower.as_str())
        || bad_ext
        || lower.starts_with("microsoft ")
        || lower.starts_with("powerpoint")
        || lower.starts_with("präsentation");
    if bad {
        path.file_stem().unwrap_or_default().to_string_lossy().to_string()
    } else {
        t.to_string()
    }
}

impl PageCache {
    pub fn open(pdf_path: &Path, work: &Path) -> Result<Arc<PageCache>, String> {
        let bytes = std::fs::read(pdf_path).map_err(|e| crate::i18n::tx(&format!("The PDF cannot be read: {e}"), &format!("PDF kann nicht gelesen werden: {e}")))?;
        let info = pdfium_host::doc_info(&bytes)?;
        let key = &sha1_hex(norm_path(pdf_path).as_bytes())[..10];
        let stem = pdf_path.file_stem().unwrap_or_default().to_string_lossy();
        let safe: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .chars()
            .take(40)
            .collect();
        let safe = if safe.is_empty() { "dokument".to_string() } else { safe };
        let dir = work.join("cache").join(format!("{safe}_{key}"));
        let text_dir = dir.join("seiten");
        let img_dir = dir.join("bilder");
        let _ = std::fs::create_dir_all(&text_dir);
        let _ = std::fs::create_dir_all(&img_dir);
        let n = info.pages;
        let c = Arc::new(PageCache {
            pdf_path: pdf_path.to_path_buf(),
            title: nice_title(&info.title, pdf_path),
            labels: info.labels,
            toc: info.toc,
            digits: n.to_string().len().max(4),
            focus: AtomicU32::new(1),
            tokens: Mutex::new(None),
            error: Mutex::new(None),
            text_ready: (Mutex::new(false), Condvar::new()),
            images_done: AtomicU32::new(0),
            n,
            dir,
            text_dir,
            img_dir,
        });
        let fp = fingerprint(&bytes);
        let c2 = c.clone();
        std::thread::spawn(move || c2.run(bytes, fp));
        Ok(c)
    }

    pub fn text_path(&self, p: u32) -> PathBuf {
        self.text_dir.join(format!("seite_{:0w$}.txt", p, w = self.digits))
    }
    pub fn img_path(&self, p: u32) -> PathBuf {
        self.img_dir.join(format!("seite_{:0w$}.jpg", p, w = self.digits))
    }
    pub fn overview_path(&self) -> PathBuf {
        self.dir.join("uebersicht.md")
    }
    pub fn fulltext_path(&self) -> PathBuf {
        self.dir.join("volltext.md")
    }

    pub fn label(&self, p: u32) -> String {
        let l = self.labels.get((p as usize).wrapping_sub(1)).cloned().unwrap_or_default();
        if l.is_empty() {
            p.to_string()
        } else {
            l
        }
    }

    pub fn page_name(&self, p: u32) -> String {
        let l = self.label(p);
        if l == p.to_string() {
            format!("page {p}")
        } else {
            format!("page {p} (printed: {l})")
        }
    }

    fn run(self: Arc<Self>, bytes: Vec<u8>, fp: String) {
        if let Err(e) = self.build_text(&bytes, &fp) {
            crate::logf!("Seitentexte: {e}");
            *self.error.lock().unwrap() = Some(crate::i18n::tx(&format!("Text could not be extracted: {e}"), &format!("Text konnte nicht extrahiert werden: {e}")));
        }
        {
            let (m, cv) = &self.text_ready;
            *m.lock().unwrap() = true;
            cv.notify_all();
        }
        if let Err(e) = self.build_images(&bytes) {
            crate::logf!("Seitenbilder: {e}");
        }
    }

    fn build_text(&self, bytes: &[u8], fp: &str) -> Result<(), String> {
        let meta_path = self.dir.join("meta.json");
        if let Ok(t) = std::fs::read_to_string(&meta_path) {
            if let Ok(m) = serde_json::from_str::<serde_json::Value>(&t) {
                if m["version"].as_u64() == Some(CACHE_VERSION)
                    && m["pages"].as_u64() == Some(self.n as u64)
                    && m["fingerprint"].as_str() == Some(fp)
                    && self.fulltext_path().exists()
                    && self.overview_path().exists()
                    && (1..=self.n).all(|p| self.text_path(p).exists())
                {
                    *self.tokens.lock().unwrap() = m["tokens"].as_u64();
                    return Ok(());
                }
            }
        }
        // Bilder nur verwerfen, wenn sich das Dokument geändert hat (nicht bei neuer Cache-Version)
        let same_doc = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .map(|m| m["pages"].as_u64() == Some(self.n as u64) && m["fingerprint"].as_str() == Some(fp))
            .unwrap_or(false);
        if !same_doc {
            if let Ok(rd) = std::fs::read_dir(&self.img_dir) {
                for e in rd.flatten() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let mut full = String::new();
        let mut p = 0u32;
        while p < self.n {
            let to = (p + 20).min(self.n);
            let texts = pdfium_host::page_texts(bytes, p, to)?;
            for (k, t) in texts.into_iter().enumerate() {
                let pno = p + 1 + k as u32;
                write_atomic(&self.text_path(pno), t.as_bytes()).map_err(|e| e.to_string())?;
                if !full.is_empty() {
                    full.push_str("\n\n");
                }
                full.push_str(&format!("===== {} =====\n{}", self.page_name(pno), t));
            }
            p = to;
        }
        let tokens = (full.chars().count() as f64 / CHARS_PER_TOKEN) as u64;
        *self.tokens.lock().unwrap() = Some(tokens);
        write_atomic(&self.fulltext_path(), full.as_bytes()).map_err(|e| e.to_string())?;
        write_atomic(&self.overview_path(), self.overview(tokens).as_bytes()).map_err(|e| e.to_string())?;
        let meta = json!({"version": CACHE_VERSION, "pages": self.n, "tokens": tokens, "fingerprint": fp});
        write_atomic(&meta_path, meta.to_string().as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn overview(&self, tokens: u64) -> String {
        let fmt_num = |n: u64| {
            let s = n.to_string();
            let mut out = String::new();
            for (i, c) in s.chars().enumerate() {
                if i > 0 && (s.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(c);
            }
            out
        };
        let mut lines = vec![
            format!("# Overview: {}", self.title),
            String::new(),
            format!("File: {}", self.pdf_path.file_name().unwrap_or_default().to_string_lossy()),
            format!("Number of pages: {} (PDF pages 1–{})", self.n, self.n),
            format!("Estimated text length: about {} tokens", fmt_num(tokens)),
            String::new(),
            "## Printed page numbers".into(),
        ];
        lines.extend(self.label_ranges());
        lines.push(String::new());
        lines.push("## Table of contents (from the PDF's bookmarks)".into());
        if self.toc.is_empty() {
            lines.push("No bookmarks available. If needed, find sections by searching the page texts.".into());
        } else {
            for (lvl, title, page) in self.toc.iter().take(2500) {
                let mut wo = if *page >= 1 { format!("PDF page {page}") } else { "no page given".into() };
                if *page >= 1 && self.label(*page) != page.to_string() {
                    wo.push_str(&format!(", printed {}", self.label(*page)));
                }
                lines.push(format!("{}- {} → {}", "  ".repeat((*lvl).max(1) as usize - 1), title, wo));
            }
        }
        lines.join("\n") + "\n"
    }

    fn label_ranges(&self) -> Vec<String> {
        if (1..=self.n).all(|p| self.label(p) == p.to_string()) {
            return vec!["Printed page numbers match the PDF page numbers.".into()];
        }
        let num = |p: u32| self.label(p).parse::<i64>().ok();
        let mut out = vec![];
        let mut p = 1;
        while p <= self.n {
            match num(p) {
                Some(v) => {
                    let off = v - p as i64;
                    let mut q = p;
                    while q < self.n && num(q + 1).map(|x| x - (q + 1) as i64) == Some(off) {
                        q += 1;
                    }
                    if q > p {
                        out.push(format!("- PDF pages {p}–{q} = printed pages {}–{}", self.label(p), self.label(q)));
                    } else {
                        out.push(format!("- PDF page {p} = printed {}", self.label(p)));
                    }
                    p = q + 1;
                }
                None => {
                    out.push(format!("- PDF page {p} = printed {}", self.label(p)));
                    p += 1;
                }
            }
            if out.len() > 300 {
                out.push("- … (further mappings omitted)".into());
                break;
            }
        }
        out
    }

    fn build_images(&self, bytes: &[u8]) -> Result<(), String> {
        let mut missing: BTreeSet<u32> = (1..=self.n).filter(|p| !self.img_path(*p).exists()).collect();
        self.images_done.store(self.n - missing.len() as u32, Ordering::Relaxed);
        while !missing.is_empty() {
            let f = self.focus.load(Ordering::Relaxed) as i64;
            let mut order: Vec<u32> = missing.iter().copied().collect();
            order.sort_by_key(|q| ((*q as i64 - f).abs(), *q));
            let chunk: Vec<u32> = order.into_iter().take(CHUNK).collect();
            let idx: Vec<u32> = chunk.iter().map(|p| p - 1).collect();
            for (i, data) in pdfium_host::render_pages_jpeg(bytes, &idx, MAX_EDGE)? {
                let p = i + 1;
                if !self.img_path(p).exists() {
                    write_atomic(&self.img_path(p), &data).map_err(|e| e.to_string())?;
                }
                missing.remove(&p);
            }
            self.images_done.store(self.n - missing.len() as u32, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Bestimmte Seitenbilder sofort erzeugen (vor einer Frage).
    pub fn ensure_images(&self, pages: impl Iterator<Item = u32>) {
        let need: Vec<u32> = pages.filter(|p| *p >= 1 && *p <= self.n && !self.img_path(*p).exists()).collect();
        if need.is_empty() {
            return;
        }
        let Ok(bytes) = std::fs::read(&self.pdf_path) else { return };
        let idx: Vec<u32> = need.iter().map(|p| p - 1).collect();
        if let Ok(res) = pdfium_host::render_pages_jpeg(&bytes, &idx, MAX_EDGE) {
            for (i, data) in res {
                if !self.img_path(i + 1).exists() {
                    let _ = write_atomic(&self.img_path(i + 1), &data);
                }
            }
        }
    }

    pub fn wait_text(&self, secs: u64) -> bool {
        let (m, cv) = &self.text_ready;
        let g = m.lock().unwrap();
        let (g, _) = cv.wait_timeout_while(g, std::time::Duration::from_secs(secs), |ready| !*ready).unwrap();
        *g
    }

    pub fn text_ready(&self) -> bool {
        *self.text_ready.0.lock().unwrap()
    }

    pub fn page_text(&self, p: u32) -> String {
        std::fs::read_to_string(self.text_path(p)).unwrap_or_default()
    }
    pub fn fulltext(&self) -> String {
        std::fs::read_to_string(self.fulltext_path()).unwrap_or_default()
    }
    pub fn overview_text(&self) -> String {
        std::fs::read_to_string(self.overview_path()).unwrap_or_default()
    }

    pub fn status(&self) -> serde_json::Value {
        json!({
            "pages": self.n,
            "textReady": self.text_ready(),
            "tokens": *self.tokens.lock().unwrap(),
            "imagesDone": self.images_done.load(Ordering::Relaxed).min(self.n),
            "error": self.error.lock().unwrap().clone(),
        })
    }
}
