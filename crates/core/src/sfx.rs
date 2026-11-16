//! The sound kit: the CC0 pack in the library, a small synthesized kit for the everyday UI sounds,
//! levels set against the voice, and the mixing rules.
use crate::beats::Sfx;
use crate::config::*;
use crate::proc;
use crate::timeline::EWord;
use crate::util::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::OnceLock;

fn manifest() -> &'static Value {
    static M: OnceLock<Value> = OnceLock::new();
    M.get_or_init(|| read_json_or(&library_dir().join("sfx").join("manifest.json"), json!({})))
}

/// (lavfi source, extra audio filter, role) for the sounds Kesit makes itself.
fn synth(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match id {
        "pop-1" => ("aevalsrc=0.8*sin(2*PI*(430+700*exp(-t*36))*t)*exp(-t*34):d=0.14:s=48000", "afade=t=out:st=0.1:d=0.04", "ui"),
        "pop-2" | "pop-3" => ("aevalsrc=0.75*sin(2*PI*(620+900*exp(-t*40))*t)*exp(-t*38):d=0.12:s=48000", "afade=t=out:st=0.09:d=0.03", "ui"),
        "click-1" | "shutter-1" => ("aevalsrc=0.6*(random(0)*2-1)*exp(-t*240)+0.45*sin(2*PI*2100*t)*exp(-t*170):d=0.06:s=48000", "afade=t=out:st=0.04:d=0.02", "ui"),
        "tick-1" => ("aevalsrc=0.55*sin(2*PI*1750*t)*exp(-t*130):d=0.05:s=48000", "afade=t=out:st=0.03:d=0.02", "ui"),
        "typing-1" | "tech-keyboard-burst" => ("aevalsrc=0.5*(random(0)*2-1)*exp(-(t*11.7-floor(t*11.7))*15):d=0.9:s=48000", "highpass=f=900,afade=t=out:st=0.8:d=0.1", "ui"),
        "ding-1" | "kaching-1" | "tech-ui-confirm" => ("aevalsrc=0.55*sin(2*PI*1320*t)*exp(-t*5)+0.28*sin(2*PI*2640*t)*exp(-t*8):d=0.9:s=48000", "afade=t=out:st=0.7:d=0.2", "ui"),
        "whoosh-1" | "whoosh-2" | "whoosh-3" | "send-swoosh" | "tech-digital-whoosh" => ("anoisesrc=color=pink:d=0.55:a=0.9:r=48000", "highpass=f=350,lowpass=f=5200,afade=t=in:d=0.3:curve=qsin,afade=t=out:st=0.3:d=0.25,volume=2.2", "whoosh"),
        "whoosh-cine" => ("anoisesrc=color=pink:d=0.34:a=0.9:r=48000", "highpass=f=250,lowpass=f=3200,afade=t=in:d=0.2:curve=qsin,afade=t=out:st=0.2:d=0.14,volume=2.4", "whoosh"),
        "riser-1" | "cinematic-rumble-riser" => ("anoisesrc=color=pink:d=1.2:a=0.9:r=48000", "highpass=f=500,afade=t=in:d=1.15:curve=exp,afade=t=out:st=1.15:d=0.05,volume=2.2", "whoosh"),
        "impact-1" | "impact-2" | "cinematic-cine-hit" | "wood-hit" => ("aevalsrc=0.95*sin(2*PI*(46+70*exp(-t*20))*t)*exp(-t*7)+0.3*(random(0)*2-1)*exp(-t*70):d=0.7:s=48000", "afade=t=out:st=0.5:d=0.2", "impact"),
        "buzzer-1" | "tech-ui-error" => ("aevalsrc=0.4*sgn(sin(2*PI*118*t)):d=0.42:s=48000", "lowpass=f=1800,afade=t=in:d=0.01,afade=t=out:st=0.36:d=0.06", "meme"),
        _ => return None,
    })
}

fn alias(id: &str) -> &str {
    match id {
        "vine-boom" | "cinematic-deep-boom" => "deep-boom",
        "record-scratch" => "vinyl-scratch",
        "tech-glitch-1" | "tech-glitch-2" => "glitch-hit",
        "reaction-tape-rewind" => "whoosh-reverse",
        x => x,
    }
}

pub struct Sound {
    pub path: PathBuf,
    pub role: String,
    pub peak_db: f64,
}

/// A sound by id: the library file when it is there, else the synthesized kit (made once, cached).
pub fn resolve(cfg: &Config, id: &str) -> Option<Sound> {
    let id = alias(id);
    let m = &manifest()[id];
    if m.is_object() {
        let p = library_dir().join("sfx").join(jsd(m, "file", ""));
        if p.is_file() {
            return Some(Sound { path: p, role: jsd(m, "role", "ui").into(), peak_db: jfd(m, "peak_db", -1.0) });
        }
    }
    let (src, af, role) = synth(id)?;
    let dir = data_dir().join("sfx");
    let out = dir.join(format!("{id}.wav"));
    if !out.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        let ff = find_tool(cfg, "ffmpeg")?;
        let o = proc::run(proc::cmd(&ff).args(["-hide_banner", "-y", "-v", "error", "-f", "lavfi", "-i", src, "-af", af, "-ac", "2", "-ar", "48000"]).arg(&out)).ok()?;
        if !o.ok {
            return None;
        }
    }
    Some(Sound { path: out, role: role.into(), peak_db: -3.0 })
}

pub fn catalog(cfg: &Config) -> Vec<Value> {
    let mut out = vec![];
    if let Some(o) = manifest().as_object() {
        for (id, m) in o {
            let here = library_dir().join("sfx").join(jsd(m, "file", "")).is_file();
            let made = synth(id).is_some();
            if here || made {
                out.push(json!({"id": id, "role": m["role"], "tags": m["tags"], "duration": m["duration"], "source": if here { "library (CC0)" } else { "synthesized" },
                                "path": if here { json!(fwd(&library_dir().join("sfx").join(jsd(m, "file", "")))) } else { resolve(cfg, id).map(|s| json!(fwd(&s.path))).unwrap_or(Value::Null) }}));
            }
        }
    }
    out
}

/// Gain for each sound so its peak sits `roleDb` under the voice's peak. Returns (event, file, gain dB).
pub fn levels(cfg: &Config, events: &[Sfx], reel: &Value, audio: &Value, warn: &mut Vec<String>) -> Vec<(Sfx, PathBuf, f64)> {
    let lufs = jfd(audio, "lufs", -20.0);
    let voice_peak = (jfd(audio, "true_peak", -4.0) + (-16.0 - lufs)).min(-1.0).max(-12.0);
    let mut out = vec![];
    for e in events {
        let Some(snd) = resolve(cfg, &e.id) else {
            warn.push(format!("sound '{}' at {} is not in the library (`library` lists what is)", e.id, fmt_t(e.t)));
            continue;
        };
        let role = if e.role.is_empty() { snd.role.clone() } else { e.role.clone() };
        let target = jf(&reel["mix"]["roleDb"], &role).unwrap_or(match role.as_str() {
            "ui" => -14.0,
            "whoosh" | "riser" | "foley" => -12.0,
            "impact" => -8.0,
            "meme" => -3.0,
            _ => -12.0,
        });
        let gain = voice_peak + target - snd.peak_db + e.db;
        let mut ev = e.clone();
        ev.role = role;
        out.push((ev, snd.path, (gain * 10.0).round() / 10.0));
    }
    out.sort_by(|a, b| a.0.t.partial_cmp(&b.0.t).unwrap());
    out
}

/// The rules that came from getting it wrong.
pub fn rules(events: &[(Sfx, PathBuf, f64)], words: &[EWord], total: f64, warn: &mut Vec<String>) {
    let minutes = (total / 60.0).max(0.25);
    let memes: Vec<&Sfx> = events.iter().map(|e| &e.0).filter(|e| e.role == "meme").collect();
    for (i, m) in memes.iter().enumerate() {
        if memes[..i].iter().any(|p| p.id == m.id) {
            warn.push(format!("the meme sound '{}' plays twice in one reel", m.id));
        }
        if i > 0 && m.t - memes[i - 1].t < 8.0 {
            warn.push(format!("meme sounds at {} and {} are less than 8 s apart", fmt_t(memes[i - 1].t), fmt_t(m.t)));
        }
        if words.iter().any(|w| w.s < m.t + 0.35 && w.e > m.t + 0.05) {
            warn.push(format!("the meme sound '{}' at {} lands on speech; give it a pause (a take `hold`, or end the take later)", m.id, fmt_t(m.t)));
        }
    }
    if memes.len() as f64 / minutes > 3.5 {
        warn.push(format!("{} meme sounds in {:.0} s is more than 3.5 a minute", memes.len(), total));
    }
    let mut hits: Vec<f64> = events.iter().filter(|e| matches!(e.0.role.as_str(), "whoosh" | "impact" | "meme")).map(|e| e.0.t).collect();
    hits.dedup_by(|a, b| (*a - *b).abs() < 0.25);
    if hits.len() as f64 / minutes > 14.0 {
        warn.push(format!("{} whoosh / impact / meme hits is more than 14 a minute; default to restraint", hits.len()));
    }
}
