//! Running child processes: capture, ffmpeg progress, cancellation, media probing.
use crate::util::*;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Progress and cancellation handed through every long operation.
#[derive(Clone)]
pub struct Rep {
    cb: Arc<dyn Fn(f64, &str) + Send + Sync>,
    pub cancel: Arc<AtomicBool>,
    lo: f64,
    hi: f64,
}
impl Rep {
    pub fn new(cb: Arc<dyn Fn(f64, &str) + Send + Sync>, cancel: Arc<AtomicBool>) -> Rep {
        Rep { cb, cancel, lo: 0.0, hi: 1.0 }
    }
    pub fn none() -> Rep {
        Rep::new(Arc::new(|_, _| {}), Arc::new(AtomicBool::new(false)))
    }
    pub fn p(&self, frac: f64, msg: &str) {
        (self.cb)(self.lo + (self.hi - self.lo) * clampf(frac, 0.0, 1.0), msg)
    }
    /// A reporter for one stage that maps its 0..1 onto lo..hi of this one.
    pub fn sub(&self, lo: f64, hi: f64) -> Rep {
        let w = self.hi - self.lo;
        Rep { cb: self.cb.clone(), cancel: self.cancel.clone(), lo: self.lo + w * lo, hi: self.lo + w * hi }
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled() { bail!("cancelled") } else { Ok(()) }
    }
}

pub fn cmd(program: &Path) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flash from the GUI
    }
    c.stdin(Stdio::null());
    c
}

/// A child that runs below normal priority, so a render leaves the computer usable.
pub fn cmd_low(program: &Path) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000 | 0x0000_4000);
    }
    c.stdin(Stdio::null());
    c
}

pub struct Out {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(c: &mut Command) -> Result<Out> {
    let o = c.stdout(Stdio::piped()).stderr(Stdio::piped()).output().with_context(|| format!("cannot start {:?}", c.get_program()))?;
    Ok(Out { ok: o.status.success(), stdout: String::from_utf8_lossy(&o.stdout).into(), stderr: String::from_utf8_lossy(&o.stderr).into() })
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Run ffmpeg with progress. `total` is the expected output length in seconds. Returns stderr.
pub fn ffmpeg(ffmpeg: &Path, args: &[String], cwd: Option<&Path>, total: f64, rep: &Rep, label: &str) -> Result<String> {
    let mut c = cmd(ffmpeg);
    c.args(["-hide_banner", "-y", "-nostdin", "-nostats", "-progress", "pipe:1"]).args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("cannot start ffmpeg")?;
    let mut err = child.stderr.take().unwrap();
    let th = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).to_string()
    });
    let out = BufReader::new(child.stdout.take().unwrap());
    for line in out.lines().map_while(|l| l.ok()) {
        if rep.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("cancelled");
        }
        if let Some(us) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
            if let Ok(us) = us.trim().parse::<f64>() {
                if total > 0.0 {
                    rep.p(us / 1e6 / total, label);
                }
            }
        }
    }
    let status = child.wait()?;
    let stderr = th.join().unwrap_or_default();
    if !status.success() {
        bail!("ffmpeg failed ({label}):\n{}", tail(&stderr, 14));
    }
    Ok(stderr)
}

/// Run ffmpeg and return raw stdout bytes (PCM or raw video on a pipe).
pub fn ffmpeg_bytes(ffmpeg: &Path, args: &[String]) -> Result<Vec<u8>> {
    let mut c = cmd(ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-v", "error"]).args(args);
    let o = c.stdout(Stdio::piped()).stderr(Stdio::piped()).output().context("cannot start ffmpeg")?;
    if !o.status.success() {
        bail!("ffmpeg failed:\n{}", tail(&String::from_utf8_lossy(&o.stderr), 10));
    }
    Ok(o.stdout)
}

#[derive(Clone, Debug, Default)]
pub struct Media {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub vcodec: String,
    pub acodec: String,
    pub hdr: bool,
    pub size: u64,
}
impl Media {
    pub fn json(&self) -> Value {
        json!({"duration": self.duration, "width": self.width, "height": self.height, "fps": self.fps, "has_video": self.has_video,
               "has_audio": self.has_audio, "vcodec": self.vcodec, "acodec": self.acodec, "hdr": self.hdr, "size": self.size})
    }
    pub fn from_json(v: &Value) -> Media {
        Media {
            duration: jfd(v, "duration", 0.0),
            width: jfd(v, "width", 0.0) as u32,
            height: jfd(v, "height", 0.0) as u32,
            fps: jfd(v, "fps", 30.0),
            has_video: jb(v, "has_video").unwrap_or(true),
            has_audio: jb(v, "has_audio").unwrap_or(true),
            vcodec: jsd(v, "vcodec", "").into(),
            acodec: jsd(v, "acodec", "").into(),
            hdr: jb(v, "hdr").unwrap_or(false),
            size: jfd(v, "size", 0.0) as u64,
        }
    }
}

fn ratio(s: &str) -> f64 {
    let mut it = s.split('/');
    let a: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let b: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(1.0);
    if b == 0.0 { 0.0 } else { a / b }
}

/// Width and height are as displayed (phone rotation applied), like ffmpeg decodes them.
pub fn probe(ffprobe: &Path, file: &Path) -> Result<Media> {
    let o = run(cmd(ffprobe).args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(file))?;
    if !o.ok {
        bail!("ffprobe cannot read {}: {}", file.display(), tail(&o.stderr, 3));
    }
    let v: Value = serde_json::from_str(&o.stdout).map_err(|e| anyhow!("ffprobe output: {e}"))?;
    let mut m = Media::default();
    m.duration = v["format"]["duration"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    m.size = v["format"]["size"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    for s in ja(&v, "streams") {
        match jsd(s, "codec_type", "") {
            "video" if !m.has_video && s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0 => {
                m.has_video = true;
                m.vcodec = jsd(s, "codec_name", "").into();
                m.width = jfd(s, "width", 0.0) as u32;
                m.height = jfd(s, "height", 0.0) as u32;
                let f = ratio(jsd(s, "avg_frame_rate", "0/1"));
                m.fps = if f > 1.0 && f < 240.0 { f } else { ratio(jsd(s, "r_frame_rate", "30/1")) };
                let mut rot = s["tags"]["rotate"].as_str().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
                for sd in ja(s, "side_data_list") {
                    if let Some(r) = jf(sd, "rotation") {
                        rot = r;
                    }
                }
                if (rot.abs() as i64 % 180) == 90 {
                    std::mem::swap(&mut m.width, &mut m.height);
                }
                let trc = jsd(s, "color_transfer", "");
                m.hdr = trc == "arib-std-b67" || trc == "smpte2084";
            }
            "audio" if !m.has_audio => {
                m.has_audio = true;
                m.acodec = jsd(s, "codec_name", "").into();
            }
            _ => {}
        }
    }
    if m.duration <= 0.0 {
        bail!("{} has no readable duration", file.display());
    }
    Ok(m)
}

/// Run a process and hand every output line (stdout and stderr) to `on_line` as it arrives.
pub fn run_lines(c: &mut Command, rep: &Rep, mut on_line: impl FnMut(&str)) -> Result<Out> {
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().with_context(|| format!("cannot start {:?}", c.get_program()))?;
    let (tx, rx) = std::sync::mpsc::channel::<(bool, String)>();
    let mut handles = vec![];
    let so = child.stdout.take().unwrap();
    let se = child.stderr.take().unwrap();
    let tx2 = tx.clone();
    handles.push(std::thread::spawn(move || read_chunks(so, false, tx2)));
    handles.push(std::thread::spawn(move || read_chunks(se, true, tx)));
    let (mut out, mut err) = (String::new(), String::new());
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok((is_err, line)) => {
                on_line(&line);
                let dst = if is_err { &mut err } else { &mut out };
                if dst.len() < 4_000_000 {
                    dst.push_str(&line);
                    dst.push('\n');
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
        if rep.cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("cancelled");
        }
    }
    for h in handles {
        let _ = h.join();
    }
    let status = child.wait()?;
    Ok(Out { ok: status.success(), stdout: out, stderr: err })
}

/// Lines split on \n or \r (progress bars redraw with \r), lossy UTF-8.
fn read_chunks(mut r: impl Read, is_err: bool, tx: std::sync::mpsc::Sender<(bool, String)>) {
    let mut buf = [0u8; 4096];
    let mut cur: Vec<u8> = vec![];
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &b in &buf[..n] {
            if b == b'\n' || b == b'\r' {
                if !cur.is_empty() {
                    let _ = tx.send((is_err, String::from_utf8_lossy(&cur).to_string()));
                    cur.clear();
                }
            } else {
                cur.push(b);
            }
        }
    }
    if !cur.is_empty() {
        let _ = tx.send((is_err, String::from_utf8_lossy(&cur).to_string()));
    }
}

pub fn err_tail(s: &str, n: usize) -> String {
    tail(s, n)
}
