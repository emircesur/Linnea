//! A reel's edit as a timeline other editors open: Final Cut Pro (FCPXML 1.10), Premiere Pro and DaVinci Resolve
//! (xmeml, the Final Cut 7 XML both import; Resolve also reads FCPXML) and any editor at all (a CMX 3600 EDL).
//!
//! What goes across: every cut of the source in edit order, with its speed and its hold at the end (a freeze in
//! FCPXML; a gap in xmeml and the EDL, which have no freeze of their own), the source framed to fill (or fit) the
//! reel's size, the music bed on its own track at the reel's level, and the captions as an SRT next to it.
//! What stays in Linnea: captions burned in, cards, layers, sound effects and the face-tracking crop.
use crate::project::Project;
use crate::styles;
use crate::timeline::Timeline;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A frame rate as editors know it: a whole timebase, NTSC (×1000/1001) or not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    pub base: u32,
    pub ntsc: bool,
}

const RATES: &[(f64, u32, bool)] = &[(23.976, 24, true), (24.0, 24, false), (25.0, 25, false), (29.97, 30, true), (30.0, 30, false), (50.0, 50, false), (59.94, 60, true), (60.0, 60, false)];

impl Rate {
    /// The editors' rate nearest `fps`.
    pub fn of(fps: f64) -> Rate {
        let mut best = RATES[4];
        for r in RATES {
            if (r.0 - fps).abs() < (best.0 - fps).abs() {
                best = *r;
            }
        }
        Rate { base: best.1, ntsc: best.2 }
    }
    pub fn fps(&self) -> f64 {
        if self.ntsc { self.base as f64 * 1000.0 / 1001.0 } else { self.base as f64 }
    }
    pub fn frames(&self, t: f64) -> i64 {
        (t * self.fps()).round() as i64
    }
    /// `n` frames as an FCPXML rational time.
    pub fn fcp(&self, n: i64) -> String {
        if n == 0 {
            "0s".into()
        } else if self.ntsc {
            format!("{}/{}s", n * 1001, self.base * 1000)
        } else {
            format!("{}/{}s", n, self.base)
        }
    }
    /// One frame, as FCPXML's frameDuration.
    pub fn frame_duration(&self) -> String {
        if self.ntsc { format!("1001/{}s", self.base * 1000) } else { format!("1/{}s", self.base) }
    }
    /// Non-drop timecode of a frame count.
    pub fn tc(&self, n: i64) -> String {
        let b = self.base as i64;
        let n = n.max(0);
        format!("{:02}:{:02}:{:02}:{:02}", n / (b * 3600), n / (b * 60) % 60, n / b % 60, n % b)
    }
}

/// One cut of the source on the edit.
#[derive(Clone, Debug)]
pub struct Cut {
    /// Source seconds.
    pub a: f64,
    pub b: f64,
    pub speed: f64,
    /// Edit frames: where the cut starts, where its moving picture ends, where its hold ends.
    pub start: i64,
    pub end: i64,
    pub hold_end: i64,
}

#[derive(Clone, Debug)]
pub struct Music {
    pub path: PathBuf,
    pub len: f64,
    pub db: f64,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub name: String,
    pub rate: Rate,
    pub width: u32,
    pub height: u32,
    pub source: PathBuf,
    pub src_rate: Rate,
    pub src_w: u32,
    pub src_h: u32,
    pub src_dur: f64,
    pub has_audio: bool,
    /// Fit the source inside the frame instead of filling it.
    pub fit: bool,
    pub cuts: Vec<Cut>,
    pub total: i64,
    pub music: Option<Music>,
}

impl Plan {
    /// The source's scale in the frame, 1 = its own pixels.
    pub fn scale(&self) -> f64 {
        let (sx, sy) = (self.width as f64 / self.src_w.max(1) as f64, self.height as f64 / self.src_h.max(1) as f64);
        if self.fit { sx.min(sy) } else { sx.max(sy) }
    }
}

/// The plan from a reel's takes, on the editors' rate nearest the reel's.
pub fn plan_from(reel: &Value, name: &str, source: &Path, m: &crate::proc::Media, music: Option<Music>) -> Plan {
    let tl = Timeline::from_reel(reel, m.duration);
    let rate = Rate::of(tl.fps);
    let cuts = tl
        .segs
        .iter()
        .map(|s| Cut { a: s.a, b: s.b, speed: s.speed, start: rate.frames(s.start), end: rate.frames(s.start + s.len()), hold_end: rate.frames(s.start + s.len() + s.hold) })
        .filter(|c| c.end > c.start)
        .collect();
    Plan {
        name: name.to_string(),
        rate,
        width: even_u(jfd(reel, "width", 1080.0)),
        height: even_u(jfd(reel, "height", 1920.0)),
        source: source.to_path_buf(),
        src_rate: Rate::of(if m.fps > 0.0 { m.fps } else { tl.fps }),
        src_w: m.width.max(1),
        src_h: m.height.max(1),
        src_dur: m.duration,
        has_audio: m.has_audio,
        fit: jsd(&reel["crop"], "mode", "auto") == "fit",
        cuts,
        total: rate.frames(tl.body),
        music,
    }
}

fn even_u(x: f64) -> u32 {
    (((x / 2.0).round() as i64 * 2).max(2)) as u32
}

pub fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(*c as u32, 0..=8 | 11 | 12 | 14..=31))
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// An absolute path as a file URL, each part percent-encoded: file:///Users/a%20b.mp4, file:///C:/Videos/a.mp4.
pub fn file_url(p: &Path) -> String {
    let s = fwd(p);
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "source".into())
}

// ---------- FCPXML ----------

pub fn fcpxml(p: &Plan) -> String {
    let r = p.rate;
    let src_r = p.src_rate;
    let src = xml(&file_name(&p.source));
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.10\">\n  <resources>\n");
    o.push_str(&format!("    <format id=\"r1\" name=\"Linnea {}x{}\" frameDuration=\"{}\" width=\"{}\" height=\"{}\" colorSpace=\"1-1-1 (Rec. 709)\"/>\n", p.width, p.height, r.frame_duration(), p.width, p.height));
    o.push_str(&format!("    <format id=\"r2\" frameDuration=\"{}\" width=\"{}\" height=\"{}\" colorSpace=\"1-1-1 (Rec. 709)\"/>\n", src_r.frame_duration(), p.src_w, p.src_h));
    o.push_str(&format!(
        "    <asset id=\"r3\" name=\"{src}\" start=\"0s\" duration=\"{}\" hasVideo=\"1\" format=\"r2\"{}>\n      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>\n",
        src_r.fcp(src_r.frames(p.src_dur)),
        if p.has_audio { " hasAudio=\"1\" audioSources=\"1\" audioChannels=\"2\" audioRate=\"48000\"" } else { "" },
        xml(&file_url(&p.source))
    ));
    if let Some(m) = &p.music {
        o.push_str(&format!(
            "    <asset id=\"r4\" name=\"{}\" start=\"0s\" duration=\"{}\" hasAudio=\"1\" audioSources=\"1\" audioChannels=\"2\" audioRate=\"48000\">\n      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>\n",
            xml(&file_name(&m.path)),
            r.fcp(r.frames(m.len)),
            xml(&file_url(&m.path))
        ));
    }
    o.push_str("  </resources>\n  <library>\n    <event name=\"Linnea\">\n");
    o.push_str(&format!("      <project name=\"{}\">\n", xml(&p.name)));
    o.push_str(&format!(
        "        <sequence format=\"r1\" duration=\"{}\" tcStart=\"0s\" tcFormat=\"NDF\" audioLayout=\"stereo\" audioRate=\"48k\">\n          <spine>\n",
        r.fcp(p.total)
    ));
    let mut at = 0i64;
    for (i, c) in p.cuts.iter().enumerate() {
        if c.start > at {
            o.push_str(&format!("            <gap name=\"Gap\" offset=\"{}\" start=\"0s\" duration=\"{}\"/>\n", r.fcp(at), r.fcp(c.start - at)));
        }
        let len = c.hold_end - c.start;
        let retimed = (c.speed - 1.0).abs() > 1e-6 || c.hold_end > c.end;
        // with a time map, `start` is on the retimed clip's own clock (the map's `time`), the map's `value` the media's
        let start = if retimed { r.frames(c.a / c.speed) } else { r.frames(c.a) };
        o.push_str(&format!(
            "            <asset-clip ref=\"r3\" name=\"{src} {}\" offset=\"{}\" start=\"{}\" duration=\"{}\" format=\"r2\" tcFormat=\"NDF\">\n",
            i + 1,
            r.fcp(c.start),
            r.fcp(start),
            r.fcp(len)
        ));
        if retimed {
            let b_at = start + (c.end - c.start);
            o.push_str("              <timeMap>\n                <timept time=\"0s\" value=\"0s\" interp=\"linear\"/>\n");
            o.push_str(&format!("                <timept time=\"{}\" value=\"{}\" interp=\"linear\"/>\n", r.fcp(b_at), r.fcp(r.frames(c.b))));
            if c.hold_end > c.end {
                // the hold: the picture stays on the cut's last frame
                o.push_str(&format!("                <timept time=\"{}\" value=\"{}\" interp=\"linear\"/>\n", r.fcp(b_at + c.hold_end - c.end), r.fcp(r.frames(c.b))));
            }
            o.push_str("              </timeMap>\n");
        }
        o.push_str(&format!("              <adjust-conform type=\"{}\"/>\n", if p.fit { "fit" } else { "fill" }));
        if i == 0 {
            if let Some(m) = &p.music {
                let d = r.frames(m.len).min(p.total);
                o.push_str(&format!(
                    "              <asset-clip ref=\"r4\" lane=\"-1\" name=\"{}\" offset=\"{}\" start=\"0s\" duration=\"{}\" audioRole=\"music\">\n                <adjust-volume amount=\"{:.1}dB\"/>\n              </asset-clip>\n",
                    xml(&file_name(&m.path)),
                    r.fcp(start),
                    r.fcp(d),
                    m.db
                ));
            }
        }
        o.push_str("            </asset-clip>\n");
        at = c.hold_end;
    }
    o.push_str("          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n");
    o
}

// ---------- xmeml (Premiere Pro, DaVinci Resolve) ----------

fn tf(b: bool) -> &'static str {
    if b { "TRUE" } else { "FALSE" }
}
fn rate_x(r: Rate, pad: &str) -> String {
    format!("{pad}<rate>\n{pad}  <timebase>{}</timebase>\n{pad}  <ntsc>{}</ntsc>\n{pad}</rate>\n", r.base, tf(r.ntsc))
}
fn tc_x(r: Rate, pad: &str) -> String {
    format!("{pad}<timecode>\n{}{pad}  <string>00:00:00:00</string>\n{pad}  <frame>0</frame>\n{pad}  <displayformat>NDF</displayformat>\n{pad}</timecode>\n", rate_x(r, &format!("{pad}  ")))
}

struct XFile<'a> {
    id: &'a str,
    path: &'a Path,
    rate: Rate,
    frames: i64,
    video: Option<(u32, u32)>,
    audio: bool,
}

fn file_x(f: &XFile, pad: &str) -> String {
    let mut o = format!("{pad}<file id=\"{}\">\n{pad}  <name>{}</name>\n{pad}  <pathurl>{}</pathurl>\n", f.id, xml(&file_name(f.path)), xml(&file_url(f.path).replacen("file:///", "file://localhost/", 1)));
    o.push_str(&rate_x(f.rate, &format!("{pad}  ")));
    o.push_str(&format!("{pad}  <duration>{}</duration>\n", f.frames));
    o.push_str(&tc_x(f.rate, &format!("{pad}  ")));
    o.push_str(&format!("{pad}  <media>\n"));
    if let Some((w, h)) = f.video {
        o.push_str(&format!("{pad}    <video>\n{pad}      <samplecharacteristics>\n"));
        o.push_str(&rate_x(f.rate, &format!("{pad}        ")));
        o.push_str(&format!("{pad}        <width>{w}</width>\n{pad}        <height>{h}</height>\n{pad}        <anamorphic>FALSE</anamorphic>\n{pad}        <pixelaspectratio>square</pixelaspectratio>\n{pad}        <fielddominance>none</fielddominance>\n{pad}      </samplecharacteristics>\n{pad}    </video>\n"));
    }
    if f.audio {
        o.push_str(&format!("{pad}    <audio>\n{pad}      <samplecharacteristics>\n{pad}        <depth>16</depth>\n{pad}        <samplerate>48000</samplerate>\n{pad}      </samplecharacteristics>\n{pad}      <channelcount>2</channelcount>\n{pad}    </audio>\n"));
    }
    o.push_str(&format!("{pad}  </media>\n{pad}</file>\n"));
    o
}

fn speed_x(speed: f64, media: &str, pad: &str) -> String {
    let mut o = format!("{pad}<filter>\n{pad}  <effect>\n{pad}    <name>Time Remap</name>\n{pad}    <effectid>timeremap</effectid>\n{pad}    <effectcategory>motion</effectcategory>\n{pad}    <effecttype>motion</effecttype>\n{pad}    <mediatype>{media}</mediatype>\n");
    for (id, v) in [("variablespeed", "0".to_string()), ("speed", format!("{}", (speed * 100.0 * 1000.0).round() / 1000.0)), ("reverse", "FALSE".into()), ("frameblending", "FALSE".into())] {
        o.push_str(&format!("{pad}    <parameter authoringApp=\"PremierePro\">\n{pad}      <parameterid>{id}</parameterid>\n{pad}      <name>{id}</name>\n{pad}      <value>{v}</value>\n{pad}    </parameter>\n"));
    }
    o.push_str(&format!("{pad}  </effect>\n{pad}</filter>\n"));
    o
}

fn motion_x(scale_pct: f64, pad: &str) -> String {
    format!("{pad}<filter>\n{pad}  <effect>\n{pad}    <name>Basic Motion</name>\n{pad}    <effectid>basic</effectid>\n{pad}    <effectcategory>motion</effectcategory>\n{pad}    <effecttype>motion</effecttype>\n{pad}    <mediatype>video</mediatype>\n{pad}    <parameter>\n{pad}      <parameterid>scale</parameterid>\n{pad}      <name>Scale</name>\n{pad}      <valuemin>0</valuemin>\n{pad}      <valuemax>1000</valuemax>\n{pad}      <value>{:.3}</value>\n{pad}    </parameter>\n{pad}    <parameter>\n{pad}      <parameterid>center</parameterid>\n{pad}      <name>Center</name>\n{pad}      <value>\n{pad}        <horiz>0</horiz>\n{pad}        <vert>0</vert>\n{pad}      </value>\n{pad}    </parameter>\n{pad}  </effect>\n{pad}</filter>\n", scale_pct)
}

fn level_x(db: f64, pad: &str) -> String {
    let gain = 10f64.powf(db / 20.0).clamp(0.0, 3.98109);
    format!("{pad}<filter>\n{pad}  <effect>\n{pad}    <name>Audio Levels</name>\n{pad}    <effectid>audiolevels</effectid>\n{pad}    <effectcategory>audiolevels</effectcategory>\n{pad}    <effecttype>audiolevels</effecttype>\n{pad}    <mediatype>audio</mediatype>\n{pad}    <parameter>\n{pad}      <parameterid>level</parameterid>\n{pad}      <name>Level</name>\n{pad}      <valuemin>0</valuemin>\n{pad}      <valuemax>3.98109</valuemax>\n{pad}      <value>{gain:.5}</value>\n{pad}    </parameter>\n{pad}  </effect>\n{pad}</filter>\n")
}

pub fn xmeml(p: &Plan) -> String {
    let r = p.rate;
    let sr = p.src_rate;
    let src_frames = sr.frames(p.src_dur);
    let source = XFile { id: "file-1", path: &p.source, rate: sr, frames: src_frames, video: Some((p.src_w, p.src_h)), audio: p.has_audio };
    let name = file_name(&p.source);
    let mut written = false;
    let mut file_once = |pad: &str| -> String {
        if written {
            format!("{pad}<file id=\"file-1\"/>\n")
        } else {
            written = true;
            file_x(&source, pad)
        }
    };
    let clip = |kind: &str, i: usize, c: &Cut, pad: &str, file: String, linked: bool| -> String {
        let id = format!("clip-{kind}{}", i + 1);
        let mut o = format!("{pad}<clipitem id=\"{id}\">\n{pad}  <name>{}</name>\n{pad}  <enabled>TRUE</enabled>\n{pad}  <duration>{src_frames}</duration>\n", xml(&name));
        o.push_str(&rate_x(sr, &format!("{pad}  ")));
        o.push_str(&format!("{pad}  <start>{}</start>\n{pad}  <end>{}</end>\n{pad}  <in>{}</in>\n{pad}  <out>{}</out>\n", c.start, c.end, sr.frames(c.a), sr.frames(c.b)));
        if kind == "v" {
            o.push_str(&format!("{pad}  <alphatype>none</alphatype>\n{pad}  <pixelaspectratio>square</pixelaspectratio>\n{pad}  <anamorphic>FALSE</anamorphic>\n"));
        }
        o.push_str(&file);
        if kind == "a" {
            o.push_str(&format!("{pad}  <sourcetrack>\n{pad}    <mediatype>audio</mediatype>\n{pad}    <trackindex>1</trackindex>\n{pad}  </sourcetrack>\n"));
        } else {
            o.push_str(&motion_x(p.scale() * 100.0, &format!("{pad}  ")));
        }
        if (c.speed - 1.0).abs() > 1e-6 {
            o.push_str(&speed_x(c.speed, if kind == "v" { "video" } else { "audio" }, &format!("{pad}  ")));
        }
        if linked {
            for (k, media, t) in [("v", "video", 1), ("a", "audio", 1)] {
                o.push_str(&format!("{pad}  <link>\n{pad}    <linkclipref>clip-{k}{}</linkclipref>\n{pad}    <mediatype>{media}</mediatype>\n{pad}    <trackindex>{t}</trackindex>\n{pad}    <clipindex>{}</clipindex>\n{}{pad}  </link>\n", i + 1, i + 1, if k == "a" { format!("{pad}    <groupindex>1</groupindex>\n") } else { String::new() }));
            }
        }
        o.push_str(&format!("{pad}</clipitem>\n"));
        o
    };
    let pad = "          ";
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<xmeml version=\"4\">\n  <sequence id=\"sequence-1\">\n");
    o.push_str(&format!("    <name>{}</name>\n    <duration>{}</duration>\n", xml(&p.name), p.total));
    o.push_str(&rate_x(r, "    "));
    o.push_str(&tc_x(r, "    "));
    o.push_str("    <in>-1</in>\n    <out>-1</out>\n    <media>\n      <video>\n        <format>\n          <samplecharacteristics>\n");
    o.push_str(&rate_x(r, "            "));
    o.push_str(&format!("            <width>{}</width>\n            <height>{}</height>\n            <anamorphic>FALSE</anamorphic>\n            <pixelaspectratio>square</pixelaspectratio>\n            <fielddominance>none</fielddominance>\n          </samplecharacteristics>\n        </format>\n        <track>\n", p.width, p.height));
    for (i, c) in p.cuts.iter().enumerate() {
        let f = file_once(&format!("{pad}  "));
        o.push_str(&clip("v", i, c, pad, f, p.has_audio));
    }
    o.push_str("          <enabled>TRUE</enabled>\n          <locked>FALSE</locked>\n        </track>\n      </video>\n      <audio>\n        <numOutputChannels>2</numOutputChannels>\n        <format>\n          <samplecharacteristics>\n            <depth>16</depth>\n            <samplerate>48000</samplerate>\n          </samplecharacteristics>\n        </format>\n");
    if p.has_audio {
        o.push_str("        <track premiereTrackType=\"Stereo\">\n");
        for (i, c) in p.cuts.iter().enumerate() {
            let f = file_once(&format!("{pad}  "));
            o.push_str(&clip("a", i, c, pad, f, true));
        }
        o.push_str("          <enabled>TRUE</enabled>\n          <locked>FALSE</locked>\n        </track>\n");
    }
    if let Some(m) = &p.music {
        let mr = r;
        let mf = mr.frames(m.len);
        let len = mf.min(p.total);
        let mfile = XFile { id: "file-2", path: &m.path, rate: mr, frames: mf, video: None, audio: true };
        o.push_str("        <track premiereTrackType=\"Stereo\">\n");
        o.push_str(&format!("{pad}<clipitem id=\"clip-music\">\n{pad}  <name>{}</name>\n{pad}  <enabled>TRUE</enabled>\n{pad}  <duration>{mf}</duration>\n", xml(&file_name(&m.path))));
        o.push_str(&rate_x(mr, &format!("{pad}  ")));
        o.push_str(&format!("{pad}  <start>0</start>\n{pad}  <end>{len}</end>\n{pad}  <in>0</in>\n{pad}  <out>{len}</out>\n"));
        o.push_str(&file_x(&mfile, &format!("{pad}  ")));
        o.push_str(&format!("{pad}  <sourcetrack>\n{pad}    <mediatype>audio</mediatype>\n{pad}    <trackindex>1</trackindex>\n{pad}  </sourcetrack>\n"));
        o.push_str(&level_x(m.db, &format!("{pad}  ")));
        o.push_str(&format!("{pad}</clipitem>\n"));
        o.push_str("          <enabled>TRUE</enabled>\n          <locked>FALSE</locked>\n        </track>\n");
    }
    o.push_str("      </audio>\n    </media>\n  </sequence>\n</xmeml>\n");
    o
}

// ---------- CMX 3600 EDL ----------

pub fn edl(p: &Plan) -> String {
    let r = p.rate;
    let rec0 = r.base as i64 * 3600; // record timecode starts at 01:00:00:00, as editors expect
    let name = file_name(&p.source);
    let mut o = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", p.name.replace(['\r', '\n'], " "));
    let ch = if p.has_audio { "AA/V" } else { "V   " };
    for (i, c) in p.cuts.iter().enumerate() {
        let n = i + 1;
        let len = c.end - c.start;
        let s_in = r.frames(c.a);
        let s_out = s_in + (len as f64 * c.speed).round() as i64;
        o.push_str(&format!("{n:03}  AX       {ch}  C        {} {} {} {}\n", r.tc(s_in), r.tc(s_out), r.tc(rec0 + c.start), r.tc(rec0 + c.end)));
        if (c.speed - 1.0).abs() > 1e-6 {
            o.push_str(&format!("M2   AX       {:05.1}                {}\n", r.fps() * c.speed, r.tc(s_in)));
        }
        o.push_str(&format!("* FROM CLIP NAME: {name}\n"));
        if c.hold_end > c.end {
            o.push_str(&format!("* LINNEA HOLD: the last frame stays {:.2} s here\n", (c.hold_end - c.end) as f64 / r.fps()));
        }
        o.push('\n');
    }
    o
}

// ---------- a film: its rendered picture, its sounds apart, its markers ----------

/// One sound on a film's timeline.
#[derive(Clone, Debug)]
pub struct AClip {
    pub path: PathBuf,
    pub name: String,
    /// Film seconds.
    pub at: f64,
    /// Seconds into the file, and how many of them.
    pub from: f64,
    pub len: f64,
    pub file_len: f64,
    pub speed: f64,
    pub db: f64,
}

#[derive(Clone, Debug)]
pub struct FilmPlan {
    pub name: String,
    pub rate: Rate,
    pub width: u32,
    pub height: u32,
    /// The rendered version: the film's picture and its whole mix.
    pub video: PathBuf,
    pub len: f64,
    /// The sounds the mix is made of, each on a track of its own, off (the render's sound already holds them):
    /// turn a track on and the render's own sound off to mix them again in the editor.
    pub sounds: Vec<AClip>,
    pub markers: Vec<(f64, String)>,
}

pub fn film_xmeml(p: &FilmPlan) -> String {
    let r = p.rate;
    let total = r.frames(p.len);
    let v = XFile { id: "file-1", path: &p.video, rate: r, frames: total, video: Some((p.width, p.height)), audio: true };
    let pad = "          ";
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<xmeml version=\"4\">\n  <sequence id=\"sequence-1\">\n");
    o.push_str(&format!("    <name>{}</name>\n    <duration>{total}</duration>\n", xml(&p.name)));
    o.push_str(&rate_x(r, "    "));
    o.push_str(&tc_x(r, "    "));
    o.push_str("    <in>-1</in>\n    <out>-1</out>\n    <media>\n      <video>\n        <format>\n          <samplecharacteristics>\n");
    o.push_str(&rate_x(r, "            "));
    o.push_str(&format!("            <width>{}</width>\n            <height>{}</height>\n            <anamorphic>FALSE</anamorphic>\n            <pixelaspectratio>square</pixelaspectratio>\n            <fielddominance>none</fielddominance>\n          </samplecharacteristics>\n        </format>\n        <track>\n", p.width, p.height));
    let item = |id: &str, name: &str, kind: &str, file: String, start: i64, end: i64, i_in: i64, i_out: i64, dur: i64, extra: String| -> String {
        let mut x = format!("{pad}<clipitem id=\"{id}\">\n{pad}  <name>{}</name>\n{pad}  <enabled>TRUE</enabled>\n{pad}  <duration>{dur}</duration>\n", xml(name));
        x.push_str(&rate_x(r, &format!("{pad}  ")));
        x.push_str(&format!("{pad}  <start>{start}</start>\n{pad}  <end>{end}</end>\n{pad}  <in>{i_in}</in>\n{pad}  <out>{i_out}</out>\n"));
        x.push_str(&file);
        if kind == "audio" {
            x.push_str(&format!("{pad}  <sourcetrack>\n{pad}    <mediatype>audio</mediatype>\n{pad}    <trackindex>1</trackindex>\n{pad}  </sourcetrack>\n"));
        }
        x.push_str(&extra);
        x.push_str(&format!("{pad}</clipitem>\n"));
        x
    };
    let link = |pad: &str| -> String {
        [("clip-v", "video"), ("clip-a", "audio")]
            .iter()
            .map(|(id, m)| format!("{pad}  <link>\n{pad}    <linkclipref>{id}</linkclipref>\n{pad}    <mediatype>{m}</mediatype>\n{pad}    <trackindex>1</trackindex>\n{pad}    <clipindex>1</clipindex>\n{pad}  </link>\n"))
            .collect()
    };
    let name = file_name(&p.video);
    o.push_str(&item("clip-v", &name, "video", file_x(&v, &format!("{pad}  ")), 0, total, 0, total, total, link(pad)));
    o.push_str("          <enabled>TRUE</enabled>\n          <locked>FALSE</locked>\n        </track>\n      </video>\n      <audio>\n        <numOutputChannels>2</numOutputChannels>\n        <format>\n          <samplecharacteristics>\n            <depth>16</depth>\n            <samplerate>48000</samplerate>\n          </samplecharacteristics>\n        </format>\n");
    o.push_str("        <track premiereTrackType=\"Stereo\">\n");
    o.push_str(&item("clip-a", &name, "audio", format!("{pad}  <file id=\"file-1\"/>\n"), 0, total, 0, total, total, link(pad)));
    o.push_str("          <enabled>TRUE</enabled>\n          <locked>FALSE</locked>\n        </track>\n");
    for (i, c) in p.sounds.iter().enumerate() {
        let id = format!("file-{}", i + 2);
        let ff = r.frames(c.file_len);
        let f = XFile { id: &id, path: &c.path, rate: r, frames: ff, video: None, audio: true };
        let start = r.frames(c.at);
        let end = (start + r.frames(c.len / c.speed)).min(total).max(start + 1);
        let i_in = r.frames(c.from);
        let i_out = i_in + ((end - start) as f64 * c.speed).round() as i64;
        let mut extra = level_x(c.db, &format!("{pad}  "));
        if (c.speed - 1.0).abs() > 1e-6 {
            extra.push_str(&speed_x(c.speed, "audio", &format!("{pad}  ")));
        }
        o.push_str("        <track premiereTrackType=\"Stereo\">\n");
        o.push_str(&item(&format!("clip-s{}", i + 1), &c.name, "audio", file_x(&f, &format!("{pad}  ")), start, end, i_in, i_out, ff, extra));
        // off: the render's own sound already holds it
        o.push_str("          <enabled>FALSE</enabled>\n          <locked>FALSE</locked>\n        </track>\n");
    }
    o.push_str("      </audio>\n    </media>\n");
    for (t, name) in &p.markers {
        o.push_str(&format!("    <marker>\n      <name>{}</name>\n      <comment></comment>\n      <in>{}</in>\n      <out>-1</out>\n    </marker>\n", xml(name), r.frames(*t)));
    }
    o.push_str("  </sequence>\n</xmeml>\n");
    o
}

pub fn film_fcpxml(p: &FilmPlan) -> String {
    let r = p.rate;
    let total = r.frames(p.len);
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.10\">\n  <resources>\n");
    o.push_str(&format!("    <format id=\"r1\" name=\"Linnea {}x{}\" frameDuration=\"{}\" width=\"{}\" height=\"{}\" colorSpace=\"1-1-1 (Rec. 709)\"/>\n", p.width, p.height, r.frame_duration(), p.width, p.height));
    o.push_str(&format!(
        "    <asset id=\"r2\" name=\"{}\" start=\"0s\" duration=\"{}\" hasVideo=\"1\" hasAudio=\"1\" format=\"r1\" audioSources=\"1\" audioChannels=\"2\" audioRate=\"48000\">\n      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>\n",
        xml(&file_name(&p.video)), r.fcp(total), xml(&file_url(&p.video))
    ));
    for (i, c) in p.sounds.iter().enumerate() {
        o.push_str(&format!(
            "    <asset id=\"s{}\" name=\"{}\" start=\"0s\" duration=\"{}\" hasAudio=\"1\" audioSources=\"1\" audioChannels=\"2\" audioRate=\"48000\">\n      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>\n",
            i + 1, xml(&c.name), r.fcp(r.frames(c.file_len)), xml(&file_url(&c.path))
        ));
    }
    o.push_str(&format!("  </resources>\n  <library>\n    <event name=\"Linnea\">\n      <project name=\"{}\">\n", xml(&p.name)));
    o.push_str(&format!("        <sequence format=\"r1\" duration=\"{}\" tcStart=\"0s\" tcFormat=\"NDF\" audioLayout=\"stereo\" audioRate=\"48k\">\n          <spine>\n", r.fcp(total)));
    o.push_str(&format!("            <asset-clip ref=\"r2\" name=\"{}\" offset=\"0s\" start=\"0s\" duration=\"{}\" format=\"r1\" tcFormat=\"NDF\">\n", xml(&file_name(&p.video)), r.fcp(total)));
    for (i, c) in p.sounds.iter().enumerate() {
        let len = r.frames(c.len / c.speed).min(total - r.frames(c.at)).max(1);
        o.push_str(&format!(
            "              <asset-clip ref=\"s{}\" lane=\"-{}\" name=\"{}\" offset=\"{}\" start=\"{}\" duration=\"{}\" enabled=\"0\" audioRole=\"dialogue\">\n",
            i + 1, i + 1, xml(&c.name), r.fcp(r.frames(c.at)), r.fcp(r.frames(c.from / c.speed)), r.fcp(len)
        ));
        if (c.speed - 1.0).abs() > 1e-6 {
            o.push_str(&format!(
                "                <timeMap>\n                  <timept time=\"0s\" value=\"0s\" interp=\"linear\"/>\n                  <timept time=\"{}\" value=\"{}\" interp=\"linear\"/>\n                </timeMap>\n",
                r.fcp(r.frames(c.file_len / c.speed)), r.fcp(r.frames(c.file_len))
            ));
        }
        o.push_str(&format!("                <adjust-volume amount=\"{:.1}dB\"/>\n              </asset-clip>\n", c.db));
    }
    o.push_str("            </asset-clip>\n          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n");
    o
}

fn film_plan(c: &Ctx, key: &str) -> Result<(crate::film::Film, FilmPlan)> {
    let f = crate::film::open(&c.cfg, key)?;
    let info = crate::film::info(&c.cfg, &f);
    let vs = crate::film::versions(&f);
    let latest = vs.iter().rev().map(|v| PathBuf::from(jsd(v, "file", ""))).find(|p| p.is_file()).ok_or_else(|| anyhow::anyhow!("render the film first: the editor gets its rendered picture"))?;
    let m = crate::proc::probe(&crate::config::need_tool(&c.cfg, "ffprobe")?, &latest)?;
    let rate = Rate::of(if m.fps > 0.0 { m.fps } else { jfd(&info, "fps", 30.0) });
    let tw = crate::studio::tweaks(&f);
    let len_of = |p: &Path| crate::film::audio_len(&c.cfg, p).unwrap_or(0.0);
    let mut sounds = vec![];
    let (ls, _) = crate::studio::layers(c, &f);
    for l in &ls {
        let p = PathBuf::from(jsd(l, "file", ""));
        let fl = len_of(&p);
        if fl > 0.0 {
            sounds.push(AClip { name: jsd(l, "id", "sound").to_string(), path: p, at: 0.0, from: 0.0, len: fl.min(m.duration), file_len: fl, speed: 1.0, db: jfd(&tw["sound"]["levels"], jsd(l, "id", ""), 0.0) });
        }
    }
    for cl in ja(&tw["audio"], "clips") {
        if jb(cl, "mute") == Some(true) {
            continue;
        }
        let p = f.dir.join(jsd(cl, "file", ""));
        let fl = len_of(&p);
        if fl <= 0.0 {
            continue;
        }
        let from = jfd(cl, "from", 0.0).clamp(0.0, fl);
        sounds.push(AClip {
            name: js(cl, "name").map(|s| s.to_string()).unwrap_or_else(|| file_name(&p)),
            path: p,
            at: jfd(cl, "at", 0.0).max(0.0),
            from,
            len: jf(cl, "len").unwrap_or(fl - from).min(fl - from).max(0.01),
            file_len: fl,
            speed: jfd(cl, "speed", 1.0).clamp(0.25, 4.0),
            db: jfd(cl, "gain", 0.0),
        });
    }
    let markers = ja(&tw, "markers").iter().map(|x| (jfd(x, "t", 0.0), jsd(x, "name", "Marker").to_string())).collect();
    let plan = FilmPlan { name: jsd(&info, "title", &f.id).to_string(), rate, width: m.width.max(2), height: m.height.max(2), video: latest, len: m.duration, sounds, markers };
    Ok((f, plan))
}

fn export_film(c: &Ctx, a: &Value, key: &str) -> Result<Value> {
    let (f, plan) = film_plan(c, key)?;
    let format = jsd(a, "format", "all");
    if !["all", "fcpxml", "premiere", "xmeml"].contains(&format) {
        bail!("a film goes out as fcpxml, premiere (xmeml) or all");
    }
    let dir = f.dir.join("out");
    std::fs::create_dir_all(&dir)?;
    let base = slug(&plan.name);
    let mut files = vec![];
    if format == "all" || format == "fcpxml" {
        let p = dir.join(format!("{base}.fcpxml"));
        write_text(&p, &film_fcpxml(&plan))?;
        files.push(json!({"format": "fcpxml", "for": "Final Cut Pro, DaVinci Resolve", "file": fwd(&p)}));
    }
    if format != "fcpxml" {
        let p = dir.join(format!("{base}.xml"));
        write_text(&p, &film_xmeml(&plan))?;
        files.push(json!({"format": "xmeml", "for": "Premiere Pro, DaVinci Resolve", "file": fwd(&p)}));
    }
    Ok(json!({"files": files, "video": fwd(&plan.video), "sounds": plan.sounds.len(), "markers": plan.markers.len(),
              "notes": ["the picture is the newest render, with its whole mix on the first audio track; each sound it was mixed from is on a track of its own, switched off: turn them on (and the render's sound off) to mix again", "markers go across in the Premiere XML"]}))
}

// ---------- the tool ----------

/// The reel's music bed when it is on disk, with its level as the render mixes it.
fn music_of(c: &Ctx, p: &Project, reel: &Value) -> Option<Music> {
    let music = &reel["music"];
    if !music.is_object() {
        return None;
    }
    let lib = read_json_or(&library_dir().join("music").join("manifest.json"), json!({}));
    let f = js(music, "src").map(|s| p.dir.join(s)).or_else(|| js(music, "id").filter(|i| *i != "auto").map(|i| library_dir().join("music").join(jsd(&lib[i], "file", ""))))?;
    if !f.is_file() {
        return None;
    }
    let lufs = js(music, "id").map(|i| jfd(&lib[i], "lufs_i", -14.0)).unwrap_or(-14.0);
    let len = crate::film::audio_len(&c.cfg, &f).unwrap_or(0.0);
    (len > 0.0).then(|| Music { path: f, len, db: -16.0 + jfd(music, "db", -20.0) - lufs })
}

fn t_export(c: &Ctx, a: &Value) -> Result<Value> {
    if let Some(film) = js(a, "film").filter(|s| !s.is_empty()) {
        return export_film(c, a, film);
    }
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    let reel = styles::resolve(&p.load_reel(&rid)?);
    let source = p.source()?;
    let m = p.media();
    let title = jsd(&reel, "title", "").to_string();
    let title = if title.is_empty() { p.title() } else { title };
    let plan = plan_from(&reel, &title, &source, &m, music_of(c, &p, &reel));
    if plan.cuts.is_empty() {
        bail!("the reel has no cuts to export");
    }
    let format = jsd(a, "format", "all");
    let base = format!("{}{}", slug(&title), if rid == "main" { String::new() } else { format!("-{rid}") });
    let want = |f: &str| format == "all" || format == f;
    if !["all", "fcpxml", "premiere", "xmeml", "edl"].contains(&format) {
        bail!("format is fcpxml, premiere (xmeml), edl or all");
    }
    let mut files = vec![];
    if want("fcpxml") {
        let f = p.p(&format!("renders/{base}.fcpxml"));
        write_text(&f, &fcpxml(&plan))?;
        files.push(json!({"format": "fcpxml", "for": "Final Cut Pro, DaVinci Resolve", "file": fwd(&f)}));
    }
    if want("premiere") || want("xmeml") {
        let f = p.p(&format!("renders/{base}.xml"));
        write_text(&f, &xmeml(&plan))?;
        files.push(json!({"format": "xmeml", "for": "Premiere Pro, DaVinci Resolve", "file": fwd(&f)}));
    }
    if want("edl") {
        let f = p.p(&format!("renders/{base}.edl"));
        write_text(&f, &edl(&plan))?;
        files.push(json!({"format": "edl", "for": "any editor (Avid, Premiere, Resolve)", "file": fwd(&f)}));
    }
    let mut notes = vec!["burned-in captions, cards, layers, sound effects and the face-tracking crop stay in Linnea: the source is framed centred to fill the reel's size".to_string()];
    if jb(a, "captions") != Some(false) {
        match crate::extra::export_subs(c, &json!({"project": p.id(), "reel": rid})) {
            Ok(r) => files.push(json!({"format": "srt", "for": "the captions, as a subtitle track", "file": r["file"]})),
            Err(e) => notes.push(format!("no caption file: {e}")),
        }
    }
    let holds = plan.cuts.iter().filter(|c| c.hold_end > c.end).count();
    if holds > 0 && format != "fcpxml" {
        notes.push(format!("{holds} hold(s) at the end of a cut are a freeze in FCPXML and a gap in the xmeml and the EDL: fill each with a freeze frame there"));
    }
    if reel["music"].is_object() && plan.music.is_none() {
        notes.push("the music bed is not on disk, so it is not in the timeline".into());
    }
    Ok(json!({"files": files, "cuts": plan.cuts.len(), "length": r2(plan.total as f64 / plan.rate.fps()), "fps": plan.rate.fps(), "size": [plan.width, plan.height], "notes": notes}))
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub const TOOLS: &[Tool] = &[Tool {
    name: "export_timeline",
    desc: "Take the reel's edit to another editor: every cut of the source with its speed and hold, framed to the reel's size, the music bed on its own track and the captions as an SRT. format: fcpxml (Final Cut Pro, DaVinci Resolve), premiere (xmeml: Premiere Pro, Resolve), edl (CMX 3600, any editor) or all (default). Files go to the project's renders/ folder. With film: the newest render, each sound it was mixed from on a muted track of its own and the timeline markers, into the film's out/ folder (fcpxml or premiere).",
    params: &[("project", "string", "Project (a reel)"), ("film", "string", "Film instead: its newest render with every sound on a track of its own and the markers"), ("reel", "string", "Reel id"), ("format", "string", "fcpxml | premiere | edl | all"), ("captions", "boolean", "Also write the captions as SRT (default true)")],
    long: false,
    app_only: false,
    run: t_export,
}];

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        let reel = json!({"fps": 30, "width": 1080, "height": 1920, "takes": [{"a": 10.0, "b": 14.0}, {"a": 20.0, "b": 22.0, "speed": 2.0, "hold": 0.5}, {"a": 30.0, "b": 31.0}]});
        let m = crate::proc::Media::from_json(&json!({"duration": 60.0, "width": 1920, "height": 1080, "fps": 29.97, "has_audio": true}));
        plan_from(&reel, "Test & <reel>", Path::new("/Users/me/My Videos/talk.mp4"), &m, None)
    }

    #[test]
    fn rates() {
        assert_eq!(Rate::of(29.97), Rate { base: 30, ntsc: true });
        assert_eq!(Rate::of(30.0), Rate { base: 30, ntsc: false });
        assert_eq!(Rate::of(24.5), Rate { base: 24, ntsc: false });
        assert_eq!(Rate::of(30.0).fcp(45), "45/30s");
        assert_eq!(Rate::of(29.97).fcp(2), "2002/30000s");
        assert_eq!(Rate::of(30.0).tc(30 * 3661 + 7), "01:01:01:07");
    }

    #[test]
    fn cuts_share_edges() {
        let p = plan();
        assert_eq!(p.cuts.len(), 3);
        assert_eq!((p.cuts[0].start, p.cuts[0].end), (0, 120));
        // 2 s at twice the speed is 1 s, then a half-second hold
        assert_eq!((p.cuts[1].start, p.cuts[1].end, p.cuts[1].hold_end), (120, 150, 165));
        assert_eq!(p.cuts[2].start, 165);
        assert_eq!(p.total, 195);
        assert!((p.scale() - 1920.0 / 1080.0).abs() < 1e-9);
    }

    #[test]
    fn writes_formats() {
        let p = plan();
        let f = fcpxml(&p);
        assert!(f.contains("<fcpxml version=\"1.10\">"));
        assert!(f.contains("Test &amp; &lt;reel&gt;"));
        assert!(f.contains("file:///Users/me/My%20Videos/talk.mp4"));
        assert!(f.contains("<timeMap>"));
        assert_eq!(f.matches("<asset-clip ref=\"r3\"").count(), 3);
        let x = xmeml(&p);
        assert!(x.contains("file://localhost/Users/me/My%20Videos/talk.mp4"));
        assert_eq!(x.matches("<file id=\"file-1\">").count(), 1);
        assert!(x.contains("<parameterid>speed</parameterid>"));
        assert!(x.contains("<value>200</value>"));
        let e = edl(&p);
        assert!(e.contains("001  AX       AA/V  C        00:00:10:00 00:00:14:00 01:00:00:00 01:00:04:00"));
        assert!(e.contains("M2   AX       060.0"));
        assert!(e.contains("LINNEA HOLD"));
    }

    #[test]
    fn windows_paths() {
        assert_eq!(file_url(Path::new("C:/Videos/a b.mp4")), "file:///C:/Videos/a%20b.mp4");
    }
}
