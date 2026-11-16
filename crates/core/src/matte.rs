//! Person matte on the GPU (ONNX Runtime + DirectML, CPU fallback), the method ghost-editor uses
//! (BiRefNet) plus Robust Video Matting, which is made for video: fast and steady from frame to frame.
//! Mattes are made only for the seconds that need them (words behind the speaker, cutout effects) and
//! cached as lossless gray videos. The same model tracks the person for 9:16 framing.
use crate::config::*;
use crate::proc::{self, Rep};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use ort::session::Session;
use ort::value::Tensor;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub struct ModelInfo {
    pub id: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub about: &'static str,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo { id: "rvm", file: "rvm_mobilenetv3_fp32.onnx", url: "https://github.com/PeterL1n/RobustVideoMatting/releases/download/v1.0.0/rvm_mobilenetv3_fp32.onnx", about: "Robust Video Matting (MobileNetV3, 15 MB): fast, steady across frames; the default" },
    ModelInfo { id: "rvm-hq", file: "rvm_resnet50_fp32.onnx", url: "https://github.com/PeterL1n/RobustVideoMatting/releases/download/v1.0.0/rvm_resnet50_fp32.onnx", about: "Robust Video Matting (ResNet50, 107 MB): finer hair and edges" },
    ModelInfo { id: "birefnet-lite", file: "birefnet_lite.onnx", url: "https://huggingface.co/onnx-community/BiRefNet_lite-ONNX/resolve/main/onnx/model.onnx", about: "BiRefNet lite (224 MB): ghost-editor's cutout model, frame by frame at 1024" },
    ModelInfo { id: "birefnet-portrait", file: "birefnet_portrait.onnx", url: "https://huggingface.co/onnx-community/BiRefNet-portrait-ONNX/resolve/main/onnx/model.onnx", about: "BiRefNet portrait (973 MB): ghost-editor's behind-the-speaker model, the sharpest and slowest" },
];

pub fn model(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}
pub fn model_path(id: &str) -> Option<PathBuf> {
    let m = model(id)?;
    models_dirs().into_iter().map(|d| d.join(m.file)).find(|p| p.is_file())
}
/// The model to use: the reel's choice, the setting, or the best one installed.
pub fn pick(cfg: &Config, want: &str) -> Option<(&'static str, PathBuf)> {
    let pref = if want.is_empty() { cfg.str("matte.model") } else { want.to_string() };
    let order: Vec<&str> = if pref.is_empty() { vec!["rvm", "rvm-hq", "birefnet-lite", "birefnet-portrait"] } else { vec![pref.as_str()] };
    order.into_iter().find_map(|id| model_path(id).map(|p| (model(id).unwrap().id, p)))
}

fn session(path: &Path) -> Result<Session> {
    let gpu = Session::builder()
        .and_then(|b| b.with_execution_providers([ort::execution_providers::DirectMLExecutionProvider::default().build()]))
        .and_then(|b| b.commit_from_file(path));
    match gpu {
        Ok(s) => Ok(s),
        Err(_) => Session::builder().and_then(|b| b.commit_from_file(path)).map_err(|e| anyhow!("cannot load {}: {e}", path.display())),
    }
}

/// One model, run frame after frame (RVM keeps its recurrent state between frames).
pub struct Matter {
    s: Session,
    rvm: bool,
    pub w: usize,
    pub h: usize,
    state: Vec<Tensor<f32>>,
    prev: Vec<f32>,
    input: String,
}

impl Matter {
    pub fn new(id: &str, path: &Path, w: usize, h: usize) -> Result<Matter> {
        let s = session(path)?;
        let rvm = id.starts_with("rvm");
        let input = s.inputs.first().map(|i| i.name.clone()).unwrap_or_else(|| "input".into());
        let (w, h) = if rvm { (w, h) } else { (1024, 1024) };
        let mut m = Matter { s, rvm, w, h, state: vec![], prev: vec![], input };
        m.reset()?;
        Ok(m)
    }
    pub fn reset(&mut self) -> Result<()> {
        self.state.clear();
        if self.rvm {
            for _ in 0..4 {
                self.state.push(Tensor::from_array(([1usize, 1, 1, 1], vec![0f32]))?);
            }
        }
        self.prev.clear();
        Ok(())
    }
    /// RGB24 frame of w x h in, alpha 0..255 of w x h out.
    pub fn run(&mut self, rgb: &[u8]) -> Result<Vec<u8>> {
        let (w, h) = (self.w, self.h);
        let n = w * h;
        let mut x = vec![0f32; 3 * n];
        let (mean, std) = if self.rvm { ([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]) } else { ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]) };
        for i in 0..n {
            for c in 0..3 {
                x[c * n + i] = (rgb[i * 3 + c] as f32 / 255.0 - mean[c]) / std[c];
            }
        }
        let src = Tensor::from_array(([1usize, 3, h, w], x))?;
        let alpha: Vec<f32> = if self.rvm {
            let ratio = (512.0 / h.max(w) as f32).clamp(0.125, 1.0);
            let ds = Tensor::from_array(([1usize], vec![ratio]))?;
            let st = std::mem::take(&mut self.state);
            let mut it = st.into_iter();
            let out = self.s.run(ort::inputs!["src" => src, "r1i" => it.next().unwrap(), "r2i" => it.next().unwrap(), "r3i" => it.next().unwrap(), "r4i" => it.next().unwrap(), "downsample_ratio" => ds])?;
            for k in ["r1o", "r2o", "r3o", "r4o"] {
                let (shape, data) = out[k].try_extract_tensor::<f32>()?;
                let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
                self.state.push(Tensor::from_array((dims, data.to_vec()))?);
            }
            let (_, pha) = out["pha"].try_extract_tensor::<f32>()?;
            pha.to_vec()
        } else {
            let name = self.input.clone();
            let out = self.s.run(ort::inputs![name.as_str() => src])?;
            let (_, d) = out[0].try_extract_tensor::<f32>()?;
            let logits = d.iter().any(|v| *v < -0.01 || *v > 1.01);
            let mut a: Vec<f32> = d.iter().take(n).map(|v| if logits { 1.0 / (1.0 + (-v).exp()) } else { *v }).collect();
            // BiRefNet sees each frame alone; a light blend with the previous frame calms the edges.
            if self.prev.len() == a.len() {
                for (v, p) in a.iter_mut().zip(&self.prev) {
                    *v = 0.7 * *v + 0.3 * p;
                }
            }
            self.prev = a.clone();
            a
        };
        Ok(alpha.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect())
    }
}

/// Cut a still picture out of its background: the same picture as a PNG with alpha. BiRefNet (made for
/// cutouts, objects too) when installed, else RVM. Returns the model used.
pub fn cutout(cfg: &Config, src: &Path, out: &Path) -> Result<String> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let fp = need_tool(cfg, "ffprobe")?;
    let (id, path) = ["birefnet-lite", "birefnet-portrait", "rvm-hq", "rvm"]
        .iter()
        .find_map(|id| model_path(id).map(|p| (*id, p)))
        .ok_or_else(|| anyhow!("no cutout model is installed: Settings > Keys and services > download the person matte model (setup_tools what=[\"matte\"]); BiRefNet (what=[\"matte:birefnet-lite\"]) cuts out objects best"))?;
    let m = proc::probe(&fp, src)?;
    let (iw, ih) = (m.width.max(1) as usize, m.height.max(1) as usize);
    let s = (1024.0 / iw.max(ih) as f64).min(1.0);
    let (mw, mh) = (((iw as f64 * s) as usize / 2 * 2).max(2), ((ih as f64 * s) as usize / 2 * 2).max(2));
    let mut mt = Matter::new(id, &path, mw, mh)?;
    let (w, h) = (mt.w, mt.h);
    let o = proc::cmd(&ff)
        .args(["-hide_banner", "-v", "error", "-i"])
        .arg(src)
        .args(["-vf", &format!("scale={w}:{h}:flags=bicubic,format=rgb24"), "-frames:v", "1", "-f", "rawvideo", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !o.status.success() || o.stdout.len() < w * h * 3 {
        bail!("reading the picture: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 2));
    }
    let rgb = &o.stdout[..w * h * 3];
    let mut alpha = mt.run(rgb)?;
    // RVM settles over a few frames: a still is shown to it again
    if id.starts_with("rvm") {
        for _ in 0..3 {
            alpha = mt.run(rgb)?;
        }
    }
    let araw = out.with_extension("alpha.raw");
    std::fs::write(&araw, &alpha)?;
    let r = proc::run(
        proc::cmd(&ff)
            .args(["-hide_banner", "-y", "-v", "error", "-i"])
            .arg(src)
            .args(["-f", "rawvideo", "-pix_fmt", "gray", "-s", &format!("{w}x{h}"), "-i"])
            .arg(&araw)
            .args(["-filter_complex", &format!("[1:v]scale={iw}:{ih}:flags=bicubic,format=gray[a];[0:v]format=rgba[c];[c][a]alphamerge,format=rgba"), "-frames:v", "1"])
            .arg(out),
    );
    let _ = std::fs::remove_file(&araw);
    match r {
        Ok(x) if x.ok => Ok(id.to_string()),
        Ok(x) => bail!("cutting out: {}", proc::err_tail(&x.stderr, 3)),
        Err(e) => Err(e),
    }
}

/// Matte one stretch of a video: frames from `src` between a and b (its own seconds), written as a gray
/// FFV1 video `out` of the model's size, starting at 0.
pub fn matte_range(cfg: &Config, src: &Path, a: f64, b: f64, fps: f64, model_id: &str, out: &Path, rep: &Rep) -> Result<()> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let fp = need_tool(cfg, "ffprobe")?;
    let (id, path) = pick(cfg, model_id).ok_or_else(|| anyhow!("no matte model installed. Settings > Tools > download (setup_tools what=[\"matte\"])"))?;
    let m = proc::probe(&fp, src)?;
    let scale = (512.0 / m.width.max(m.height) as f64).min(1.0);
    let (mw, mh) = (((m.width as f64 * scale) as usize / 2 * 2).max(2), ((m.height as f64 * scale) as usize / 2 * 2).max(2));
    let mut mt = Matter::new(id, &path, mw, mh)?;
    let (w, h) = (mt.w, mt.h);
    let mut dec = proc::cmd(&ff)
        .args(["-hide_banner", "-v", "error", "-ss", &format!("{a:.4}"), "-t", &format!("{:.4}", b - a), "-i"])
        .arg(src)
        .args(["-vf", &format!("fps={fps},scale={w}:{h}:flags=bilinear,format=rgb24"), "-f", "rawvideo", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = out.with_extension("part.mkv");
    let mut enc = proc::cmd(&ff)
        .args(["-hide_banner", "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "gray", "-s", &format!("{w}x{h}"), "-framerate", &format!("{fps}"), "-i", "-", "-c:v", "ffv1", "-pix_fmt", "gray"])
        .arg(&tmp)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut rd = dec.stdout.take().unwrap();
    let mut wr = enc.stdin.take().unwrap();
    let mut buf = vec![0u8; w * h * 3];
    let total = ((b - a) * fps).ceil().max(1.0);
    let mut k = 0.0;
    loop {
        if rep.cancelled() {
            let _ = dec.kill();
            let _ = enc.kill();
            bail!("cancelled");
        }
        if rd.read_exact(&mut buf).is_err() {
            break;
        }
        let alpha = mt.run(&buf)?;
        wr.write_all(&alpha)?;
        k += 1.0;
        rep.p(k / total, &format!("matte ({id})"));
    }
    drop(wr);
    let _ = dec.wait();
    let o = enc.wait_with_output()?;
    if !o.status.success() || k == 0.0 {
        bail!("matte encoding failed: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 3));
    }
    crate::util::rename_retry(&tmp, out)?;
    Ok(())
}

/// Track the person (not only a face) at 4 frames a second: [t, cx, cy, w, h] in 0..1, like face.json.
pub fn person_track(cfg: &Config, src: &Path, from: f64, to: f64, out: &Path, rep: &Rep) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let fp = need_tool(cfg, "ffprobe")?;
    let (id, path) = pick(cfg, "rvm").or_else(|| pick(cfg, "")).ok_or_else(|| anyhow!("no matte model installed for person tracking"))?;
    let m = proc::probe(&fp, src)?;
    let to = if to > 0.0 { to.min(m.duration) } else { m.duration };
    let w = 256usize;
    let h = ((w as f64 * m.height as f64 / m.width.max(1) as f64) as usize / 2 * 2).max(2);
    let mut mt = Matter::new(id, &path, w, h)?;
    let (mw, mh) = (mt.w, mt.h);
    let rate = 4.0;
    let raw = proc::ffmpeg_bytes(&ff, &["-ss".into(), format!("{from:.3}"), "-t".into(), format!("{:.3}", to - from), "-i".into(), fwd(src), "-vf".into(), format!("fps={rate},scale={mw}:{mh},format=rgb24"), "-f".into(), "rawvideo".into(), "-".into()])?;
    let mut samples: Vec<Value> = read_json_or(out, json!({}))["samples"].as_array().cloned().unwrap_or_default();
    samples.retain(|x| x[0].as_f64().map(|t| t < from - 0.01 || t > to + 0.01).unwrap_or(false));
    let frames: Vec<&[u8]> = raw.chunks_exact(mw * mh * 3).collect();
    let n = frames.len().max(1) as f64;
    for (i, f) in frames.iter().enumerate() {
        rep.check()?;
        let a = mt.run(f)?;
        let (mut sx, mut sw) = (0f64, 0f64);
        let (mut x0, mut x1, mut y0) = (mw, 0usize, mh);
        for y in 0..mh {
            for x in 0..mw {
                let v = a[y * mw + x] as f64 / 255.0;
                if v > 0.5 {
                    sx += x as f64 * v;
                    sw += v;
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                }
            }
        }
        if sw > (mw * mh) as f64 * 0.01 {
            let r = |v: f64| (v * 1000.0).round() / 1000.0;
            // Centre on the upper body: the head top plus a little, like a face box would sit.
            let cx = sx / sw / mw as f64;
            let head = y0 as f64 / mh as f64;
            let bw = (x1 - x0) as f64 / mw as f64;
            samples.push(json!([r(from + i as f64 / rate), r(cx), r((head + 0.12).min(0.9)), r(bw * 0.5), r(0.18)]));
        }
        rep.p((i + 1) as f64 / n, "tracking the person");
    }
    samples.sort_by(|a, b| a[0].as_f64().partial_cmp(&b[0].as_f64()).unwrap());
    let v = json!({"rate": rate, "kind": "person", "model": id, "samples": samples});
    write_json(out, &v)?;
    Ok(json!({"samples": v["samples"].as_array().map(|a| a.len()).unwrap_or(0), "model": id}))
}

pub fn status() -> Value {
    json!(MODELS.iter().map(|m| json!({"id": m.id, "installed": model_path(m.id).is_some(), "about": m.about})).collect::<Vec<_>>())
}
