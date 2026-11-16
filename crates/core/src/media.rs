//! Getting a video in (a file or a link through yt-dlp), preparing it, and transcribing it to word times.
use crate::config::*;
use crate::net;
use crate::proc::{self, Rep};
use crate::project::{Project, Word, Words};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn s(x: impl AsRef<str>) -> String {
    x.as_ref().to_string()
}
pub fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

/// The H.264 encoder to use: NVENC when the settings allow it and a test encode works, else x264.
pub fn h264_encoder(cfg: &Config) -> &'static str {
    static NV: OnceLock<bool> = OnceLock::new();
    let want = cfg.str("render.encoder");
    if want == "x264" {
        return "libx264";
    }
    let ok = *NV.get_or_init(|| {
        let Some(ff) = find_tool(cfg, "ffmpeg") else { return false };
        proc::run(proc::cmd(&ff).args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.2:r=30", "-c:v", "h264_nvenc", "-f", "null", "-"]))
            .map(|o| o.ok)
            .unwrap_or(false)
    });
    if ok { "h264_nvenc" } else { "libx264" }
}

pub fn is_url(src: &str) -> bool {
    src.starts_with("http://") || src.starts_with("https://")
}

/// Create a project from a local file or a link, then prepare it (proxy, thumbnail, audio, waveform).
pub fn import(cfg: &Config, source: &str, title: Option<&str>, rep: &Rep) -> Result<Project> {
    let source = source.trim().trim_matches('"');
    let ffprobe = need_tool(cfg, "ffprobe")?;
    let mut p;
    if is_url(source) {
        let ytdlp = need_tool(cfg, "yt-dlp")?;
        let ffmpeg = need_tool(cfg, "ffmpeg")?;
        rep.p(0.01, "reading the link");
        let t = match title {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                let o = proc::run(proc::cmd(&ytdlp).args(["--no-playlist", "--skip-download", "--no-warnings", "--print", "title", source]))?;
                if !o.ok {
                    bail!("yt-dlp cannot read this link: {}", proc::err_tail(&o.stderr, 3));
                }
                o.stdout.lines().next().unwrap_or("video").trim().to_string()
            }
        };
        p = Project::create(cfg, &t)?;
        p.manifest["source"] = json!({"origin": "url", "url": source});
        p.manifest["status"] = json!("downloading");
        p.save_manifest()?;
        let out_tpl = fwd(&p.p("source.%(ext)s"));
        let mut c = proc::cmd(&ytdlp);
        c.args(["--no-playlist", "--newline", "--no-warnings", "-f", "bv*[height<=1080]+ba/b[height<=1080]/b", "--merge-output-format", "mp4", "-o", &out_tpl]);
        if let Some(d) = ffmpeg.parent() {
            c.arg("--ffmpeg-location").arg(d);
        }
        c.arg(source);
        let dl = rep.sub(0.02, 0.6);
        let o = proc::run_lines(&mut c, rep, |line| {
            if let Some(i) = line.find('%') {
                if line.starts_with("[download]") {
                    let num: String = line[..i].chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>().chars().rev().collect();
                    if let Ok(pc) = num.parse::<f64>() {
                        dl.p(pc / 100.0, "downloading");
                    }
                }
            }
        })?;
        if !o.ok {
            p.set("status", json!("failed"))?;
            bail!("the download failed: {}", proc::err_tail(&o.stderr, 4));
        }
        let file = std::fs::read_dir(&p.dir)?
            .flatten()
            .map(|e| e.path())
            .find(|f| f.file_stem().map(|s| s == "source").unwrap_or(false) && f.extension().map(|e| e != "part").unwrap_or(false))
            .ok_or_else(|| anyhow!("yt-dlp finished but no file was written"))?;
        p.manifest["source"]["path"] = json!(file.file_name().unwrap().to_string_lossy());
    } else {
        let file = PathBuf::from(source);
        if !file.is_file() {
            bail!("no such file: {source}");
        }
        let t = title.filter(|t| !t.is_empty()).map(|t| t.to_string()).unwrap_or_else(|| file.file_stem().unwrap_or_default().to_string_lossy().to_string());
        p = Project::create(cfg, &t)?;
        p.manifest["source"] = json!({"origin": "file", "path": fwd(&file)});
    }
    let media = proc::probe(&ffprobe, &p.source()?)?;
    if !media.has_video {
        p.set("status", json!("failed"))?;
        bail!("this file has no video stream");
    }
    p.manifest["media"] = media.json();
    p.manifest["status"] = json!("preparing");
    p.save_manifest()?;
    prepare(cfg, &p, &rep.sub(0.6, 1.0))?;
    p.set("status", json!("ready"))?;
    Ok(p)
}

/// Thumbnail, 16 kHz voice file, a small proxy the app can play, waveform peaks and loudness.
pub fn prepare(cfg: &Config, p: &Project, rep: &Rep) -> Result<()> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let src = fwd(&p.source()?);
    let m = p.media();
    std::fs::create_dir_all(p.p("assets"))?;
    std::fs::create_dir_all(p.p("build"))?;
    rep.p(0.0, "thumbnail");
    let at = format!("{:.2}", (m.duration * 0.1).min(3.0));
    let _ = proc::ffmpeg(&ff, &[s("-ss"), at, s("-i"), src.clone(), s("-frames:v"), s("1"), s("-vf"), s("scale=-2:480"), s("-q:v"), s("4"), fwd(&p.p("assets/thumb.jpg"))], None, 0.0, rep, "thumbnail");
    if m.has_audio {
        proc::ffmpeg(
            &ff,
            &[s("-i"), src.clone(), s("-vn"), s("-ac"), s("1"), s("-ar"), s("16000"), s("-c:a"), s("pcm_s16le"), fwd(&p.p("assets/audio16k.wav"))],
            None,
            m.duration,
            &rep.sub(0.02, 0.2),
            "voice track",
        )?;
        let _ = peaks(cfg, p);
        let _ = loudness(cfg, p);
    }
    let enc = h264_encoder(cfg);
    let mut a = vec![s("-i"), src];
    let tone = if m.hdr { "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable,zscale=t=bt709:m=bt709:r=tv," } else { "" };
    a.extend([s("-vf"), format!("{tone}scale=-2:540:flags=bilinear,fps=30,format=yuv420p"), s("-g"), s("15")]);
    if enc == "h264_nvenc" {
        a.extend(args(&["-c:v", "h264_nvenc", "-preset", "p2", "-rc", "vbr", "-cq", "30", "-b:v", "0"]));
    } else {
        a.extend(args(&["-c:v", "libx264", "-preset", "veryfast", "-crf", "27"]));
    }
    a.extend(args(&["-c:a", "aac", "-b:a", "96k", "-ac", "2", "-movflags", "+faststart"]));
    a.push(fwd(&p.p("assets/proxy.mp4")));
    proc::ffmpeg(&ff, &a, None, m.duration, &rep.sub(0.2, 1.0), "preview proxy")?;
    Ok(())
}

/// Waveform peaks (0..255) for the timeline, 50 per second (25 for long sources).
pub fn peaks(cfg: &Config, p: &Project) -> Result<()> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let wav = p.p("assets/audio16k.wav");
    let rate = if p.media().duration > 1200.0 { 25 } else { 50 };
    let sr = 8000;
    let raw = proc::ffmpeg_bytes(&ff, &[s("-i"), fwd(&wav), s("-ac"), s("1"), s("-ar"), sr.to_string(), s("-f"), s("s16le"), s("-")])?;
    let win = sr / rate;
    let mut out: Vec<u8> = Vec::with_capacity(raw.len() / 2 / win + 1);
    for chunk in raw.chunks(win * 2) {
        let mut mx = 0i32;
        for sm in chunk.chunks_exact(2) {
            mx = mx.max((i16::from_le_bytes([sm[0], sm[1]]) as i32).abs());
        }
        out.push(((mx as f64 / 32768.0).sqrt() * 255.0).min(255.0) as u8);
    }
    write_json(&p.p("build/peaks.json"), &json!({"rate": rate, "peaks": out}))
}

/// Integrated loudness and true peak of the voice, so sound effects can be set against it.
pub fn loudness(cfg: &Config, p: &Project) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let err = proc::ffmpeg(&ff, &[s("-i"), fwd(&p.p("assets/audio16k.wav")), s("-af"), s("loudnorm=print_format=json"), s("-f"), s("null"), s("-")], None, 0.0, &Rep::none(), "loudness")?;
    let j = err.rfind('{').and_then(|i| err[i..].find('}').map(|e| &err[i..=i + e])).ok_or_else(|| anyhow!("no loudness report"))?;
    let v: Value = serde_json::from_str(j)?;
    let num = |k: &str| v[k].as_str().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let out = json!({"lufs": num("input_i"), "true_peak": num("input_tp"), "lra": num("input_lra")});
    write_json(&p.p("build/audio.json"), &out)?;
    Ok(out)
}

// ---------- transcription ----------

fn clean_words(mut w: Vec<Word>) -> Vec<Word> {
    w.retain(|x| !x.w.trim().is_empty() && x.e >= x.s);
    for x in &mut w {
        x.w = x.w.trim().to_string();
    }
    // Punctuation that arrived as its own token joins the word before it.
    let mut out: Vec<Word> = vec![];
    for x in w {
        if x.w.chars().all(|c| c.is_ascii_punctuation()) && !out.is_empty() {
            let last = out.last_mut().unwrap();
            last.w.push_str(&x.w);
        } else {
            out.push(x);
        }
    }
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap());
    out
}

fn whisper_cpp(cfg: &Config, p: &Project, lang: &str, model: Option<&str>, rep: &Rep) -> Result<Words> {
    let bin = need_tool(cfg, "whisper")?;
    let model = find_whisper_model(cfg, model).ok_or_else(|| anyhow!("no whisper model found. Put a ggml-*.bin model in {} (the `doctor` tool lists the download).", models_dirs()[0].display()))?;
    let prefix = p.p("build/whisper");
    let mut c = proc::cmd(&bin);
    c.arg("-m").arg(&model).arg("-f").arg(p.p("assets/audio16k.wav")).args(["-l", if lang.is_empty() { "auto" } else { lang }, "-oj", "-ml", "1", "-sow", "-mc", "0", "-pp", "-of"]).arg(&prefix);
    let threads = cfg.get("transcribe.threads").and_then(|v| v.as_u64()).unwrap_or(0);
    if threads > 0 {
        c.args(["-t", &threads.to_string()]);
    }
    let o = proc::run_lines(&mut c, rep, |line| {
        if let Some(i) = line.find("progress =") {
            if let Ok(pc) = line[i + 10..].trim().trim_end_matches('%').trim().parse::<f64>() {
                rep.p(pc / 100.0, "transcribing");
            }
        }
    })?;
    let out = prefix.with_extension("json");
    let out = if out.exists() { out } else { PathBuf::from(format!("{}.json", prefix.display())) };
    if !o.ok || !out.exists() {
        bail!("whisper.cpp failed: {}", proc::err_tail(&o.stderr, 6));
    }
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&std::fs::read(&out)?))?;
    let mut words = vec![];
    for seg in ja(&v, "transcription") {
        let t = jsd(seg, "text", "");
        if t.trim().is_empty() || t.trim().starts_with("[_") || t.trim().starts_with('[') && t.trim().ends_with(']') {
            continue;
        }
        words.push(Word { w: t.to_string(), s: jfd(&seg["offsets"], "from", 0.0) / 1000.0, e: jfd(&seg["offsets"], "to", 0.0) / 1000.0, p: 1.0 });
    }
    Ok(Words { lang: jsd(&v["result"], "language", lang).to_string(), backend: format!("whisper.cpp/{}", model.file_stem().unwrap_or_default().to_string_lossy()), words: clean_words(words) })
}

/// A compact voice file for the cloud speech APIs (they cap uploads at about 25 MB).
fn api_audio(cfg: &Config, p: &Project) -> Result<Vec<u8>> {
    let out = p.p("build/audio.m4a");
    if !out.exists() {
        let ff = need_tool(cfg, "ffmpeg")?;
        proc::ffmpeg(&ff, &[s("-i"), fwd(&p.p("assets/audio16k.wav")), s("-c:a"), s("aac"), s("-b:a"), s("48k"), fwd(&out)], None, 0.0, &Rep::none(), "audio for upload")?;
    }
    Ok(std::fs::read(out)?)
}

fn openai_like(cfg: &Config, p: &Project, lang: &str, base: &str, key: &str, model: &str, name: &str) -> Result<Words> {
    let audio = api_audio(cfg, p)?;
    let mut fields = vec![("model", model), ("response_format", "verbose_json"), ("timestamp_granularities[]", "word")];
    if !lang.is_empty() && lang != "auto" {
        fields.push(("language", lang));
    }
    let (ct, body) = net::multipart(&fields, "file", "audio.m4a", "audio/mp4", &audio);
    let (status, text) = net::post_bytes(&format!("{base}/audio/transcriptions"), &[("Authorization", format!("Bearer {key}"))], &ct, &body, 1800)?;
    if status != 200 {
        bail!("{name} transcription failed ({status}): {}", text.chars().take(400).collect::<String>());
    }
    let v: Value = serde_json::from_str(&text)?;
    let words = ja(&v, "words").iter().map(|w| Word { w: jsd(w, "word", "").to_string(), s: jfd(w, "start", 0.0), e: jfd(w, "end", 0.0), p: 1.0 }).collect();
    Ok(Words { lang: jsd(&v, "language", lang).to_string(), backend: format!("{name}/{model}"), words: clean_words(words) })
}

fn elevenlabs(cfg: &Config, p: &Project, lang: &str, key: &str) -> Result<Words> {
    let audio = api_audio(cfg, p)?;
    let mut fields = vec![("model_id", "scribe_v1"), ("timestamps_granularity", "word"), ("tag_audio_events", "false")];
    if !lang.is_empty() && lang != "auto" {
        fields.push(("language_code", lang));
    }
    let (ct, body) = net::multipart(&fields, "file", "audio.m4a", "audio/mp4", &audio);
    let (status, text) = net::post_bytes("https://api.elevenlabs.io/v1/speech-to-text", &[("xi-api-key", key.to_string())], &ct, &body, 1800)?;
    if status != 200 {
        bail!("ElevenLabs transcription failed ({status}): {}", text.chars().take(400).collect::<String>());
    }
    let v: Value = serde_json::from_str(&text)?;
    let words = ja(&v, "words").iter().filter(|w| jsd(w, "type", "word") == "word").map(|w| Word { w: jsd(w, "text", "").to_string(), s: jfd(w, "start", 0.0), e: jfd(w, "end", 0.0), p: 1.0 }).collect();
    Ok(Words { lang: jsd(&v, "language_code", lang).to_string(), backend: "elevenlabs/scribe_v1".into(), words: clean_words(words) })
}

/// Word-level transcript of the whole source -> build/words.json.
pub fn transcribe(cfg: &Config, p: &Project, backend: Option<&str>, lang: Option<&str>, model: Option<&str>, rep: &Rep) -> Result<Words> {
    if !p.has("assets/audio16k.wav") {
        bail!("this video has no audio track to transcribe");
    }
    let backend = backend.filter(|b| !b.is_empty()).map(|b| b.to_string()).unwrap_or_else(|| cfg.str("transcribe.backend"));
    let lang_cfg = cfg.str("transcribe.lang");
    let lang = lang.filter(|l| !l.is_empty()).unwrap_or(&lang_cfg);
    rep.p(0.0, "transcribing");
    let need = |n: &str| cfg.key(n).ok_or_else(|| anyhow!("no {n} API key. Add it in settings (keys.{n}) or switch transcribe.backend to whisper.cpp."));
    let w = match backend.as_str() {
        "whisper.cpp" | "local" | "" => whisper_cpp(cfg, p, lang, model, rep)?,
        "openai" => openai_like(cfg, p, lang, "https://api.openai.com/v1", &need("openai")?, model.filter(|m| !m.is_empty()).unwrap_or("whisper-1"), "openai")?,
        "groq" => openai_like(cfg, p, lang, "https://api.groq.com/openai/v1", &need("groq")?, model.filter(|m| !m.is_empty()).unwrap_or("whisper-large-v3-turbo"), "groq")?,
        "elevenlabs" => elevenlabs(cfg, p, lang, &need("elevenlabs")?)?,
        other => bail!("unknown transcription backend '{other}'. Use whisper.cpp, openai, groq or elevenlabs."),
    };
    if w.words.is_empty() {
        bail!("the transcript came back empty (no speech found)");
    }
    write_json(&p.p("build/words.json"), &serde_json::to_value(&w)?)?;
    rep.p(1.0, "transcribed");
    Ok(w)
}

pub fn file_exists(p: &Path) -> bool {
    p.exists()
}
