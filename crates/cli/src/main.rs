//! `linnea`: the same tools as the app and the MCP server, from a terminal.
//!   linnea tools                         list the tools
//!   linnea <tool> key=value ...          run one (values are JSON when they parse, else text)
//!   linnea <tool> '{"json": "args"}'     or pass the arguments as one JSON object
//!   linnea serve [--port N]              the local server (open the printed address in a browser)
//!   linnea mcp                           MCP server over stdio, for Claude Code and other agents
use linnea_core::{config::Config, proc::Rep, server, tools};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn parse_args(rest: &[String]) -> Result<Value, String> {
    if rest.len() == 1 && rest[0].trim_start().starts_with('{') {
        return serde_json::from_str(&rest[0]).map_err(|e| format!("the JSON arguments do not parse: {e}"));
    }
    let mut o = serde_json::Map::new();
    for a in rest {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("`{a}` is not key=value"))?;
        let k = k.trim_start_matches('-');
        // Dotted keys build nested objects: captions.style=hype
        let val: Value = serde_json::from_str(v).unwrap_or_else(|_| json!(v));
        let parts: Vec<&str> = k.split('.').collect();
        let mut cur = &mut o;
        for p in &parts[..parts.len() - 1] {
            cur = cur.entry(p.to_string()).or_insert_with(|| json!({})).as_object_mut().ok_or_else(|| format!("`{k}` crosses a value that is not an object"))?;
        }
        cur.insert(parts[parts.len() - 1].to_string(), val);
    }
    Ok(Value::Object(o))
}

fn help() {
    println!("linnea {} - local reel editor\n", linnea_core::VERSION);
    println!("  linnea tools                      list the tools\n  linnea <tool> key=value ...       run a tool\n  linnea serve [--port N]           local server for the interface\n  linnea mcp                        MCP server (stdio)\n  linnea agent                      how to connect Claude Code\n");
    println!("First run:  linnea doctor");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(|s| s.as_str()) else { return help() };
    match cmd {
        "-h" | "--help" | "help" => help(),
        "mcp" => linnea_core::mcp::serve(),
        "serve" => {
            let port = args.iter().position(|a| a == "--port").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok());
            match server::start(port) {
                Ok(p) => {
                    println!("Linnea is at http://127.0.0.1:{p}  (Ctrl+C stops it)");
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3600));
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "tools" | "list" => {
            for t in tools::all().iter().filter(|t| !t.app_only) {
                println!("  {:<12} {}", t.name, t.desc.split(". ").next().unwrap_or(""));
            }
        }
        "agent" => {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "linnea".into());
            println!("Claude Code:\n  claude mcp add linnea -- \"{exe}\" mcp\n\nAny MCP client (JSON):\n  {{\"mcpServers\": {{\"linnea\": {{\"command\": \"{}\", \"args\": [\"mcp\"]}}}}}}\n\nThe skill for agents is in skill/linnea/SKILL.md (copy the folder to ~/.claude/skills/linnea).", exe.replace('\\', "/"));
        }
        name => {
            let a = match parse_args(&args[1..]) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(2);
                }
            };
            let last = std::sync::Mutex::new(-1i64);
            let rep = Rep::new(
                Arc::new(move |f, msg| {
                    let pc = (f * 100.0) as i64;
                    let mut l = last.lock().unwrap();
                    if pc != *l {
                        *l = pc;
                        eprint!("\r{msg} {pc:>3}%   ");
                    }
                }),
                Arc::new(AtomicBool::new(false)),
            );
            let res = tools::call_with(&Config::load(), "terminal", name, &a, false, rep);
            linnea_core::film::close_pages();
            match res {
                Ok(v) => {
                    eprint!("\r                                        \r");
                    // Tables read better as lines.
                    if let Some(s) = v.get("sentences").and_then(|s| s.as_array()) {
                        for l in s {
                            println!("{}", l.as_str().unwrap_or(""));
                        }
                    } else {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    }
                }
                Err(e) => {
                    eprintln!("\nError: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
