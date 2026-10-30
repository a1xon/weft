//! weft - a canvas for coding agents. tmux is the only runtime.
//!
//! One small daemon per project keeps the flow graph, watches every agent
//! through tmux control mode and serves the canvas UI on loopback.

// a card's row for the browsers is one json! with many keys (engine/ui.rs `rt_json`)
#![recursion_limit = "256"]

mod asks;
mod boards;
mod browser;
mod client;
mod dev; // dev: the page follows edits to web/ while weft's UI is worked on
mod engine;
mod launch;
mod model;
mod server;
mod term;
mod tmux;
mod web;
mod words;

use serde_json::json;
use std::{
    io::{IsTerminal, Read},
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

const USAGE: &str = "\
weft - a canvas for coding agents (tmux is the only runtime)

USAGE
  weft [dir]                 open the canvas for a project (starts the daemon); a folder
                             inside a board's folder, or one its agents work in, opens
                             that board, with new agents starting in the folder
  weft new [dir]             a board of its own for the folder, even inside another one
  weft app [dir] [--pick]    the same in a window of its own; no dir = the project
                             opened last, --pick asks for a folder
  weft desktop               put weft in the app grid (pin it to the dock from there)
  weft extension             the browser extension: mark something on a web page, say
                             what is wrong, and it becomes a card on your board
  weft serve [dir] [--port N] [--no-open]
                             run the daemon in the foreground
  weft stop [dir] [--keep]   quit: ends the daemon and every agent of the project
                             (--keep leaves the agents running in tmux)
  weft ls                    list nodes and their status
  weft send <node> <text|->  message an agent: typed now if it is free, else queued
        [--queue|--now]      force queueing / force typing immediately
  weft result <node>         print an agent's latest result
  weft read <node> [-n N]    print a node's terminal history
  weft run                   run the flow (start nodes that have prompts)
  weft done [text]           (inside an agent) DONE: report the node finished;
                             text or stdin becomes its output for the next nodes
  weft issue [text]          ISSUE: it cannot go on without the person (shown on its card)
  weft fail [text]           FAILURE: it tried and it cannot be done
  weft tell <id[,id]|all> <text|->
                             (inside an agent) say something to the agents it is
                             linked to talk with; nobody else hears it
  weft retry <node> [text|-] run a finished card again with the input it had; the
                             text (what was fixed since) goes with its task

Nodes can be addressed by id (n1) or by name.
Agents live in tmux socket `weft`:  tmux -L weft attach
Config: ~/.config/weft/config.json   Flow: <project>/.weft/flow.json";

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match rt.block_on(real_main(args)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("weft: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn dir_from(args: &[String]) -> Result<PathBuf, String> {
    let p = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    p.canonicalize().map_err(|e| format!("{}: {e}", p.display()))
}

async fn real_main(args: Vec<String>) -> Result<i32, String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match sub {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        "-V" | "--version" | "version" => {
            println!("weft {} (build {})", env!("CARGO_PKG_VERSION"), model::BUILD);
            Ok(0)
        }
        "serve" => serve(rest).await.map(|_| 0),
        "stop" => stop(rest).await,
        "done" => report(rest, Report::Done),
        "issue" => report(rest, Report::Issue),
        "fail" => report(rest, Report::Fail),
        "ls" => {
            let r = client::call(json!({"op": "ls"}))?;
            for n in r["nodes"].as_array().cloned().unwrap_or_default() {
                let eta = n["eta"].as_i64().map(|s| format!(" eta={}s", s)).unwrap_or_default();
                println!(
                    "{:<5} {:<26} {:<8} {:<10} runs={} queued={}{}",
                    n["id"].as_str().unwrap_or(""),
                    n["name"].as_str().unwrap_or(""),
                    n["agent"].as_str().unwrap_or(""),
                    n["status"].as_str().unwrap_or(""),
                    n["runs"],
                    n["queued"],
                    eta
                );
            }
            Ok(0)
        }
        "read" => {
            let name = rest.first().ok_or("usage: weft read <node> [-n lines]")?;
            let lines =
                rest.iter().position(|a| a == "-n").and_then(|i| rest.get(i + 1)).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let id = client::resolve(name)?;
            let r = client::call(json!({"op": "read", "id": id, "lines": lines}))?;
            println!("{}", r["text"].as_str().unwrap_or(""));
            Ok(0)
        }
        "send" => {
            let mode = if rest.iter().any(|a| a == "--queue") {
                "queue"
            } else if rest.iter().any(|a| a == "--now") {
                "now"
            } else {
                "auto"
            };
            let rest: Vec<String> = rest.iter().filter(|a| *a != "--queue" && *a != "--now").cloned().collect();
            let name = rest.first().ok_or("usage: weft send [--queue|--now] <node> <text|->")?;
            let mut text = rest[1..].join(" ");
            if text == "-" || (text.is_empty() && !std::io::stdin().is_terminal()) {
                text.clear();
                std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
            }
            let id = client::resolve(name)?;
            client::call(json!({"op": "send", "id": id, "text": text.trim_end(), "mode": mode, "submit": true}))?;
            Ok(0)
        }
        "result" => {
            let name = rest.first().ok_or("usage: weft result <node>")?;
            let id = client::resolve(name)?;
            let r = client::call(json!({"op": "result", "id": id}))?;
            println!("{}", r["text"].as_str().unwrap_or(""));
            Ok(0)
        }
        "run" => {
            let r = client::call(json!({"op": "run"}))?;
            println!("started {} node(s)", r["started"]);
            Ok(0)
        }
        "shot" => shot(rest),
        "tell" => tell(rest),
        "retry" => retry(rest),
        "eta" => eta(rest),
        "open" => open(rest).await.map(|_| 0),
        "app" => match launch::app_dir(rest).map_err(launch::tell)? {
            Some(dir) => {
                let (home, at) = if rest.iter().any(|a| a == "--new") { (dir, None) } else { board_for(dir) };
                open_dir(home, at, launch::Show::Window).await.map(|_| 0).map_err(launch::tell)
            }
            None => Ok(0),
        },
        // a board of its own, even inside another board's folder
        "new" => {
            let show = if rest.iter().any(|a| a == "--no-open") { launch::Show::Nothing } else { launch::Show::Tab };
            open_dir(dir_from(rest)?, None, show).await.map(|_| 0)
        }
        "extension" => extension(),
        // the browser starts weft as the extension's native messaging host (see browser.rs)
        s if s.starts_with("chrome-extension://") => Ok(browser::host().await),
        "desktop" if cfg!(target_os = "macos") => Err("weft desktop is for Linux desktops; on macOS, `weft app` opens a window of its own".into()),
        "desktop" => {
            let entry = launch::install()?;
            println!("weft is in the app grid now ({}): pin it to the dock from there", entry.display());
            Ok(0)
        }
        // `weft`, `weft <dir>`, `weft --no-open`: open the canvas
        _ => open(&args).await.map(|_| 0),
    }
}

// -------------------------------------------------------------- extension

/// `weft extension`: the extension's files where a browser can load them, and the browsers told
/// where weft is.
fn extension() -> Result<i32, String> {
    let (dir, browsers) = browser::install()?;
    let shown = dir.display();
    if browsers.is_empty() {
        println!("weft: the extension is in {shown}, but no Brave, Chrome, Chromium, Edge or Vivaldi profile was found to connect it to");
        return Ok(1);
    }
    println!("weft marks: connected to {}.
", browsers.join(", "));
    println!("Load it once (after a new weft, run this again and press the extension's reload arrow):");
    println!("  1. open brave://extensions (chrome://extensions in Chrome)");
    println!("  2. switch on \"Developer mode\" (top right)");
    println!("  3. \"Load unpacked\" and choose  {shown}\n");
    println!("Then on any page: Alt+Shift+W (or the weft button), click what is wrong (drag for an area),");
    println!("say what you want, ctrl+enter. The card lands on the board whose folder the page's server runs in.");
    println!("Select text first to mark it; drop a .md file on the browser to view and mark selections.");
    Ok(0)
}

// ------------------------------------------------------------------- open

async fn open(args: &[String]) -> Result<(), String> {
    let show = if args.iter().any(|a| a == "--no-open") { launch::Show::Nothing } else { launch::Show::Tab };
    let (home, at) = board_for(dir_from(args)?);
    open_dir(home, at, show).await
}

/// The board a folder belongs to (see `boards::owner`) and, when that is not the folder itself,
/// the folder, for its new agents to start in.
fn board_for(dir: PathBuf) -> (PathBuf, Option<PathBuf>) {
    match boards::owner(&dir) {
        Some(home) if home != dir => (home, Some(dir)),
        _ => (dir, None),
    }
}

async fn open_dir(dir: PathBuf, at: Option<PathBuf>, show: launch::Show) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let info = boards::ensure(&dir, &exe, true).await?;
    let url = boards::url(&info, at.as_deref());
    match &at {
        Some(a) => println!(
            "weft  {}\n      new agents start in {}  (`weft new` makes it a board of its own)\n      {url}",
            dir.display(),
            a.display()
        ),
        None => println!("weft  {}\n      {url}", dir.display()),
    }
    if show != launch::Show::Nothing {
        launch::remember(&dir);
    }
    launch::show(&url, show);
    Ok(())
}

// ------------------------------------------------------------------ serve

/// The access token: from the system's randomness, or no daemon at all (never a guessable one).
fn random_hex(n: usize) -> Result<String, String> {
    let mut b = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| format!("cannot read /dev/urandom for the access token: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

async fn serve(args: &[String]) -> Result<(), String> {
    let dir = dir_from(args)?;
    let port: u16 = args.iter().position(|a| a == "--port").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok()).unwrap_or(0);

    if let Some(i) = client::find_running(&dir).filter(|i| i.dir == dir) {
        return Err(format!("already running for {} (pid {}, port {})", dir.display(), i.pid, i.port));
    }
    // one daemon per board, even when two `weft` replace the old one at once (an install noticed by
    // the daemon itself and an agent's `weft open`): held until this process ends. A daemon that was
    // just told to quit still holds it for a moment (and its port): that is waited for.
    let _ = std::fs::create_dir_all(model::state_dir());
    let lock = std::fs::File::create(model::state_dir().join(format!("{}.lock", model::project_id(&dir)))).map_err(|e| e.to_string())?;
    let taken = tokio::task::spawn_blocking(move || lock.lock().map(|_| lock));
    let Ok(Ok(Ok(_lock))) = tokio::time::timeout(Duration::from_secs(5), taken).await else {
        return Err(format!("another daemon is starting for {}", dir.display()));
    };
    let cfg = model::load_config();
    let proj = model::project_id(&dir);
    let session = format!("weft-{proj}");
    let sock = std::env::var("WEFT_SOCKET").unwrap_or_else(|_| "weft".into());
    // Agents that survived a restart still hold the old WEFT_URL / WEFT_TOKEN:
    // reuse both when we can so `weft done` keeps working inside them.
    let previous: Option<client::Info> = model::load_json(&client::info_path(&dir));
    let want_port = if port != 0 { port } else { previous.as_ref().map_or(0, |i| i.port) };
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", want_port)).await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| format!("bind: {e}"))?,
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = match previous.map(|i| i.token).filter(|t| t.len() == 32) {
        Some(t) => t,
        None => random_hex(16)?,
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<engine::Ev>();
    let tm = match tmux::connect(&sock, &session, cfg.cols, cfg.rows, tx.clone()).await {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("weft: {e}");
            None
        }
    };
    let boot = engine::Boot {
        config_file: Some(model::config_path()),
        name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into()),
        dir: dir.clone(),
        session: session.clone(),
        sock: sock.clone(),
        exe,
        url: format!("http://127.0.0.1:{port}"),
        token: token.clone(),
    };
    let mut eng = engine::Engine::new(boot, cfg, tx.clone(), tm);
    eng.adopt().await;
    let engine_task = tokio::spawn(eng.run(rx));

    let info = client::Info {
        pid: std::process::id(),
        port,
        token: token.clone(),
        dir: dir.to_string_lossy().into_owned(),
        session,
        sock,
        build: model::BUILD.to_string(),
    };
    let ipath = client::info_path(&dir);
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::create_dir_all(model::state_dir());
        let _ = std::fs::set_permissions(model::state_dir(), std::fs::Permissions::from_mode(0o700));
        model::write_atomic(&ipath, &serde_json::to_vec(&info).unwrap()).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(&ipath, std::fs::Permissions::from_mode(0o600));
    }

    let app = server::router(server::App {
        eng: tx,
        token: token.clone(),
        port,
        next: Arc::new(AtomicU64::new(1)),
        shots: dir.join(".weft").join("shots"),
    });
    println!("weft serving {} on http://127.0.0.1:{port}/#t={token}", dir.display());
    if let Some(web) = dev::init(&dir) {
        // dev: see src/dev.rs
        println!("weft: the page is read from {} and open tabs follow its edits (WEFT_WEB_DIR=off: embedded)", web.display());
    }
    if let Ok(exe) = std::env::current_exe() {
        tokio::spawn(follow_installs(exe, dir.clone()));
    }
    let srv = axum::serve(listener, app);
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|e| e.to_string())?;
    tokio::select! {
        r = srv => { if let Err(e) = r { eprintln!("weft: server: {e}"); } }
        // a moment for the farewell and the reply to `quit` to reach their sockets
        _ = engine_task => tokio::time::sleep(Duration::from_millis(150)).await,
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    Ok(())
}

/// A new build installed over the binary this daemon runs from replaces it, as typing `weft` in this
/// folder would: every board gets the upgrade, not only the one it was installed for. Not for a
/// build in `target/`, which cargo rewrites while tests run on it.
async fn follow_installs(exe: PathBuf, dir: PathBuf) {
    use std::os::unix::{fs::MetadataExt, process::CommandExt};
    if exe.components().any(|c| c.as_os_str() == "target") {
        return;
    }
    let look = |p: &PathBuf| std::fs::metadata(p).ok().map(|m| (m.ino(), m.len(), m.mtime()));
    let mut seen = look(&exe);
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let now = look(&exe);
        if now.is_none() || now == seen {
            continue;
        }
        // (one still being written does not run: asked again on the next look)
        let Ok(out) = tokio::process::Command::new(&exe).arg("--version").output().await else { continue };
        let says = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() || !says.contains("(build ") {
            continue;
        }
        seen = now;
        if says.contains(&format!("(build {})", model::BUILD)) {
            continue;
        }
        // the agent that installed it may be replacing this board itself right now
        tokio::time::sleep(Duration::from_secs(3)).await;
        eprintln!("weft: a new build was installed ({}); replacing this daemon with it", says.trim());
        if let Ok(mut child) = std::process::Command::new(&exe)
            .arg("open")
            .arg(&dir)
            .arg("--no-open")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
        {
            std::thread::spawn(move || child.wait());
        }
        return;
    }
}

// ------------------------------------------------------------------- stop

async fn stop(args: &[String]) -> Result<i32, String> {
    let dir = dir_from(args)?;
    let end = !args.iter().any(|a| a == "--keep");
    // the daemon of the folder that was asked for, wherever this command runs from
    let Some(info) = client::find_running(&dir) else {
        println!("weft: no daemon running for {}", dir.display());
        if end {
            // a daemon that died leaves its agents behind: this is the way to clear them
            let sock = std::env::var("WEFT_SOCKET").unwrap_or_else(|_| "weft".into());
            let kill = ["kill-session".to_string(), "-t".to_string(), format!("weft-{}", model::project_id(&dir))];
            if tmux::run(&sock, &kill).await.0 {
                println!("weft: ended the agents it left behind");
            }
        }
        return Ok(0);
    };
    let r = client::call_at(info.port, &info.token, json!({"op": "quit", "end": end}))
        .map_err(|e| format!("could not stop the daemon: {e}"))?;
    let n = r["agents"].as_u64().unwrap_or(0);
    match (end, n) {
        (true, 0) => println!("weft: stopped"),
        (true, n) => println!("weft: stopped, {n} agent{} ended", if n == 1 { "" } else { "s" }),
        (false, n) => {
            println!("weft: stopped; {n} agent{} keep running (weft stop in this folder ends them)", if n == 1 { "" } else { "s" })
        }
    }
    Ok(0)
}

// ------------------------------------------------------------------- done

/// What a command typed inside an agent can carry: `--node ID`, `--note TEXT`, switches and words.
#[derive(Default)]
struct AgentArgs {
    node: Option<String>,
    note: Option<String>,
    switches: Vec<String>,
    words: Vec<String>,
}

impl AgentArgs {
    fn parse(args: &[String], switches: &[&str]) -> AgentArgs {
        // (the card of an agent that went to another board has an id of its own there)
        let node = boards::moved_here().map(|m| m.id).or_else(|| std::env::var("WEFT_NODE").ok());
        let mut a = AgentArgs { node, ..AgentArgs::default() };
        let mut it = args.iter();
        while let Some(w) = it.next() {
            match w.as_str() {
                "--note" | "-n" => a.note = it.next().cloned(),
                "--node" => a.node = it.next().cloned(),
                s if switches.contains(&s) => a.switches.push(s.to_string()),
                _ => a.words.push(w.clone()),
            }
        }
        a
    }

    fn node(&self) -> Result<String, String> {
        self.node.clone().ok_or_else(|| "not inside a weft node (WEFT_NODE unset); use --node <id>".to_string())
    }

    /// `--note TEXT`, or whatever words came after the first positional one.
    fn note(&self) -> String {
        self.note.clone().unwrap_or_else(|| self.words.iter().skip(1).cloned().collect::<Vec<_>>().join(" ")).trim().to_string()
    }
}

/// `weft eta <seconds|2m|1m30s> [--note TEXT]`: an agent says how long it will take. Returns at once.
fn eta(args: &[String]) -> Result<i32, String> {
    let a = AgentArgs::parse(args, &[]);
    let when = a.words.first().ok_or("usage: weft eta <seconds|2m|1m30s> [--note \"what you are doing\"]")?;
    let node = a.node()?;
    let seconds = parse_duration(when).ok_or_else(|| format!("cannot read {when:?} as a duration (try 90, 2m or 1m30s)"))?;
    client::call(json!({"op": "eta", "node": node, "seconds": seconds, "note": a.note()}))?;
    println!("noted: {}", if seconds >= 60 { format!("{}m{:02}s", seconds / 60, seconds % 60) } else { format!("{seconds}s") });
    Ok(0)
}

/// 90, 90s, 2m, 1m30s, 1h, 1h5m
fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Ok(n) = s.parse::<u64>() {
        return Some(n);
    }
    let (mut total, mut cur, mut any) = (0u64, String::new(), false);
    for c in s.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
            continue;
        }
        let n: u64 = cur.parse().ok()?;
        cur.clear();
        total += n * match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        any = true;
    }
    if !cur.is_empty() {
        return None;
    }
    any.then_some(total)
}

/// `weft shot <image> [--note TEXT]`: an agent shows its work. Returns at once.
fn shot(args: &[String]) -> Result<i32, String> {
    let a = AgentArgs::parse(args, &[]);
    let file = a.words.first().ok_or("usage: weft shot <image.png> [--note \"what it shows\"]")?;
    let node = a.node()?;
    let path = std::path::Path::new(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
    client::call(json!({"op": "feed_file", "node": node, "path": path.to_string_lossy(), "note": a.note()}))?;
    println!("shown to the reviewer; carry on");
    Ok(0)
}

/// `weft tell <id[,id]|all> <text|->`: an agent says something to the agents it talks with (the
/// person linked them on the canvas). It reaches them at once, or as soon as they can take it.
fn tell(args: &[String]) -> Result<i32, String> {
    let usage = "usage: weft tell <id[,id...]|all> \"<message>\"  (or the message on stdin)";
    let a = AgentArgs::parse(args, &[]);
    let to = a.words.first().ok_or(usage)?;
    let node = a.node()?;
    let mut text = a.words[1..].join(" ");
    if text.is_empty() {
        text = a.note.clone().unwrap_or_default();
    }
    if text == "-" || (text.is_empty() && !std::io::stdin().is_terminal()) {
        text.clear();
        std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
    }
    if text.trim().is_empty() {
        return Err(usage.into());
    }
    let r = client::call(json!({"op": "tell", "from": node, "to": to, "text": text}))?;
    println!("{}", r["said"].as_str().filter(|s| !s.is_empty()).unwrap_or("told"));
    Ok(0)
}

/// `weft retry <node> ["what was fixed"]`: a card that failed runs again with the input it had, and
/// what it hands on goes down its links as the first time. Returns at once.
fn retry(args: &[String]) -> Result<i32, String> {
    let usage = "usage: weft retry <node> [\"<what was wrong and what you changed>\"]  (- reads it from stdin)";
    let a = AgentArgs::parse(args, &[]);
    let name = a.words.first().ok_or(usage)?;
    let mut text = a.note();
    if text == "-" {
        text.clear();
        std::io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
    }
    let id = client::resolve(name)?;
    client::call(json!({"op": "retry", "id": id, "text": text.trim(), "by": a.node.clone().unwrap_or_default()}))?;
    println!("{id} runs again; it hands its result on when it is done");
    Ok(0)
}

/// How an agent ends what it was asked: finished, stuck on something only the person can supply,
/// or unable to do it.
#[derive(Clone, Copy, PartialEq)]
enum Report {
    Done,
    Issue,
    Fail,
}

/// `weft done|issue|fail [text]`: the agent reports its step; the text (or stdin) is what the next
/// step receives, or what the person is told.
fn report(args: &[String], default: Report) -> Result<i32, String> {
    let a = AgentArgs::parse(args, &["--fail", "-f", "--issue", "--ok"]);
    let kind = if a.switches.iter().any(|s| s == "--fail" || s == "-f") {
        Report::Fail
    } else if a.switches.iter().any(|s| s == "--issue") {
        Report::Issue
    } else {
        default
    };
    let node = a.node()?;
    let mut text = a.words.join(" ");
    // stdin only when the text is not given: Claude Code runs a command that has a heredoc anywhere in
    // it with a socket as stdin that never ends, and `weft fail "<why>"` waited on it for good
    if text == "-" || (text.is_empty() && !std::io::stdin().is_terminal()) {
        text.clear();
        std::io::stdin().read_to_string(&mut text).ok();
        text = text.trim().to_string();
    }
    let id = if node.starts_with('n') && node[1..].chars().all(|c| c.is_ascii_digit()) { node } else { client::resolve(&node)? };
    let r = client::call(json!({"op": "done", "id": id, "ok": kind != Report::Fail, "issue": kind == Report::Issue, "text": text}))?;
    let held = r["held"].as_u64().unwrap_or(0);
    println!(
        "{}",
        match kind {
            Report::Done if held > 0 => format!(
                "weft: your report still asks the person {} ({held}), so it waits for their answer like an ISSUE. Stop now: \
their answer will arrive in this session.",
                if held == 1 { "something" } else { "several things" }
            ),
            Report::Done => "weft: DONE reported".into(),
            Report::Fail => "weft: FAILURE reported".into(),
            Report::Issue => "weft: ISSUE reported. Stop now: the person's answer will arrive in this session.".into(),
        }
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::parse_duration;

    #[test]
    fn durations_are_read_the_way_people_write_them() {
        assert_eq!(parse_duration("90"), Some(90));
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("2m"), Some(120));
        assert_eq!(parse_duration("1m30s"), Some(90));
        assert_eq!(parse_duration("1h5m"), Some(3900));
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("12x"), None);
        assert_eq!(parse_duration(""), None);
    }
}
