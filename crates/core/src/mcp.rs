//! MCP server over stdio (JSON-RPC 2.0, one message per line). stdout belongs to the protocol.
use crate::tools;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const INSTRUCTIONS: &str = "Linnea is a local video studio with two kinds of work. FILMS written as code (VideoHub's contract): hub_new_film makes a folder with index.html + film.js where window.renderOut(t) draws second t of the film on a fixed-size canvas, synchronously or by returning a promise, as a pure function of t (no Date.now, no carried state, no unseeded random); window.READY = true once assets load; window.VH_SCENES = [{name, t0, t1}] labels the timeline; sound goes in audio/mix.wav. Draw every on-screen element through el()/KE.text with a stable id (see the template) so the user can move, resize and re-type it in the app; their edits sit in tweaks.json `edits` and must survive your rewrites. Reusable things (objects, characters, icons, animated props) belong in the puppet library: puppet_list first, puppet_save new ones (.ppet, or .rppet with animation states), puppet_link into the film and draw them with KE.puppet. Loop: write the code -> hub_live (it checks the page headless and shows it in the app) -> hub_frame to look at your own frames -> STOP and let the user review -> hub_notes (read images.crop first) -> fix -> hub_note_update status=fixed -> the preview reloads by itself. Never render: the user renders in the app when they are happy, and a partial render redoes only the chunks your fixed notes touch. FOOTAGE reels from a recording or a link: import -> transcribe -> clips_find / autocut / retakes -> style_apply -> beat_add -> snapshot. Every time in a footage reel is an ORIGINAL-recording second from `words`. Long tools return a job id: call hub_job (or job) with wait=true.";

pub fn serve() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(|l| l.ok()) {
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("");
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => Ok(json!({
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "linnea", "version": crate::VERSION},
                "instructions": INSTRUCTIONS,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools::all().iter().filter(|t| !t.app_only).map(|t| json!({"name": t.name, "description": t.desc, "inputSchema": t.schema()})).collect::<Vec<_>>()})),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("");
                let args = msg["params"].get("arguments").cloned().unwrap_or(json!({}));
                Ok(match tools::call("agent", name, &args, true) {
                    Ok(v) => json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}], "isError": false}),
                    Err(e) => json!({"content": [{"type": "text", "text": format!("Error: {e}")}], "isError": true}),
                })
            }
            m if m.starts_with("notifications/") => continue,
            "resources/list" => Ok(json!({"resources": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            other => Err((-32601, format!("method not found: {other}"))),
        };
        let Some(id) = id else { continue };
        let reply = match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        };
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}
