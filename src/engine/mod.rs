//! The engine is a single actor. It owns the flow, one screen model per agent
//! and every client connection, so there are no locks: tmux events, API
//! commands and a 100 ms housekeeping tick are all just messages.
//!
//! Mental model (the same one the UI draws):
//!
//! * an **agent** is a long-lived terminal session with a FIFO **queue** of prompts;
//! * each queued prompt runs as a **step**; when the queue drains the agent
//!   **settles** (done or failed) and its **result** is what the links carry;
//! * a **link** fires when its source settles and its condition matches;
//! * an agent with several incoming links waits at a **junction**: *all*
//!   (`Promise.all`) or *any* (`Promise.any`), and shows **blocked** when the
//!   junction can no longer be satisfied;
//! * runs started by the flow keep the flow going; runs you start by chatting
//!   with a settled agent wait for you to hand the result over.

use crate::{
    model::*,
    term,
    tmux::{self, TEv, Tmux},
    words,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot};

mod agent;
mod checks;
mod commands;
mod flow;
mod gates;
mod git;
mod graph;
mod handover;
mod info;
mod repo;
mod run;
mod state;
#[cfg(test)]
mod tests;
mod ui;
mod util;

pub use commands::Cmd;
use commands::{Mv, SendMode};
pub use state::{Facts, GitJobKind, Status};
pub use util::now_ms;

use agent::*;
use repo::*;
use state::*;
use util::*;

pub struct Spawned {
    pub pane: String,
    pub win: String,
    pub worktree: Option<String>,
}
pub enum Ev {
    Tmux(TEv),
    TmuxUp(Tmux),
    Spawned {
        id: String,
        /// which start this answers: a node that was stopped, deleted or replaced by a new one with
        /// the same id meanwhile must not adopt a window that is not its own
        ticket: u64,
        res: Result<Spawned, String>,
    },
    /// A git job of a commit, push or PR step finished. `gen` discards results of a reset round.
    Git {
        id: String,
        gen: u64,
        job: GitJobKind,
        res: Result<String, String>,
        facts: Option<Facts>,
    },
    /// A gate's capture command or test suite finished.
    Gate {
        id: String,
        gen: u64,
        tests: bool,
        ok: bool,
        out: String,
    },
    /// Branch and uncommitted changes of the project folder.
    Proj(Value),
    /// A watch's own command finished (capture or test run).
    Check {
        node: String,
        check: String,
        kind: CheckKind,
        ok: bool,
        out: String,
        img: String,
    },
    /// The facts for a handover are in: compose it and start the new agent.
    Hand {
        to: String,
        from: String,
        brief: Option<String>,
        facts: String,
    },
    /// What tmux shows in an agent's pane right now: weft's own copy of the screen is checked
    /// against it when it holds an agent busy for a long time without changing.
    Snapshot {
        id: String,
        snap: Option<Snap>,
    },
    /// What an agent's CLI says it can run.
    Models {
        agent: String,
        res: Result<Vec<(String, String)>, String>,
    },
    Conn {
        id: u64,
        tx: mpsc::Sender<String>,
    },
    Disc {
        id: u64,
    },
    Cmd {
        client: Option<u64>,
        cmd: Cmd,
        reply: Option<oneshot::Sender<Value>>,
    },
}
/// A pane as tmux shows it: the rows (with history when asked for), the cursor, whether the
/// program is on the alternate screen, and the pane's size.
#[derive(Clone, Debug)]
pub struct Snap {
    pub text: String,
    pub cursor: Option<(u16, u16)>,
    pub alt: bool,
    pub rows: u16,
    pub cols: u16,
}

impl Snap {
    /// A fresh emulated screen that shows exactly what the pane shows.
    pub(super) fn screen(&self) -> vt100::Parser {
        term::seed_screen(self.rows, self.cols, SCROLLBACK, &self.text, self.cursor, self.alt)
    }
}

/// Ask tmux what a pane shows: `history` takes what scrolled out of it too (a restart rebuilds
/// the scrollback from it; a check on a busy agent only needs the screen).
pub(super) async fn snapshot(t: &Tmux, pane: &str, history: bool) -> Option<Snap> {
    let mut args: Vec<String> = vec!["capture-pane".into(), "-e".into(), "-p".into(), "-J".into()];
    if history {
        args.extend(["-S".into(), "-".into()]);
    }
    args.extend(["-t".into(), pane.to_string()]);
    let (ok, text) = tmux::run(&t.sock, &args).await;
    if !ok {
        return None;
    }
    let fmt = "#{cursor_y} #{cursor_x} #{alternate_on} #{pane_height} #{pane_width}";
    let (ok, meta) = tmux::run(&t.sock, &["display-message".into(), "-p".into(), "-t".into(), pane.to_string(), fmt.into()]).await;
    let m: Vec<u16> = meta.split_whitespace().filter_map(|v| v.parse().ok()).collect();
    if !ok || m.len() < 5 || m[3] < 2 || m[4] < 2 {
        return None;
    }
    Some(Snap { text, cursor: Some((m[0], m[1])), alt: m[2] == 1, rows: m[3], cols: m[4] })
}

pub struct Boot {
    /// where the user's settings are written back (`None`: nowhere, as in tests)
    pub config_file: Option<PathBuf>,
    pub dir: PathBuf,
    pub name: String,
    pub session: String,
    pub sock: String,
    pub exe: String,
    pub url: String,
    pub token: String,
}
pub struct Engine {
    boot: Boot,
    flow: Flow,
    cfg: Config,
    flow_path: PathBuf,
    state_path: PathBuf,
    rt: HashMap<String, Rt>,
    panes: HashMap<String, String>,
    wins: HashMap<String, String>,
    orphans: HashMap<String, Vec<u8>>,
    clients: HashMap<u64, Client>,
    tmux: Option<Tmux>,
    tx: mpsc::UnboundedSender<Ev>,
    next_q: u64,
    state_dirty: bool,
    /// set once the daemon was asked to leave; the loop ends when no start-up is in flight
    exit: Option<Exit>,
    /// windows being created right now: a window must not appear after the session is gone
    spawns: usize,
    /// the last number handed to a start (see `Ev::Spawned`)
    spawn_seq: u64,
    /// branch / dirty files of the project folder, refreshed on demand
    proj: Value,
    /// a reconnect loop for tmux is running
    tmux_retrying: bool,
    /// the second the watches were last looked at
    last_check_sec: u64,
    /// models each agent's CLI reported, and which lookups are still running
    models: HashMap<String, Vec<(String, String)>>,
    models_busy: std::collections::HashSet<String>,
    /// one git job at a time per folder: two commit steps merging into the same checkout would
    /// fight over its index
    git_locks: HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>,
    /// when the state file was last written
    last_save: Instant,
}

/// How the daemon was asked to leave: the agents carry on in tmux (the next `weft` adopts them),
/// or every agent ends with it and no terminal is left behind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Keep,
    End,
}

/// The actor's heartbeat while something is happening, and its safety tick at rest.
const TICK: Duration = Duration::from_millis(100);
const TICK_AT_REST: Duration = Duration::from_secs(1);
/// Changed state is written at most this often (and always when the daemon stops).
const SAVE_EVERY: Duration = Duration::from_millis(500);

impl Engine {
    pub fn new(boot: Boot, cfg: Config, tx: mpsc::UnboundedSender<Ev>, tmux: Option<Tmux>) -> Engine {
        let flow_path = boot.dir.join(".weft").join("flow.json");
        let state_path = boot.dir.join(".weft").join("state.json");
        let flow: Flow = load_json(&flow_path).unwrap_or_default();
        let mut e = Engine {
            boot,
            flow,
            cfg,
            flow_path,
            state_path,
            rt: HashMap::new(),
            panes: HashMap::new(),
            wins: HashMap::new(),
            orphans: HashMap::new(),
            clients: HashMap::new(),
            tmux,
            tx,
            next_q: 1,
            state_dirty: false,
            exit: None,
            spawns: 0,
            spawn_seq: 0,
            proj: Value::Null,
            tmux_retrying: false,
            last_check_sec: 0,
            models: HashMap::new(),
            models_busy: std::collections::HashSet::new(),
            git_locks: HashMap::new(),
            last_save: Instant::now(),
        };
        let mut saved: HashMap<String, Saved> = load_json(&e.state_path).unwrap_or_default();
        let mut max_q = 0;
        for n in &e.flow.nodes {
            let mut rt = Rt::new(e.cfg.cols, e.cfg.rows);
            if let Some(s) = saved.remove(&n.id) {
                max_q = max_q.max(s.queue.iter().map(|q| q.id).max().unwrap_or(0));
                rt.restore(s);
            }
            e.rt.insert(n.id.clone(), rt);
        }
        e.next_q = max_q + 1;
        let notes: Vec<String> = e.flow.nodes.iter().filter(|n| n.kind() == Kind::Note).map(|n| n.id.clone()).collect();
        for id in notes {
            e.refresh_note(&id);
        }
        // a queued prompt whose agent no longer matches the link it hangs on is put right before
        // anything runs (the stored agent is a copy: see `reconcile_hosts`)
        if e.reconcile_hosts() > 0 {
            e.flow_changed();
        }
        e
    }

    /// Re-attach to agent windows that survived a daemon restart, and go on with the steps they
    /// were answering.
    pub async fn adopt(&mut self) {
        self.adopt_windows().await;
        self.resume_steps();
    }

    async fn adopt_windows(&mut self) {
        let Some(t) = self.tmux.clone() else { return };
        let (ok, out) = tmux::run(
            &t.sock,
            &[
                "list-panes".into(),
                "-s".into(),
                "-t".into(),
                t.session.clone(),
                "-F".into(),
                "#{window_name} #{pane_id} #{window_id} #{pane_dead} #{pane_dead_status}".into(),
            ],
        )
        .await;
        if !ok {
            return;
        }
        for line in out.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            if f.len() < 5 || !self.rt.contains_key(f[0]) {
                continue;
            }
            let (id, pane, win, dead) = (f[0].to_string(), f[1].to_string(), f[2].to_string(), f[3] == "1");
            let snap = snapshot(&t, &pane, true).await;
            let Some(rt) = self.rt.get_mut(&id) else { return };
            if let Some(s) = snap {
                rt.parser = s.screen();
            }
            rt.pane = Some(pane.clone());
            rt.win = Some(win.clone());
            rt.alive = !dead;
            rt.dirty = true;
            rt.last_change = Instant::now();
            if !rt.finished {
                rt.status = if dead { Status::Exited } else { Status::Starting };
            }
            self.panes.insert(pane, id.clone());
            self.wins.insert(win, id);
        }
    }

    /// After a restart: an agent that came back goes on with the step it was answering, read from
    /// its rebuilt screen; one that did not come back answers nothing. A daemon of an older build
    /// kept no record of the step: a busy step that names the agent is the one it answers (else its
    /// answer would count as its own result and start that step all over again).
    pub(super) fn resume_steps(&mut self) {
        let ids: Vec<String> = self.rt.keys().cloned().collect();
        for id in ids {
            let Some(r) = self.rt.get(&id) else { continue };
            if !r.alive {
                let r = self.rt.get_mut(&id).unwrap();
                r.cur = None;
                r.reply_to = None;
                continue;
            }
            if r.cur.is_none() && r.reply_to.is_none() && r.armed && !r.finished && r.queue.is_empty() {
                let asked = r.asked.clone();
                let step = self.flow.nodes.iter().map(|n| n.id.clone()).find(|s| {
                    self.node(s).is_some_and(|n| n.kind() == Kind::Step && n.host == id)
                        && self.rt.get(s).is_some_and(|x| !x.finished && x.runs > 0)
                });
                if let Some(step) = step {
                    let expect_signal = asked.contains(&format!("{} done", self.boot.exe));
                    let rule = !expect_signal && self.cfg.chain_status && self.step_is_consumed(&id, &Src::Reply(step.clone()));
                    if let Some(s) = self.rt.get_mut(&step) {
                        s.step_wait = Some(id.clone());
                    }
                    let r = self.rt.get_mut(&id).unwrap();
                    r.reply_to = Some(step);
                    // (an agent that was briefed before this step was given the short rule)
                    let extra = if rule { words::status_rule(false) } else { String::new() };
                    r.cur = Some(Step { expect_signal, rule, extra, ..Step::new(asked, Vec::new()) });
                }
            }
            let r = self.rt.get_mut(&id).unwrap();
            if r.cur.is_some() {
                let lines = term::lines(&mut r.parser);
                if let Some(c) = r.cur.as_mut() {
                    c.rebase(lines);
                }
            }
        }
    }

    pub async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Ev>) {
        // runtime files (prompt files, screenshots, state) stay out of `git add .`
        ensure_weft_gitignore(&self.boot.dir);
        // the first thing the UI shows is where you are
        self.refresh_project();
        // a takeover that was waiting for its note when the daemon stopped goes on with what is on disk
        let pending: Vec<String> =
            self.rt.iter().filter(|(_, r)| r.hand.from.is_some() && r.hand.phase == HandPhase::None).map(|(i, _)| i.clone()).collect();
        for id in pending {
            self.takeover_compose(&id, None);
        }
        let timer = |every: Duration| {
            let mut t = tokio::time::interval(every);
            t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            t
        };
        let (mut fast, mut slow) = (timer(TICK), timer(TICK_AT_REST));
        loop {
            // what is happening decides the heartbeat: events wake the actor by themselves, so a
            // canvas at rest only gets the slow safety tick and costs next to nothing
            let moving = self.in_motion();
            tokio::select! {
                ev = rx.recv() => match ev {
                    Some(ev) => self.guarded(|e| e.handle(ev)),
                    None => break,
                },
                _ = fast.tick(), if moving => self.guarded(Engine::tick),
                _ = slow.tick(), if !moving => self.guarded(Engine::tick),
            }
            if self.exit.is_some() && self.spawns == 0 {
                break;
            }
        }
        self.save_state();
        if self.exit == Some(Exit::End) {
            // one command ends every window of the project (and tmux itself when it was the last session)
            let kill = ["kill-session".to_string(), "-t".to_string(), self.boot.session.clone()];
            tmux::run(&self.boot.sock, &kill).await;
        }
    }

    /// Is anything running, waiting for a timer or not yet shown to the browsers?
    /// A finished agent with nothing queued and a still screen is at rest.
    fn in_motion(&self) -> bool {
        self.state_dirty
            || self.rt.values().any(|r| {
                r.ui || r.scr
                    || r.dirty
                    || r.typing.is_some()
                    || r.spawning
                    || r.cur.is_some()
                    || !r.queue.is_empty()
                    || r.bg
                    || r.git.busy
                    || r.gate.phase != GatePhase::Idle
                    || r.hand.phase != HandPhase::None
                    || (r.alive && !r.finished && (r.armed || matches!(r.status, Status::New | Status::Starting)))
            })
    }

    /// A bug in one handler must not take the daemon, and with it the whole canvas, down: the panic
    /// is logged (the daemon's log has the message), shown once, and the actor goes on.
    fn guarded(&mut self, f: impl FnOnce(&mut Engine)) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self))).is_err() {
            self.toast("err", "Something went wrong inside weft; it is in the daemon's log. The canvas carries on.");
        }
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Tmux(t) => self.on_tmux(t),
            Ev::TmuxUp(t) => {
                self.tmux = Some(t);
                self.tmux_retrying = false;
                self.toast("info", "tmux is available again");
                // prompts that were waiting for it can go now
                let waiting: Vec<String> = self.rt.keys().cloned().collect();
                for id in waiting {
                    self.restart_if_queued(&id);
                }
            }
            Ev::Spawned { id, ticket, res } => {
                self.spawns = self.spawns.saturating_sub(1);
                self.on_spawned(id, ticket, res)
            }
            Ev::Git { id, gen, job, res, facts } => self.on_git(id, gen, job, res, facts),
            Ev::Gate { id, gen, tests, ok, out } => self.on_gate(id, gen, tests, ok, out),
            Ev::Snapshot { id, snap } => self.on_snapshot(id, snap),
            Ev::Proj(v) => {
                self.proj = v.clone();
                self.bcast(&json!({"t": "proj", "git": v}));
            }
            Ev::Hand { to, from, brief, facts } => self.on_hand(&to, &from, brief, facts),
            Ev::Check { node, check, kind, ok, out, img } => self.on_check(&node, &check, kind, ok, &out, &img),
            Ev::Models { agent, res } => {
                self.models_busy.remove(&agent);
                let (list, err) = match res {
                    Ok(l) => (l, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                self.models.insert(agent.clone(), list.clone());
                let items: Vec<Value> = list.iter().map(|(id, label)| json!({"id": id, "label": label})).collect();
                self.bcast(&json!({"t": "models", "agent": agent, "models": items, "err": err}));
            }
            Ev::Conn { id, tx } => {
                self.clients.insert(id, Client { tx, watch: None });
                let st = self.full_state();
                self.send_to(id, &st);
            }
            Ev::Disc { id } => {
                self.clients.remove(&id);
            }
            Ev::Cmd { client, cmd, reply } => {
                let r = self.cmd(client, cmd);
                if let Some(rp) = reply {
                    let _ = rp.send(r);
                }
            }
        }
    }

    fn on_tmux(&mut self, t: TEv) {
        match t {
            TEv::Output { pane, data } => {
                if let Some(id) = self.panes.get(&pane) {
                    if let Some(rt) = self.rt.get_mut(id) {
                        rt.parser.process(&data);
                        rt.dirty = true;
                        rt.painted = true;
                    }
                } else {
                    if self.orphans.len() > 64 {
                        self.orphans.clear();
                    }
                    let b = self.orphans.entry(pane).or_default();
                    if b.len() < 256 * 1024 {
                        b.extend_from_slice(&data);
                    }
                }
            }
            TEv::Dead { pane, status } => {
                let Some(id) = self.panes.get(&pane).cloned() else { return };
                let Some(rt) = self.rt.get_mut(&id) else { return };
                rt.alive = false;
                rt.ui = true;
                let (armed, finished) = (rt.armed, rt.finished);
                if armed && !finished {
                    self.step_done(&id, status == 0, None, How::Exit);
                } else if !finished {
                    self.set_status(&id, Status::Exited);
                }
                self.restart_if_queued(&id);
            }
            TEv::WinClosed { win } => {
                let Some(id) = self.wins.remove(&win) else { return };
                if let Some(rt) = self.rt.get_mut(&id) {
                    if let Some(p) = rt.pane.take() {
                        self.panes.remove(&p);
                    }
                    rt.win = None;
                    rt.alive = false;
                    rt.spawning = false;
                    rt.cur = None;
                    let fin = rt.finished;
                    rt.ui = true;
                    if !fin {
                        self.set_status(&id, Status::Stopped);
                    }
                }
            }
            TEv::Layout { win, cols, rows } => {
                if let Some(id) = self.wins.get(&win).cloned() {
                    if let Some(rt) = self.rt.get_mut(&id) {
                        if rt.parser.screen().size() != (rows, cols) && cols > 1 && rows > 1 {
                            rt.parser.screen_mut().set_size(rows, cols);
                            rt.dirty = true;
                            rt.painted = true;
                            rt.scr = true;
                        }
                    }
                }
            }
            TEv::Gone => {
                self.tmux = None;
                let ids: Vec<String> = self.rt.iter().filter(|(_, r)| r.alive).map(|(i, _)| i.clone()).collect();
                for id in ids {
                    if let Some(rt) = self.rt.get_mut(&id) {
                        rt.alive = false;
                        rt.cur = None;
                    }
                    self.set_status(&id, Status::Stopped);
                }
                self.toast("warn", "tmux connection lost - reconnecting");
                self.tmux_reconnect();
            }
        }
    }

    /// Keep trying to reach tmux in the background until it answers (once at a time).
    fn tmux_reconnect(&mut self) {
        if self.tmux_retrying {
            return;
        }
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        self.tmux_retrying = true;
        let (sock, session, c, r, tx) = (self.boot.sock.clone(), self.boot.session.clone(), self.cfg.cols, self.cfg.rows, self.tx.clone());
        rt.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Ok(t) = tmux::connect(&sock, &session, c, r, tx.clone()).await {
                    let _ = tx.send(Ev::TmuxUp(t));
                    break;
                }
            }
        });
    }

    fn tick(&mut self) {
        self.tick_checks();
        let now = Instant::now();
        let alive: Vec<String> = self.rt.iter().filter(|(_, r)| r.alive).map(|(i, _)| i.clone()).collect();
        for id in alive {
            self.eval(&id, now);
        }
        self.flush_ui();
        // state is written when it changed, at most twice a second
        if self.state_dirty && self.last_save.elapsed() >= SAVE_EVERY {
            self.save_state();
        }
    }
}
