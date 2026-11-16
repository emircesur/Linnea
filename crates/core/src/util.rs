//! Small helpers shared by every module: JSON accessors, time formatting, file IO.
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn jf(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64())
}
pub fn jfd(v: &Value, k: &str, d: f64) -> f64 {
    jf(v, k).unwrap_or(d)
}
pub fn js<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}
pub fn jsd<'a>(v: &'a Value, k: &str, d: &'a str) -> &'a str {
    js(v, k).unwrap_or(d)
}
pub fn jb(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}
pub fn ja<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

/// RFC 7386 JSON merge patch: objects merge, null deletes, everything else replaces.
pub fn merge(base: &mut Value, patch: &Value) {
    match (base.as_object_mut(), patch.as_object()) {
        (Some(b), Some(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    b.remove(k);
                } else if v.is_object() && b.get(k).map(|x| x.is_object()).unwrap_or(false) {
                    merge(b.get_mut(k).unwrap(), v);
                } else {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        _ => *base = patch.clone(),
    }
}

pub fn read_json(p: &Path) -> Result<Value> {
    let s = std::fs::read_to_string(p).with_context(|| format!("cannot read {}", p.display()))?;
    let s = s.trim_start_matches('\u{feff}');
    serde_json::from_str(s).with_context(|| format!("invalid JSON in {}", p.display()))
}
pub fn read_json_or(p: &Path, d: Value) -> Value {
    read_json(p).unwrap_or(d)
}
/// Write through a temp file so a reader never sees a half-written file.
pub fn write_json(p: &Path, v: &Value) -> Result<()> {
    write_text(p, &serde_json::to_string_pretty(v)?)
}
pub fn write_text(p: &Path, s: &str) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension(format!("tmp{}", fastrand::u32(..)));
    std::fs::write(&tmp, s)?;
    if std::fs::rename(&tmp, p).is_err() {
        std::fs::copy(&tmp, p)?;
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
pub fn new_id(prefix: &str) -> String {
    format!("{}{:x}{:03x}", prefix, now_ms() / 1000, fastrand::u32(..4096))
}

/// 83.4 -> "1:23.4"
pub fn fmt_t(t: f64) -> String {
    let t = t.max(0.0);
    let m = (t / 60.0).floor() as i64;
    format!("{}:{:04.1}", m, t - m as f64 * 60.0)
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = match c {
            'ç' | 'Ç' => 'c',
            'ğ' | 'Ğ' => 'g',
            'ı' | 'İ' => 'i',
            'ö' | 'Ö' => 'o',
            'ş' | 'Ş' => 's',
            'ü' | 'Ü' => 'u',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    let out: String = out.chars().take(48).collect();
    if out.is_empty() { "video".into() } else { out }
}

/// Forward slashes, for JSON and ffmpeg arguments.
pub fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| PathBuf::from("."))
}

/// Find a resource folder: env override, next to the exe, then any ancestor of the exe or cwd.
pub fn find_resource(env: &str, rel: &str, marker: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(env) {
        let p = PathBuf::from(p);
        if p.join(marker).exists() {
            return Some(p);
        }
    }
    let mut starts = vec![exe_dir()];
    if let Ok(c) = std::env::current_dir() {
        starts.push(c);
    }
    for s in starts {
        let mut cur: Option<&Path> = Some(&s);
        while let Some(d) = cur {
            let c = d.join(rel);
            if c.join(marker).exists() {
                return Some(c);
            }
            cur = d.parent();
        }
    }
    None
}

pub fn library_dir() -> PathBuf {
    find_resource("KESIT_LIBRARY", "library", "styles").unwrap_or_else(|| exe_dir().join("library"))
}

pub fn clampf(x: f64, a: f64, b: f64) -> f64 {
    x.max(a).min(b)
}

pub fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(v[v.len() / 2])
}

/// Rename that survives Windows briefly locking a fresh file (antivirus, indexer, a player): it tries
/// again for a few seconds, then copies. A folder that stays locked is moved file by file.
pub fn rename_retry<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> std::io::Result<()> {
    let (f, t) = (from.as_ref(), to.as_ref());
    let mut last = None;
    for i in 0..25u64 {
        match std::fs::rename(f, t) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(80 + 20 * i));
            }
        }
    }
    if f.is_file() {
        std::fs::copy(f, t)?;
        let _ = std::fs::remove_file(f);
        return Ok(());
    }
    if f.is_dir() {
        std::fs::create_dir_all(t)?;
        for e in std::fs::read_dir(f)?.flatten() {
            let to = t.join(e.file_name());
            if std::fs::rename(e.path(), &to).is_err() && e.path().is_file() {
                let _ = std::fs::copy(e.path(), &to);
                let _ = std::fs::remove_file(e.path());
            }
        }
        let _ = std::fs::remove_dir_all(f);
        return Ok(());
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}
