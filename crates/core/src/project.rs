//! A project is a folder in the workspace: the source, its transcript and analysis, reels, notes, renders.
use crate::config::Config;
use crate::proc::Media;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Word {
    pub w: String,
    pub s: f64,
    pub e: f64,
    #[serde(default)]
    pub p: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Words {
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub backend: String,
    pub words: Vec<Word>,
}

pub struct Project {
    pub dir: PathBuf,
    pub manifest: Value,
}

impl Project {
    pub fn open(cfg: &Config, key: &str) -> Result<Project> {
        if key.is_empty() {
            bail!("no project given. `projects` lists them.");
        }
        let as_path = Path::new(key);
        let dir = if as_path.is_absolute() && as_path.join("kesit.json").exists() {
            as_path.to_path_buf()
        } else {
            let d = cfg.workspace().join(key);
            if d.join("kesit.json").exists() {
                d
            } else {
                // by title or id prefix
                let hit = Self::dirs(cfg).into_iter().find(|d| {
                    let m = read_json_or(&d.join("kesit.json"), json!({}));
                    jsd(&m, "title", "").eq_ignore_ascii_case(key) || jsd(&m, "id", "").starts_with(key)
                });
                hit.ok_or_else(|| anyhow!("project '{key}' not found in {}. `projects` lists them.", cfg.workspace().display()))?
            }
        };
        let manifest = read_json(&dir.join("kesit.json"))?;
        Ok(Project { dir, manifest })
    }

    fn dirs(cfg: &Config) -> Vec<PathBuf> {
        let mut v = vec![];
        if let Ok(rd) = std::fs::read_dir(cfg.workspace()) {
            for e in rd.flatten() {
                if e.path().join("kesit.json").exists() {
                    v.push(e.path());
                }
            }
        }
        v
    }

    pub fn create(cfg: &Config, title: &str) -> Result<Project> {
        let ws = cfg.workspace();
        std::fs::create_dir_all(&ws)?;
        let base = slug(title);
        let mut id = base.clone();
        let mut n = 2;
        while ws.join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let dir = ws.join(&id);
        for d in ["assets", "build", "reels", "renders"] {
            std::fs::create_dir_all(dir.join(d))?;
        }
        let manifest = json!({ "id": id, "title": title, "created": now_ms(), "status": "new", "active_reel": "main" });
        let p = Project { dir, manifest };
        p.save_manifest()?;
        Ok(p)
    }

    pub fn list(cfg: &Config) -> Vec<Value> {
        let mut v: Vec<Value> = Self::dirs(cfg)
            .into_iter()
            .filter_map(|d| {
                let m = read_json(&d.join("kesit.json")).ok()?;
                let p = Project { dir: d, manifest: m };
                Some(p.summary())
            })
            .collect();
        v.sort_by(|a, b| jfd(b, "created", 0.0).partial_cmp(&jfd(a, "created", 0.0)).unwrap());
        v
    }

    pub fn id(&self) -> String {
        jsd(&self.manifest, "id", "").to_string()
    }
    pub fn title(&self) -> String {
        jsd(&self.manifest, "title", "").to_string()
    }
    pub fn p(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }
    pub fn save_manifest(&self) -> Result<()> {
        write_json(&self.dir.join("kesit.json"), &self.manifest)
    }
    pub fn set(&mut self, k: &str, v: Value) -> Result<()> {
        self.manifest[k] = v;
        self.save_manifest()
    }
    pub fn source(&self) -> Result<PathBuf> {
        let s = jsd(&self.manifest["source"], "path", "");
        if s.is_empty() {
            bail!("this project has no source video yet (import still running or failed)");
        }
        let p = if Path::new(s).is_absolute() { PathBuf::from(s) } else { self.dir.join(s) };
        if !p.exists() {
            bail!("the source video is missing: {}", p.display());
        }
        Ok(p)
    }
    pub fn media(&self) -> Media {
        Media::from_json(&self.manifest["media"])
    }

    // ---- reels ----
    pub fn reel_path(&self, rid: &str) -> PathBuf {
        if rid == "main" { self.p("reel.json") } else { self.p(&format!("reels/{rid}.json")) }
    }
    pub fn active_reel(&self) -> String {
        jsd(&self.manifest, "active_reel", "main").to_string()
    }
    pub fn rid(&self, want: Option<&str>) -> String {
        want.filter(|s| !s.is_empty()).map(|s| s.to_string()).unwrap_or_else(|| self.active_reel())
    }
    pub fn load_reel(&self, rid: &str) -> Result<Value> {
        let p = self.reel_path(rid);
        if !p.exists() {
            if rid == "main" {
                return Ok(default_reel(&self.title()));
            }
            bail!("reel '{rid}' not found. Reels: {}", self.reel_ids().join(", "));
        }
        read_json(&p)
    }
    pub fn save_reel(&self, rid: &str, reel: &Value) -> Result<()> {
        if !rid.contains("__") {
            crate::extra::remember(self, rid);
        }
        write_json(&self.reel_path(rid), reel)
    }
    pub fn reel_ids(&self) -> Vec<String> {
        let mut v = vec!["main".to_string()];
        if let Ok(rd) = std::fs::read_dir(self.p("reels")) {
            let mut ids: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(|s| s.to_string())).collect();
            ids.sort();
            v.extend(ids);
        }
        v
    }

    // ---- transcript ----
    pub fn words(&self) -> Result<Words> {
        let p = self.p("build/words.json");
        if !p.exists() {
            bail!("no transcript yet. Run `transcribe` first.");
        }
        Ok(serde_json::from_value(read_json(&p)?)?)
    }
    pub fn has(&self, rel: &str) -> bool {
        self.p(rel).exists()
    }

    // ---- notes ----
    pub fn notes(&self) -> Vec<Value> {
        read_json_or(&self.p("notes.json"), json!([])).as_array().cloned().unwrap_or_default()
    }
    pub fn save_notes(&self, n: &[Value]) -> Result<()> {
        write_json(&self.p("notes.json"), &json!(n))
    }

    pub fn renders(&self) -> Vec<Value> {
        let mut v = vec![];
        if let Ok(rd) = std::fs::read_dir(self.p("renders")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.ends_with(".mp4") {
                    let md = e.metadata().ok();
                    let report = e.path().with_extension("qa.json");
                    v.push(json!({
                        "file": n, "path": fwd(&e.path()),
                        "size": md.as_ref().map(|m| m.len()).unwrap_or(0),
                        "modified": md.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0),
                        "qa": if report.exists() { read_json_or(&report, Value::Null)["verdict"].clone() } else { Value::Null },
                    }));
                }
            }
        }
        v.sort_by(|a, b| jfd(b, "modified", 0.0).partial_cmp(&jfd(a, "modified", 0.0)).unwrap());
        v
    }

    pub fn summary(&self) -> Value {
        let m = &self.manifest;
        let thumb = self.p("assets/thumb.jpg");
        json!({
            "id": m["id"], "title": m["title"], "created": m["created"], "status": m["status"],
            "dir": fwd(&self.dir), "duration": m["media"]["duration"], "width": m["media"]["width"], "height": m["media"]["height"],
            "origin": m["source"]["origin"],
            "thumb": if thumb.exists() { json!(fwd(&thumb)) } else { Value::Null },
            "has_words": self.has("build/words.json"),
            "reels": self.reel_ids().len(), "renders": self.renders().len(),
            "open_notes": self.notes().iter().filter(|n| jsd(n, "status", "open") == "open").count(),
        })
    }
}

pub fn default_reel(title: &str) -> Value {
    json!({
        "title": title, "fps": 30, "width": 1080, "height": 1920, "platform": "instagram",
        "style": "clean", "takes": [], "crop": { "mode": "auto" },
        "captions": {}, "zoom": { "snaps": [], "pushes": [] }, "beats": [], "sfx": [], "outro": 0
    })
}

// ---- activity log: what every tool call did, from the app, the terminal or an agent ----

fn activity_path(cfg: &Config) -> PathBuf {
    cfg.workspace().join(".kesit").join("activity.jsonl")
}

pub fn log_activity(cfg: &Config, origin: &str, tool: &str, project: &str, ok: bool, summary: &str) {
    let p = activity_path(cfg);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let line = json!({"ts": now_ms(), "origin": origin, "tool": tool, "project": project, "ok": ok, "summary": summary});
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn read_activity(cfg: &Config, since: u64, limit: usize) -> Vec<Value> {
    let s = std::fs::read_to_string(activity_path(cfg)).unwrap_or_default();
    let mut v: Vec<Value> = s.lines().rev().take(2000).filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|e| jfd(e, "ts", 0.0) as u64 > since).collect();
    v.truncate(limit);
    v.reverse();
    v
}

// ---- taste: the user's corrections across reels (a prior; the reel always wins) ----

pub fn taste_path() -> PathBuf {
    crate::config::config_dir().join("taste.json")
}
pub fn taste() -> Value {
    read_json_or(&taste_path(), json!({"version": 1, "rules": []}))
}
