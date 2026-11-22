//! reel.json -> one ffmpeg run: cut the takes, frame them for the output shape, zoom, lay pictures over,
//! burn the overlay script in, mix voice, sounds and music. Also single frames, and the render check.
use crate::analyze;
use crate::ass::{self, CapCtx, Doc, Rect};
use crate::beats;
use crate::config::*;
use crate::media::h264_encoder;
use crate::proc::{self, Rep};
use crate::project::Project;
use crate::sfx;
use crate::styles;
use crate::timeline::Timeline;
use crate::util::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Preview,
    Final,
    /// One frame at this edit time.
    Frame(f64),
    /// The cut and framed picture only (no overlays, no sound), small: what the person matte is made from.
    Plate,
}

pub struct Plan {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub total: f64,
    pub report: Value,
}

/// A piece of a take with one framing.
struct Piece {
    a: f64,
    b: f64,
    start: f64,
    hold: f64,
    crop: (f64, f64, f64, f64),
    origin: (f64, f64),
    speed: f64,
    /// Camera path inside the piece: (edit seconds into the piece, crop x px). Empty = still.
    xpath: Vec<(f64, f64)>,
    /// Two speakers at once: the two faces (cx, cy in 0..1), drawn top and bottom.
    split: Option<[(f64, f64); 2]>,
    face: Option<(f64, f64, f64, f64)>,
}

fn even(x: f64) -> i64 {
    ((x / 2.0).round() as i64 * 2).max(2)
}

fn pct(s: &str) -> Option<(f64, f64)> {
    let mut it = s.split_whitespace().map(|p| p.trim_end_matches('%').parse::<f64>().ok());
    Some((it.next()?? / 100.0, it.next()?? / 100.0))
}

/// CSS-style filter text ("contrast(1.06) saturate(0.82) grayscale(1)") as ffmpeg filters.
fn grade(css: &str) -> String {
    let mut out: Vec<String> = vec![];
    for part in css.split(')') {
        let Some((name, val)) = part.trim().split_once('(') else { continue };
        let v: f64 = val.trim().trim_end_matches('%').parse().unwrap_or(1.0);
        match name.trim() {
            "grayscale" if v > 0.0 => out.push("hue=s=0".into()),
            "contrast" => out.push(format!("eq=contrast={v:.3}")),
            "saturate" => out.push(format!("eq=saturation={v:.3}")),
            "brightness" => out.push(format!("eq=brightness={:.3}", v - 1.0)),
            _ => {}
        }
    }
    out.join(",")
}

fn ffmpeg_major(ff: &Path) -> u32 {
    proc::run(proc::cmd(ff).arg("-version")).ok().and_then(|o| o.stdout.split_whitespace().nth(2).and_then(|v| v.trim_start_matches('n').split('.').next().and_then(|m| m.parse().ok()))).unwrap_or(7)
}

fn sync_fonts(build: &Path) {
    let dst = build.join("fonts");
    let _ = std::fs::create_dir_all(&dst);
    if let Ok(rd) = std::fs::read_dir(crate::fonts::fonts_dir()) {
        for e in rd.flatten() {
            let to = dst.join(e.file_name());
            let same = to.metadata().ok().map(|m| m.len()) == e.metadata().ok().map(|m| m.len());
            if !same {
                let _ = std::fs::copy(e.path(), to);
            }
        }
    }
}

pub fn build(cfg: &Config, p: &Project, rid: &str, mode: Mode, out: &Path) -> Result<Plan> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let reel = styles::resolve(&p.load_reel(rid)?);
    let m = p.media();
    let src = p.source()?;
    let tl = Timeline::from_reel(&reel, m.duration);
    let fps = tl.fps;
    let (w, h) = (even(jfd(&reel, "width", 1080.0)) as f64, even(jfd(&reel, "height", 1920.0)) as f64);
    let (sw, sh) = (m.width as f64, m.height as f64);
    let mut warn: Vec<String> = vec![];
    let ewords = match js(&reel["captions"], "lang").and_then(|l| read_json(&p.p(&format!("build/{rid}.tr.{l}.json"))).ok()) {
        Some(tr) => translated_words(&tr, &tl),
        None => p.words().map(|x| tl.edit_words(&x.words)).unwrap_or_default(),
    };
    let build_dir = p.p("build");
    std::fs::create_dir_all(&build_dir)?;

    // ---- framing ----
    let face_json = read_json_or(&p.p("build/face.json"), json!({}));
    let person_json = read_json_or(&p.p("build/person.json"), json!({}));
    let track = jsd(&reel["crop"], "track", "face").to_string();
    let track_json = if track == "person" && !ja(&person_json, "samples").is_empty() { &person_json } else { &face_json };
    let speakers = read_json_or(&p.p("build/speakers.json"), json!({}));
    let spk_mode = !ja(&speakers, "segments").is_empty() && (jsd(&reel["crop"], "mode", "auto") == "speaker" || (jsd(&reel["crop"], "mode", "auto") == "auto" && ja(&speakers, "faces").len() > 1));
    let crop_mode = jsd(&reel["crop"], "mode", "auto").to_string();
    let fit = crop_mode == "fit";
    let ar = w / h;
    let (cw0, ch0) = if sw / sh > ar { (sh * ar, sh) } else { (sw, sw / ar) };
    let zbase = jfd(&reel["zoom"], "base", 1.0).max(1.0);
    let mut snaps: Vec<(f64, f64)> = ja(&reel["zoom"], "snaps").iter().filter_map(|s| Some((s.get(0)?.as_f64()?, s.get(1)?.as_f64()?))).collect();
    snaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut pieces: Vec<Piece> = vec![];
    for seg in &tl.segs {
        let mut cuts = vec![seg.a];
        for (t, _) in &snaps {
            if *t > seg.a + 2.5 / fps && *t < seg.b - 2.5 / fps {
                cuts.push((t * fps).round() / fps);
            }
        }
        if spk_mode {
            for sg in ja(&speakers, "segments") {
                let t = jfd(sg, "t0", 0.0);
                if t > seg.a + 2.5 / fps && t < seg.b - 2.5 / fps {
                    cuts.push((t * fps).round() / fps);
                }
            }
        }
        cuts.push(seg.b);
        cuts.sort_by(|x, y| x.partial_cmp(y).unwrap());
        cuts.dedup_by(|x, y| (*x - *y).abs() < 1.5 / fps);
        let face = analyze::face_at(track_json, seg.a, seg.b);
        let follow = crop_mode == "auto" && !fit && track != "off" && sw / sh > ar + 0.05;
        let path = if follow { cam_path(track_json, seg.a, seg.b, sw, cw0) } else { vec![] };
        let fx = match crop_mode.as_str() {
            "center" => 0.5,
            "manual" => jfd(&reel["crop"], "x", 0.5),
            _ => jf(&reel["crop"], "x").or(face.map(|f| f.0)).unwrap_or(0.5),
        };
        let fy = jf(&reel["crop"], "y").unwrap_or(if (sh - ch0) > 2.0 { face.map(|f| (f.1 + 0.08).min(1.0)).unwrap_or(0.45) } else { 0.5 });
        let bx = clampf(fx * sw - cw0 / 2.0, 0.0, sw - cw0);
        let by = clampf(fy * sh - ch0 / 2.0, 0.0, sh - ch0);
        let origin = js(&reel["zoom"], "origin").and_then(pct).unwrap_or_else(|| match face {
            Some(f) => (clampf((f.0 * sw - bx) / cw0, 0.1, 0.9), clampf((f.1 * sh - by) / ch0, 0.1, 0.9)),
            None => (0.5, 0.3),
        });
        for k in 0..cuts.len() - 1 {
            let (a, b) = (cuts[k], cuts[k + 1]);
            let z = (zbase * snaps.iter().filter(|s| s.0 <= a + 1.0 / fps).last().map(|s| s.1).unwrap_or(1.0)).max(1.0);
            let (cw, ch) = if fit { (sw, sh) } else { (cw0 / z, ch0 / z) };
            let crop = if fit { (sw, sh, 0.0, 0.0) } else { (cw, ch, bx + origin.0 * (cw0 - cw), by + origin.1 * (ch0 - ch)) };
            let xpath: Vec<(f64, f64)> = if path.len() > 1 && !fit {
                let off = origin.0 * (cw0 - crop.0);
                let mut v: Vec<(f64, f64)> = vec![(0.0, path_at(&path, a) + off)];
                for (t, x) in path.iter().filter(|(t, _)| *t > a && *t < b) {
                    v.push(((t - a) / seg.speed, x + off));
                }
                v.push(((b - a) / seg.speed, path_at(&path, b) + off));
                let (lo, hi) = v.iter().fold((f64::MAX, f64::MIN), |m, x| (m.0.min(x.1), m.1.max(x.1)));
                if hi - lo < 0.01 * sw { vec![] } else { v }
            } else {
                vec![]
            };
            let mut crop = crop;
            let mut xpath = xpath;
            let mut split = None;
            if spk_mode && !fit {
                let act = active_at(&speakers, (a + b) / 2.0);
                let fc = |id: usize| ja(&speakers, "faces").get(id).map(|f| (jfd(f, "cx", 0.5), jfd(f, "cy", 0.4)));
                if act.len() >= 2 {
                    if let (Some(f0), Some(f1)) = (fc(act[0]), fc(act[1])) {
                        split = Some(if f0.0 <= f1.0 { [f0, f1] } else { [f1, f0] });
                    }
                } else if let Some(f) = act.first().and_then(|i| fc(*i)) {
                    let bx2 = clampf(f.0 * sw - cw0 / 2.0, 0.0, sw - cw0);
                    crop.2 = bx2 + origin.0 * (cw0 - crop.0);
                    xpath = vec![];
                }
            }
            pieces.push(Piece { a, b, start: seg.start + (a - seg.a) / seg.speed, hold: if k + 2 == cuts.len() { seg.hold } else { 0.0 }, crop, origin, face, speed: seg.speed, xpath, split });
        }
    }
    if crop_mode == "auto" && sw / sh > ar + 0.05 && face_json["samples"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
        warn.push("no face track: the wide source is framed on its centre. Run `analyze` with faces, or set crop.x (0..1)".into());
    }
    // The face in output pixels at an edit time (head top to chin), for placing captions and cards.
    let face_out = |t: f64| -> Option<Rect> {
        if fit {
            return None;
        }
        let pc = pieces.iter().rev().find(|x| t >= x.start - 0.001)?;
        let f = pc.face?;
        let (cw, ch, cx, cy) = pc.crop;
        let to = |nx: f64, ny: f64| ((nx * sw - cx) / cw * w, (ny * sh - cy) / ch * h);
        let (x0, y0) = to(f.0 - f.2 * 0.6, f.1 - f.3 * 0.85);
        let (x1, y1) = to(f.0 + f.2 * 0.6, f.1 + f.3 * 0.62);
        (x1 > 0.0 && x0 < w && y1 > 0.0 && y0 < h).then_some(Rect { x0, y0, x1, y1 })
    };

    // ---- the overlay script ----
    let mut doc = Doc::new(w, h);
    let sc = doc.cv.sc;
    let bo = beats::beats(&mut doc, &reel, &tl, &ewords, &p.dir, &face_out);
    let (st, sb, sr) = match jsd(&reel, "platform", "instagram") {
        "tiktok" => (130.0, 480.0, 130.0),
        "shorts" => (120.0, 460.0, 120.0),
        "all" => (130.0, 480.0, 130.0),
        "none" => (40.0, 60.0, 40.0),
        _ => (120.0, 420.0, 100.0),
    };
    let ctx = CapCtx { tl: &tl, hide: &bo.hide, obstacles: &bo.obstacles, face: &face_out, safe_top: st * sc, safe_bottom: h - sb * sc, safe_right: sr * sc };
    doc.base = 20;
    let cap_log = ass::captions(&mut doc, &reel["_cap"], &ewords, &ctx);
    doc.base = 0;
    if ewords.is_empty() && jsd(&reel["_cap"], "style", "") != "none" {
        warn.push("no transcript: the reel has no captions. Run `transcribe` first".into());
    }
    for (want, got) in &doc.subst {
        warn.push(format!("font '{want}' is not installed; '{got}' stands in (put the .ttf files in library/fonts)"));
    }
    warn.extend(doc.warn.clone());
    let ass_name = format!("{rid}.ass");
    write_text(&build_dir.join(&ass_name), &doc.render())?;
    sync_fonts(&build_dir);

    // ---- budgets ----
    let full: f64 = bo.table.iter().filter_map(|t| t["fullscreen"].as_array()).map(|a| a[1].as_f64().unwrap_or(0.0) - a[0].as_f64().unwrap_or(0.0)).sum();
    if full / tl.total.max(0.1) > 0.25 {
        warn.push(format!("full-screen B-roll covers {:.0} % of the reel; a talking head keeps it under 25 %", 100.0 * full / tl.total));
    }

    // ---- sounds ----
    let audio_stats = read_json_or(&p.p("build/audio.json"), json!({}));
    let frame_mode = matches!(mode, Mode::Frame(_));
    let sounds = if frame_mode { vec![] } else { sfx::levels(cfg, &bo.sfx, &reel, &audio_stats, &mut warn) };
    sfx::rules(&sounds, &ewords, tl.total, &mut warn);

    // ---- ffmpeg inputs ----
    let grid = |t: f64| (t * fps).floor() / fps;
    let (ft, fpiece) = match mode {
        Mode::Frame(t) => {
            let t = clampf(t, 0.0, tl.total - 1.0 / fps);
            let i = pieces.iter().rposition(|x| t >= x.start - 0.0005).unwrap_or(0);
            (t, Some(i))
        }
        _ => (0.0, None),
    };
    let used: Vec<&Piece> = match fpiece {
        Some(i) => vec![&pieces[i]],
        None => pieces.iter().collect(),
    };
    let frame_orig = fpiece.map(|i| {
        let pc = &pieces[i];
        grid(clampf(pc.a + (ft - pc.start), pc.a, pc.b - 1.0 / fps))
    });
    let min_a = frame_orig.unwrap_or_else(|| used.iter().map(|x| x.a).fold(f64::MAX, f64::min));
    let max_b = frame_orig.map(|a| a + 1.0).unwrap_or_else(|| used.iter().map(|x| x.b).fold(0.0, f64::max));
    let ss = grid((min_a - 1.0).max(0.0));
    let mut args: Vec<String> = vec!["-ss".into(), format!("{ss:.6}"), "-t".into(), format!("{:.3}", max_b - ss + 1.5), "-i".into(), fwd(&src)];
    let mut n_in = 1;
    let mut g: Vec<String> = vec![];

    // ---- video ----
    let tone = if m.hdr { "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable,zscale=t=bt709:m=bt709:r=tv,format=yuv420p," } else { "" };
    let n = used.len();
    let half = 0.25 / fps;
    if n > 1 {
        g.push(format!("[0:v]{tone}fps={fps},split={n}{}", (0..n).map(|i| format!("[s{i}]")).collect::<String>()));
    } else {
        g.push(format!("[0:v]{tone}fps={fps}[s0]"));
    }
    let pushes: Vec<(f64, f64, f64, f64, f64)> = ja(&reel["zoom"], "pushes")
        .iter()
        .filter_map(|pz| {
            let at = tl.resolve(&pz["at"])?;
            let until = pz.get("until").and_then(|u| tl.resolve(u)).unwrap_or(at + 2.0);
            Some((at, jfd(pz, "z", 1.08).max(1.0), jfd(pz, "up", 0.4).max(0.05), until, jfd(pz, "down", 0.2).max(0.05)))
        })
        .collect();
    for (i, pc) in used.iter().enumerate() {
        let (a, b) = match frame_orig {
            Some(fo) => (fo, fo + 1.0 / fps),
            None => (pc.a, pc.b),
        };
        let spd = if (pc.speed - 1.0).abs() > 1e-6 && !frame_mode { format!("/{:.5}", pc.speed) } else { String::new() };
        let mut c = format!("[s{i}]trim=start={:.6}:end={:.6},setpts=(PTS-STARTPTS){spd}", a - ss - half, b - ss - half);
        if let (Some(sp), false) = (pc.split, fit) {
            let hh = even(h / 2.0) as f64;
            let r = w / hh;
            let mut chh = sh * 0.62;
            let mut cww = chh * r;
            if cww > sw * 0.5 {
                cww = sw * 0.5;
                chh = cww / r;
            }
            let (cww, chh) = (even(cww), even(chh));
            let pos = |f: (f64, f64)| (clampf(f.0 * sw - cww as f64 / 2.0, 0.0, sw - cww as f64) as i64, clampf(f.1 * sh - chh as f64 * 0.42, 0.0, sh - chh as f64) as i64);
            let (p0, p1) = (pos(sp[0]), pos(sp[1]));
            c.push_str(&format!(
                ",split=2[sa{i}][sb{i}];[sa{i}]crop={cww}:{chh}:{}:{},scale={}:{}:flags=bicubic,setsar=1[ta{i}];[sb{i}]crop={cww}:{chh}:{}:{},scale={}:{}:flags=bicubic,setsar=1[tb{i}];[ta{i}][tb{i}]vstack=inputs=2",
                p0.0, p0.1, w as i64, hh as i64, p1.0, p1.1, w as i64, h as i64 - hh as i64
            ));
            if pc.hold > 0.0 && !frame_mode {
                c.push_str(&format!(",tpad=stop_mode=clone:stop_duration={:.4}", pc.hold));
            }
            g.push(format!("{c}[v{i}]"));
            continue;
        }
        if !fit {
            let (cw, ch, cx, cy) = pc.crop;
            let (cw, ch) = (even(cw.min(sw)), even(ch.min(sh)));
            let xmax = sw - cw as f64;
            let xs = if pc.xpath.is_empty() {
                format!("{}", (cx.max(0.0).min(xmax)) as i64)
            } else if let Some(fo) = frame_orig {
                format!("{}", path_at(&pc.xpath, (fo - pc.a) / pc.speed).clamp(0.0, xmax) as i64)
            } else {
                format!("'{}'", path_expr(&pc.xpath, xmax))
            };
            c.push_str(&format!(",crop={cw}:{ch}:{xs}:{},scale={w}:{h}:flags=bicubic,setsar=1", (cy.max(0.0).min(sh - ch as f64)) as i64));
            let (ps, pe) = (pc.start, pc.start + (pc.b - pc.a) / pc.speed);
            let hit: Vec<&(f64, f64, f64, f64, f64)> = pushes.iter().filter(|x| x.0 < pe && x.3 + x.4 > ps).collect();
            if !hit.is_empty() && !frame_mode {
                let off = pc.start;
                let terms: Vec<String> = hit.iter().map(|x| format!("(1+{:.4}*clip((it+{off:.4}-{:.4})/{:.3},0,1)*clip(({:.4}-it-{off:.4})/{:.3},0,1))", x.1 - 1.0, x.0, x.2, x.3 + x.4, x.4)).collect();
                c.push_str(&format!(
                    ",scale={}:{}:flags=bicubic,zoompan=z='{}':x='(iw-iw/zoom)*{:.4}':y='(ih-ih/zoom)*{:.4}':d=1:s={}x{}:fps={fps}",
                    even(w * 1.5), even(h * 1.5), terms.join("*"), pc.origin.0, pc.origin.1, w as i64, h as i64
                ));
            }
        } else {
            c.push_str(",setsar=1");
        }
        if pc.hold > 0.0 && !frame_mode {
            c.push_str(&format!(",tpad=stop_mode=clone:stop_duration={:.4}", pc.hold));
        }
        g.push(format!("{c}[v{i}]"));
    }
    let mut cur = if n > 1 {
        g.push(format!("{}concat=n={n}:v=1:a=0[vc]", (0..n).map(|i| format!("[v{i}]")).collect::<String>()));
        "vc".to_string()
    } else {
        "v0".to_string()
    };
    let mut chain: Vec<String> = vec![];
    if fit {
        g.push(format!("[{cur}]split=2[fb][ff]"));
        g.push(format!("[fb]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},boxblur=24:2,eq=brightness=-0.08[fbb]"));
        g.push(format!("[ff]scale={w}:{h}:force_original_aspect_ratio=decrease[fff]"));
        g.push("[fbb][fff]overlay=(W-w)/2:(H-h)/2[vfit]".into());
        cur = "vfit".into();
    }
    if let Mode::Frame(_) = mode {
        chain.push(format!("setpts=PTS-STARTPTS+{ft:.4}/TB"));
    } else if tl.outro > 0.0 {
        chain.push(format!("tpad=stop_mode=clone:stop_duration={:.4}", tl.outro));
    }
    let look = &reel["look"];
    if let Some(gr) = js(look, "grade") {
        if look.get("asked").is_some() {
            let f = grade(gr);
            if !f.is_empty() {
                chain.push(f);
            }
        } else {
            warn.push("look.grade is ignored: the speaker's face stays as shot unless look.asked holds the user's words asking for a grade".into());
        }
    }
    // a .cube LUT (look.lut: a file in the project, or anywhere), copied next to the build so ffmpeg reads it by name
    if let Some(lut) = js(look, "lut").filter(|s| !s.is_empty()) {
        let f = if Path::new(lut).is_absolute() { PathBuf::from(lut) } else { p.dir.join(lut) };
        if look.get("asked").is_none() {
            warn.push("look.lut is ignored: the speaker's face stays as shot unless look.asked holds the user's words asking for a grade".into());
        } else if f.is_file() && f.extension().map(|e| e.eq_ignore_ascii_case("cube")).unwrap_or(false) {
            let name = format!("{}.look.cube", slug(rid));
            if std::fs::copy(&f, build_dir.join(&name)).is_ok() {
                chain.push(format!("lut3d=file={name}:interp=tetrahedral"));
            }
        } else {
            warn.push(format!("look.lut: no .cube file at {}", fwd(&f)));
        }
    }
    if jfd(look, "grain", 0.0) > 0.0 {
        chain.push(format!("noise=alls={:.0}:allf=t+u", (jfd(look, "grain", 0.0) * 100.0).min(30.0)));
    }
    if jfd(look, "vignette", 0.0) > 0.0 {
        chain.push(format!("vignette=angle={:.3}", 0.35 + 0.75 * jfd(look, "vignette", 0.0).min(1.0)));
    }
    if !chain.is_empty() {
        g.push(format!("[{cur}]{}[vl]", chain.join(",")));
        cur = "vl".into();
    }
    if mode == Mode::Plate {
        g.push(format!("[{cur}]scale={}:{}:flags=bicubic,format=yuv420p[vout]", even(w * 0.5), even(h * 0.5)));
    } else {
        // ---- the person matte: effects on the world, words behind the speaker, the speaker on top ----
        let mut wins: Vec<(f64, f64)> = bo.behind.iter().cloned().chain(bo.cutouts.iter().map(|c| (c.0, c.1))).collect();
        if let Mode::Frame(t) = mode {
            wins.retain(|w| t >= w.0 - 0.1 && t <= w.1 + 0.1);
        }
        if !wins.is_empty() {
            match ensure_mattes(cfg, p, rid, &reel, &tl, &wins) {
                Ok(mats) => {
                    let n = mats.len();
                    g.push(format!("[{cur}]split=2[mbg][mfg]"));
                    if n > 1 {
                        g.push(format!("[mfg]split={n}{}", (0..n).map(|i| format!("[mf{i}]")).collect::<String>()));
                    } else {
                        g.push("[mfg]null[mf0]".into());
                    }
                    let mut bg = "mbg".to_string();
                    let mut fx: Vec<String> = vec![];
                    for (s0, e0, v) in &bo.cutouts {
                        let en = format!("enable='between(t,{s0:.4},{e0:.4})'");
                        match jsd(v, "fx", "bg-blur") {
                            "bg-blur" => fx.push(format!("boxblur={}:2:{en}", jfd(v, "radius", 20.0).clamp(2.0, 60.0) as i64)),
                            "selective" => fx.push(format!("hue=s=0:{en}")),
                            "spotlight" => fx.push(format!("eq=brightness={:.3}:saturation=0.7:{en}", -0.6 * jfd(v, "dim", 0.3))),
                            "bg-swap" if js(v, "bg").map(|b| b.starts_with('#')).unwrap_or(true) => {
                                fx.push(format!("drawbox=x=0:y=0:w=iw:h=ih:color={}@1:t=fill:{en}", jsd(v, "bg", "#111111").split(',').next().unwrap_or("#111111")))
                            }
                            "bg-swap" => {
                                let img = p.dir.join(jsd(v, "bg", ""));
                                if img.is_file() {
                                    args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{:.3}", tl.total + 1.0), "-i".into(), fwd(&img)]);
                                    g.push(format!("[{n_in}:v]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1[bgi{n_in}]"));
                                    g.push(format!("[{bg}][bgi{n_in}]overlay=0:0:{en}[bgo{n_in}]"));
                                    bg = format!("bgo{n_in}");
                                    n_in += 1;
                                } else {
                                    warn.push(format!("cutout bg-swap at {}: the background '{}' is missing", fmt_t(*s0), jsd(v, "bg", "")));
                                }
                            }
                            other => warn.push(format!("cutout '{other}' is not in this build (bg-blur, selective, spotlight, bg-swap are)")),
                        }
                    }
                    if !fx.is_empty() {
                        g.push(format!("[{bg}]{}[mbgfx]", fx.join(",")));
                        bg = "mbgfx".into();
                    }
                    if doc.has_layers(1000, 100000) {
                        let behind = format!("{rid}.behind.ass");
                        write_text(&build_dir.join(&behind), &doc.render_layers(1000, 100000))?;
                        g.push(format!("[{bg}]subtitles=f={behind}:fontsdir=fonts[mbgt]"));
                        bg = "mbgt".into();
                    }
                    for (i, (s0, e0, file)) in mats.iter().enumerate() {
                        let shift = match mode {
                            Mode::Frame(t) => {
                                args.extend(["-ss".into(), format!("{:.4}", (t - s0).max(0.0))]);
                                t
                            }
                            _ => *s0,
                        };
                        args.extend(["-i".into(), fwd(file)]);
                        g.push(format!("[{n_in}:v]setpts=PTS-STARTPTS+{shift:.4}/TB,scale={w}:{h}:flags=bicubic,format=gray[ma{i}]"));
                        n_in += 1;
                        g.push(format!("[mf{i}]trim=start={s0:.4}:end={e0:.4}[mft{i}]"));
                        g.push(format!("[mft{i}][ma{i}]alphamerge[fg{i}]"));
                        g.push(format!("[{bg}][fg{i}]overlay=0:0:eof_action=pass:enable='between(t,{s0:.4},{e0:.4})'[mo{i}]"));
                        bg = format!("mo{i}");
                    }
                    cur = bg;
                }
                Err(e) => warn.push(format!("person matte not made: {e}")),
            }
        }
    }
    // ---- pictures and clips over the plate ----
    let mut clip_audio: Vec<(usize, f64)> = vec![];
    for (k, o) in bo.overlays.iter().enumerate() {
        if mode == Mode::Plate {
            break;
        }
        if frame_mode && !(ft >= o.s && ft < o.e) {
            continue;
        }
        let dur = o.e - o.s;
        if o.video {
            let inp = if frame_mode { o.inpoint + (ft - o.s) } else { o.inpoint };
            args.extend(["-ss".into(), format!("{inp:.3}"), "-t".into(), format!("{:.3}", if frame_mode { 0.5 } else { dur + 0.1 }), "-i".into(), fwd(&o.src)]);
        } else {
            args.extend(["-loop".into(), "1".into(), "-t".into(), format!("{:.3}", if frame_mode { 0.5 } else { dur + 0.1 }), "-i".into(), fwd(&o.src)]);
        }
        let idx = n_in;
        n_in += 1;
        let size = if o.h > 0.0 { format!("scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", even(o.w), even(o.h), even(o.w), even(o.h)) } else { format!("scale={}:-2", even(o.w)) };
        let gr = grade(&o.grade);
        let shift = if frame_mode { ft } else { o.s };
        g.push(format!("[{idx}:v]fps={fps},{size}{}{},format=rgba,setpts=PTS-STARTPTS+{shift:.4}/TB[o{k}]", if gr.is_empty() { "" } else { "," }, gr));
        g.push(format!("[{cur}][o{k}]overlay={}:{}:enable='between(t,{:.4},{:.4})':eof_action=pass[vo{k}]", o.x.round() as i64, o.y.round() as i64, o.s, o.e));
        cur = format!("vo{k}");
        if o.audio && !frame_mode {
            if let Ok(mi) = need_tool(cfg, "ffprobe").and_then(|fp| proc::probe(&fp, &o.src)) {
                if mi.has_audio {
                    clip_audio.push((idx, o.s));
                }
            }
        }
    }
    // ---- the reel's layer page (transparent), drawn on top ----
    let mut ovl_in: Option<(usize, f64)> = None;
    if mode != Mode::Plate {
        let ovf = match mode {
            Mode::Frame(t) => crate::layers::frame(cfg, p, rid, &reel, &tl, t).ok().flatten().map(|f| (f, true)),
            _ => match crate::layers::cached(p, rid, &reel, &tl) {
                Some((f, true)) => Some((f, false)),
                _ => None,
            },
        };
        if let Some((f, still)) = ovf {
            if still {
                args.extend(["-loop".into(), "1".into(), "-t".into(), "1".into(), "-i".into(), fwd(&f)]);
            } else {
                args.extend(["-i".into(), fwd(&f)]);
            }
            ovl_in = Some((n_in, if still { ft } else { 0.0 }));
            n_in += 1;
        }
    }
    let mut tail = format!("subtitles=f={ass_name}:fontsdir=fonts");
    if mode == Mode::Preview {
        tail.push_str(&format!(",scale={}:{}", even(w * 2.0 / 3.0), even(h * 2.0 / 3.0)));
    }
    if mode != Mode::Plate {
        match ovl_in {
            Some((idx, shift)) => {
                g.push(format!("[{cur}]subtitles=f={ass_name}:fontsdir=fonts[vsub]"));
                g.push(format!("[{idx}:v]format=rgba,setpts=PTS-STARTPTS+{shift:.4}/TB[ovl]"));
                let sc = if mode == Mode::Preview { format!(",scale={}:{}", even(w * 2.0 / 3.0), even(h * 2.0 / 3.0)) } else { String::new() };
                g.push(format!("[vsub][ovl]overlay=0:0:eof_action=pass{sc},format=yuv420p[vout]"));
            }
            None => g.push(format!("[{cur}]{tail},format=yuv420p[vout]")),
        }
    }

    // ---- audio ----
    let mut sfx_table = vec![];
    if !frame_mode && mode != Mode::Plate {
        let takes = &tl.segs;
        let na = takes.len();
        let dub = js(&reel["audio"], "dub").map(|d| p.dir.join(d)).filter(|x| x.is_file());
        if let Some(d) = &dub {
            // A dubbed voice (ElevenLabs, HeyGen or translate + TTS) replaces the recording's voice.
            args.extend(["-i".into(), fwd(d)]);
            g.push(format!("[{n_in}:a]aresample=48000,aformat=channel_layouts=stereo,loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,apad=pad_dur={:.3}[voice0]", tl.outro + 1.0));
            n_in += 1;
        } else if m.has_audio {
            if na > 1 {
                g.push(format!("[0:a]aresample=48000,aformat=channel_layouts=stereo,asplit={na}{}", (0..na).map(|i| format!("[as{i}]")).collect::<String>()));
            } else {
                g.push("[0:a]aresample=48000,aformat=channel_layouts=stereo[as0]".into());
            }
            for (i, s) in takes.iter().enumerate() {
                let d = s.dur();
                let mut c = format!("[as{i}]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS,afade=t=in:d=0.006,afade=t=out:st={:.4}:d=0.006", s.a - ss, s.b - ss, (d - 0.006).max(0.0));
                c.push_str(&atempo(s.speed));
                if s.hold > 0.0 {
                    c.push_str(&format!(",apad=pad_dur={:.4}", s.hold));
                }
                g.push(format!("{c}[a{i}]"));
            }
            let joined = if na > 1 {
                g.push(format!("{}concat=n={na}:v=0:a=1[ac]", (0..na).map(|i| format!("[a{i}]")).collect::<String>()));
                "ac"
            } else {
                "a0"
            };
            let clean = if jb(&reel["voice"], "clean") == Some(true) { format!("highpass=f=80,afftdn=nf={:.0}:tn=1,acompressor=threshold=-20dB:ratio=3:attack=5:release=80,", jfd(&reel["voice"], "noise", -25.0)) } else { String::new() };
            g.push(format!("[{joined}]{clean}loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,apad=pad_dur={:.3}[voice0]", tl.outro + 1.0));
        } else {
            args.extend(["-f".into(), "lavfi".into(), "-t".into(), format!("{:.3}", tl.total + 1.0), "-i".into(), "anullsrc=r=48000:cl=stereo".into()]);
            g.push(format!("[{n_in}:a]anull[voice0]"));
            n_in += 1;
        }
        let mut mix: Vec<String> = vec!["voice".into()];
        let music = &reel["music"];
        let music_file = if music.is_object() {
            let lib = read_json_or(&library_dir().join("music").join("manifest.json"), json!({}));
            let f = js(music, "src").map(|s| p.dir.join(s)).or_else(|| js(music, "id").filter(|i| *i != "auto").map(|i| library_dir().join("music").join(jsd(&lib[i], "file", ""))));
            match f {
                Some(f) if f.is_file() => Some((f, js(music, "id").map(|i| jfd(&lib[i], "lufs_i", -14.0)).unwrap_or(-14.0))),
                _ => {
                    warn.push("the music bed is not on disk (the library ships no music); set music.src to a file in the project, or add music on the platform at post time".into());
                    None
                }
            }
        } else {
            None
        };
        if let Some((f, lufs)) = &music_file {
            args.extend(["-stream_loop".into(), "-1".into(), "-i".into(), fwd(f)]);
            let gain = -16.0 + jfd(music, "db", -20.0) - lufs;
            g.push("[voice0]asplit=2[voice][key]".into());
            g.push(format!(
                "[{n_in}:a]aformat=sample_rates=48000:channel_layouts=stereo,atrim=0:{:.3},volume={gain:.1}dB,afade=t=in:d={:.2},afade=t=out:st={:.3}:d={:.2}[mus0]",
                tl.total, jfd(music, "fadeIn", 0.6), (tl.total - jfd(music, "fadeOut", 1.2)).max(0.0), jfd(music, "fadeOut", 1.2)
            ));
            g.push("[mus0][key]sidechaincompress=threshold=0.04:ratio=6:attack=15:release=380[mus]".into());
            mix.push("mus".into());
            n_in += 1;
        } else {
            g.push("[voice0]anull[voice]".into());
        }
        for (k, (ev, path, gain)) in sounds.iter().enumerate() {
            args.extend(["-i".into(), fwd(path)]);
            g.push(format!("[{n_in}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume={gain:.1}dB,adelay={}:all=1[x{k}]", ms_(ev.t)));
            mix.push(format!("x{k}"));
            n_in += 1;
            sfx_table.push(json!({"t": (ev.t * 100.0).round() / 100.0, "id": ev.id, "role": ev.role, "gain_db": gain, "why": ev.why}));
        }
        for (k, (idx, at)) in clip_audio.iter().enumerate() {
            g.push(format!("[{idx}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume=-6dB,adelay={}:all=1[cx{k}]", ms_(*at)));
            mix.push(format!("cx{k}"));
        }
        g.push(format!("{}amix=inputs={}:normalize=0:duration=first:dropout_transition=0,alimiter=limit=0.84[aout]", mix.iter().map(|x| format!("[{x}]")).collect::<String>(), mix.len()));
    }

    // ---- output ----
    let graph = g.join(";\n");
    if graph.len() > 6000 {
        let gp = build_dir.join(format!("{rid}.graph.txt"));
        write_text(&gp, &graph)?;
        if ffmpeg_major(&ff) >= 7 {
            args.extend(["-/filter_complex".into(), fwd(&gp)]);
        } else {
            args.extend(["-filter_complex_script".into(), fwd(&gp)]);
        }
    } else {
        args.extend(["-filter_complex".into(), graph.replace('\n', "")]);
    }
    args.extend(["-map".into(), "[vout]".into()]);
    match mode {
        Mode::Frame(_) => args.extend(["-frames:v".into(), "1".into(), "-q:v".into(), "3".into(), "-update".into(), "1".into()]),
        Mode::Plate => {
            args.extend(["-an".into(), "-t".into(), format!("{:.4}", tl.total)]);
            let v: &[&str] = if h264_encoder(cfg) == "h264_nvenc" { &["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "20", "-b:v", "0"] } else { &["-c:v", "libx264", "-preset", "veryfast", "-crf", "18"] };
            args.extend(v.iter().map(|s| s.to_string()));
            args.extend(["-pix_fmt".into(), "yuv420p".into(), "-r".into(), format!("{fps}")]);
        }
        _ => {
            args.extend(["-map".into(), "[aout]".into(), "-t".into(), format!("{:.4}", tl.total)]);
            let nv = h264_encoder(cfg) == "h264_nvenc";
            let v: &[&str] = match (mode, nv) {
                (Mode::Preview, true) => &["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "27", "-b:v", "0", "-maxrate", "5M", "-bufsize", "10M"],
                (Mode::Preview, false) => &["-c:v", "libx264", "-preset", "veryfast", "-crf", "24", "-maxrate", "5M", "-bufsize", "10M"],
                (_, true) => &["-c:v", "h264_nvenc", "-preset", "p6", "-rc", "vbr", "-cq", "18", "-b:v", "0", "-maxrate", "20M", "-bufsize", "40M", "-profile:v", "high"],
                (_, false) => &["-c:v", "libx264", "-preset", "medium", "-crf", "16", "-maxrate", "20M", "-bufsize", "40M", "-profile:v", "high", "-level", "4.2"],
            };
            args.extend(v.iter().map(|s| s.to_string()));
            args.extend(["-pix_fmt", "yuv420p", "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-r"].iter().map(|s| s.to_string()));
            args.push(format!("{fps}"));
            args.extend(["-c:a", "aac", "-b:a", if mode == Mode::Preview { "160k" } else { "256k" }, "-ar", "48000", "-movflags", "+faststart"].iter().map(|s| s.to_string()));
        }
    }
    args.push(fwd(out));

    let report = json!({
        "reel": rid, "total": (tl.total * 100.0).round() / 100.0, "size": [w, h], "fps": fps,
        "takes": tl.segs.iter().map(|s| json!({"a": s.a, "b": s.b, "edit": (s.start * 100.0).round() / 100.0, "hold": s.hold})).collect::<Vec<_>>(),
        "pieces": pieces.len(), "captions": cap_log, "beats": bo.table.iter().filter(|t| t.get("beat").is_some()).collect::<Vec<_>>(),
        "sfx": sfx_table, "warnings": warn, "safe": {"top": st * sc, "bottom": h - sb * sc, "right": w - sr * sc},
        "style": jsd(&reel, "style", "clean"), "caption_style": jsd(&reel["_cap"], "style", "clean"),
    });
    if !frame_mode && mode != Mode::Plate {
        write_json(&build_dir.join(format!("{rid}.build.json")), &report)?;
    }
    Ok(Plan { args, cwd: build_dir, total: tl.total, report })
}

fn ms_(t: f64) -> i64 {
    (t.max(0.0) * 1000.0).round() as i64
}

/// One exact frame of the edit with every overlay, as a JPEG.
pub fn snapshot(cfg: &Config, p: &Project, rid: &str, t: f64) -> Result<PathBuf> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let dir = p.p("build/snaps");
    std::fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{rid}-{:07}.jpg", (t * 1000.0).round() as i64));
    let plan = build(cfg, p, rid, Mode::Frame(t), &out)?;
    proc::ffmpeg(&ff, &plan.args, Some(&plan.cwd), 0.0, &Rep::none(), "frame")?;
    if !out.exists() {
        bail!("no frame came out at {}", fmt_t(t));
    }
    Ok(out)
}

pub fn render(cfg: &Config, p: &Project, rid: &str, mode: Mode, rep: &Rep) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let reel = p.load_reel(rid)?;
    let base = format!("{}{}", slug(jsd(&reel, "title", &p.title())), if rid == "main" { String::new() } else { format!("-{rid}") });
    let dir = p.p("renders");
    std::fs::create_dir_all(&dir)?;
    let out = match mode {
        Mode::Preview => dir.join(format!("{base}.preview.mp4")),
        _ => {
            let mut v = 1;
            while dir.join(format!("{base}_v{v}.mp4")).exists() {
                v += 1;
            }
            dir.join(format!("{base}_v{v}.mp4"))
        }
    };
    let tmp = out.with_extension("part.mp4");
    {
        let r0 = styles::resolve(&reel);
        let tl0 = Timeline::from_reel(&r0, p.media().duration);
        crate::layers::ensure_video(cfg, p, rid, &r0, &tl0, &rep.sub(0.0, 0.2))?;
    }
    let plan = build(cfg, p, rid, mode, &tmp)?;
    rep.p(0.0, "rendering");
    let res = proc::ffmpeg(&ff, &plan.args, Some(&plan.cwd), plan.total, &rep.sub(0.0, 0.93), "rendering");
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_file(&out);
    crate::util::rename_retry(&tmp, &out)?;
    rep.p(0.94, "checking the render");
    let qa = check(cfg, p, &out, &plan.report).unwrap_or_else(|e| json!({"verdict": "UNCHECKED", "error": e.to_string()}));
    rep.p(1.0, "done");
    Ok(json!({"file": fwd(&out), "seconds": plan.total, "mode": if mode == Mode::Preview { "preview" } else { "final" }, "warnings": plan.report["warnings"], "qa": qa}))
}

/// The render report: streams, length, loudness, black frames, silences in the speech, caption safe area.
pub fn check(cfg: &Config, _p: &Project, file: &Path, build: &Value) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let fp = need_tool(cfg, "ffprobe")?;
    let m = proc::probe(&fp, file)?;
    let mut checks: Vec<Value> = vec![];
    let mut add = |name: &str, ok: bool, hard: bool, detail: String, fix: &str| checks.push(json!({"check": name, "ok": ok, "hard": hard, "detail": detail, "fix": if ok { "" } else { fix }}));
    let want = jfd(build, "total", m.duration);
    add("streams", m.has_video && m.has_audio && m.vcodec == "h264", true, format!("{} {}x{} + {}", m.vcodec, m.width, m.height, m.acodec), "the render lost a stream; render again");
    add("length", (m.duration - want).abs() < 0.2, true, format!("{:.2} s, the edit is {:.2} s", m.duration, want), "picture and sound differ in length; check the takes at the end of the reel");
    let err = proc::ffmpeg(&ff, &[
        "-i".into(), fwd(file), "-vf".into(), "blackdetect=d=0.15:pix_th=0.08".into(), "-af".into(), "silencedetect=noise=-45dB:d=0.8,loudnorm=print_format=json".into(), "-f".into(), "null".into(), "-".into(),
    ], None, 0.0, &Rep::none(), "check")?;
    let lj = err.rfind('{').and_then(|i| err[i..].find('}').map(|e| &err[i..=i + e])).and_then(|j| serde_json::from_str::<Value>(j).ok()).unwrap_or(json!({}));
    let num = |k: &str| lj[k].as_str().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let (lufs, tp) = (num("input_i"), num("input_tp"));
    add("loudness", (lufs + 16.0).abs() <= 2.0, false, format!("{lufs:.1} LUFS (target -16)"), "the voice is fixed at -16 LUFS before the sounds; if this is far off, many loud sounds are stacked - remove some, never lift the mix");
    add("true peak", tp <= -0.3, false, format!("{tp:.1} dBTP"), "a sound peaks over the voice; lower its `db`");
    let black: Vec<&str> = err.lines().filter(|l| l.contains("black_start:")).collect();
    let tail_black = black.iter().any(|l| l.split("black_end:").nth(1).and_then(|x| x.split_whitespace().next()).and_then(|x| x.parse::<f64>().ok()).map(|e| e >= m.duration - 0.05).unwrap_or(false));
    add("black frames", black.is_empty(), tail_black, format!("{} black stretch(es){}", black.len(), if tail_black { ", one at the very end (it flashes at the loop point)" } else { "" }), "a scene or B-roll ends before the next picture starts, or the plate ends early; extend it");
    let body = want - jfd(build, "outro", 0.0);
    let sil: Vec<f64> = err.lines().filter_map(|l| l.split("silence_start:").nth(1)).filter_map(|x| x.trim().split_whitespace().next()?.parse::<f64>().ok()).filter(|t| *t > 0.2 && *t < body - 1.0).collect();
    add("pauses", sil.is_empty(), false, if sil.is_empty() { "no silence over 0.8 s inside the speech".into() } else { format!("silence at {}", sil.iter().map(|t| fmt_t(*t)).collect::<Vec<_>>().join(", ")) }, "trim the pause: run `autocut`, or end the take earlier");
    let safe = &build["safe"];
    let outside: Vec<String> = ja(build, "captions").iter().filter(|c| c["rect"][3].as_f64().unwrap_or(0.0) > jfd(safe, "bottom", 1e9) + 2.0 || c["rect"][1].as_f64().unwrap_or(1e9) < jfd(safe, "top", 0.0) - 2.0).map(|c| fmt_t(jfd(c, "s", 0.0))).collect();
    add("caption safe area", outside.is_empty(), false, if outside.is_empty() { format!("{} caption blocks inside the platform's safe area", ja(build, "captions").len()) } else { format!("outside at {}", outside.join(", ")) }, "raise captions.y or shorten the caption group");
    let small = ja(build, "captions").first().map(|c| jfd(c, "px", 60.0)).unwrap_or(60.0);
    let scale = m.height as f64 / 1920.0;
    add("caption size", small >= 36.0 * scale, false, format!("{small:.0} px"), "captions under 48 px are hard to read on a phone; raise captions.size");
    let first = ja(build, "captions").first().map(|c| jfd(c, "s", 9.0)).unwrap_or(9.0).min(ja(build, "beats").first().map(|b| jfd(b, "at", 9.0)).unwrap_or(9.0));
    add("first 1.5 s", first <= 1.5, false, format!("something is on screen from {first:.1} s"), "the first second and a half decides the swipe: start on the first word, or add a `textfx` hook");
    let hard_fail = checks.iter().any(|c| c["ok"] == json!(false) && c["hard"] == json!(true));
    let soft = checks.iter().filter(|c| c["ok"] == json!(false)).count();
    let report = json!({"verdict": if hard_fail { "FAIL" } else if soft > 0 { "PASS with warnings" } else { "PASS" }, "file": fwd(file), "checks": checks, "build_warnings": build["warnings"]});
    write_json(&file.with_extension("qa.json"), &report)?;
    Ok(report)
}

/// A contact sheet: the first frame, 1.5 s, just after every cut, every beat as it lands, and the end.
pub fn approval(cfg: &Config, p: &Project, rid: &str) -> Result<Value> {
    let m = p.media();
    let reel = styles::resolve(&p.load_reel(rid)?);
    let tl = Timeline::from_reel(&reel, m.duration);
    let mut times: Vec<(f64, String)> = vec![(0.0, "first frame".into()), (1.5f64.min(tl.total - 0.1), "1.5 s".into())];
    for (i, c) in tl.cuts().iter().enumerate().take(12) {
        times.push((c + 0.07, format!("after cut {}", i + 1)));
    }
    for (i, b) in ja(&reel, "beats").iter().enumerate().take(12) {
        if let Some(t) = b.get("at").and_then(|a| tl.resolve(a)) {
            times.push(((t + 0.6).min(tl.total - 0.05), format!("beat {i} {}", jsd(b, "type", ""))));
        }
    }
    times.push(((tl.total - 0.1).max(0.0), "last frame".into()));
    times.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    times.dedup_by(|a, b| (a.0 - b.0).abs() < 0.2);
    let mut frames = vec![];
    for (t, label) in &times {
        match snapshot(cfg, p, rid, *t) {
            Ok(f) => frames.push(json!({"t": (t * 100.0).round() / 100.0, "label": label, "file": fwd(&f)})),
            Err(e) => frames.push(json!({"t": t, "label": label, "error": e.to_string()})),
        }
    }
    let plan = build(cfg, p, rid, Mode::Preview, &p.p("build/none.mp4"))?;
    let out = json!({"reel": rid, "frames": frames, "build": plan.report});
    write_json(&p.p(&format!("build/{rid}.approval.json")), &out)?;
    Ok(out)
}

/// atempo filters for a speed (atempo takes 0.5..2 per stage).
fn atempo(speed: f64) -> String {
    if (speed - 1.0).abs() < 1e-6 {
        return String::new();
    }
    let mut s = speed;
    let mut out = String::new();
    while s > 2.0 {
        out.push_str(",atempo=2");
        s /= 2.0;
    }
    while s < 0.5 {
        out.push_str(",atempo=0.5");
        s /= 0.5;
    }
    out.push_str(&format!(",atempo={s:.5}"));
    out
}

/// A camera that follows the face (or person) like an operator would: smoothed, and still until the
/// subject leaves a dead zone. Returns (source second, crop-window left edge in source px).
fn cam_path(track: &Value, a: f64, b: f64, sw: f64, cw0: f64) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, f64)> = ja(track, "samples").iter().filter_map(|x| Some((x[0].as_f64()?, x[1].as_f64()?))).filter(|(t, _)| *t >= a - 1.0 && *t <= b + 1.0).collect();
    if pts.len() < 3 {
        return vec![];
    }
    let target = |t: f64| -> f64 {
        let (mut sw_, mut sv) = (0.0, 0.0);
        for (pt, x) in &pts {
            let wgt = (1.0 - (pt - t).abs() / 0.8).max(0.0);
            sw_ += wgt;
            sv += wgt * x;
        }
        if sw_ > 0.0 {
            sv / sw_
        } else {
            pts.iter().min_by(|p, q| (p.0 - t).abs().partial_cmp(&(q.0 - t).abs()).unwrap()).unwrap().1
        }
    };
    let dz = 0.05;
    let mut cam = target(a);
    let mut out = vec![];
    let mut t = a;
    while t <= b + 1e-6 {
        let x = target(t);
        let d = x - cam;
        if d.abs() > dz {
            // ease toward the subject, at most a quarter of the frame per second
            let step = (d.abs() - dz).min(0.25 * 0.25);
            cam += step * d.signum();
        }
        out.push((t, clampf(cam * sw - cw0 / 2.0, 0.0, sw - cw0)));
        t += 0.25;
    }
    out
}

fn path_at(path: &[(f64, f64)], t: f64) -> f64 {
    if path.is_empty() {
        return 0.0;
    }
    if t <= path[0].0 {
        return path[0].1;
    }
    for w in path.windows(2) {
        if t <= w[1].0 {
            let k = if w[1].0 > w[0].0 { (t - w[0].0) / (w[1].0 - w[0].0) } else { 1.0 };
            return w[0].1 + (w[1].1 - w[0].1) * k;
        }
    }
    path[path.len() - 1].1
}

/// The camera path as an ffmpeg expression of t (seconds into the piece), piecewise linear.
fn path_expr(path: &[(f64, f64)], xmax: f64) -> String {
    let mut pts: Vec<(f64, f64)> = vec![];
    for p in path {
        // drop points on a straight still line, so long still stretches cost nothing
        if pts.len() >= 2 {
            let (a, b) = (pts[pts.len() - 2], pts[pts.len() - 1]);
            if (a.1 - b.1).abs() < 0.5 && (b.1 - p.1).abs() < 0.5 {
                pts.pop();
            }
        }
        pts.push(*p);
    }
    let mut e = format!("{:.1}", pts.last().unwrap().1.clamp(0.0, xmax));
    for w in pts.windows(2).rev() {
        let (t0, x0, t1, x1) = (w[0].0, w[0].1.clamp(0.0, xmax), w[1].0, w[1].1.clamp(0.0, xmax));
        let seg = if t1 - t0 > 1e-6 { format!("{x0:.1}+({:.1})*(t-{t0:.4})/{:.4}", x1 - x0, t1 - t0) } else { format!("{x1:.1}") };
        e = format!("if(lt(t\\,{t1:.4})\\,{seg}\\,{e})");
    }
    e
}

/// Captions from a translation: each block's words spread over the block's (edit) time.
fn translated_words(tr: &Value, tl: &Timeline) -> Vec<crate::timeline::EWord> {
    let mut out = vec![];
    for b in ja(tr, "blocks") {
        let (Some(s), Some(e)) = (b.get("s").and_then(|x| x.as_f64()), b.get("e").and_then(|x| x.as_f64())) else { continue };
        let (Some(es), Some(ee)) = (tl.to_edit(s), tl.to_edit(e).or_else(|| tl.to_edit(e - 0.05))) else { continue };
        let words: Vec<&str> = jsd(b, "text", "").split_whitespace().collect();
        let n = words.len().max(1) as f64;
        for (i, w) in words.iter().enumerate() {
            let ws = es + (ee - es) * i as f64 / n;
            out.push(crate::timeline::EWord { text: w.to_string(), s: ws, e: ws + (ee - es) / n, seg: 0, orig: s });
        }
    }
    out
}

/// The small framed plate of the reel and the person matte of each window, cached by what they depend on.
fn ensure_mattes(cfg: &Config, p: &Project, rid: &str, reel: &Value, tl: &Timeline, wins: &[(f64, f64)]) -> Result<Vec<(f64, f64, PathBuf)>> {
    use std::hash::{Hash, Hasher};
    let model = crate::matte::pick(cfg, jsd(&reel["matte"], "model", "")).ok_or_else(|| anyhow::anyhow!("no matte model installed"))?.0;
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    for k in ["takes", "crop", "zoom", "width", "height", "fps", "outro"] {
        reel[k].to_string().hash(&mut hs);
    }
    fwd(&p.source()?).hash(&mut hs);
    let sig = format!("{:x}", hs.finish());
    let dir = p.p("build/matte");
    std::fs::create_dir_all(&dir)?;
    let plate = dir.join(format!("{rid}-{sig}.plate.mp4"));
    if !plate.is_file() {
        let ff = need_tool(cfg, "ffmpeg")?;
        let tmp = dir.join(format!("{rid}-{sig}.plate.part.mp4"));
        let plan = build(cfg, p, rid, Mode::Plate, &tmp)?;
        proc::ffmpeg(&ff, &plan.args, Some(&plan.cwd), plan.total, &Rep::none(), "plate")?;
        crate::util::rename_retry(&tmp, &plate)?;
    }
    let mut merged: Vec<(f64, f64)> = vec![];
    let mut ws: Vec<(f64, f64)> = wins.iter().map(|(s, e)| ((s - 0.1).max(0.0), (e + 0.1).min(tl.total))).collect();
    ws.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for w in ws {
        match merged.last_mut() {
            Some(l) if w.0 <= l.1 + 0.05 => l.1 = l.1.max(w.1),
            _ => merged.push(w),
        }
    }
    let mut out = vec![];
    for (s0, e0) in merged {
        let f = dir.join(format!("{rid}-{sig}-{model}-{}-{}.mkv", (s0 * 1000.0) as i64, (e0 * 1000.0) as i64));
        if !f.is_file() {
            crate::matte::matte_range(cfg, &plate, s0, e0, tl.fps, model, &f, &Rep::none())?;
        }
        out.push((s0, e0, f));
    }
    Ok(out)
}

/// The speakers active at a source second (speakers.json).
fn active_at(spk: &Value, t: f64) -> Vec<usize> {
    ja(spk, "segments")
        .iter()
        .find(|s| t >= jfd(s, "t0", 0.0) && t < jfd(s, "t1", 0.0))
        .map(|s| ja(s, "active").iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect())
        .unwrap_or_default()
}
