//! Font lookup and text measuring, so captions and cards can be laid out before libass draws them.
//! Template fonts live in library/fonts; when one is missing a Windows system face stands in.
use crate::util::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub struct FaceInfo {
    /// Name ID 1: the family name GDI and libass select by.
    pub family: String,
    /// Name ID 16 when present ("Montserrat" for "Montserrat ExtraBold").
    pub typo: String,
    pub weight: u16,
    pub italic: bool,
    pub bold_bit: bool,
    upm: f64,
    line: f64,
    data: Arc<Vec<u8>>,
    index: u32,
    pub path: PathBuf,
}

fn load_file(path: &Path, out: &mut Vec<FaceInfo>) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let data = Arc::new(bytes);
    let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    for index in 0..n {
        let Ok(face) = ttf_parser::Face::parse(&data, index) else { continue };
        let name = |id: u16| face.names().into_iter().filter(|n| n.name_id == id && n.is_unicode()).find_map(|n| n.to_string());
        let Some(family) = name(1) else { continue };
        let typo = name(16).unwrap_or_else(|| family.clone());
        let line = face.tables().os2.map(|t| t.windows_ascender() as f64 + (t.windows_descender() as f64).abs()).filter(|l| *l > 0.0).unwrap_or(face.ascender() as f64 - face.descender() as f64);
        out.push(FaceInfo { family, typo, weight: face.weight().to_number(), italic: face.is_italic(), bold_bit: face.is_bold(), upm: face.units_per_em() as f64, line, data: data.clone(), index, path: path.to_path_buf() });
    }
}

pub fn fonts_dir() -> PathBuf {
    library_dir().join("fonts")
}

const SYSTEM: &[&str] = &[
    "segoeui.ttf", "segoeuil.ttf", "segoeuisl.ttf", "seguisb.ttf", "segoeuib.ttf", "seguibl.ttf", "segoeuii.ttf", "segoeuiz.ttf", "arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf",
    "ariblk.ttf", "ARIALN.TTF", "ARIALNB.TTF", "georgia.ttf", "georgiab.ttf", "georgiai.ttf", "georgiaz.ttf", "impact.ttf", "consola.ttf", "consolab.ttf", "segoepr.ttf", "segoeprb.ttf", "Inkfree.ttf",
];
const TEMPLATE_PREFIXES: &[&str] = &["montserrat", "inter", "geist", "unbounded", "oswald", "playfair", "caveat", "jetbrains", "firasans", "fira"];

fn registry() -> &'static Vec<FaceInfo> {
    static REG: OnceLock<Vec<FaceInfo>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut v = vec![];
        let mut dirs = vec![fonts_dir()];
        if let Some(d) = dirs::data_local_dir() {
            dirs.push(d.join("Microsoft").join("Windows").join("Fonts"));
        }
        for d in dirs {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if ext == "ttf" || ext == "otf" || ext == "ttc" {
                        load_file(&e.path(), &mut v);
                    }
                }
            }
        }
        let win = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        if win.is_dir() {
            for f in SYSTEM {
                load_file(&win.join(f), &mut v);
            }
            if let Ok(rd) = std::fs::read_dir(&win) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if TEMPLATE_PREFIXES.iter().any(|p| n.starts_with(p)) && (n.ends_with(".ttf") || n.ends_with(".otf")) {
                        load_file(&e.path(), &mut v);
                    }
                }
            }
        }
        v
    })
}

fn fallback(family: &str) -> Option<&'static str> {
    Some(match family.to_lowercase().as_str() {
        "oswald" | "fira sans extra condensed" => "Arial Narrow",
        "arial narrow" => "Impact",
        "playfair display" | "great vibes" => "Georgia",
        "caveat" | "marck script" => "Segoe Print",
        "segoe print" => "Ink Free",
        "jetbrains mono" | "geist mono" | "ibm plex mono" => "Consolas",
        "segoe ui" => "Arial",
        "arial" | "georgia" | "consolas" | "impact" | "ink free" => return None,
        _ => "Segoe UI",
    })
}

#[derive(Clone)]
pub struct Font {
    /// What goes into \fn.
    pub name: String,
    pub bold: bool,
    pub italic: bool,
    pub wanted: String,
    pub substituted: bool,
    face: Option<&'static FaceInfo>,
}

pub fn pick(family: &str, weight: u16, italic: bool) -> Font {
    let reg = registry();
    let mut fam = family.to_string();
    for _ in 0..4 {
        let best = reg
            .iter()
            .filter(|f| f.typo.eq_ignore_ascii_case(&fam) || f.family.eq_ignore_ascii_case(&fam))
            .min_by_key(|f| (f.weight as i32 - weight as i32).abs() + if f.italic != italic { 1000 } else { 0 });
        if let Some(f) = best {
            return Font { name: f.family.clone(), bold: f.bold_bit, italic: f.italic || italic, wanted: family.into(), substituted: !fam.eq_ignore_ascii_case(family), face: Some(f) };
        }
        match fallback(&fam) {
            Some(n) => fam = n.to_string(),
            None => break,
        }
    }
    Font { name: "Arial".into(), bold: weight >= 600, italic, wanted: family.into(), substituted: true, face: None }
}

impl Font {
    /// ASS font size (line height) for a CSS-style em size in pixels.
    pub fn ass_size(&self, px: f64) -> f64 {
        match self.face {
            Some(f) => px * f.line / f.upm,
            None => px * 1.15,
        }
    }
    pub fn width(&self, text: &str, px: f64, tracking_em: f64) -> f64 {
        let n = text.chars().count() as f64;
        let Some(f) = self.face else { return n * px * 0.56 + n * tracking_em * px };
        let Ok(face) = ttf_parser::Face::parse(&f.data, f.index) else { return n * px * 0.56 };
        let mut units = 0.0;
        for c in text.chars() {
            units += face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map(|a| a as f64).unwrap_or(f.upm * 0.55);
        }
        units / f.upm * px + n * tracking_em * px
    }
    pub fn has_glyphs(&self, text: &str) -> bool {
        let Some(f) = self.face else { return true };
        let Ok(face) = ttf_parser::Face::parse(&f.data, f.index) else { return true };
        text.chars().all(|c| c.is_whitespace() || face.glyph_index(c).is_some())
    }
}

/// Which template families are really installed, for the doctor.
pub fn report() -> Vec<(String, bool, String)> {
    ["Montserrat", "Inter", "Inter Tight", "Geist", "Unbounded", "Oswald", "Playfair Display", "Caveat", "JetBrains Mono", "Fira Sans"]
        .iter()
        .map(|f| {
            let p = pick(f, 700, false);
            (f.to_string(), !p.substituted, p.name)
        })
        .collect()
}

/// A font by its exact name, with no lookup (emoji and other system faces).
pub fn raw(name: &str) -> Font {
    Font { name: name.into(), bold: false, italic: false, wanted: name.into(), substituted: false, face: None }
}
