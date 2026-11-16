//! A small Chrome DevTools Protocol client: a headless Edge or Chrome that loads a film page, waits for
//! `window.READY`, draws any second with `window.renderOut(t)` and hands back the frame. It replaces the
//! puppeteer script VideoHub's template renders with, so no Node is needed.
use crate::config::{data_dir, Config};
use crate::proc;
use crate::util::*;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub fn find_browser(cfg: &Config) -> Option<PathBuf> {
    let o = cfg.str("tools.browser");
    let env = std::env::var("VIDEOHUB_BROWSER").or_else(|_| std::env::var("LINNEA_BROWSER")).unwrap_or_default();
    for c in [o, env] {
        if !c.is_empty() && std::path::Path::new(&c).is_file() {
            return Some(c.into());
        }
    }
    let mut cands: Vec<PathBuf> = vec![];
    for base in [std::env::var("ProgramFiles(x86)").ok(), std::env::var("ProgramFiles").ok(), std::env::var("LOCALAPPDATA").ok()].into_iter().flatten() {
        let b = PathBuf::from(base);
        cands.push(b.join("Microsoft/Edge/Application/msedge.exe"));
        cands.push(b.join("Google/Chrome/Application/chrome.exe"));
        cands.push(b.join("Chromium/Application/chrome.exe"));
    }
    for p in ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"] {
        cands.push(p.into());
    }
    cands.into_iter().find(|p| p.is_file())
}

pub struct Page {
    child: Child,
    ws: WebSocket<TcpStream>,
    session: String,
    next: u64,
    profile: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Exceptions and console errors seen since the page loaded.
    pub errors: Vec<String>,
}

impl Drop for Page {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

impl Page {
    /// Start a browser with a viewport of exactly `width` x `height` and open `url` in it.
    pub fn open(cfg: &Config, url: &str, width: u32, height: u32, low: bool) -> Result<Page> {
        let exe = find_browser(cfg).ok_or_else(|| anyhow!("no Edge or Chrome found for rendering. Set tools.browser in settings to its .exe"))?;
        let profile = data_dir().join("browser").join(format!("p{}-{}", std::process::id(), fastrand::u32(..)));
        std::fs::create_dir_all(&profile)?;
        let mut c = if low { proc::cmd_low(&exe) } else { proc::cmd(&exe) };
        c.args([
            "--headless=new", "--remote-debugging-port=0", "--no-first-run", "--no-default-browser-check", "--disable-extensions", "--hide-scrollbars", "--mute-audio",
            "--force-device-scale-factor=1", "--disable-background-timer-throttling", "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows",
            "--autoplay-policy=no-user-gesture-required", "--allow-file-access-from-files", "--enable-unsafe-swiftshader",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--window-size={width},{height}"))
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
        let child = c.spawn().with_context(|| format!("cannot start {}", exe.display()))?;
        let port_file = profile.join("DevToolsActivePort");
        let t0 = Instant::now();
        let (port, path) = loop {
            if let Ok(s) = std::fs::read_to_string(&port_file) {
                let mut l = s.lines();
                if let (Some(p), Some(w)) = (l.next(), l.next()) {
                    break (p.trim().to_string(), w.trim().to_string());
                }
            }
            if t0.elapsed() > Duration::from_secs(20) {
                bail!("the browser did not open its debugging port");
            }
            std::thread::sleep(Duration::from_millis(60));
        };
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(60)))?;
        let (ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}{path}"), stream).map_err(|e| anyhow!("devtools connection: {e}"))?;
        let mut p = Page { child, ws, session: String::new(), next: 1, profile, width, height, errors: vec![] };
        let target = p.call("Target.createTarget", json!({"url": "about:blank"}))?;
        let att = p.call("Target.attachToTarget", json!({"targetId": target["targetId"], "flatten": true}))?;
        p.session = jsd(&att, "sessionId", "").to_string();
        for m in ["Page.enable", "Runtime.enable", "Log.enable"] {
            p.call(m, json!({}))?;
        }
        // a tab in the background gets no new frames, and a screenshot of it can wait forever
        let _ = p.call("Page.bringToFront", json!({}));
        p.call("Emulation.setDeviceMetricsOverride", json!({"width": width, "height": height, "deviceScaleFactor": 1, "mobile": false}))?;
        p.call("Page.navigate", json!({"url": url}))?;
        Ok(p)
    }

    fn note_event(&mut self, v: &Value) {
        match jsd(v, "method", "") {
            "Runtime.exceptionThrown" => {
                let d = &v["params"]["exceptionDetails"];
                let msg = d["exception"]["description"].as_str().or_else(|| d["text"].as_str()).unwrap_or("exception");
                let at = format!("{}:{}", jsd(d, "url", "").rsplit('/').next().unwrap_or(""), jfd(d, "lineNumber", 0.0) as i64 + 1);
                self.errors.push(format!("{} ({at})", msg.lines().next().unwrap_or(msg)));
            }
            "Runtime.consoleAPICalled" if jsd(&v["params"], "type", "") == "error" => {
                let args: Vec<String> = ja(&v["params"], "args").iter().map(|a| a["value"].as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(a, "description", "").to_string())).collect();
                self.errors.push(format!("console.error: {}", args.join(" ")));
            }
            "Log.entryAdded" if jsd(&v["params"]["entry"], "level", "") == "error" => {
                let e = &v["params"]["entry"];
                let url = jsd(e, "url", "").rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("").to_string();
                let text = jsd(e, "text", "");
                // for a request made from a script Chrome names the script, not the missing file
                let at = if text.starts_with("Failed to load resource") && url.ends_with(".js") { format!("(a file requested by {url})") } else { url };
                self.errors.push(format!("{text} {at}"));
            }
            _ => {}
        }
        if self.errors.len() > 40 {
            self.errors.drain(..10);
        }
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if !self.session.is_empty() {
            msg["sessionId"] = json!(self.session);
        }
        self.ws.send(Message::text(msg.to_string())).map_err(|e| anyhow!("devtools send: {e}"))?;
        loop {
            let m = self.ws.read().map_err(|e| anyhow!("devtools read ({method}): {e}"))?;
            let text = match m {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => bail!("the browser closed"),
                _ => continue,
            };
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if v["id"].as_u64() == Some(id) {
                if let Some(e) = v.get("error") {
                    bail!("{method}: {}", jsd(e, "message", "error"));
                }
                return Ok(v["result"].clone());
            }
            self.note_event(&v);
        }
    }

    /// Evaluate JavaScript (awaiting a promise) and return its value.
    pub fn eval(&mut self, expr: &str) -> Result<Value> {
        let r = self.call("Runtime.evaluate", json!({"expression": expr, "awaitPromise": true, "returnByValue": true}))?;
        if let Some(d) = r.get("exceptionDetails") {
            bail!("{}", d["exception"]["description"].as_str().or_else(|| d["text"].as_str()).unwrap_or("script error").lines().next().unwrap_or(""));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Let the page's own transparency through (layer pages over video).
    pub fn transparent(&mut self) -> Result<()> {
        self.call("Emulation.setDefaultBackgroundColorOverride", json!({"color": {"r": 0, "g": 0, "b": 0, "a": 0}}))?;
        Ok(())
    }

    /// Wait for `window.READY === true`.
    pub fn wait_ready(&mut self, timeout: Duration) -> Result<()> {
        let t0 = Instant::now();
        loop {
            if self.eval("window.READY === true").map(|v| v == json!(true)).unwrap_or(false) {
                return Ok(());
            }
            if t0.elapsed() > timeout {
                let has = self.eval("typeof window.renderOut").unwrap_or(json!("?"));
                let mut why = format!("the page never set window.READY = true (waited {} s)", timeout.as_secs());
                if has != json!("function") {
                    why.push_str("; window.renderOut is not defined");
                }
                if !self.errors.is_empty() {
                    why.push_str(&format!(". Errors: {}", self.errors.join(" | ")));
                }
                bail!(why);
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    }

    /// Draw second `t` and return the frame as JPEG (quality 1..100) or PNG (quality 0).
    pub fn frame(&mut self, t: f64, quality: u32, scale: f64) -> Result<Vec<u8>> {
        self.eval(&format!("(async () => {{ const r = window.renderOut({t:.6}); if (r && r.then) await r; return 1; }})()"))?;
        let mut p = json!({"clip": {"x": 0, "y": 0, "width": self.width, "height": self.height, "scale": scale}, "optimizeForSpeed": true, "captureBeyondViewport": false});
        if quality > 0 {
            p["format"] = json!("jpeg");
            p["quality"] = json!(quality.min(100));
        } else {
            p["format"] = json!("png");
        }
        let r = self.call("Page.captureScreenshot", p)?;
        base64::engine::general_purpose::STANDARD.decode(jsd(&r, "data", "")).map_err(|e| anyhow!("frame data: {e}"))
    }
}
