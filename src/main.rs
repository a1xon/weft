//! weft - a canvas for coding agents. tmux is the only runtime.
//!
//! One small daemon per project keeps the flow graph, watches every agent
//! through tmux control mode and serves the canvas UI on loopback.

mod client;
mod dev; // dev: the page follows edits to web/ while weft's UI is worked on
mod engine;
mod model;
mod server;
mod term;
mod tmux;
mod web;
mod words;

use serde_json::json;
use std::{
    io::{IsTerminal, Read},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::Stdio,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

const USAGE: &str = "\
weft - a canvas for coding agents (tmux is the only runtime)

USAGE
  weft [dir]                 open the canvas for a project (starts the daemon)
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
        "eta" => eta(rest),
        "open" => open(rest).await.map(|_| 0),
        // `weft`, `weft <dir>`, `weft --no-open`: open the canvas
        _ => open(&args).await.map(|_| 0),
    }
}

// ------------------------------------------------------------------- open

async fn open(args: &[String]) -> Result<(), String> {
    let dir = dir_from(args)?;
    let no_open = args.iter().any(|a| a == "--no-open");
    let running = client::find_running(&dir).filter(|i| i.dir == dir);
    let info = match running {
        Some(i) if i.build == model::BUILD => i,
        other => {
            if let Some(old) = other {
                // another build serves this folder (an older install): replace it. The agents live
                // in tmux and are adopted again; port and token stay, so open tabs reconnect.
                let was = if old.build.is_empty() { "an older build".to_string() } else { format!("build {}", old.build) };
                eprintln!("weft: this folder's daemon is {was}; restarting it with this one. Your agents keep running.");
                let _ = client::call_at(old.port, &old.token, json!({"op": "quit"}));
                for _ in 0..80 {
                    if !client::pid_alive(old.pid) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let log = model::state_dir().join(format!("{}.log", model::project_id(&dir)));
            std::fs::create_dir_all(model::state_dir()).ok();
            let logf = std::fs::File::create(&log).map_err(|e| e.to_string())?;
            std::process::Command::new(exe)
                .arg("serve")
                .arg(&dir)
                .arg("--no-open")
                .stdin(Stdio::null())
                .stdout(Stdio::from(logf.try_clone().map_err(|e| e.to_string())?))
                .stderr(Stdio::from(logf))
                .process_group(0)
                .spawn()
                .map_err(|e| format!("could not start daemon: {e}"))?;
            let mut found = None;
            for _ in 0..100 {
                tokio::time::sleep(Duration::from_millis(80)).await;
                if let Some(i) = client::find_running(&dir).filter(|i| i.dir == dir) {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or_else(|| format!("daemon did not come up - see {}", log.display()))?
        }
    };
    let url = format!("http://127.0.0.1:{}/#t={}", info.port, info.token);
    println!("weft  {}\n      {url}", dir.display());
    if !no_open {
        let _ = std::process::Command::new("xdg-open").arg(&url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
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
        let mut a = AgentArgs { node: std::env::var("WEFT_NODE").ok(), ..AgentArgs::default() };
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
    if !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).ok();
        if !s.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(s.trim());
        }
    }
    let id = if node.starts_with('n') && node[1..].chars().all(|c| c.is_ascii_digit()) { node } else { client::resolve(&node)? };
    client::call(json!({"op": "done", "id": id, "ok": kind != Report::Fail, "issue": kind == Report::Issue, "text": text}))?;
    println!(
        "{}",
        match kind {
            Report::Done => "weft: DONE reported",
            Report::Fail => "weft: FAILURE reported",
            Report::Issue => "weft: ISSUE reported. Stop now: the person's answer will arrive in this session.",
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
