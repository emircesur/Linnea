//! Tools that fill the gaps against the market editors: installing the media tools from inside the app,
//! caption files (SRT/VTT), batch renders of every clip in several shapes, undo, and saving a brand style.
use crate::analyze;
use crate::config::*;
use crate::proc::{self, Rep};
use crate::project::Project;
use crate::render::{self, Mode};
use crate::styles;
use crate::timeline::Timeline;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// ---------- downloading the tools ----------

fn download(url: &str, to: &Path, rep: &Rep, label: &str) -> Result<()> {
    let agent = crate::net::agent_with(3600, true);
    let host = url.split('/').nth(2).unwrap_or(url).to_string();
    let resp = match agent.get(url).header("User-Agent", "linnea").call() {
        Ok(r) => r,
        Err(e) if e.to_string().contains("UnknownIssuer") || e.to_string().to_lowercase().contains("certificate") => crate::net::agent_webpki(3600)
            .get(url)
            .header("User-Agent", "linnea")
            .call()
            .map_err(|e2| anyhow!("download {label} from {host}: {e2}. The certificate was not trusted; an antivirus or proxy that inspects HTTPS can cause this"))?,
        Err(e) => bail!("download {label} from {host}: {e}"),
    };
    let total: f64 = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let mut r = resp.into_body().into_reader();
    let part = to.with_extension("part");
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut f = std::fs::File::create(&part)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut got = 0f64;
    loop {
        rep.check()?;
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
        got += n as f64;
        if total > 0.0 {
            rep.p(got / total, &format!("{label} {:.0}/{:.0} MB", got / 1e6, total / 1e6));
        }
    }
    drop(f);
    if got < 1000.0 {
        bail!("download {label}: the file came back empty");
    }
    crate::util::rename_retry(&part, to)?;
    Ok(())
}

fn unzip(zip: &Path, into: &Path) -> Result<()> {
    std::fs::create_dir_all(into)?;
    let tar = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("System32").join("tar.exe");
    let o = proc::run(proc::cmd(&tar).arg("-xf").arg(zip).arg("-C").arg(into))?;
    if !o.ok {
        bail!("unpacking {}: {}", zip.display(), proc::err_tail(&o.stderr, 3));
    }
    let _ = std::fs::remove_file(zip);
    Ok(())
}

fn has_nvidia() -> bool {
    proc::run(proc::cmd(Path::new("nvidia-smi")).arg("-L")).map(|o| o.ok && o.stdout.contains("GPU")).unwrap_or(false)
}

fn setup_tools(c: &Ctx, a: &Value) -> Result<Value> {
    let tools = data_dir().join("tools");
    let models = data_dir().join("models");
    let mut want: Vec<String> = ja(a, "what").iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    if want.is_empty() {
        for t in ["ffmpeg", "yt-dlp", "whisper"] {
            if find_tool(&c.cfg, t).is_none() {
                want.push(t.into());
            }
        }
        if find_whisper_model(&c.cfg, None).is_none() {
            want.push("model".into());
        }
        if find_face_model().is_none() {
            want.push("face".into());
        }
        if crate::matte::pick(&c.cfg, "").is_none() {
            want.push("matte".into());
        }
    }
    let n = want.len().max(1) as f64;
    let mut done = vec![];
    for (i, w) in want.iter().enumerate() {
        let rep = c.rep.sub(i as f64 / n, (i + 1) as f64 / n);
        match w.as_str() {
            "ffmpeg" => {
                let z = tools.join("ffmpeg.zip");
                download("https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip", &z, &rep, "ffmpeg")?;
                unzip(&z, &tools)?;
            }
            "yt-dlp" => download("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", &tools.join("yt-dlp.exe"), &rep, "yt-dlp")?,
            "whisper" => {
                let file = if has_nvidia() { "whisper-bin-win-cuda-12.4.0-x64.zip" } else { "whisper-bin-x64.zip" };
                let z = tools.join("whisper.zip");
                download(&format!("https://github.com/ggml-org/whisper.cpp/releases/download/b5454/{file}"), &z, &rep, "whisper.cpp")?;
                unzip(&z, &tools.join("whisper"))?;
            }
            m if m == "model" || m.starts_with("model:") => {
                let name = m.strip_prefix("model:").unwrap_or("large-v3-turbo");
                download(&format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin"), &models.join(format!("ggml-{name}.bin")), &rep, &format!("whisper {name}"))?;
            }
            "face" => download("https://github.com/atomashpolskiy/rustface/raw/master/model/seeta_fd_frontal_v1.0.bin", &models.join("seeta_fd_frontal_v1.0.bin"), &rep, "face model")?,
            m if m == "matte" || m.starts_with("matte:") => {
                let id = m.strip_prefix("matte:").unwrap_or("rvm");
                let mi = crate::matte::model(id).ok_or_else(|| anyhow!("unknown matte model '{id}'"))?;
                download(mi.url, &models.join(mi.file), &rep, &format!("matte {id}"))?;
            }
            "piper" => {
                let z = tools.join("piper.zip");
                download("https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip", &z, &rep, "piper")?;
                unzip(&z, &tools)?;
            }
            v if v.starts_with("voice:") => {
                let k = &v[6..];
                let path = crate::ai::PIPER_VOICES.iter().find(|(l, _)| *l == k).map(|x| x.1.to_string()).unwrap_or_else(|| k.to_string());
                let name = path.rsplit('/').next().unwrap_or(k).to_string();
                let base = format!("https://huggingface.co/rhasspy/piper-voices/resolve/main/{path}");
                download(&format!("{base}.onnx"), &models.join("piper").join(format!("{name}.onnx")), &rep, &format!("voice {name}"))?;
                download(&format!("{base}.onnx.json"), &models.join("piper").join(format!("{name}.onnx.json")), &rep.sub(0.99, 1.0), "voice config")?;
            }
            other => bail!("unknown tool '{other}': ffmpeg, yt-dlp, whisper, model[:name], face, matte[:rvm|rvm-hq|birefnet-lite|birefnet-portrait], piper, voice:<lang>"),
        }
        done.push(w.clone());
    }
    Ok(json!({"installed": done, "tools_folder": fwd(&tools), "models_folder": fwd(&models)}))
}

// ---------- caption files ----------

fn stamp(t: f64, vtt: bool) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as i64;
    format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, if vtt { '.' } else { ',' }, ms % 1000)
}

fn export_subs(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    let reel = styles::resolve(&p.load_reel(&rid)?);
    let tl = Timeline::from_reel(&reel, p.media().duration);
    let words = tl.edit_words(&p.words()?.words);
    let group = jf(a, "group").unwrap_or_else(|| jfd(&reel["_cap"], "group", 5.0).max(3.0)) as usize;
    let vtt = jsd(a, "format", "srt") == "vtt";
    let mut blocks: Vec<(f64, f64, String)> = vec![];
    let mut cur: Vec<&crate::timeline::EWord> = vec![];
    for w in &words {
        let brk = cur.last().map(|l| w.s - l.e > 0.6 || l.text.ends_with(['.', '?', '!'])).unwrap_or(false);
        if !cur.is_empty() && (cur.len() >= group || brk) {
            blocks.push((cur[0].s, cur.last().unwrap().e, cur.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ")));
            cur.clear();
        }
        cur.push(w);
    }
    if !cur.is_empty() {
        blocks.push((cur[0].s, cur.last().unwrap().e, cur.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ")));
    }
    let mut out = if vtt { "WEBVTT\n\n".to_string() } else { String::new() };
    for (i, (s, e, t)) in blocks.iter().enumerate() {
        if !vtt {
            out.push_str(&format!("{}\n", i + 1));
        }
        out.push_str(&format!("{} --> {}\n{}\n\n", stamp(*s, vtt), stamp(e.max(s + 0.3), vtt), t));
    }
    let file = p.p(&format!("renders/{}{}.{}", slug(jsd(&reel, "title", &p.title())), if rid == "main" { String::new() } else { format!("-{rid}") }, if vtt { "vtt" } else { "srt" }));
    write_text(&file, &out)?;
    Ok(json!({"file": fwd(&file), "captions": blocks.len()}))
}

// ---------- batch render ----------

fn batch(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let reels: Vec<String> = match a.get("reels") {
        Some(Value::Array(v)) => v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect(),
        _ => p.reel_ids().into_iter().filter(|r| r != "main" || p.reel_ids().len() == 1).collect(),
    };
    let formats: Vec<String> = ja(a, "formats").iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    let formats = if formats.is_empty() { vec!["native".to_string()] } else { formats };
    let mode = if jsd(a, "mode", "final") == "preview" { Mode::Preview } else { Mode::Final };
    let jobs: Vec<(String, String)> = reels.iter().flat_map(|r| formats.iter().map(move |f| (r.clone(), f.clone()))).collect();
    let n = jobs.len().max(1) as f64;
    let mut out = vec![];
    for (i, (rid, f)) in jobs.iter().enumerate() {
        let rep = c.rep.sub(i as f64 / n, (i + 1) as f64 / n);
        rep.p(0.0, &format!("{rid} {f}"));
        let size = match f.as_str() {
            "9:16" => Some((1080, 1920)),
            "1:1" => Some((1080, 1080)),
            "4:5" => Some((1080, 1350)),
            "16:9" => Some((1920, 1080)),
            _ => None,
        };
        let (use_rid, tmp) = match size {
            Some((w, h)) => {
                let mut reel = p.load_reel(rid)?;
                if jfd(&reel, "width", 1080.0) as i64 == w && jfd(&reel, "height", 1920.0) as i64 == h {
                    (rid.clone(), None)
                } else {
                    reel["width"] = json!(w);
                    reel["height"] = json!(h);
                    reel["title"] = json!(format!("{} {}", jsd(&reel, "title", rid), f.replace(':', "x")));
                    let t = format!("{rid}__{}", f.replace(':', "x"));
                    p.save_reel(&t, &reel)?;
                    (t.clone(), Some(t))
                }
            }
            None => (rid.clone(), None),
        };
        let r = render::render(&c.cfg, &p, &use_rid, mode, &rep);
        if let Some(t) = tmp {
            let _ = std::fs::remove_file(p.reel_path(&t));
        }
        out.push(match r {
            Ok(v) => json!({"reel": rid, "format": f, "file": v["file"], "qa": v["qa"]["verdict"]}),
            Err(e) => {
                if e.to_string() == "cancelled" {
                    return Err(e);
                }
                json!({"reel": rid, "format": f, "error": e.to_string()})
            }
        });
    }
    Ok(json!({"renders": out}))
}

// ---------- undo ----------

fn hist_dir(p: &Project, rid: &str, redo: bool) -> PathBuf {
    p.p(&format!("build/history/{rid}{}", if redo { "/redo" } else { "" }))
}

/// Called before every reel save: keep the previous version (last 60).
pub fn remember(p: &Project, rid: &str) {
    let cur = p.reel_path(rid);
    if !cur.exists() {
        return;
    }
    let d = hist_dir(p, rid, false);
    if std::fs::create_dir_all(&d).is_err() {
        return;
    }
    let _ = std::fs::copy(&cur, d.join(format!("{:013}.json", now_ms())));
    if let Ok(rd) = std::fs::read_dir(&d) {
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|x| x.is_file()).collect();
        v.sort();
        for old in v.iter().take(v.len().saturating_sub(60)) {
            let _ = std::fs::remove_file(old);
        }
    }
}

fn undo(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    let redo = jb(a, "redo") == Some(true);
    let (from, to) = (hist_dir(&p, &rid, redo), hist_dir(&p, &rid, !redo));
    let mut v: Vec<PathBuf> = std::fs::read_dir(&from).map(|rd| rd.flatten().map(|e| e.path()).filter(|x| x.is_file()).collect()).unwrap_or_default();
    v.sort();
    let last = v.pop().ok_or_else(|| anyhow!("nothing to {}", if redo { "redo" } else { "undo" }))?;
    std::fs::create_dir_all(&to)?;
    let cur = p.reel_path(&rid);
    if cur.exists() {
        std::fs::copy(&cur, to.join(format!("{:013}.json", now_ms())))?;
    }
    std::fs::copy(&last, &cur)?;
    std::fs::remove_file(&last)?;
    Ok(json!({"reel": rid, "restored": last.file_name().map(|n| n.to_string_lossy().to_string()), "left": v.len()}))
}

// ---------- brand style ----------

fn style_save(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    let reel = p.load_reel(&rid)?;
    let name = js(a, "name").ok_or_else(|| anyhow!("`name` is the new style's name"))?;
    let base = styles::load_preset(jsd(&reel, "style", "clean")).unwrap_or(json!({}));
    let mut out = base.clone();
    for k in ["brand", "captions", "sfxProfile", "look", "music", "scenes"] {
        if let Some(v) = reel.get(k) {
            let mut b = out.get(k).cloned().unwrap_or(json!({}));
            if v.is_object() && b.is_object() {
                merge(&mut b, v);
            } else {
                b = v.clone();
            }
            out[k] = b;
        }
    }
    if let Some(o) = out["captions"].as_object_mut() {
        for k in ["fixes", "phrases", "keywords", "highlight", "tags"] {
            o.remove(k); // per-recording, never part of a style
        }
    }
    out["_name"] = json!(name);
    out["_about"] = json!(jsd(a, "about", &format!("Saved from {} / {}", p.title(), rid)));
    out["_recipe"] = Value::Null;
    let id = slug(name);
    let file = library_dir().join("styles").join("user").join(format!("{id}.json"));
    write_json(&file, &out)?;
    Ok(json!({"style": id, "file": fwd(&file)}))
}


fn matte_tool(c: &Ctx, a: &Value) -> Result<Value> {
    let src = match js(a, "file") {
        Some(f) => PathBuf::from(f),
        None => Project::open(&c.cfg, jsd(a, "project", ""))?.source()?,
    };
    let (from, to) = (jfd(a, "from", 0.0), jfd(a, "to", jfd(a, "from", 0.0) + 3.0));
    let out = PathBuf::from(js(a, "out").map(|s| s.to_string()).unwrap_or_else(|| fwd(&src.with_extension("matte.mkv"))));
    let t0 = std::time::Instant::now();
    crate::matte::matte_range(&c.cfg, &src, from, to, jfd(a, "fps", 30.0), jsd(a, "model", ""), &out, &c.rep)?;
    let secs = t0.elapsed().as_secs_f64();
    Ok(json!({"matte": fwd(&out), "seconds": (secs * 100.0).round() / 100.0, "frames": ((to - from) * jfd(a, "fps", 30.0)).round(), "models": crate::matte::status()}))
}

fn trash(c: &Ctx, a: &Value) -> Result<Value> {
    let p = PathBuf::from(jsd(a, "path", ""));
    let ws = c.cfg.workspace().canonicalize().unwrap_or_default();
    let cp = p.canonicalize().map_err(|_| anyhow!("no such path"))?;
    if !cp.starts_with(&ws) || cp == ws {
        bail!("only files inside the workspace can be sent to the Recycle Bin from here");
    }
    crate::manage::recycle(&cp)?;
    Ok(json!({"trashed": fwd(&cp)}))
}

pub const TOOLS: &[Tool] = &[
    Tool { name: "trash", desc: "Move a file or folder inside the workspace to the Recycle Bin (never deletes outright).", params: &[("path!", "string", "Path")], long: false, app_only: true, run: trash },
    Tool { name: "matte", desc: "Cut the person out of a stretch of video (a project source or any `file`): a gray matte video (white = person). Local, on the GPU. model: rvm (default, fast and steady), rvm-hq, birefnet-lite, birefnet-portrait (ghost-editor's).", params: &[("project", "string", "Project"), ("file", "string", "Any video instead"), ("from", "number", "Start s"), ("to", "number", "End s"), ("model", "string", "rvm | rvm-hq | birefnet-lite | birefnet-portrait"), ("out", "string", "Output .mkv")], long: true, app_only: false, run: matte_tool },
    Tool { name: "setup_tools", desc: "Download the media tools into Linnea's own folder: ffmpeg, yt-dlp, whisper (whisper.cpp, the CUDA build when an NVIDIA card is present), model (ggml large-v3-turbo; model:small for a smaller one), face (the face model). With no `what` it fetches whatever is missing.", params: &[("what", "array", "ffmpeg | yt-dlp | whisper | model[:name] | face | matte[:id] | piper | voice:<lang>")], long: true, app_only: false, run: setup_tools },
    Tool { name: "export_subs", desc: "Write the reel's captions as an SRT or VTT file (edit times) into renders/, for platforms that take a caption file.", params: &[("project!", "string", "Project"), ("reel", "string", "Reel id"), ("format", "string", "srt | vtt"), ("group", "number", "Words per caption")], long: false, app_only: false, run: export_subs },
    Tool { name: "render_batch", desc: "Render several reels (default: every clip reel) in one or more shapes (9:16, 1:1, 4:5, 16:9; default the reel's own). Each render is checked. Only when the user asked for renders.", params: &[("project!", "string", "Project"), ("reels", "array", "Reel ids"), ("formats", "array", "9:16 | 1:1 | 4:5 | 16:9"), ("mode", "string", "final | preview")], long: true, app_only: false, run: batch },
    Tool { name: "reel_undo", desc: "Undo the last change to a reel (every save keeps the previous version); redo=true steps forward again.", params: &[("project!", "string", "Project"), ("reel", "string", "Reel id"), ("redo", "boolean", "Redo instead")], long: false, app_only: false, run: undo },
    Tool { name: "style_save", desc: "Save this reel's look (brand colours and font, caption template and settings, sound profile, look) as a new style the whole library can use: a brand kit.", params: &[("project!", "string", "Project"), ("reel", "string", "Reel id"), ("name!", "string", "Style name"), ("about", "string", "One line")], long: false, app_only: false, run: style_save },
];

/// Silences the unused-import lint when the face tracker is compiled out.
#[allow(dead_code)]
fn _keep() {
    let _ = analyze::sentences;
}
