//! Settings on disk, the external tools (ffmpeg, yt-dlp, whisper.cpp) and where things live.
use crate::util::*;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A folder that was called Kesit before the rename: moved to its Linnea name once, or kept where it is when
/// the move is not possible (a file in use), so nothing the user made is left behind.
pub fn renamed(base: &Path, old: &str, new: &str) -> PathBuf {
    let (o, n) = (base.join(old), base.join(new));
    if !n.exists() && o.is_dir() && std::fs::rename(&o, &n).is_err() {
        return o;
    }
    n
}
fn home_env() -> Option<PathBuf> {
    std::env::var("LINNEA_HOME").or_else(|_| std::env::var("KESIT_HOME")).ok().map(PathBuf::from)
}
pub fn config_dir() -> PathBuf {
    home_env().unwrap_or_else(|| renamed(&dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")), "Kesit", "Linnea"))
}
pub fn data_dir() -> PathBuf {
    home_env().unwrap_or_else(|| renamed(&dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")), "Kesit", "Linnea"))
}
/// Linnea's own state inside a folder (the workspace or a project): `.linnea`, formerly `.kesit`.
pub fn state_dir(d: &Path) -> PathBuf {
    renamed(d, ".kesit", ".linnea")
}

fn defaults() -> Value {
    let videos = dirs::video_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
    // the user's films stay where an earlier version put them (Videos\Kesit); they are never moved
    let ws = if !videos.join("Linnea").exists() && videos.join("Kesit").is_dir() { videos.join("Kesit") } else { videos.join("Linnea") };
    json!({
        "workspace": fwd(&ws),
        "port": 7790,
        "tools": {},
        "transcribe": { "backend": "whisper.cpp", "model": "", "lang": "auto", "threads": 0 },
        "llm": { "provider": "none", "base_url": "http://127.0.0.1:11434/v1", "model": "" },
        "keys": {},
        "render": { "encoder": "auto" },
        "ui": { "theme": "grafit" }
    })
}

#[derive(Clone)]
pub struct Config(pub Value);

impl Config {
    pub fn load() -> Config {
        let mut v = defaults();
        if let Ok(user) = read_json(&config_dir().join("config.json")) {
            merge(&mut v, &user);
        }
        Config(v)
    }
    pub fn save(&self) -> Result<()> {
        write_json(&config_dir().join("config.json"), &self.0)
    }
    /// Dotted lookup: `get("transcribe.backend")`.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let mut cur = &self.0;
        for k in path.split('.') {
            cur = cur.get(k)?;
        }
        Some(cur)
    }
    pub fn str(&self, path: &str) -> String {
        self.get(path).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
    pub fn workspace(&self) -> PathBuf {
        PathBuf::from(self.str("workspace"))
    }
    /// The workspace's own state folder (jobs, focus, activity).
    pub fn state(&self) -> PathBuf {
        state_dir(&self.workspace())
    }
    /// An API key from settings, else from the usual environment variable.
    pub fn key(&self, name: &str) -> Option<String> {
        let k = self.str(&format!("keys.{name}"));
        if !k.is_empty() {
            return Some(k);
        }
        let env = match name {
            "openai" => "OPENAI_API_KEY",
            "groq" => "GROQ_API_KEY",
            "elevenlabs" => "ELEVENLABS_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "pexels" => "PEXELS_API_KEY",
            "pixabay" => "PIXABAY_API_KEY",
            "heygen" => "HEYGEN_API_KEY",
            "gemini" => "GEMINI_API_KEY",
            "sieve" => "SIEVE_API_KEY",
            "tiktok" => "TIKTOK_ACCESS_TOKEN",
            "instagram" => "INSTAGRAM_ACCESS_TOKEN",
            _ => return None,
        };
        std::env::var(env).ok().filter(|s| !s.is_empty())
    }
    /// Settings for display: keys are never returned, only whether they are set.
    pub fn masked(&self) -> Value {
        let mut v = self.0.clone();
        let mut keys = serde_json::Map::new();
        for n in KEYS {
            keys.insert(n.to_string(), json!(self.key(n).is_some()));
        }
        v["keys"] = Value::Object(keys);
        v
    }
}

pub const KEYS: &[&str] = &["openai", "groq", "elevenlabs", "anthropic", "pexels", "pixabay", "heygen", "gemini", "sieve", "tiktok", "instagram", "youtube_client_id", "youtube_client_secret", "youtube_refresh"];

// ---------- external tools ----------

fn exe_names(tool: &str) -> Vec<String> {
    let base: Vec<&str> = match tool {
        "whisper" => vec!["whisper-cli", "whisper-cpp", "whisper"],
        "yt-dlp" => vec!["yt-dlp"],
        t => vec![t],
    };
    let mut out = vec![];
    for b in base {
        if cfg!(windows) {
            out.push(format!("{b}.exe"));
        }
        out.push(b.to_string());
    }
    out
}

fn search_dir(dir: &Path, names: &[String], depth: u32) -> Option<PathBuf> {
    for n in names {
        let p = dir.join(n);
        if p.is_file() {
            return Some(p);
        }
    }
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().is_dir() {
            if let Some(p) = search_dir(&e.path(), names, depth - 1) {
                return Some(p);
            }
        }
    }
    None
}

pub fn tools_dirs() -> Vec<PathBuf> {
    let mut v = vec![data_dir().join("tools")];
    if let Some(p) = find_resource("LINNEA_TOOLS", "tools", ".") {
        v.push(p);
    }
    v
}

/// Settings override, Linnea's own tools folders, then PATH.
pub fn find_tool(cfg: &Config, tool: &str) -> Option<PathBuf> {
    let o = cfg.str(&format!("tools.{tool}"));
    if !o.is_empty() && Path::new(&o).is_file() {
        return Some(PathBuf::from(o));
    }
    let names = exe_names(tool);
    for d in tools_dirs() {
        if d.is_dir() {
            if let Some(p) = search_dir(&d, &names, 3) {
                return Some(p);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    for d in std::env::split_paths(&path) {
        for n in &names {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn need_tool(cfg: &Config, tool: &str) -> Result<PathBuf> {
    find_tool(cfg, tool).ok_or_else(|| {
        anyhow::anyhow!("{tool} is not installed. Run the `doctor` tool for the fix, or set tools.{tool} in settings to its full path.")
    })
}

pub fn models_dirs() -> Vec<PathBuf> {
    let mut v = vec![data_dir().join("models")];
    for d in tools_dirs() {
        v.push(d.join("models"));
    }
    v
}

/// A whisper.cpp ggml model: the configured path or name, else the largest one found.
pub fn find_whisper_model(cfg: &Config, want: Option<&str>) -> Option<PathBuf> {
    let want = want.map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| cfg.str("transcribe.model"));
    if !want.is_empty() && Path::new(&want).is_file() {
        return Some(PathBuf::from(want));
    }
    let mut found: Vec<PathBuf> = vec![];
    for d in models_dirs() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("ggml-") && n.ends_with(".bin") {
                    if !want.is_empty() && n == format!("ggml-{want}.bin") {
                        return Some(e.path());
                    }
                    found.push(e.path());
                }
            }
        }
    }
    found.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    found.pop()
}

pub fn find_face_model() -> Option<PathBuf> {
    for d in models_dirs() {
        let p = d.join("seeta_fd_frontal_v1.0.bin");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
