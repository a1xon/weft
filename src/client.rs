//! Thin synchronous client used by the CLI subcommands (`weft done`, `ls`, ...).
//! Inside an agent pane it uses WEFT_URL / WEFT_TOKEN; elsewhere it finds the
//! daemon of the current project through the state directory.

use crate::model::{project_id, state_dir};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Info {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub dir: String,
    pub session: String,
    pub sock: String,
    /// the build of weft that is serving (empty for daemons started before builds were stamped)
    #[serde(default)]
    pub build: String,
}

pub fn info_path(dir: &Path) -> PathBuf {
    state_dir().join(format!("{}.json", project_id(dir)))
}

pub fn pid_alive(pid: u32) -> bool {
    if Path::new("/proc/self").exists() {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    // no /proc (macOS): ask the system
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The running daemon for `dir` or the nearest parent that has one.
pub fn find_running(dir: &Path) -> Option<Info> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if let Ok(b) = std::fs::read(info_path(d)) {
            if let Ok(i) = serde_json::from_slice::<Info>(&b) {
                if pid_alive(i.pid) && TcpStream::connect(("127.0.0.1", i.port)).is_ok() {
                    return Some(i);
                }
            }
        }
        cur = d.parent();
    }
    None
}

/// Candidate daemons in order of preference: the pane's own environment, then
/// the project it was started for (survives a daemon restart), then the cwd.
pub fn endpoints() -> Vec<(u16, String)> {
    // an agent whose card went to another board reaches it there, and only there (on the board it was
    // started on, its card's id may be another card's by now)
    if let Some(m) = crate::boards::moved_here() {
        return find_running(Path::new(&m.dir)).map(|i| vec![(i.port, i.token)]).unwrap_or_default();
    }
    let mut v: Vec<(u16, String)> = Vec::new();
    if let (Ok(url), Ok(tok)) = (std::env::var("WEFT_URL"), std::env::var("WEFT_TOKEN")) {
        if let Some(p) = url.rsplit(':').next().and_then(|p| p.trim_end_matches('/').parse::<u16>().ok()) {
            v.push((p, tok));
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("WEFT_PROJECT") {
        dirs.push(PathBuf::from(p));
    }
    if let Ok(c) = std::env::current_dir() {
        dirs.push(c.canonicalize().unwrap_or(c));
    }
    for d in dirs {
        if let Some(i) = find_running(&d) {
            if !v.iter().any(|(p, _)| *p == i.port) {
                v.push((i.port, i.token));
            }
        }
    }
    v
}

/// One request to the daemon. HTTP/1.0 on purpose: the reply is then a plain body that ends when
/// the connection closes (no chunked framing to undo), which is all this client needs.
fn post(port: u16, token: &str, body: &str) -> Result<String, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let req = format!(
        "POST /api/cmd HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nX-Weft-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&resp).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").ok_or("bad response")?;
    if !head.starts_with("HTTP/1.") || !head.lines().next().unwrap_or("").contains(" 200") {
        return Err(format!("daemon refused request ({})", head.lines().next().unwrap_or("")));
    }
    Ok(body.to_string())
}

/// A daemon's answer: one that says `"ok": false` is an error.
fn reply(raw: &str) -> Result<Value, String> {
    let r: Value = serde_json::from_str(raw).map_err(|e| format!("bad reply: {e}"))?;
    if r.get("ok") == Some(&json!(false)) {
        return Err(r.get("error").and_then(|e| e.as_str()).unwrap_or("failed").to_string());
    }
    Ok(r)
}

/// Talk to one specific daemon (the one `find_running` found), not the one the cwd implies.
pub fn call_at(port: u16, token: &str, v: Value) -> Result<Value, String> {
    reply(&post(port, token, &v.to_string())?)
}

pub fn call(v: Value) -> Result<Value, String> {
    let eps = endpoints();
    if eps.is_empty() {
        return Err("no weft daemon for this directory - run `weft` first".into());
    }
    let body = v.to_string();
    let mut last = String::new();
    for (port, token) in eps {
        match post(port, &token, &body) {
            Ok(raw) => return reply(&raw),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Resolve a node by id (`n3`) or by name (`reviewer`).
pub fn resolve(name: &str) -> Result<String, String> {
    let r = call(json!({"op": "ls"}))?;
    let nodes = r["nodes"].as_array().map_or(&[][..], Vec::as_slice);
    for n in nodes {
        if n["id"] == name {
            return Ok(name.to_string());
        }
    }
    for n in nodes {
        if n["name"] == name {
            return Ok(n["id"].as_str().unwrap_or_default().to_string());
        }
    }
    Err(format!("no node called \"{name}\""))
}
