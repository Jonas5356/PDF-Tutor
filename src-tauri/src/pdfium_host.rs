//! Zugriff auf PDFium (die PDF-Bibliothek von Chrome): Text, Seitenbilder, Vorschaubilder,
//! Inhaltsverzeichnis. Alle Aufrufe laufen nacheinander über eine Sperre.

use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use pdfium_render::prelude::*;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

pub fn init(dirs: &[PathBuf]) {
    PDFIUM.get_or_init(|| {
        let mut last = String::from("keine Bibliothek gefunden");
        for d in dirs {
            let lib = Pdfium::pdfium_platform_library_name_at_path(d);
            if !lib.exists() {
                continue;
            }
            match Pdfium::bind_to_library(&lib) {
                Ok(b) => {
                    crate::logf!("PDFium geladen: {}", lib.display());
                    return Ok(Pdfium::new(b));
                }
                Err(e) => last = format!("{}: {e:?}", lib.display()),
            }
        }
        match Pdfium::bind_to_system_library() {
            Ok(b) => Ok(Pdfium::new(b)),
            Err(_) => Err(format!("PDFium konnte nicht geladen werden ({last})")),
        }
    });
}

fn with<R>(f: impl FnOnce(&Pdfium) -> Result<R, String>) -> Result<R, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match PDFIUM.get() {
        Some(Ok(p)) => f(p),
        Some(Err(e)) => Err(e.clone()),
        None => Err("PDFium nicht initialisiert".into()),
    }
}

fn err<E: std::fmt::Debug>(e: E) -> String {
    format!("{e:?}")
}

pub struct DocInfo {
    pub pages: u32,
    pub title: String,
    pub labels: Vec<String>,
    /// (Ebene, Titel, PDF-Seite ab 1 oder 0)
    pub toc: Vec<(u32, String, u32)>,
}

fn collect_toc(b: PdfBookmark, level: u32, out: &mut Vec<(u32, String, u32)>, guard: &mut usize) {
    let mut cur = Some(b);
    while let Some(bm) = cur {
        *guard += 1;
        if *guard > 5000 {
            return;
        }
        let page = bm
            .destination()
            .and_then(|d| d.page_index().ok())
            .map(|i| i as u32 + 1)
            .unwrap_or(0);
        out.push((level, bm.title().unwrap_or_default().trim().to_string(), page));
        if let Some(child) = bm.first_child() {
            collect_toc(child, level + 1, out, guard);
        }
        cur = bm.next_sibling();
    }
}

pub fn doc_info(bytes: &[u8]) -> Result<DocInfo, String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let pages = doc.pages().len() as u32;
        let title = doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().to_string())
            .unwrap_or_default();
        let mut labels = Vec::with_capacity(pages as usize);
        for page in doc.pages().iter() {
            labels.push(page.label().unwrap_or("").to_string());
        }
        let mut toc = vec![];
        if let Some(root) = doc.bookmarks().root() {
            let mut guard = 0;
            collect_toc(root, 1, &mut toc, &mut guard);
        }
        Ok(DocInfo { pages, title, labels, toc })
    })
}

/// Text mehrerer Seiten (Index ab 0).
pub fn page_texts(bytes: &[u8], from: u32, to: u32) -> Result<Vec<String>, String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let mut out = vec![];
        for i in from..to {
            let page = doc.pages().get(i as i32).map_err(err)?;
            let t = page.text().map(|t| t.all()).unwrap_or_default();
            out.push(t.replace("\r\n", "\n").replace('\r', "\n").trim().to_string());
        }
        Ok(out)
    })
}

fn render_cfg(page: &PdfPage, max_edge: i32, annotations: bool) -> PdfRenderConfig {
    let w = page.width().value;
    let h = page.height().value;
    let cfg = PdfRenderConfig::new().render_annotations(annotations).render_form_data(annotations);
    if w >= h {
        cfg.set_target_width(max_edge)
    } else {
        cfg.set_target_height(max_edge)
    }
}

fn encode(img: DynamicImage, fmt: ImageFormat) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let rgb = DynamicImage::ImageRgb8(img.into_rgb8());
    rgb.write_to(&mut buf, fmt).map_err(err)?;
    Ok(buf.into_inner())
}

/// Seitenbilder als JPEG (Index ab 0) für den Seiten-Cache.
pub fn render_pages_jpeg(bytes: &[u8], idx: &[u32], max_edge: i32) -> Result<Vec<(u32, Vec<u8>)>, String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let mut out = vec![];
        for &i in idx {
            let page = doc.pages().get(i as i32).map_err(err)?;
            let cfg = render_cfg(&page, max_edge, false);
            let img = page.render_with_config(&cfg).map_err(err)?.as_image().map_err(err)?;
            out.push((i, encode(img, ImageFormat::Jpeg)?));
        }
        Ok(out)
    })
}

/// Vorschaubild der ersten Seite (JPEG) und Seitenzahl/Titel.
pub fn thumbnail(bytes: &[u8], width: i32) -> Result<(Vec<u8>, u32, String), String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let pages = doc.pages().len() as u32;
        let title = doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().to_string())
            .unwrap_or_default();
        let page = doc.pages().get(0).map_err(err)?;
        let cfg = PdfRenderConfig::new().set_target_width(width).render_annotations(true);
        let img = page.render_with_config(&cfg).map_err(err)?.as_image().map_err(err)?;
        Ok((encode(img, ImageFormat::Jpeg)?, pages, title))
    })
}

/// Seitenbild für Claude mit eingezeichneter Stelle (rote Umrandung um die Rechtecke
/// in PDF-Koordinaten). Die Markierung selbst ist als Anmerkung im PDF und wird mitgezeichnet.
pub fn render_mark_png(bytes: &[u8], page_idx: u32, rects: &[[f32; 4]], out: &Path) -> Result<(), String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let page = doc.pages().get(page_idx as i32).map_err(err)?;
        let cfg = render_cfg(&page, 1600, true);
        let img = page.render_with_config(&cfg).map_err(err)?.as_image().map_err(err)?;
        let mut rgb: RgbImage = img.into_rgb8();
        if !rects.is_empty() {
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for r in rects {
                for (px, py) in [(r[0], r[1]), (r[2], r[3]), (r[0], r[3]), (r[2], r[1])] {
                    if let Ok((x, y)) = page.points_to_pixels(PdfPoints::new(px), PdfPoints::new(py), &cfg) {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            if x0 < x1 && y0 < y1 {
                let (w, h) = (rgb.width() as i32, rgb.height() as i32);
                let red = Rgb([215u8, 25, 25]);
                for t in 0..3 {
                    let (a, b, c, d) = (x0 - 6 - t, y0 - 6 - t, x1 + 6 + t, y1 + 6 + t);
                    for x in a.max(0)..=c.min(w - 1) {
                        for y in [b, d] {
                            if y >= 0 && y < h {
                                rgb.put_pixel(x as u32, y as u32, red);
                            }
                        }
                    }
                    for y in b.max(0)..=d.min(h - 1) {
                        for x in [a, c] {
                            if x >= 0 && x < w {
                                rgb.put_pixel(x as u32, y as u32, red);
                            }
                        }
                    }
                }
            }
        }
        let data = encode(DynamicImage::ImageRgb8(rgb), ImageFormat::Png)?;
        crate::util::write_atomic(out, &data).map_err(err)
    })
}

/// Text innerhalb der Rechtecke einer Seite (für Markierungen aus anderen Programmen).
pub fn texts_in_rects(bytes: &[u8], items: &[(u32, Vec<[f32; 4]>)]) -> Result<Vec<String>, String> {
    with(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(err)?;
        let mut out = vec![];
        for (page_idx, rects) in items {
            let mut s = String::new();
            if let Ok(page) = doc.pages().get(*page_idx as i32) {
                if let Ok(text) = page.text() {
                    for r in rects {
                        let rect = PdfRect::new_from_values(r[1] - 1.0, r[0] - 1.0, r[3] + 1.0, r[2] + 1.0);
                        let t = text.inside_rect(rect);
                        if !s.is_empty() {
                            s.push(' ');
                        }
                        s.push_str(t.trim());
                    }
                }
            }
            out.push(s.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        Ok(out)
    })
}
