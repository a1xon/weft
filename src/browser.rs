//! Marks from the browser. The extension in `extension/` lets the person mark something on a web
//! page and say what is wrong; weft makes a card of it on the board they pick, with the page, the
//! element, its component and screenshots (the wording is `words::marked`, the card `Cmd::Mark`).
//!
//! The browser reaches weft through native messaging: it starts this binary (`weft
//! chrome-extension://<id>/`) for every request, writes one message to its stdin and reads the
//! answer from its stdout, each a JSON object behind its length. So neither the extension nor any
//! page ever knows a daemon's address or token, and only this extension (its id is fixed by the key
//! in its manifest) may start the host. `weft extension` puts the extension's files in place and
//! tells the browsers about the host.
//!
//! A PDF cannot be marked in the browser's own viewer (no extension may run there), so the
//! extension opens it in a viewer of its own (`viewer.html`, PDF.js) where lines, paragraphs and
//! areas of the pages are marked; the card then says the file, the page and the text. Markdown
//! (a `.md` file dropped on the browser goes there by itself) opens in its Markdown viewer
//! (`md.html`, markdown-it), where selected text and blocks are marked; the card says the file, the
//! lines and the text.

use crate::{
    boards, client,
    model::{self, home, image_ext},
    words,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// The host's name, as the extension asks for it (`HOST` in background.js).
pub const HOST_NAME: &str = "weft.browser";
/// The extension's id: derived from the public key in its manifest, the same on every machine.
pub const EXTENSION_ID: &str = "jbabimhfbehpngcbcpdllclojiebcomn";

/// The extension, as `weft extension` writes it out (built into the binary like the page is).
const FILES: &[(&str, &[u8])] = &[
    ("manifest.json", include_bytes!("../extension/manifest.json")),
    ("background.js", include_bytes!("../extension/background.js")),
    ("mark.js", include_bytes!("../extension/mark.js")),
    ("probe.js", include_bytes!("../extension/probe.js")),
    ("viewer.html", include_bytes!("../extension/viewer.html")),
    ("viewer.js", include_bytes!("../extension/viewer.js")),
    ("viewer.css", include_bytes!("../extension/viewer.css")),
    ("view.mjs", include_bytes!("../extension/view.mjs")),
    ("md.html", include_bytes!("../extension/md.html")),
    ("md.js", include_bytes!("../extension/md.js")),
    ("md-open.js", include_bytes!("../extension/md-open.js")),
    ("pdfjs/LICENSE", include_bytes!("../extension/pdfjs/LICENSE")),
    ("mdit/LICENSE", include_bytes!("../extension/mdit/LICENSE")),
    // weft's own fonts, for the box and the viewers
    ("fonts/jbmono.woff2", include_bytes!("../web/jbmono.woff2")),
    ("fonts/chakra-600.woff2", include_bytes!("../web/chakra-600.woff2")),
    ("icons/16.png", include_bytes!("../extension/icons/16.png")),
    ("icons/32.png", include_bytes!("../extension/icons/32.png")),
    ("icons/48.png", include_bytes!("../extension/icons/48.png")),
    ("icons/128.png", include_bytes!("../extension/icons/128.png")),
];

/// PDF.js and markdown-it, gzipped by build.rs: written out unpacked.
const PACKED: &[(&str, &[u8])] = &[
    ("pdfjs/pdf.min.mjs", include_bytes!(concat!(env!("OUT_DIR"), "/pdf.min.mjs.gz"))),
    ("pdfjs/pdf.worker.min.mjs", include_bytes!(concat!(env!("OUT_DIR"), "/pdf.worker.min.mjs.gz"))),
    ("mdit/markdown-it.min.mjs", include_bytes!(concat!(env!("OUT_DIR"), "/markdown-it.min.mjs.gz"))),
];

/// Where browsers of the Chromium family look for native messaging hosts, under the config folder.
const BROWSERS: &[(&str, &str)] = &[
    ("Brave", "BraveSoftware/Brave-Browser"),
    ("Chrome", "google-chrome"),
    ("Chrome Beta", "google-chrome-beta"),
    ("Chrome Dev", "google-chrome-unstable"),
    ("Chromium", "chromium"),
    ("Edge", "microsoft-edge"),
    ("Vivaldi", "vivaldi"),
];

/// The same on macOS, under `~/Library/Application Support`.
const MAC_BROWSERS: &[(&str, &str)] = &[
    ("Brave", "BraveSoftware/Brave-Browser"),
    ("Chrome", "Google/Chrome"),
    ("Chrome Beta", "Google/Chrome Beta"),
    ("Chrome Canary", "Google/Chrome Canary"),
    ("Chromium", "Chromium"),
    ("Edge", "Microsoft Edge"),
    ("Vivaldi", "Vivaldi"),
];

/// A browser message is at most this big (the browser itself allows 64 MiB to a host).
const MAX_IN: usize = 64 << 20;

// ------------------------------------------------------------------ what the extension sends

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub w: u32,
    pub h: u32,
    pub dpr: f64,
    /// a PDF in the extension's viewer (`url` is the PDF's)
    pub pdf: Option<PdfDoc>,
    /// a Markdown file in the extension's Markdown viewer (`url` is the file's)
    pub md: Option<MdDoc>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct MdDoc {
    /// the file on this machine, for a `file://` address
    pub file: String,
    pub lines: u32,
}

/// Which lines of a Markdown file a mark is (from 1), what they are and the lines themselves.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct MdAt {
    pub from: u32,
    pub to: u32,
    /// text (selected), heading, paragraph, list, item, quote, code, table, row, divider, section, file, line, area
    pub kind: String,
    pub source: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct PdfDoc {
    /// the file on this machine, for a `file://` PDF
    pub file: String,
    pub pages: u32,
}

/// Where on the pages of a PDF a mark is, in points (1/72 inch) from the top left of its page.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct PdfAt {
    pub page: u32,
    /// the page it ends on, when it goes over more than one
    pub last: u32,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// the page's size
    pub pw: i64,
    pub ph: i64,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// A component that rendered the marked element, and where it is written.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct CompRef {
    pub name: String,
    pub src: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Comp {
    /// react, vue, svelte, angular, astro
    pub fw: String,
    /// where the element itself is written, when the framework tells
    pub at: String,
    /// the components around it, innermost first
    pub chain: Vec<CompRef>,
}

/// One mark: what the person said about it and what the element is.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Mark {
    pub note: String,
    /// `button.cta`
    pub label: String,
    pub selector: String,
    pub text: String,
    pub html: String,
    pub styles: String,
    pub attrs: BTreeMap<String, String>,
    pub rect: Rect,
    /// an area was drawn (the label names the element that holds it)
    pub area: bool,
    pub comp: Option<Comp>,
    /// data-testid, data-astro-source-file, inspector plugins' attributes
    pub hints: BTreeMap<String, String>,
    /// in a PDF: the page and the place on it (`label` and `text` say what is there)
    pub pdf: Option<PdfAt>,
    /// in a Markdown file: the lines
    pub md: Option<MdAt>,
    /// the text that was selected, when that is the mark
    pub quote: String,
    /// screenshots as data URLs: the mark with room around it, the whole viewport
    pub crop: Option<String>,
    pub view: Option<String>,
    /// where they were saved
    #[serde(skip)]
    pub crop_file: String,
    #[serde(skip)]
    pub view_file: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct LogLine {
    /// error, warn, uncaught, rejection, load
    pub k: String,
    pub m: String,
    pub n: u32,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Failed {
    pub status: u32,
    pub url: String,
    pub kind: String,
}

/// What went wrong on the page (probe.js).
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct PageLog {
    pub log: Vec<LogLine>,
    pub failed: Vec<Failed>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
struct Send {
    board: String,
    new_board: bool,
    cwd: String,
    to: String,
    agent: String,
    draft: bool,
    page: Page,
    marks: Vec<Mark>,
    log: PageLog,
}

// ------------------------------------------------------------------ the host

/// The native messaging host: answers the browser's messages until it closes stdin.
pub async fn host() -> i32 {
    let mut stdin = std::io::stdin().lock();
    loop {
        let mut len = [0u8; 4];
        if stdin.read_exact(&mut len).is_err() {
            return 0;
        }
        let n = u32::from_ne_bytes(len) as usize;
        if n > MAX_IN {
            return 1;
        }
        let mut buf = vec![0u8; n];
        if stdin.read_exact(&mut buf).is_err() {
            return 1;
        }
        let reply = match serde_json::from_slice::<Value>(&buf) {
            Ok(req) => handle(req).await,
            Err(e) => json!({"ok": false, "error": format!("not understood: {e}")}),
        };
        if write_message(&mut std::io::stdout().lock(), &reply).is_err() {
            return 1;
        }
    }
}

fn write_message(out: &mut impl Write, v: &Value) -> std::io::Result<()> {
    let b = v.to_string();
    out.write_all(&(b.len() as u32).to_ne_bytes())?;
    out.write_all(b.as_bytes())?;
    out.flush()
}

async fn handle(req: Value) -> Value {
    match req["op"].as_str().unwrap_or("") {
        "boards" => boards_reply(req["url"].as_str().unwrap_or("")),
        "send" => match serde_json::from_value::<Send>(req) {
            Ok(s) => send(s).await.unwrap_or_else(|e| json!({"ok": false, "error": e})),
            Err(e) => json!({"ok": false, "error": format!("not understood: {e}")}),
        },
        op => json!({"ok": false, "error": format!("unknown request {op:?}")}),
    }
}

/// The boards to choose from (with the agents of those that run), the agents a new card can be,
/// and which board the page most likely belongs to.
fn boards_reply(url: &str) -> Value {
    let cfg = model::load_config();
    let all = boards::all();
    let kinds: Vec<String> = cfg.agents.keys().filter(|k| *k != "sh").cloned().collect();
    let list: Vec<Value> = all
        .iter()
        .map(|b| {
            let mut agents: Vec<Value> = Vec::new();
            if let Some(i) = b.up.then(|| client::find_running(&b.dir)).flatten().filter(|i| Path::new(&i.dir) == b.dir) {
                if let Ok(r) = client::call_at(i.port, &i.token, json!({"op": "ls"})) {
                    for n in r["nodes"].as_array().into_iter().flatten() {
                        if kinds.iter().any(|k| n["agent"] == json!(k)) {
                            agents.push(json!({"id": n["id"], "name": n["name"], "agent": n["agent"], "status": n["status"]}));
                        }
                    }
                }
            }
            json!({"dir": b.dir, "name": b.name(), "up": b.up, "working": if b.up { b.live.working } else { 0 }, "agents": agents})
        })
        .collect();
    // in the launch pad's order (`agentEntries` in 30-cards.js)
    let rank = |k: &str| ["claude", "agy", "codex"].iter().position(|x| *x == k).unwrap_or(99);
    let mut agents = kinds.clone();
    agents.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    json!({"ok": true, "build": model::BUILD, "boards": list, "agents": agents,
           "agent": cfg.prefs["agent"].as_str().unwrap_or(""), "guess": guess(url, &all)})
}

/// Make the card: the board's daemon woken (or started, for a new board), the screenshots saved
/// in its shots folder, the prompt written, and the daemon asked.
async fn send(mut s: Send) -> Result<Value, String> {
    if s.marks.is_empty() {
        return Err("nothing was marked".into());
    }
    let dir = PathBuf::from(&s.board);
    let known = boards::all().iter().any(|b| b.dir == dir);
    if !known && !(s.new_board && dir.is_dir()) {
        return Err(format!("{} is not a weft board", dir.display()));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut info = boards::ensure(&dir, &exe, false).await?;
    let shots = dir.join(".weft").join("shots");
    let stamp = crate::engine::now_ms();
    for (i, m) in s.marks.iter_mut().enumerate() {
        if let Some(p) = m.crop.take().and_then(|u| save_image(&shots, &format!("mark-{stamp}-{}", i + 1), &u)) {
            m.crop_file = p.to_string_lossy().into_owned();
        }
        if let Some(p) = m.view.take().and_then(|u| save_image(&shots, &format!("mark-{stamp}-{}-page", i + 1), &u)) {
            m.view_file = p.to_string_lossy().into_owned();
        }
    }
    let text = words::marked(&s.page, &s.marks, &s.log);
    let cwd = (!s.cwd.trim().is_empty()).then(|| s.cwd.trim().to_string());
    let cmd = json!({"op": "mark", "text": text, "to": s.to, "agent": s.agent, "cwd": cwd, "draft": s.draft});
    let r = match client::call_at(info.port, &info.token, cmd.clone()) {
        // a daemon of an older build does not know marks yet: replaced by this one, as `weft` in a
        // terminal would (its agents keep running), then asked again
        Err(e) if e.contains(" 422") || e.contains(" 400") => {
            info = boards::ensure(&dir, &exe, true).await?;
            client::call_at(info.port, &info.token, cmd)?
        }
        r => r?,
    };
    let name = boards::all().into_iter().find(|b| b.dir == dir).map(|b| b.name()).unwrap_or_default();
    Ok(json!({"ok": true, "id": r["id"], "name": card_name(&info, r["id"].as_str().unwrap_or("")), "queued": r["queued"],
              "typed": r["typed"], "host": r["host"], "board": name, "url": boards::url(&info, None)}))
}

fn card_name(info: &client::Info, id: &str) -> String {
    let Ok(r) = client::call_at(info.port, &info.token, json!({"op": "ls"})) else { return String::new() };
    r["nodes"].as_array().into_iter().flatten().find(|n| n["id"] == json!(id)).and_then(|n| n["name"].as_str()).unwrap_or("").to_string()
}

/// A screenshot from a data URL, saved as `<name>.<png|jpg|webp>` (only images are kept).
fn save_image(dir: &Path, name: &str, data_url: &str) -> Option<PathBuf> {
    let b64 = data_url.split_once(";base64,").map(|(_, b)| b)?;
    let bytes = base64(b64)?;
    let ext = image_ext(&bytes)?;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{name}.{ext}"));
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        acc = (acc << 6) | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

// ------------------------------------------------------------------ which board a page belongs to

/// The board of the page's server: the folder of the process that listens on the page's port (a
/// dev server on this machine), its repository, the board that has it. A weft canvas is its own
/// board. A folder no board has yet is offered as a new one. A file opened from this machine (a
/// page or a PDF): see `guess_file`.
fn guess(url: &str, all: &[boards::Board]) -> Value {
    if let Some(path) = file_path(url) {
        return guess_file(&path, all);
    }
    let Some(port) = local_port(url) else { return Value::Null };
    let Some(pid) = listener(port) else { return Value::Null };
    // a board's own canvas
    for b in all.iter().filter(|b| b.up) {
        if let Some(i) = client::find_running(&b.dir).filter(|i| i.port == port && i.pid == pid) {
            return json!({"dir": i.dir, "cwd": "", "new": false, "why": format!("the canvas of {}", b.name())});
        }
    }
    let Some(cwd) = cwd_of(pid) else { return Value::Null };
    if cwd == home() || cwd.parent().is_none() {
        return Value::Null;
    }
    let folder = repo_root(&cwd).unwrap_or(cwd);
    let shown = tilde(&folder);
    match boards::owner_in(all, &folder, &home()) {
        Some(b) => json!({"dir": b, "cwd": if b == folder { String::new() } else { folder.to_string_lossy().into_owned() },
                          "new": false, "why": format!("the server on :{port} runs in {shown}")}),
        None => {
            json!({"dir": folder, "cwd": "", "new": true, "why": format!("the server on :{port} runs in {shown}, which has no board yet")})
        }
    }
}

/// The board of a file on this machine: the board that has its folder (or the repository it is
/// in). Only a repository is offered as a new board: a PDF in the downloads folder is nobody's.
fn guess_file(path: &Path, all: &[boards::Board]) -> Value {
    let Some(dir) = path.parent().filter(|d| *d != home() && d.parent().is_some()) else { return Value::Null };
    let repo = repo_root(dir).filter(|r| *r != home());
    let folder = repo.clone().unwrap_or_else(|| dir.to_path_buf());
    let shown = tilde(&folder);
    match boards::owner_in(all, &folder, &home()) {
        // (a new agent works in the repository when the board is a folder above it)
        Some(b) => json!({"dir": b, "cwd": repo.filter(|r| *r != b).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default(),
                          "new": false, "file": true, "why": format!("the file is in {shown}")}),
        None if repo.is_some() => json!({"dir": folder, "cwd": "", "new": true, "file": true,
                                         "why": format!("the file is in {shown}, which has no board yet")}),
        None => Value::Null,
    }
}

/// The file a `file://` address names (`file:///home/x/a%20b.pdf`).
fn file_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let path = rest.split(['?', '#']).next()?;
    if !path.starts_with('/') {
        return None;
    }
    let b = path.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).and_then(|c| hex(*c)), b.get(i + 2).and_then(|c| hex(*c))) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// The port of a page served from this machine.
fn local_port(url: &str) -> Option<u16> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => {
            let (h, p) = v6.split_once(']')?;
            (h, p.strip_prefix(':'))
        }
        None => match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (authority, None),
        },
    };
    let local = host == "localhost" || host.ends_with(".localhost") || host == "::1" || host == "0.0.0.0" || host.starts_with("127.");
    if !local {
        return None;
    }
    match port {
        Some(p) => p.parse().ok(),
        None => Some(if scheme == "https" { 443 } else { 80 }),
    }
}

/// The folder a process works in.
fn cwd_of(pid: u32) -> Option<PathBuf> {
    if Path::new("/proc/self").exists() {
        return std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    }
    // no /proc (macOS): `n<path>` in lsof's field output
    let out = lsof(&["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])?;
    out.lines().find_map(|l| l.strip_prefix('n')).map(PathBuf::from)
}

fn lsof(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("lsof").args(args).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The process (of this user) that listens on a TCP port of this machine.
fn listener(port: u16) -> Option<u32> {
    if !Path::new("/proc/net/tcp").exists() {
        // no /proc (macOS)
        return lsof(&["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])?.lines().next()?.trim().parse().ok();
    }
    let mut inodes: Vec<String> = Vec::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(t) = std::fs::read_to_string(table) else { continue };
        for line in t.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            // local address `ip:PORT` in hex, state 0A = listening, then the socket's inode
            if f.len() > 9 && f[3] == "0A" && f[1].rsplit(':').next().and_then(|p| u16::from_str_radix(p, 16).ok()) == Some(port) {
                inodes.push(format!("socket:[{}]", f[9]));
            }
        }
    }
    if inodes.is_empty() {
        return None;
    }
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            if std::fs::read_link(fd.path()).is_ok_and(|l| inodes.iter().any(|i| l.as_os_str() == i.as_str())) {
                return Some(pid);
            }
        }
    }
    None
}

/// The top of the git repository a folder is in.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--show-toplevel"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !p.is_empty()).then(|| PathBuf::from(p))
}

fn tilde(p: &Path) -> String {
    match p.strip_prefix(home()) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

// ------------------------------------------------------------------ `weft extension`

/// Where the extension's files go: `~/.local/share/weft/extension`.
fn extension_dir() -> PathBuf {
    model::data_home().join("weft").join("extension")
}

/// The host manifest a browser reads: this binary, for this extension only.
fn host_manifest(exe: &Path) -> String {
    serde_json::to_string_pretty(&json!({
        "name": HOST_NAME,
        "description": "weft: what you mark on a web page becomes a card on your board",
        "path": exe,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    }))
    .unwrap_or_default()
}

/// Write the extension out and register the host with every browser of the Chromium family that is
/// set up here. Answers the folder and the browsers.
pub fn install() -> Result<(PathBuf, Vec<&'static str>), String> {
    let dir = extension_dir();
    let unpacked: Vec<(&str, Vec<u8>)> = PACKED
        .iter()
        .map(|(name, gz)| {
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(*gz).read_to_end(&mut bytes).map(|_| (*name, bytes)).map_err(|e| format!("{name}: {e}"))
        })
        .collect::<Result<_, _>>()?;
    for (name, bytes) in FILES.iter().copied().chain(unpacked.iter().map(|(n, b)| (*n, b.as_slice()))) {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        if std::fs::read(&p).ok().as_deref() != Some(bytes) {
            std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let manifest = host_manifest(&exe);
    let mut found = Vec::new();
    let (base, browsers) = if cfg!(target_os = "macos") {
        (home().join("Library/Application Support"), MAC_BROWSERS)
    } else {
        (model::config_home(), BROWSERS)
    };
    for (label, sub) in browsers {
        let profile = base.join(sub);
        if !profile.is_dir() {
            continue;
        }
        let hosts = profile.join("NativeMessagingHosts");
        std::fs::create_dir_all(&hosts).map_err(|e| format!("{}: {e}", hosts.display()))?;
        model::write_atomic(&hosts.join(format!("{HOST_NAME}.json")), manifest.as_bytes())
            .map_err(|e| format!("{}: {e}", hosts.display()))?;
        found.push(*label);
    }
    Ok((dir, found))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_served_from_this_machine_have_a_port_to_look_up() {
        assert_eq!(local_port("http://localhost:5173/pricing?x=1"), Some(5173));
        assert_eq!(local_port("http://127.0.0.1:8080"), Some(8080));
        assert_eq!(local_port("https://app.localhost/"), Some(443));
        assert_eq!(local_port("http://[::1]:3000/a"), Some(3000));
        assert_eq!(local_port("http://0.0.0.0:4321/#top"), Some(4321));
        assert_eq!(local_port("http://localhost/"), Some(80));
        assert_eq!(local_port("https://example.com:8443/"), None);
        assert_eq!(local_port("http://localhost.example.com:3000/"), None);
        assert_eq!(local_port("file:///home/x/index.html"), None);
    }

    #[test]
    fn a_file_address_names_its_file() {
        assert_eq!(file_path("file:///home/x/Report%20Q3.pdf"), Some(PathBuf::from("/home/x/Report Q3.pdf")));
        assert_eq!(file_path("file:///home/x/a.pdf#page=3"), Some(PathBuf::from("/home/x/a.pdf")));
        assert_eq!(file_path("file://localhost/tmp/b%C3%BCro.html"), Some(PathBuf::from("/tmp/büro.html")));
        assert_eq!(file_path("file:///home/x/100%.pdf"), Some(PathBuf::from("/home/x/100%.pdf")));
        assert_eq!(file_path("http://localhost:3000/a.pdf"), None);
    }

    #[test]
    fn a_file_belongs_to_the_board_of_its_folder() {
        let tmp = std::env::temp_dir().join(format!("weft-file-guess-{}", std::process::id()));
        let repo = tmp.join("shop");
        std::fs::create_dir_all(repo.join("docs")).unwrap();
        std::fs::create_dir_all(tmp.join("loose")).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"]).arg(&repo).status().unwrap().success());
        let pdf = repo.join("docs/report.pdf");
        let board = |d: &Path| boards::Board { dir: d.to_path_buf(), up: false, live: boards::Live::default() };
        let g = guess(&format!("file://{}", pdf.display()), &[board(&repo)]);
        assert_eq!((g["dir"].as_str(), g["cwd"].as_str(), g["new"].as_bool()), (repo.to_str(), Some(""), Some(false)));
        // a board above the repository: a new agent works in the repository
        let g = guess(&format!("file://{}", pdf.display()), &[board(&tmp)]);
        assert_eq!((g["dir"].as_str(), g["cwd"].as_str()), (tmp.to_str(), repo.to_str()));
        // nobody's repository: offered as a new board; a folder that is no repository: no guess
        let g = guess(&format!("file://{}", pdf.display()), &[]);
        assert_eq!((g["dir"].as_str(), g["new"].as_bool(), g["file"].as_bool()), (repo.to_str(), Some(true), Some(true)));
        assert_eq!(guess(&format!("file://{}", tmp.join("loose/x.pdf").display()), &[]), Value::Null);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn base64_screenshots_are_decoded() {
        assert_eq!(base64("aGVsbG8gd2VmdA==").unwrap(), b"hello weft");
        assert_eq!(base64("iVBORw0KGgo=").unwrap()[..4], [0x89, b'P', b'N', b'G']);
        assert!(base64("not*base64").is_none());
    }

    #[test]
    fn a_screenshot_is_saved_only_when_it_is_an_image() {
        let dir = std::env::temp_dir().join(format!("weft-marks-{}", std::process::id()));
        let png = save_image(&dir, "a", "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==").unwrap();
        assert!(png.ends_with("a.png") && png.is_file());
        assert!(save_image(&dir, "b", "data:text/html;base64,PGgxPmhpPC9oMT4=").is_none(), "html is no screenshot");
        assert!(save_image(&dir, "c", "garbage").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_host_speaks_length_prefixed_json() {
        let mut out = Vec::new();
        write_message(&mut out, &json!({"ok": true})).unwrap();
        assert_eq!(u32::from_ne_bytes(out[..4].try_into().unwrap()) as usize, out.len() - 4);
        assert_eq!(&out[4..], br#"{"ok":true}"#);
    }

    #[test]
    fn the_host_manifest_lets_only_this_extension_in() {
        let m: Value = serde_json::from_str(&host_manifest(Path::new("/opt/weft"))).unwrap();
        assert_eq!(m["name"], HOST_NAME);
        assert_eq!(m["path"], "/opt/weft");
        assert_eq!(m["allowed_origins"], json!([format!("chrome-extension://{EXTENSION_ID}/")]));
        // the extension's manifest carries the key this id is made from
        let ext: Value = serde_json::from_slice(FILES[0].1).unwrap();
        assert!(ext["key"].as_str().is_some_and(|k| k.len() > 300));
        assert!(ext["permissions"].as_array().unwrap().contains(&json!("nativeMessaging")));
    }

    #[test]
    fn a_server_listening_here_is_found_with_its_folder() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        if !Path::new("/proc/net/tcp").exists() {
            return;
        }
        assert_eq!(listener(port), Some(std::process::id()));
        drop(l);
        assert_eq!(listener(port), None);
    }
}
