//! Posting a finished video: YouTube (OAuth, resumable upload), TikTok (Content Posting API: to the
//! inbox as a draft by default, or a direct post) and Instagram Reels (Graph API, resumable upload).
//! Publishing is outward-facing, so agents get a refusal unless the user turned publish.agent on.
use crate::config::*;
use crate::net;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

fn guard(c: &Ctx) -> Result<()> {
    if c.origin == "agent" && c.cfg.get("publish.agent").and_then(|v| v.as_bool()) != Some(true) {
        bail!("publishing is the user's step: they press Publish in Linnea. Tell them the video is ready instead.");
    }
    Ok(())
}
fn video(a: &Value) -> Result<(PathBuf, Vec<u8>)> {
    let f = PathBuf::from(jsd(a, "file", ""));
    if !f.is_file() {
        bail!("no such video: {}", f.display());
    }
    let b = std::fs::read(&f)?;
    Ok((f, b))
}
fn save_key(name: &str, val: &str) -> Result<()> {
    let mut cfg = Config::load();
    cfg.0["keys"][name] = json!(val);
    cfg.save()
}

// ---------- YouTube ----------

fn redirect(port: u16) -> String {
    format!("http://127.0.0.1:{port}/oauth/youtube")
}

/// The Google sign-in page for this app's OAuth client (a Desktop client from Google Cloud Console).
fn t_login(c: &Ctx, _a: &Value) -> Result<Value> {
    let id = c.cfg.key("youtube_client_id").ok_or_else(|| anyhow!("add keys.youtube_client_id and keys.youtube_client_secret (a Desktop OAuth client in Google Cloud Console with the YouTube Data API enabled)"))?;
    let port = read_json_or(&config_dir().join("server.json"), json!({}))["port"].as_u64().unwrap_or(7790) as u16;
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={id}&redirect_uri={}&response_type=code&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fyoutube.upload&access_type=offline&prompt=consent",
        redirect(port).replace(':', "%3A").replace('/', "%2F")
    );
    Ok(json!({"url": url, "note": "open it, allow access; Linnea keeps the refresh token"}))
}

/// Called by the local server when Google sends the browser back with a code.
pub fn oauth_youtube(code: &str, port: u16) -> Result<()> {
    let cfg = Config::load();
    let (id, secret) = (cfg.key("youtube_client_id").unwrap_or_default(), cfg.key("youtube_client_secret").unwrap_or_default());
    let body = format!("code={code}&client_id={id}&client_secret={secret}&redirect_uri={}&grant_type=authorization_code", redirect(port));
    let (st, text) = net::post_bytes("https://oauth2.googleapis.com/token", &[], "application/x-www-form-urlencoded", body.as_bytes(), 60)?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    let rt = js(&v, "refresh_token").ok_or_else(|| anyhow!("Google did not return a refresh token ({st}): {}", text.chars().take(200).collect::<String>()))?;
    save_key("youtube_refresh", rt)
}

fn youtube_token(c: &Ctx) -> Result<String> {
    let rt = c.cfg.key("youtube_refresh").ok_or_else(|| anyhow!("not signed in to YouTube: Settings > Publishing > Connect YouTube"))?;
    let body = format!("refresh_token={rt}&client_id={}&client_secret={}&grant_type=refresh_token", c.cfg.key("youtube_client_id").unwrap_or_default(), c.cfg.key("youtube_client_secret").unwrap_or_default());
    let (_, text) = net::post_bytes("https://oauth2.googleapis.com/token", &[], "application/x-www-form-urlencoded", body.as_bytes(), 60)?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    js(&v, "access_token").map(|s| s.to_string()).ok_or_else(|| anyhow!("YouTube sign-in expired: connect again ({})", text.chars().take(200).collect::<String>()))
}

fn t_youtube(c: &Ctx, a: &Value) -> Result<Value> {
    guard(c)?;
    let (_, bytes) = video(a)?;
    let tok = youtube_token(c)?;
    let meta = json!({
        "snippet": {"title": jsd(a, "title", "Video"), "description": jsd(a, "description", ""), "tags": a["tags"], "categoryId": jsd(a, "category", "22")},
        "status": {"privacyStatus": jsd(a, "privacy", "private"), "selfDeclaredMadeForKids": false}
    });
    c.rep.p(0.02, "YouTube");
    let (st, hs, text) = net::send(
        "POST",
        "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status",
        &[("Authorization", format!("Bearer {tok}")), ("Content-Type", "application/json; charset=UTF-8".into()), ("X-Upload-Content-Type", "video/mp4".into()), ("X-Upload-Content-Length", bytes.len().to_string())],
        meta.to_string().as_bytes(),
        &["location"],
        120,
    )?;
    let loc = hs.iter().find(|(k, _)| k == "location").map(|x| x.1.clone()).ok_or_else(|| anyhow!("YouTube refused the upload ({st}): {}", text.chars().take(300).collect::<String>()))?;
    c.rep.p(0.1, "uploading to YouTube");
    let (st, _, text) = net::send("PUT", &loc, &[("Content-Type", "video/mp4".into())], &bytes, &[], 3600)?;
    if st >= 300 {
        bail!("YouTube upload failed ({st}): {}", text.chars().take(300).collect::<String>());
    }
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    let id = jsd(&v, "id", "");
    Ok(json!({"video_id": id, "url": format!("https://youtu.be/{id}"), "privacy": jsd(a, "privacy", "private")}))
}

// ---------- TikTok ----------

fn t_tiktok(c: &Ctx, a: &Value) -> Result<Value> {
    guard(c)?;
    let (_, bytes) = video(a)?;
    let tok = c.cfg.key("tiktok").ok_or_else(|| anyhow!("add keys.tiktok: a TikTok access token with video.upload (draft) or video.publish (direct) scope from your TikTok developer app"))?;
    let direct = jsd(a, "mode", "draft") == "direct";
    let n = bytes.len();
    let mut body = json!({"source_info": {"source": "FILE_UPLOAD", "video_size": n, "chunk_size": n, "total_chunk_count": 1}});
    if direct {
        body["post_info"] = json!({"title": jsd(a, "title", ""), "privacy_level": jsd(a, "privacy", "SELF_ONLY"), "disable_comment": false, "disable_duet": false, "disable_stitch": false});
    }
    let url = if direct { "https://open.tiktokapis.com/v2/post/publish/video/init/" } else { "https://open.tiktokapis.com/v2/post/publish/inbox/video/init/" };
    let (st, v) = net::post_json(url, &[("Authorization", format!("Bearer {tok}"))], &body, 120)?;
    let up = jsd(&v["data"], "upload_url", "").to_string();
    if st >= 300 || up.is_empty() {
        bail!("TikTok refused the post ({st}): {}", v["error"]);
    }
    c.rep.p(0.1, "uploading to TikTok");
    let (st, _, text) = net::send("PUT", &up, &[("Content-Type", "video/mp4".into()), ("Content-Range", format!("bytes 0-{}/{n}", n - 1))], &bytes, &[], 3600)?;
    if st >= 300 {
        bail!("TikTok upload failed ({st}): {}", text.chars().take(300).collect::<String>());
    }
    let pid = jsd(&v["data"], "publish_id", "").to_string();
    let (_, stv) = net::post_json("https://open.tiktokapis.com/v2/post/publish/status/fetch/", &[("Authorization", format!("Bearer {tok}"))], &json!({"publish_id": pid}), 60)?;
    Ok(json!({"publish_id": pid, "status": stv["data"]["status"], "where": if direct { "posted (privacy as chosen)" } else { "in your TikTok inbox as a draft: open TikTok to finish the post" }}))
}

// ---------- Instagram ----------

fn t_instagram(c: &Ctx, a: &Value) -> Result<Value> {
    guard(c)?;
    let (_, bytes) = video(a)?;
    let tok = c.cfg.key("instagram").ok_or_else(|| anyhow!("add keys.instagram: a long-lived Instagram Graph API token (instagram_content_publish) and instagram.user_id"))?;
    let uid = c.cfg.str("instagram.user_id");
    if uid.is_empty() {
        bail!("set instagram.user_id (the Instagram professional account id) in settings");
    }
    let ver = { let v = c.cfg.str("instagram.version"); if v.is_empty() { "v22.0".to_string() } else { v } };
    let cap: String = jsd(a, "caption", "").chars().map(|ch| if ch.is_ascii_alphanumeric() || "-_.~".contains(ch) { ch.to_string() } else { ch.to_string().bytes().map(|b| format!("%{b:02X}")).collect() }).collect();
    let (st, _, text) = net::send(
        "POST",
        &format!("https://graph.facebook.com/{ver}/{uid}/media?media_type=REELS&upload_type=resumable&caption={cap}&share_to_feed=true&access_token={tok}"),
        &[],
        b"",
        &[],
        120,
    )?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({}));
    let cid = js(&v, "id").ok_or_else(|| anyhow!("Instagram refused the reel ({st}): {}", text.chars().take(300).collect::<String>()))?.to_string();
    c.rep.p(0.1, "uploading to Instagram");
    let (st, _, text) = net::send(
        "POST",
        &format!("https://rupload.facebook.com/ig-api-upload/{ver}/{cid}"),
        &[("Authorization", format!("OAuth {tok}")), ("offset", "0".into()), ("file_size", bytes.len().to_string())],
        &bytes,
        &[],
        3600,
    )?;
    if st >= 300 {
        bail!("Instagram upload failed ({st}): {}", text.chars().take(300).collect::<String>());
    }
    for _ in 0..120 {
        c.rep.check()?;
        std::thread::sleep(std::time::Duration::from_secs(5));
        let (_, s) = net::get_json(&format!("https://graph.facebook.com/{ver}/{cid}?fields=status_code,status&access_token={tok}"), &[], 60)?;
        match jsd(&s, "status_code", "") {
            "FINISHED" => break,
            "ERROR" | "EXPIRED" => bail!("Instagram could not process the video: {}", jsd(&s, "status", "")),
            _ => c.rep.p(0.6, "Instagram is processing"),
        }
    }
    let (st, _, text) = net::send("POST", &format!("https://graph.facebook.com/{ver}/{uid}/media_publish?creation_id={cid}&access_token={tok}"), &[], b"", &[], 120)?;
    if st >= 300 {
        bail!("Instagram publish failed ({st}): {}", text.chars().take(300).collect::<String>());
    }
    Ok(json!({"media": serde_json::from_str::<Value>(&text).unwrap_or(json!({}))["id"], "note": "posted to Instagram"}))
}

const F: (&str, &str, &str) = ("file!", "string", "The rendered video");

pub const TOOLS: &[Tool] = &[
    Tool { name: "publish_login", desc: "The Google sign-in page for posting to YouTube (needs keys.youtube_client_id/secret of a Desktop OAuth client).", params: &[], long: false, app_only: false, run: t_login },
    Tool { name: "publish_youtube", desc: "Upload a rendered video to YouTube (private by default). Only when the user asked; agents are refused unless publish.agent is on.", params: &[F, ("title!", "string", "Title"), ("description", "string", "Description, add #Shorts for a Short"), ("privacy", "string", "private | unlisted | public"), ("tags", "array", "Tags")], long: true, app_only: false, run: t_youtube },
    Tool { name: "publish_tiktok", desc: "Send a rendered video to TikTok: mode draft (to the account's inbox, finished in the app) or direct. Only when the user asked.", params: &[F, ("title", "string", "Caption (direct)"), ("mode", "string", "draft | direct"), ("privacy", "string", "SELF_ONLY | MUTUAL_FOLLOW_FRIENDS | FOLLOWER_OF_CREATOR | PUBLIC_TO_EVERYONE")], long: true, app_only: false, run: t_tiktok },
    Tool { name: "publish_instagram", desc: "Post a rendered video as an Instagram Reel (Graph API). Only when the user asked.", params: &[F, ("caption", "string", "Caption")], long: true, app_only: false, run: t_instagram },
];
