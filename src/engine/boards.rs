//! The other boards (see `crate::boards`). What this board is doing goes into its live file; what
//! the others are doing reaches this board's pages, looked at every few seconds while one is open.
//! From here a board is opened (its daemon started when it sleeps), left off the list, or renamed.
//! The person's settings are shared by every board: another board may have changed them, so they
//! are read again when a page connects or comes to the front.

use super::*;
use crate::boards::{self as bd, Live};

/// How often the other boards are looked at while a page is open.
const BOARDS_EVERY: Duration = Duration::from_secs(3);
/// This board's live file is written at most this often (only when the picture changed).
const LIVE_EVERY: Duration = Duration::from_secs(1);
/// A board's name is a name, not a description.
const NAME_MAX: usize = 60;

/// The boards the pages were told about last, and this board's own last picture.
pub(super) struct Boards {
    list: Vec<Value>,
    looked: Option<Instant>,
    busy: bool,
    live: Live,
    live_at: Instant,
    /// when the settings file was last read or written (another board's write changes it)
    cfg_seen: Option<SystemTime>,
}

impl Boards {
    pub(super) fn new(config_file: Option<&Path>) -> Boards {
        let cfg_seen = config_file.and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        Boards { list: Vec::new(), looked: None, busy: false, live: Live::default(), live_at: Instant::now(), cfg_seen }
    }

    pub(super) fn list(&self) -> &[Value] {
        &self.list
    }
}

impl Engine {
    /// The board's name: the one it was given, else its folder's.
    pub(super) fn board_name(&self) -> String {
        if self.flow.name.trim().is_empty() { self.boot.name.clone() } else { self.flow.name.clone() }
    }

    /// What this board is doing, as the other boards see it (counted as its own ticker counts).
    pub(super) fn live(&self) -> Live {
        let mut l = Live { name: self.board_name(), ..Live::default() };
        let worktrees = state_dir().join("worktrees");
        for n in &self.flow.nodes {
            if n.kind() == Kind::Note {
                continue;
            }
            l.cards += 1;
            let Some(rt) = self.rt.get(&n.id) else { continue };
            match rt.status {
                Status::Busy | Status::Starting => l.working += 1,
                Status::Attention => l.needs += 1,
                _ => {}
            }
            if n.kind() == Kind::Agent {
                if rt.alive {
                    l.agents += 1;
                }
                let d = self.node_dir(n);
                if d != self.boot.dir && !d.starts_with(&worktrees) && !l.dirs.contains(&d) {
                    l.dirs.push(d);
                }
            }
        }
        l
    }

    pub(super) fn tick_boards(&mut self) {
        // (a test engine has no settings file and must not write into the person's state folder)
        if self.boot.config_file.is_none() {
            return;
        }
        if self.boards.live_at.elapsed() >= LIVE_EVERY {
            self.boards.live_at = Instant::now();
            let l = self.live();
            if !l.same(&self.boards.live) {
                let l = Live { at: now_ms() / 1000, ..l };
                bd::write_live(&self.boot.dir, &l);
                self.boards.live = l;
            }
        }
        if !self.clients.is_empty() && self.boards.looked.is_none_or(|t| t.elapsed() >= BOARDS_EVERY) {
            self.look_at_boards();
        }
    }

    /// Read every board's live file (off the actor: it is the disk).
    pub(super) fn look_at_boards(&mut self) {
        if self.boards.busy || self.boot.config_file.is_none() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        self.boards.busy = true;
        self.boards.looked = Some(Instant::now());
        let (me, tx) = (self.boot.dir.clone(), self.tx.clone());
        handle.spawn_blocking(move || {
            let _ = tx.send(Ev::Boards(bd::listing(&me)));
        });
    }

    pub(super) fn on_boards(&mut self, list: Vec<Value>) {
        self.boards.busy = false;
        if list != self.boards.list {
            self.boards.list = list;
            let m = json!({"t": "boards", "list": self.boards.list});
            self.bcast(&m);
        }
    }

    /// Open a board: its daemon is started when it sleeps (a folder that is no board yet becomes
    /// one). Answers with its address; `window`: it opens in a window of its own as well.
    /// Answered off the actor, like the log: a daemon takes a moment to come up.
    pub(super) fn cmd_board_open(&self, dir: String, window: bool, reply: Option<oneshot::Sender<Value>>) {
        let Some(reply) = reply else { return };
        let d = self.abs_dir(&dir);
        let d = d.canonicalize().unwrap_or(d);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            let _ = reply.send(json!({"ok": false, "error": "not running"}));
            return;
        };
        if !d.is_dir() {
            let _ = reply.send(json!({"ok": false, "error": format!("{} is not a folder", d.display())}));
            return;
        }
        let exe = PathBuf::from(&self.boot.exe);
        handle.spawn(async move {
            let r = match bd::ensure(&d, &exe, false).await {
                Ok(i) => {
                    let url = bd::url(&i, None);
                    let (u, d2) = (url.clone(), d.clone());
                    let _ = tokio::task::spawn_blocking(move || {
                        crate::launch::remember(&d2);
                        if window {
                            crate::launch::show(&u, crate::launch::Show::Window);
                        }
                    })
                    .await;
                    json!({"ok": true, "url": url, "dir": d})
                }
                Err(e) => json!({"ok": false, "error": e}),
            };
            let _ = reply.send(r);
        });
    }

    /// Leave a board that sleeps off the list (its cards stay in its folder).
    pub(super) fn cmd_board_forget(&mut self, dir: String) -> Value {
        let d = self.abs_dir(&dir);
        if d == self.boot.dir {
            return json!({"ok": false, "error": "this is the board you are on"});
        }
        if self.boot.config_file.is_none() {
            return json!({"ok": false, "error": "not here"});
        }
        if let Err(e) = bd::forget(&d) {
            return json!({"ok": false, "error": e});
        }
        // (a look already under way is followed by another at the next tick, never run beside it)
        self.boards.looked = None;
        self.look_at_boards();
        json!({"ok": true})
    }

    /// Make way for a daemon of another build (`boards::ensure`, after an install): the agents keep
    /// running for it to adopt, and the pages are told, so they come back to it (and reload to its
    /// page) instead of saying weft has stopped.
    pub(super) fn cmd_make_way(&mut self) -> Value {
        let agents = self.rt.values().filter(|r| r.alive || r.spawning).count();
        self.exit = Some(Exit::Keep);
        self.bcast(&json!({"t": "bye", "end": false, "replace": true, "agents": agents}));
        json!({"ok": true, "end": false, "agents": agents})
    }

    /// Give the board a name ("" = its folder's again).
    pub(super) fn cmd_board_name(&mut self, name: String) -> Value {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX).collect();
        let name = if name == self.boot.name { String::new() } else { name };
        if name == self.flow.name {
            return json!({"ok": true, "name": self.board_name()});
        }
        self.flow.name = name;
        save_json(&self.flow_path, &self.flow);
        let m = json!({"t": "board", "name": self.board_name()});
        self.bcast(&m);
        // the other boards' lists show it at their next look, this board's at its next tick
        if self.boot.config_file.is_some() {
            let l = Live { at: now_ms() / 1000, ..self.live() };
            bd::write_live(&self.boot.dir, &l);
            self.boards.live = l;
        }
        self.boards.looked = None;
        json!({"ok": true, "name": self.board_name()})
    }

    /// Settings another board changed since this one last read them: the pages here get them too.
    pub(super) fn sync_user_config(&mut self) {
        let Some(path) = self.boot.config_file.clone() else { return };
        let seen = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if seen == self.boards.cfg_seen || seen.is_none() {
            return;
        }
        self.boards.cfg_seen = seen;
        let fresh = load_config();
        if fresh.prefs != self.cfg.prefs && fresh.prefs.is_object() {
            self.cfg.prefs = fresh.prefs;
            let m = json!({"t": "prefs", "prefs": self.cfg.prefs});
            self.bcast(&m);
        }
        if serde_json::to_value(&fresh.library).ok() != serde_json::to_value(&self.cfg.library).ok() {
            self.cfg.library = fresh.library;
            let m = json!({"t": "lib", "library": self.cfg.library});
            self.bcast(&m);
        }
        if serde_json::to_value(&fresh.networks).ok() != serde_json::to_value(&self.cfg.networks).ok() {
            self.cfg.networks = fresh.networks;
            let m = json!({"t": "nets", "networks": self.cfg.networks});
            self.bcast(&m);
        }
    }
}
