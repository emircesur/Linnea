//! Blocking HTTP for the optional cloud backends (speech-to-text and LLM APIs).
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::time::Duration;

/// Certificates come from Windows' own store, so networks that inspect HTTPS (antivirus web shields,
/// company proxies) work the way they do in the browser.
pub fn agent_with(timeout_s: u64, status_errors: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(status_errors)
        .timeout_global(Some(Duration::from_secs(timeout_s)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        .build()
        .into()
}
/// The same, trusting the roots that ship with the program instead of Windows' store: a second try when
/// the store does not know a download's issuer (a proxy or an incomplete chain).
pub fn agent_webpki(timeout_s: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(true)
        .timeout_global(Some(Duration::from_secs(timeout_s)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::WebPki).build())
        .build()
        .into()
}
fn agent(timeout_s: u64) -> ureq::Agent {
    agent_with(timeout_s, false)
}

pub fn post_bytes(url: &str, headers: &[(&str, String)], content_type: &str, body: &[u8], timeout_s: u64) -> Result<(u16, String)> {
    let mut req = agent(timeout_s).post(url).header("Content-Type", content_type);
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let mut resp = req.send(body).map_err(|e| anyhow!("request to {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string().map_err(|e| anyhow!("reading the response from {url}: {e}"))?;
    Ok((status, text))
}

pub fn post_json(url: &str, headers: &[(&str, String)], body: &Value, timeout_s: u64) -> Result<(u16, Value)> {
    let (status, text) = post_bytes(url, headers, "application/json", body.to_string().as_bytes(), timeout_s)?;
    let v = serde_json::from_str(&text).unwrap_or(Value::String(text));
    Ok((status, v))
}

pub fn get(url: &str, timeout_s: u64) -> Result<(u16, String)> {
    let mut resp = agent(timeout_s).get(url).call().map_err(|e| anyhow!("request to {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().with_config().limit(16 * 1024 * 1024).read_to_string().unwrap_or_default();
    Ok((status, text))
}

/// A multipart/form-data body. Returns (content type, bytes).
pub fn multipart(fields: &[(&str, &str)], file_field: &str, file_name: &str, file_type: &str, file: &[u8]) -> (String, Vec<u8>) {
    let b = format!("----kesit{:x}{:x}", fastrand::u64(..), fastrand::u64(..));
    let mut out = Vec::with_capacity(file.len() + 1024);
    for (k, v) in fields {
        out.extend_from_slice(format!("--{b}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("--{b}\r\nContent-Disposition: form-data; name=\"{file_field}\"; filename=\"{file_name}\"\r\nContent-Type: {file_type}\r\n\r\n").as_bytes());
    out.extend_from_slice(file);
    out.extend_from_slice(format!("\r\n--{b}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={b}"), out)
}

/// POST JSON, get raw bytes back (audio, images).
pub fn post_json_bytes(url: &str, headers: &[(&str, String)], body: &Value, timeout_s: u64) -> Result<(u16, Vec<u8>)> {
    let mut req = agent(timeout_s).post(url).header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let mut resp = req.send(body.to_string().as_bytes()).map_err(|e| anyhow!("request to {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let bytes = resp.body_mut().with_config().limit(512 * 1024 * 1024).read_to_vec().map_err(|e| anyhow!("reading {url}: {e}"))?;
    Ok((status, bytes))
}

/// GET with headers, raw bytes back.
pub fn get_bytes(url: &str, headers: &[(&str, String)], timeout_s: u64) -> Result<(u16, Vec<u8>)> {
    let mut req = agent(timeout_s).get(url);
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let mut resp = req.call().map_err(|e| anyhow!("request to {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let bytes = resp.body_mut().with_config().limit(2048 * 1024 * 1024).read_to_vec().map_err(|e| anyhow!("reading {url}: {e}"))?;
    Ok((status, bytes))
}

pub fn get_json(url: &str, headers: &[(&str, String)], timeout_s: u64) -> Result<(u16, Value)> {
    let (s, b) = get_bytes(url, headers, timeout_s)?;
    Ok((s, serde_json::from_slice(&b).unwrap_or(Value::String(String::from_utf8_lossy(&b).into()))))
}

/// A request with any method and body; returns status, headers we ask for, and the body text.
pub fn send(method: &str, url: &str, headers: &[(&str, String)], body: &[u8], want: &[&str], timeout_s: u64) -> Result<(u16, Vec<(String, String)>, String)> {
    let a = agent(timeout_s);
    let mut resp = match method {
        "PUT" => {
            let mut r = a.put(url);
            for (k, v) in headers {
                r = r.header(*k, v.as_str());
            }
            r.send(body)
        }
        _ => {
            let mut r = a.post(url);
            for (k, v) in headers {
                r = r.header(*k, v.as_str());
            }
            r.send(body)
        }
    }
    .map_err(|e| anyhow!("request to {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    let hs = want.iter().filter_map(|k| resp.headers().get(*k).and_then(|v| v.to_str().ok()).map(|v| (k.to_string(), v.to_string()))).collect();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    Ok((status, hs, text))
}
