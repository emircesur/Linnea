//! Translation and dubbing, B-roll, eye contact and speed: each with a local way and API ways.
//! Local: any OpenAI-compatible model server for translation, Piper for voices, an Automatic1111/Forge
//! server for pictures, a command of your choice for eye contact. APIs: ElevenLabs (dubbing, voices),
//! HeyGen (lip-synced translation), OpenAI (voices, pictures), Gemini (pictures), Pexels and Pixabay
//! (stock), Sieve (eye contact). Nothing is sent anywhere unless the user picked that provider.
use crate::analyze;
use crate::config::*;
use crate::llm;
use crate::media;
use crate::net;
use crate::proc::{self, Rep};
use crate::project::Project;
use crate::render::{self, Mode};
use crate::timeline::Timeline;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn s(x: impl AsRef<str>) -> String {
    x.as_ref().to_string()
}
fn proj(c: &Ctx, a: &Value) -> Result<Project> {
    Project::open(&c.cfg, jsd(a, "project", ""))
}
fn key(c: &Ctx, name: &str) -> Result<String> {
    c.cfg.key(name).ok_or_else(|| anyhow!("no {name} key. Add it in Settings > Keys (keys.{name})"))
}
fn rel(p: &Project, f: &Path) -> String {
    f.strip_prefix(&p.dir).map(|r| fwd(r)).unwrap_or_else(|_| fwd(f))
}

/// The sentences a reel keeps, in original seconds.
fn reel_sentences(p: &Project, reel: &Value) -> Result<Vec<(f64, f64, String)>> {
    let tl = Timeline::from_reel(reel, p.media().duration);
    let words = p.words()?.words;
    Ok(analyze::sentences(&words).into_iter().filter(|s| tl.to_edit((s.s + s.e) / 2.0).is_some()).map(|s| (s.s, s.e, s.text)).collect())
}

// ---------- translation ----------

fn translate_lines(cfg: &Config, lines: &[String], lang: &str) -> Result<Vec<String>> {
    let mut out = vec![];
    for chunk in lines.chunks(40) {
        let sys = "You translate video subtitles. Keep each line's meaning, tone and length close to the original so it fits the same seconds. Answer with a JSON array of strings only, one per input line, same order and count.";
        let user = format!("Target language: {lang}\nLines (JSON):\n{}", json!(chunk));
        let text = llm::complete(cfg, sys, &user)?;
        let arr = llm::extract_json(&text).and_then(|v| v.as_array().cloned()).ok_or_else(|| anyhow!("the model's translation could not be read"))?;
        if arr.len() != chunk.len() {
            bail!("the model returned {} lines for {}", arr.len(), chunk.len());
        }
        out.extend(arr.into_iter().map(|v| v.as_str().unwrap_or("").to_string()));
    }
    Ok(out)
}

fn t_translate(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let lang = js(a, "lang").ok_or_else(|| anyhow!("`lang` is the target language (en, de, es, ar...)"))?;
    let mut reel = p.load_reel(&rid)?;
    let sents = reel_sentences(&p, &reel)?;
    c.rep.p(0.1, "translating");
    let tr = translate_lines(&c.cfg, &sents.iter().map(|x| x.2.clone()).collect::<Vec<_>>(), lang)?;
    let blocks: Vec<Value> = sents.iter().zip(&tr).map(|((s0, e0, src), t)| json!({"s": s0, "e": e0, "src": src, "text": t})).collect();
    let file = p.p(&format!("build/{rid}.tr.{lang}.json"));
    write_json(&file, &json!({"lang": lang, "provider": llm::provider(&c.cfg), "blocks": blocks}))?;
    if jb(a, "apply") != Some(false) {
        reel["captions"]["lang"] = json!(lang);
        p.save_reel(&rid, &reel)?;
    }
    Ok(json!({"lang": lang, "sentences": blocks.len(), "file": fwd(&file), "note": "captions now show the translation (captions.lang); set it to null for the original. Edit the file to fix a line."}))
}

// ---------- the reel's voice, cut ----------

fn cut_voice(c: &Ctx, p: &Project, reel: &Value, out: &Path) -> Result<f64> {
    let ff = need_tool(&c.cfg, "ffmpeg")?;
    let tl = Timeline::from_reel(reel, p.media().duration);
    let mut g = vec![];
    for (i, sg) in tl.segs.iter().enumerate() {
        let mut ch = format!("[0:a]atrim=start={:.4}:end={:.4},asetpts=PTS-STARTPTS", sg.a, sg.b);
        if (sg.speed - 1.0).abs() > 1e-6 {
            ch.push_str(&format!(",atempo={:.4}", sg.speed.clamp(0.5, 2.0)));
        }
        if sg.hold > 0.0 {
            ch.push_str(&format!(",apad=pad_dur={:.3}", sg.hold));
        }
        g.push(format!("{ch}[a{i}]"));
    }
    let n = tl.segs.len();
    g.push(format!("{}concat=n={n}:v=0:a=1[out]", (0..n).map(|i| format!("[a{i}]")).collect::<String>()));
    proc::ffmpeg(&ff, &[s("-i"), fwd(&p.source()?), s("-filter_complex"), g.join(";"), s("-map"), s("[out]"), s("-ac"), s("1"), s("-ar"), s("44100"), fwd(out)], None, tl.total, &Rep::none(), "voice")?;
    Ok(tl.total)
}

// ---------- voices (TTS) ----------

pub fn tts(c: &Ctx, provider: &str, voice: &str, lang: &str, text: &str, out: &Path) -> Result<()> {
    match provider {
        "elevenlabs" => {
            let v = if voice.is_empty() { c.cfg.str("voices.elevenlabs") } else { voice.into() };
            let v = if v.is_empty() { "JBFqnCBsd6RMkjVDRZzb".to_string() } else { v };
            let (st, bytes) = net::post_json_bytes(
                &format!("https://api.elevenlabs.io/v1/text-to-speech/{v}?output_format=mp3_44100_128"),
                &[("xi-api-key", key(c, "elevenlabs")?)],
                &json!({"text": text, "model_id": "eleven_multilingual_v2", "language_code": lang}),
                300,
            )?;
            if st != 200 {
                bail!("ElevenLabs voice failed ({st}): {}", String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>());
            }
            std::fs::write(out, bytes)?;
        }
        "openai" => {
            let (st, bytes) = net::post_json_bytes(
                "https://api.openai.com/v1/audio/speech",
                &[("Authorization", format!("Bearer {}", key(c, "openai")?))],
                &json!({"model": "gpt-4o-mini-tts", "voice": if voice.is_empty() { "alloy" } else { voice }, "input": text, "response_format": "mp3"}),
                300,
            )?;
            if st != 200 {
                bail!("OpenAI voice failed ({st}): {}", String::from_utf8_lossy(&bytes).chars().take(300).collect::<String>());
            }
            std::fs::write(out, bytes)?;
        }
        _ => {
            // Piper: a local voice, fully offline.
            let piper = find_tool(&c.cfg, "piper").ok_or_else(|| anyhow!("Piper is not installed: Settings > Tools > download piper and a voice (setup_tools what=[\"piper\",\"voice:{lang}\"])"))?;
            let model = piper_voice(voice, lang).ok_or_else(|| anyhow!("no Piper voice for '{lang}': setup_tools what=[\"voice:{lang}\"]"))?;
            let mut ch = proc::cmd(&piper).arg("--model").arg(&model).arg("--output_file").arg(out).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).spawn()?;
            {
                use std::io::Write;
                let mut si = ch.stdin.take().unwrap();
                si.write_all(text.as_bytes())?;
            }
            let o = ch.wait_with_output()?;
            if !o.status.success() || !out.is_file() {
                bail!("Piper failed: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 3));
            }
        }
    }
    Ok(())
}

/// Piper voices, by language: the first default per language that is installed.
pub const PIPER_VOICES: &[(&str, &str)] = &[
    ("tr", "tr/tr_TR/dfki/medium/tr_TR-dfki-medium"),
    ("en", "en/en_US/lessac/medium/en_US-lessac-medium"),
    ("de", "de/de_DE/thorsten/medium/de_DE-thorsten-medium"),
    ("es", "es/es_ES/davefx/medium/es_ES-davefx-medium"),
    ("fr", "fr/fr_FR/siwis/medium/fr_FR-siwis-medium"),
    ("it", "it/it_IT/riccardo/x_low/it_IT-riccardo-x_low"),
    ("pt", "pt/pt_BR/faber/medium/pt_BR-faber-medium"),
    ("ru", "ru/ru_RU/denis/medium/ru_RU-denis-medium"),
    ("ar", "ar/ar_JO/kareem/medium/ar_JO-kareem-medium"),
    ("nl", "nl/nl_NL/mls/medium/nl_NL-mls-medium"),
];
fn piper_voice(voice: &str, lang: &str) -> Option<PathBuf> {
    let name = if !voice.is_empty() { voice.to_string() } else { PIPER_VOICES.iter().find(|(l, _)| *l == lang)?.1.rsplit('/').next()?.to_string() };
    models_dirs().into_iter().map(|d| d.join("piper").join(format!("{name}.onnx"))).find(|p| p.is_file())
}

// ---------- dubbing ----------

fn t_dub(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let lang = js(a, "lang").ok_or_else(|| anyhow!("`lang` is the target language code (en, de, es...)"))?.to_string();
    let provider = jsd(a, "provider", "elevenlabs-dubbing").to_string();
    let mut reel = p.load_reel(&rid)?;
    std::fs::create_dir_all(p.p("assets/dub"))?;
    let out = p.p(&format!("assets/dub/{rid}.{lang}.{}", if provider == "elevenlabs-dubbing" { "mp3" } else { "wav" }));
    if provider == "elevenlabs-dubbing" {
        let k = key(c, "elevenlabs")?;
        let voice = p.p(&format!("build/{rid}.voice.wav"));
        c.rep.p(0.05, "cutting the voice");
        cut_voice(c, &p, &reel, &voice)?;
        let src_lang = p.words().map(|w| w.lang).unwrap_or_default();
        let mut fields = vec![("target_lang", lang.as_str()), ("num_speakers", "1"), ("watermark", "false")];
        if src_lang.len() == 2 {
            fields.push(("source_lang", src_lang.as_str()));
        }
        let (ct, body) = net::multipart(&fields, "file", "voice.wav", "audio/wav", &std::fs::read(&voice)?);
        c.rep.p(0.1, "uploading to ElevenLabs");
        let (st, text) = net::post_bytes("https://api.elevenlabs.io/v1/dubbing", &[("xi-api-key", k.clone())], &ct, &body, 600)?;
        if st != 200 {
            bail!("ElevenLabs dubbing failed ({st}): {}", text.chars().take(300).collect::<String>());
        }
        let v: Value = serde_json::from_str(&text)?;
        let id = jsd(&v, "dubbing_id", "").to_string();
        let expect = jfd(&v, "expected_duration_sec", 60.0).max(10.0);
        let t0 = now_ms();
        loop {
            c.rep.check()?;
            std::thread::sleep(std::time::Duration::from_secs(5));
            let (_, m) = net::get_json(&format!("https://api.elevenlabs.io/v1/dubbing/{id}"), &[("xi-api-key", k.clone())], 60)?;
            let status = jsd(&m, "status", "");
            c.rep.p(0.15 + 0.75 * ((now_ms() - t0) as f64 / 1000.0 / expect).min(1.0), &format!("ElevenLabs: {status}"));
            match status {
                "dubbed" => break,
                "failed" => bail!("ElevenLabs dubbing failed: {}", jsd(&m, "error", "")),
                _ => {}
            }
            if now_ms() - t0 > 60 * 60 * 1000 {
                bail!("ElevenLabs is still dubbing after an hour (id {id})");
            }
        }
        let (st, bytes) = net::get_bytes(&format!("https://api.elevenlabs.io/v1/dubbing/{id}/audio/{lang}"), &[("xi-api-key", k)], 600)?;
        if st != 200 {
            bail!("downloading the dub failed ({st})");
        }
        std::fs::write(&out, bytes)?;
    } else {
        // Translate the sentences, voice each one, fit it to the sentence's time.
        let tl = Timeline::from_reel(&reel, p.media().duration);
        let sents = reel_sentences(&p, &reel)?;
        let trf = p.p(&format!("build/{rid}.tr.{lang}.json"));
        let lines: Vec<String> = match read_json(&trf) {
            Ok(v) if ja(&v, "blocks").len() == sents.len() => ja(&v, "blocks").iter().map(|b| jsd(b, "text", "").to_string()).collect(),
            _ => {
                c.rep.p(0.05, "translating");
                let tr = translate_lines(&c.cfg, &sents.iter().map(|x| x.2.clone()).collect::<Vec<_>>(), &lang)?;
                let blocks: Vec<Value> = sents.iter().zip(&tr).map(|((s0, e0, src), t)| json!({"s": s0, "e": e0, "src": src, "text": t})).collect();
                write_json(&trf, &json!({"lang": lang, "blocks": blocks}))?;
                tr
            }
        };
        let ff = need_tool(&c.cfg, "ffmpeg")?;
        let fp = need_tool(&c.cfg, "ffprobe")?;
        let tmp = p.p("build/dub");
        std::fs::create_dir_all(&tmp)?;
        let mut args: Vec<String> = vec![];
        let mut g = vec![];
        let n = sents.len();
        for (i, ((s0, e0, _), line)) in sents.iter().zip(&lines).enumerate() {
            c.rep.p(0.1 + 0.8 * i as f64 / n.max(1) as f64, &format!("voice {}/{n}", i + 1));
            if line.trim().is_empty() {
                continue;
            }
            let f = tmp.join(format!("{rid}-{lang}-{i:03}.{}", if provider == "piper" { "wav" } else { "mp3" }));
            tts(c, &provider, jsd(a, "voice", ""), &lang, line, &f)?;
            let es = tl.to_edit_snap(*s0);
            let slot = sents.get(i + 1).map(|x| tl.to_edit_snap(x.0)).unwrap_or(tl.total).max(tl.to_edit_snap(*e0)) - es;
            let len = proc::probe(&fp, &f).map(|m| m.duration).unwrap_or(slot);
            let k = (len / slot.max(0.3)).clamp(1.0, 1.6);
            args.extend([s("-i"), fwd(&f)]);
            let idx = args.len() / 2 - 1;
            g.push(format!("[{idx}:a]aresample=44100,aformat=channel_layouts=mono,atempo={k:.4},adelay={}:all=1[d{idx}]", (es * 1000.0).round() as i64));
        }
        if g.is_empty() {
            bail!("nothing to voice");
        }
        let labels: String = (0..g.len()).map(|i| format!("[d{}]", g[i].split('[').nth(2).and_then(|x| x.split(']').next()).unwrap_or("").trim_start_matches('d'))).collect();
        let _ = labels;
        let outs: Vec<String> = g.iter().map(|x| x.rsplit('[').next().unwrap_or("").trim_end_matches(']').to_string()).collect();
        g.push(format!("{}amix=inputs={}:normalize=0:duration=longest,apad[out]", outs.iter().map(|o| format!("[{o}]")).collect::<String>(), outs.len()));
        args.extend([s("-filter_complex"), g.join(";"), s("-map"), s("[out]"), s("-t"), format!("{:.3}", tl.total), fwd(&out)]);
        proc::ffmpeg(&ff, &args, None, tl.total, &Rep::none(), "dub mix")?;
    }
    if jb(a, "apply") != Some(false) {
        reel["audio"]["dub"] = json!(rel(&p, &out));
        if a.get("captions").map(|v| v != &json!(false)).unwrap_or(true) && p.p(&format!("build/{rid}.tr.{lang}.json")).is_file() {
            reel["captions"]["lang"] = json!(lang);
        }
        p.save_reel(&rid, &reel)?;
    }
    Ok(json!({"dub": fwd(&out), "provider": provider, "note": "the reel's voice is now the dub (audio.dub); set it to null for the original"}))
}

fn t_heygen(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let k = key(c, "heygen")?;
    let lang = js(a, "language").ok_or_else(|| anyhow!("`language` is a HeyGen language name, e.g. \"Spanish (Spain)\" or \"English (United States)\""))?.to_string();
    // A clean cut of the reel: no captions, cards or sounds, small enough for the 50 MB upload.
    let mut clean = p.load_reel(&rid)?;
    clean["captions"] = json!({"style": "none"});
    clean["beats"] = json!([]);
    clean["sfx"] = json!([]);
    clean["music"] = Value::Null;
    if let Some(o) = clean["audio"].as_object_mut() {
        o.remove("dub");
    }
    let crid = format!("{rid}__clean");
    p.save_reel(&crid, &clean)?;
    c.rep.p(0.02, "cutting the clean video");
    let r = render::render(&c.cfg, &p, &crid, Mode::Preview, &c.rep.sub(0.0, 0.2));
    let _ = std::fs::remove_file(p.reel_path(&crid));
    let file = PathBuf::from(r?["file"].as_str().unwrap_or("").to_string());
    let bytes = std::fs::read(&file)?;
    if bytes.len() > 50_000_000 {
        bail!("the clean cut is {} MB; HeyGen takes 50 MB. Use a shorter reel", bytes.len() / 1_000_000);
    }
    c.rep.p(0.22, "uploading to HeyGen");
    let (ct, body) = net::multipart(&[], "file", "reel.mp4", "video/mp4", &bytes);
    let (st, text) = net::post_bytes("https://api.heygen.com/v3/assets", &[("x-api-key", k.clone())], &ct, &body, 900)?;
    if st >= 300 {
        bail!("HeyGen upload failed ({st}): {}", text.chars().take(300).collect::<String>());
    }
    let asset = serde_json::from_str::<Value>(&text)?["data"]["asset_id"].as_str().ok_or_else(|| anyhow!("HeyGen gave no asset_id"))?.to_string();
    let (st, v) = net::post_json(
        "https://api.heygen.com/v3/video-translations",
        &[("x-api-key", k.clone())],
        &json!({"video": {"type": "asset_id", "asset_id": asset}, "output_languages": [lang], "mode": jsd(a, "mode", "speed"), "translate_audio_only": jb(a, "audio_only").unwrap_or(false), "speaker_num": 1, "title": p.title()}),
        120,
    )?;
    if st >= 300 {
        bail!("HeyGen translation failed ({st}): {}", v.to_string().chars().take(300).collect::<String>());
    }
    let id = v["data"]["video_translation_ids"][0].as_str().ok_or_else(|| anyhow!("HeyGen gave no translation id"))?.to_string();
    let t0 = now_ms();
    let url = loop {
        c.rep.check()?;
        std::thread::sleep(std::time::Duration::from_secs(10));
        let (_, m) = net::get_json(&format!("https://api.heygen.com/v3/video-translations/{id}"), &[("x-api-key", k.clone())], 60)?;
        let d = &m["data"];
        match jsd(d, "status", "") {
            "completed" => break jsd(d, "video_url", "").to_string(),
            "failed" => bail!("HeyGen translation failed: {}", d["error"]),
            st => c.rep.p(0.25 + 0.6 * ((now_ms() - t0) as f64 / 900_000.0).min(1.0), &format!("HeyGen: {st}")),
        }
        if now_ms() - t0 > 3 * 60 * 60 * 1000 {
            bail!("HeyGen is still translating after 3 hours (id {id})");
        }
    };
    let (st, vid) = net::get_bytes(&url, &[], 1800)?;
    if st != 200 {
        bail!("downloading HeyGen's video failed ({st})");
    }
    let dl = p.p(&format!("renders/{}-{}.heygen.mp4", slug(&p.title()), slug(&lang)));
    std::fs::write(&dl, vid)?;
    c.rep.p(0.9, "importing");
    let np = media::import(&c.cfg, &fwd(&dl), Some(&format!("{} ({lang})", p.title())), &c.rep.sub(0.9, 1.0))?;
    Ok(json!({"video": fwd(&dl), "project": np.id(), "note": "the translated, lip-synced video is a new project: transcribe it for captions in the new language"}))
}

// ---------- B-roll ----------

fn download_to(url: &str, to: &Path) -> Result<()> {
    let (st, b) = net::get_bytes(url, &[], 900)?;
    if st != 200 || b.len() < 1000 {
        bail!("download failed ({st}): {url}");
    }
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(to, b)?;
    Ok(())
}

fn manifest_add(p: &Project, entry: Value) -> Result<()> {
    let f = p.p("broll/manifest.json");
    let mut m = read_json_or(&f, json!([])).as_array().cloned().unwrap_or_default();
    m.push(entry);
    write_json(&f, &json!(m))
}

/// Search stock footage or photos; download the best matches into broll/.
fn search(c: &Ctx, p: &Project, query: &str, source: &str, kind: &str, portrait: bool, count: usize) -> Result<Vec<Value>> {
    let q = query.replace(' ', "+");
    let mut found: Vec<(String, String, String)> = vec![]; // (url, credit, page)
    match (source, kind) {
        ("pexels", "photo") => {
            let (_, v) = net::get_json(&format!("https://api.pexels.com/v1/search?query={q}&per_page={count}{}", if portrait { "&orientation=portrait" } else { "" }), &[("Authorization", key(c, "pexels")?)], 60)?;
            for ph in ja(&v, "photos") {
                found.push((jsd(&ph["src"], if portrait { "portrait" } else { "large2x" }, jsd(&ph["src"], "large", "")).to_string(), jsd(ph, "photographer", "").to_string(), jsd(ph, "url", "").to_string()));
            }
        }
        ("pexels", _) => {
            let (_, v) = net::get_json(&format!("https://api.pexels.com/videos/search?query={q}&per_page={count}{}", if portrait { "&orientation=portrait" } else { "" }), &[("Authorization", key(c, "pexels")?)], 60)?;
            for vd in ja(&v, "videos") {
                let mut files: Vec<&Value> = ja(vd, "video_files").iter().filter(|f| jsd(f, "file_type", "") == "video/mp4").collect();
                files.sort_by_key(|f| (jfd(f, "height", 0.0).max(jfd(f, "width", 0.0)) - 1920.0).abs() as i64);
                if let Some(f) = files.first() {
                    found.push((jsd(f, "link", "").to_string(), jsd(&vd["user"], "name", "").to_string(), jsd(vd, "url", "").to_string()));
                }
            }
        }
        ("pixabay", "photo") => {
            let (_, v) = net::get_json(&format!("https://pixabay.com/api/?key={}&q={q}&image_type=photo&per_page={}{}", key(c, "pixabay")?, count.max(3), if portrait { "&orientation=vertical" } else { "" }), &[], 60)?;
            for h in ja(&v, "hits") {
                found.push((jsd(h, "largeImageURL", "").to_string(), jsd(h, "user", "").to_string(), jsd(h, "pageURL", "").to_string()));
            }
        }
        ("pixabay", _) => {
            let (_, v) = net::get_json(&format!("https://pixabay.com/api/videos/?key={}&q={q}&per_page={}", key(c, "pixabay")?, count.max(3)), &[], 60)?;
            for h in ja(&v, "hits") {
                let vs = &h["videos"];
                let u = ["large", "medium", "small"].iter().map(|k| jsd(&vs[*k], "url", "")).find(|u| !u.is_empty()).unwrap_or("");
                found.push((u.to_string(), jsd(h, "user", "").to_string(), jsd(h, "pageURL", "").to_string()));
            }
        }
        _ => bail!("unknown stock source '{source}': pexels or pixabay"),
    }
    let mut out = vec![];
    for (i, (url, credit, page)) in found.into_iter().filter(|f| !f.0.is_empty()).take(count).enumerate() {
        let ext = if kind == "photo" { "jpg" } else { "mp4" };
        let f = p.p(&format!("broll/{}-{}-{}.{ext}", slug(query), source, now_ms() % 1_000_000 + i as u64));
        download_to(&url, &f)?;
        let e = json!({"file": rel(p, &f), "query": query, "source": source, "credit": credit, "page": page, "license": if source == "pexels" { "Pexels License" } else { "Pixabay Content License" }});
        manifest_add(p, e.clone())?;
        out.push(e);
    }
    Ok(out)
}

fn t_broll_search(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let r = search(c, &p, jsd(a, "query", ""), jsd(a, "source", "pexels"), jsd(a, "kind", "video"), jb(a, "portrait").unwrap_or(true), jfd(a, "count", 3.0) as usize)?;
    Ok(json!({"found": r, "note": "use one with beat_add {type: broll, src, at, to} (video) or {type: scene, kind: image, src} (photo)"}))
}

/// Generate a picture: OpenAI, Gemini, or a local Automatic1111 / Forge server.
fn generate(c: &Ctx, p: &Project, prompt: &str, provider: &str, portrait: bool) -> Result<PathBuf> {
    let f = p.p(&format!("broll/gen-{}-{}.png", slug(prompt).chars().take(30).collect::<String>(), now_ms() % 1_000_000));
    std::fs::create_dir_all(p.p("broll"))?;
    let b64 = match provider {
        "openai" => {
            let (st, v) = net::post_json(
                "https://api.openai.com/v1/images/generations",
                &[("Authorization", format!("Bearer {}", key(c, "openai")?))],
                &json!({"model": "gpt-image-1", "prompt": prompt, "size": if portrait { "1024x1536" } else { "1536x1024" }, "n": 1}),
                600,
            )?;
            if st != 200 {
                bail!("OpenAI image failed ({st}): {}", v.to_string().chars().take(300).collect::<String>());
            }
            jsd(&v["data"][0], "b64_json", "").to_string()
        }
        "gemini" => {
            let (st, v) = net::post_json(
                &format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent?key={}", key(c, "gemini")?),
                &[],
                &json!({"contents": [{"parts": [{"text": format!("{prompt}. {} format, photographic, no text.", if portrait { "Vertical 9:16" } else { "Wide 16:9" })}]}]}),
                600,
            )?;
            if st != 200 {
                bail!("Gemini image failed ({st}): {}", v.to_string().chars().take(300).collect::<String>());
            }
            ja(&v["candidates"][0]["content"], "parts").iter().find_map(|x| x["inlineData"]["data"].as_str().map(|s| s.to_string())).ok_or_else(|| anyhow!("Gemini returned no picture"))?
        }
        _ => {
            let base = c.cfg.str("images.local_url");
            let base = if base.is_empty() { "http://127.0.0.1:7860".to_string() } else { base };
            let (st, v) = net::post_json(&format!("{}/sdapi/v1/txt2img", base.trim_end_matches('/')), &[], &json!({"prompt": prompt, "width": if portrait { 768 } else { 1344 }, "height": if portrait { 1344 } else { 768 }, "steps": 28}), 1200)?;
            if st != 200 {
                bail!("the local image server at {base} answered {st} (Automatic1111 / Forge with --api)");
            }
            jsd(&v["images"][0], "", v["images"][0].as_str().unwrap_or("")).to_string()
        }
    };
    if b64.is_empty() {
        bail!("no picture came back");
    }
    std::fs::write(&f, base64::engine::general_purpose::STANDARD.decode(b64.trim())?)?;
    manifest_add(p, json!({"file": rel(p, &f), "prompt": prompt, "source": provider, "license": "generated"}))?;
    Ok(f)
}

fn t_broll_gen(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let f = generate(c, &p, jsd(a, "prompt", ""), jsd(a, "provider", "openai"), jb(a, "portrait").unwrap_or(true))?;
    Ok(json!({"file": rel(&p, &f), "path": fwd(&f)}))
}

/// Pick the lines that want a picture, find or make one for each, and place them as beats.
fn t_broll_auto(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    let tl = Timeline::from_reel(&reel, p.media().duration);
    let sents = reel_sentences(&p, &reel)?;
    let per_min = jfd(a, "per_minute", 3.0);
    let max = jf(a, "count").map(|x| x as usize).unwrap_or(((tl.total / 60.0) * per_min).ceil().max(1.0) as usize);
    let list: String = sents.iter().enumerate().map(|(i, x)| format!("{i}\t{:.1}\t{}\n", x.1 - x.0, x.2)).collect();
    let picks: Vec<(usize, String)> = match llm::complete(
        &c.cfg,
        "You are a video editor choosing B-roll. Pick the sentences where showing a picture of the thing said helps most (concrete nouns, places, actions), never the hook or a personal line. Answer with a JSON array only.",
        &format!("Pick at most {max}. Sentences: index, seconds, text.\n{list}\nJSON: [{{\"i\": index, \"query\": \"2-4 English words for a stock search\"}}]"),
    )
    .ok()
    .and_then(|t| llm::extract_json(&t))
    {
        Some(Value::Array(v)) => v.iter().filter_map(|x| Some((x["i"].as_u64()? as usize, x["query"].as_str()?.to_string()))).collect(),
        _ => {
            // No model: the longest content words of the longest sentences.
            let mut idx: Vec<usize> = (1..sents.len()).collect();
            idx.sort_by(|x, y| sents[*y].2.len().cmp(&sents[*x].2.len()));
            idx.into_iter().take(max).map(|i| (i, sents[i].2.split_whitespace().max_by_key(|w| w.chars().count()).unwrap_or("").trim_matches(|c: char| !c.is_alphanumeric()).to_string())).collect()
        }
    };
    let source = jsd(a, "source", "pexels").to_string();
    let mode = jsd(a, "mode", "split").to_string();
    let mut beats = ja(&reel, "beats").to_vec();
    let mut added = vec![];
    for (i, q) in picks.into_iter().take(max) {
        let Some((s0, e0, _)) = sents.get(i).cloned() else { continue };
        let (at, to) = (s0 + 0.1, (s0 + 2.8).min(e0 + 0.4));
        let beat = if source == "openai" || source == "gemini" || source == "local" {
            let f = generate(c, &p, &q, &source, true)?;
            json!({"type": "scene", "kind": "image", "src": rel(&p, &f), "at": at, "to": to, "captions": true})
        } else {
            let r = search(c, &p, &q, &source, "video", true, 1)?;
            let Some(f) = r.first() else { continue };
            let mut b = json!({"type": "broll", "src": f["file"], "at": at, "to": to});
            if mode == "split" {
                b["mode"] = json!("split");
            }
            b
        };
        added.push(json!({"at": at, "query": q, "beat": beat["src"]}));
        beats.push(beat);
    }
    reel["beats"] = json!(beats);
    p.save_reel(&rid, &reel)?;
    Ok(json!({"added": added, "note": "each B-roll is a beat now: look at it with snapshot, remove with beat_remove"}))
}

// ---------- eye contact ----------

fn t_eye(c: &Ctx, a: &Value) -> Result<Value> {
    let mut p = proj(c, a)?;
    let src = p.source()?;
    let out = p.p("assets/source.eyecontact.mp4");
    let provider = jsd(a, "provider", if c.cfg.str("eye_contact.command").is_empty() { "sieve" } else { "command" }).to_string();
    match provider.as_str() {
        "command" => {
            let tpl = c.cfg.str("eye_contact.command");
            if tpl.is_empty() {
                bail!("set eye_contact.command in settings to a local tool, with {{in}} and {{out}} (for example NVIDIA Maxine's gaze redirection sample)");
            }
            let line = tpl.replace("{in}", &format!("\"{}\"", src.display())).replace("{out}", &format!("\"{}\"", out.display()));
            let o = proc::run(proc::cmd(Path::new("cmd")).args(["/C", &line]))?;
            if !o.ok || !out.is_file() {
                bail!("the eye contact command failed: {}", proc::err_tail(&o.stderr, 4));
            }
        }
        "sieve" => {
            let url = js(a, "video_url").ok_or_else(|| anyhow!("Sieve reads the video from a public URL: give `video_url` (your own storage link)"))?;
            let k = key(c, "sieve")?;
            let func = { let f = c.cfg.str("eye_contact.sieve_function"); if f.is_empty() { "sieve/eye-contact-correction".to_string() } else { f } };
            let input = { let f = c.cfg.str("eye_contact.sieve_input"); if f.is_empty() { "file".to_string() } else { f } };
            let mut inputs = json!({});
            inputs[&input] = json!({"url": url});
            let (st, v) = net::post_json("https://mango.sievedata.com/v2/push", &[("X-API-Key", k.clone())], &json!({"function": func, "inputs": inputs}), 120)?;
            if st >= 300 {
                bail!("Sieve push failed ({st}): {}", v.to_string().chars().take(300).collect::<String>());
            }
            let id = jsd(&v, "id", "").to_string();
            let result = loop {
                c.rep.check()?;
                std::thread::sleep(std::time::Duration::from_secs(8));
                let (_, j) = net::get_json(&format!("https://mango.sievedata.com/v2/jobs/{id}"), &[("X-API-Key", k.clone())], 60)?;
                match jsd(&j, "status", "") {
                    "finished" => break j,
                    "error" | "failed" => bail!("Sieve job failed: {}", j["error"]),
                    st => c.rep.p(0.5, &format!("Sieve: {st}")),
                }
            };
            fn first_url(v: &Value) -> Option<String> {
                match v {
                    Value::String(s) if s.starts_with("http") => Some(s.clone()),
                    Value::Array(a) => a.iter().find_map(first_url),
                    Value::Object(o) => o.get("url").and_then(first_url).or_else(|| o.values().find_map(first_url)),
                    _ => None,
                }
            }
            let u = first_url(&result["outputs"]).ok_or_else(|| anyhow!("Sieve finished but returned no file"))?;
            download_to(&u, &out)?;
        }
        other => bail!("unknown eye contact provider '{other}': command or sieve"),
    }
    let orig = p.manifest["source"]["path"].clone();
    p.manifest["source"]["original"] = orig;
    p.manifest["source"]["path"] = json!("assets/source.eyecontact.mp4");
    p.save_manifest()?;
    media::prepare(&c.cfg, &p, &c.rep.sub(0.9, 1.0))?;
    Ok(json!({"source": fwd(&out), "note": "the project now uses the corrected video; source.original keeps the old one"}))
}

// ---------- speed ----------

fn t_speed(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    let mut takes = ja(&reel, "takes").to_vec();
    if takes.is_empty() {
        takes = vec![json!({"a": jfd(&reel["clip"], "a", 0.0), "b": jf(&reel["clip"], "b").unwrap_or(p.media().duration)})];
    }
    let i = jfd(a, "take", 0.0) as usize;
    let t = takes.get(i).cloned().ok_or_else(|| anyhow!("there are {} takes", takes.len()))?;
    let (ta, tb) = (jfd(&t, "a", 0.0), jfd(&t, "b", 0.0));
    let (from, to) = (jfd(a, "from", ta).max(ta), jfd(a, "to", tb).min(tb));
    let (s0, s1) = (jfd(a, "speed", 1.0), jfd(a, "end_speed", jfd(a, "speed", 1.0)));
    let steps = if (s0 - s1).abs() < 1e-6 { 1 } else { jfd(a, "steps", 6.0).clamp(2.0, 24.0) as usize };
    let mut parts = vec![];
    if from - ta > 0.05 {
        parts.push(json!({"a": ta, "b": from}));
    }
    for k in 0..steps {
        let (x, y) = (from + (to - from) * k as f64 / steps as f64, from + (to - from) * (k + 1) as f64 / steps as f64);
        let sp = if steps == 1 { s0 } else { s0 + (s1 - s0) * (k as f64 + 0.5) / steps as f64 };
        parts.push(json!({"a": (x * 1000.0).round() / 1000.0, "b": (y * 1000.0).round() / 1000.0, "speed": (sp * 1000.0).round() / 1000.0}));
    }
    if tb - to > 0.05 {
        let mut last = json!({"a": to, "b": tb});
        if t.get("hold").is_some() {
            last["hold"] = t["hold"].clone();
        }
        parts.push(last);
    }
    takes.splice(i..=i, parts.clone());
    reel["takes"] = json!(takes);
    p.save_reel(&rid, &reel)?;
    Ok(json!({"takes": parts, "note": "a ramp is a run of short takes with stepped speeds; the voice is sped up with the picture"}))
}

const P: (&str, &str, &str) = ("project!", "string", "Project");
const R: (&str, &str, &str) = ("reel", "string", "Reel id");

pub const TOOLS: &[Tool] = &[
    Tool { name: "translate", desc: "Translate the reel's spoken sentences (any language) with the configured model (Claude, OpenAI, or a local Ollama / LM Studio server) and show them as captions (captions.lang). The translation is a file you can edit.", params: &[P, R, ("lang!", "string", "Target language code"), ("apply", "boolean", "Switch the captions to it (default true)")], long: true, app_only: false, run: t_translate },
    Tool { name: "dub", desc: "Dub the reel into another language. provider: elevenlabs-dubbing (ElevenLabs keeps the speaker's voice; API), elevenlabs or openai (translate, then a voice per sentence; API), piper (translate, then a local offline voice). The dub replaces the voice in renders (audio.dub).", params: &[P, R, ("lang!", "string", "Target language code"), ("provider", "string", "elevenlabs-dubbing | elevenlabs | openai | piper"), ("voice", "string", "Voice id / name"), ("apply", "boolean", "Use it in the reel (default true)")], long: true, app_only: false, run: t_dub },
    Tool { name: "heygen_translate", desc: "HeyGen video translation with lip sync (API): the reel's clean cut is uploaded, translated, and comes back as a new project.", params: &[P, R, ("language!", "string", "HeyGen language name, e.g. \"Spanish (Spain)\""), ("mode", "string", "speed | precision"), ("audio_only", "boolean", "Translate the audio only")], long: true, app_only: false, run: t_heygen },
    Tool { name: "broll_search", desc: "Find stock B-roll (Pexels or Pixabay, free keys) and download the best matches into the project's broll/ folder with their credits.", params: &[P, ("query!", "string", "What to show, in English"), ("source", "string", "pexels | pixabay"), ("kind", "string", "video | photo"), ("portrait", "boolean", "Vertical (default true)"), ("count", "number", "How many, default 3")], long: true, app_only: false, run: t_broll_search },
    Tool { name: "broll_gen", desc: "Generate a picture for B-roll: openai (gpt-image-1), gemini, or local (an Automatic1111 / Forge server, images.local_url).", params: &[P, ("prompt!", "string", "The picture"), ("provider", "string", "openai | gemini | local"), ("portrait", "boolean", "Vertical (default true)")], long: true, app_only: false, run: t_broll_gen },
    Tool { name: "broll_auto", desc: "Automatic B-roll: picks the sentences that want a picture (with the configured model, else by keywords), finds or generates one for each and places it as a beat. Talking-head budget: about 3 a minute.", params: &[P, R, ("source", "string", "pexels | pixabay | openai | gemini | local"), ("mode", "string", "split (picture over the top half) | full"), ("count", "number", "How many"), ("per_minute", "number", "Default 3")], long: true, app_only: false, run: t_broll_auto },
    Tool { name: "eye_contact", desc: "Correct the speaker's gaze toward the camera. provider command: a local tool from settings (eye_contact.command with {in} {out}); sieve: Sieve's API (needs a public video_url). The project then uses the corrected video.", params: &[P, ("provider", "string", "command | sieve"), ("video_url", "string", "Public URL of the source (sieve)")], long: true, app_only: false, run: t_eye },
    Tool { name: "speed", desc: "Change the speed of a take, or ramp it: speed (and end_speed for a ramp) over from..to (original seconds) inside take `take`. The voice speeds up with the picture.", params: &[P, R, ("take!", "number", "Take index"), ("speed!", "number", "0.25..4"), ("end_speed", "number", "Ramp to this speed"), ("from", "number", "Ramp start s"), ("to", "number", "Ramp end s"), ("steps", "number", "Ramp steps, default 6")], long: false, app_only: false, run: t_speed },
];
