//! The optional language model behind the in-app assistant and AI clip picking.
//! Two transports: the Claude Messages API, and any OpenAI-compatible chat endpoint (a local Ollama
//! or LM Studio server keeps everything on this machine). Nothing here runs unless the user sets it up.
use crate::config::Config;
use crate::net;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// What a tool hands back to the model: text, and pictures (JPEG bytes) it should look at.
pub struct ToolOut {
    pub text: String,
    pub images: Vec<Vec<u8>>,
}
impl From<String> for ToolOut {
    fn from(text: String) -> Self {
        ToolOut { text, images: vec![] }
    }
}

const TOOL_TEXT_MAX: usize = 24000;
fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn provider(cfg: &Config) -> String {
    let p = cfg.str("llm.provider");
    if p.is_empty() { "none".into() } else { p }
}

fn model(cfg: &Config) -> String {
    let m = cfg.str("llm.model");
    if !m.is_empty() {
        return m;
    }
    match provider(cfg).as_str() {
        "anthropic" => "claude-opus-5-5".into(),
        "openai" => "gpt-5".into(),
        _ => "llama3.1".into(),
    }
}

fn claude(cfg: &Config, system: &str, messages: &Value, tools: &[ToolSpec]) -> Result<Value> {
    let key = cfg.key("anthropic").ok_or_else(|| anyhow!("no Anthropic API key. Add keys.anthropic in settings, or set ANTHROPIC_API_KEY."))?;
    let mut body = json!({"model": model(cfg), "max_tokens": 16000, "system": system, "messages": messages, "fallbacks": "default"});
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.schema})).collect::<Vec<_>>());
        // a tool loop resends the tool list, the system prompt and the growing history every step: cache them
        body["cache_control"] = json!({"type": "ephemeral"});
    }
    let headers = [("x-api-key", key), ("anthropic-version", "2026-10-21".to_string()), ("anthropic-beta", "server-side-fallback-2026-07-01".to_string())];
    // busy or overloaded: wait and ask again, twice at most
    let mut tries = 0u64;
    let (status, v) = loop {
        let r = net::post_json("https://api.anthropic.com/v1/messages", &headers, &body, 1200)?;
        if matches!(r.0, 429 | 500 | 502 | 503 | 529) && tries < 2 {
            tries += 1;
            std::thread::sleep(std::time::Duration::from_secs(5 * tries));
            continue;
        }
        break r;
    };
    match status {
        200 => Ok(v),
        401 | 403 => bail!("the Anthropic API key was refused ({status})"),
        429 => bail!("the Claude API is rate limiting this key; try again in a moment"),
        s if s >= 500 => bail!("the Claude API is having trouble ({s}); try again"),
        s => bail!("Claude API error {s}: {}", jsd(&v["error"], "message", &v.to_string())),
    }
}

fn openai_chat(cfg: &Config, messages: &Value, tools: &[ToolSpec]) -> Result<Value> {
    let prov = provider(cfg);
    let base = if prov == "openai" { "https://api.openai.com/v1".to_string() } else { cfg.str("llm.base_url").trim_end_matches('/').to_string() };
    let mut body = json!({"model": model(cfg), "messages": messages});
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.schema}})).collect::<Vec<_>>());
    }
    let mut headers = vec![];
    if let Some(k) = cfg.key("openai").filter(|_| prov == "openai").or_else(|| Some(cfg.str("llm.api_key")).filter(|k| !k.is_empty())) {
        headers.push(("Authorization", format!("Bearer {k}")));
    }
    let (status, v) = net::post_json(&format!("{base}/chat/completions"), &headers, &body, 900)?;
    if status != 200 {
        bail!("the model server at {base} answered {status}: {}", v.to_string().chars().take(300).collect::<String>());
    }
    Ok(v)
}

/// One answer, no tools.
pub fn complete(cfg: &Config, system: &str, user: &str) -> Result<String> {
    match provider(cfg).as_str() {
        "none" => bail!("no language model is set up. Settings > Assistant: pick Claude, OpenAI, or a local server."),
        "anthropic" => {
            let v = claude(cfg, system, &json!([{"role": "user", "content": user}]), &[])?;
            if jsd(&v, "stop_reason", "") == "refusal" {
                bail!("the model declined this request");
            }
            Ok(ja(&v, "content").iter().filter(|b| jsd(b, "type", "") == "text").map(|b| jsd(b, "text", "")).collect::<Vec<_>>().join(""))
        }
        _ => {
            let v = openai_chat(cfg, &json!([{"role": "system", "content": system}, {"role": "user", "content": user}]), &[])?;
            Ok(jsd(&v["choices"][0]["message"], "content", "").to_string())
        }
    }
}

/// A tool loop: the model calls tools until it is done. `exec` runs one tool and returns its output.
pub fn agent(cfg: &Config, system: &str, user: &str, tools: &[ToolSpec], exec: &mut dyn FnMut(&str, &Value) -> ToolOut, max_steps: usize) -> Result<String> {
    match provider(cfg).as_str() {
        "none" => bail!("no language model is set up. Settings > Assistant: pick Claude, OpenAI, or a local server."),
        "anthropic" => {
            let mut messages = vec![json!({"role": "user", "content": user})];
            let mut last_text = String::new();
            for _ in 0..max_steps {
                let v = claude(cfg, system, &json!(messages), tools)?;
                let content = v["content"].clone();
                let text: String = ja(&v, "content").iter().filter(|b| jsd(b, "type", "") == "text").map(|b| jsd(b, "text", "")).collect::<Vec<_>>().join("");
                if !text.is_empty() {
                    last_text = text;
                }
                match jsd(&v, "stop_reason", "") {
                    "refusal" => bail!("the model declined this request"),
                    "tool_use" => {
                        // The whole assistant turn goes back unchanged; every result in ONE user message.
                        messages.push(json!({"role": "assistant", "content": content}));
                        let results: Vec<Value> = ja(&v, "content")
                            .iter()
                            .filter(|b| jsd(b, "type", "") == "tool_use")
                            .map(|b| {
                                let out = exec(jsd(b, "name", ""), &b["input"]);
                                let text: String = out.text.chars().take(TOOL_TEXT_MAX).collect();
                                let content = if out.images.is_empty() {
                                    json!(text)
                                } else {
                                    let mut blocks = vec![json!({"type": "text", "text": if text.is_empty() { "done".to_string() } else { text }})];
                                    blocks.extend(out.images.iter().map(|i| json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": b64(i)}})));
                                    json!(blocks)
                                };
                                json!({"type": "tool_result", "tool_use_id": b["id"], "content": content, "is_error": out.text.starts_with("Error:")})
                            })
                            .collect();
                        messages.push(json!({"role": "user", "content": results}));
                    }
                    "pause_turn" => messages.push(json!({"role": "assistant", "content": content})),
                    // cut off mid-answer: a tool call in this turn may be half written, so none of them run
                    "max_tokens" => return Ok(format!("{last_text}\n(the answer was cut off at the length limit; ask again for the rest)")),
                    _ => return Ok(last_text),
                }
            }
            Ok(format!("{last_text}\n(stopped after {max_steps} steps)"))
        }
        _ => {
            let mut messages = vec![json!({"role": "system", "content": system}), json!({"role": "user", "content": user})];
            let mut last_text = String::new();
            for _ in 0..max_steps {
                let v = openai_chat(cfg, &json!(messages), tools)?;
                let msg = v["choices"][0]["message"].clone();
                if let Some(t) = js(&msg, "content") {
                    if !t.is_empty() {
                        last_text = t.to_string();
                    }
                }
                let calls = ja(&msg, "tool_calls").to_vec();
                if calls.is_empty() {
                    return Ok(last_text);
                }
                messages.push(msg);
                let mut seen: Vec<Value> = vec![];
                for c in calls {
                    let name = jsd(&c["function"], "name", "");
                    // arguments that do not parse go back to the model as an error, not to the tool as {}
                    let args: Result<Value, String> = match &c["function"]["arguments"] {
                        Value::String(s) if s.trim().is_empty() => Ok(json!({})),
                        Value::String(s) => serde_json::from_str(s).map_err(|e| format!("Error: the arguments of {name} are not valid JSON ({e}); call it again")),
                        other => Ok(other.clone()),
                    };
                    let out = match args {
                        Ok(a) => exec(name, &a),
                        Err(e) => e.into(),
                    };
                    messages.push(json!({"role": "tool", "tool_call_id": c["id"], "content": out.text.chars().take(TOOL_TEXT_MAX).collect::<String>()}));
                    seen.extend(out.images.iter().map(|i| json!({"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64(i))}})));
                }
                // chat-completions tool messages are text only: the pictures follow as a user turn (OpenAI
                // itself; a local server may not have a vision model, so it gets the file paths alone)
                if !seen.is_empty() && provider(cfg) == "openai" {
                    let mut content = vec![json!({"type": "text", "text": "The pictures the tools above returned, in order."})];
                    content.extend(seen);
                    messages.push(json!({"role": "user", "content": content}));
                }
            }
            Ok(format!("{last_text}\n(stopped after {max_steps} steps)"))
        }
    }
}

/// Pull the first JSON array or object out of a model answer.
pub fn extract_json(text: &str) -> Option<Value> {
    for (open, close) in [('[', ']'), ('{', '}')] {
        if let (Some(a), Some(b)) = (text.find(open), text.rfind(close)) {
            if b > a {
                if let Ok(v) = serde_json::from_str::<Value>(&text[a..=b]) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// One answer about pictures (JPEG bytes): Claude, OpenAI, or a local vision model (llava and the like
/// on an OpenAI-compatible server).
pub fn complete_vision(cfg: &Config, system: &str, user: &str, images: &[Vec<u8>]) -> Result<String> {
    use base64::Engine;
    let b64: Vec<String> = images.iter().map(|i| base64::engine::general_purpose::STANDARD.encode(i)).collect();
    match provider(cfg).as_str() {
        "none" => bail!("no model is set up for looking at frames. Settings > Assistant: Claude, OpenAI, or a local server with a vision model."),
        "anthropic" => {
            let mut content: Vec<Value> = b64.iter().map(|d| json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": d}})).collect();
            content.push(json!({"type": "text", "text": user}));
            let v = claude(cfg, system, &json!([{"role": "user", "content": content}]), &[])?;
            if jsd(&v, "stop_reason", "") == "refusal" {
                bail!("the model declined this request");
            }
            Ok(ja(&v, "content").iter().filter(|b| jsd(b, "type", "") == "text").map(|b| jsd(b, "text", "")).collect::<Vec<_>>().join(""))
        }
        _ => {
            let mut content: Vec<Value> = vec![json!({"type": "text", "text": user})];
            content.extend(b64.iter().map(|d| json!({"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{d}")}})));
            let v = openai_chat(cfg, &json!([{"role": "system", "content": system}, {"role": "user", "content": content}]), &[])?;
            Ok(jsd(&v["choices"][0]["message"], "content", "").to_string())
        }
    }
}
