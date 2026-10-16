//! Boards. Every folder weft was started in is a board: a daemon of its own (port, token, tmux
//! session) and its cards in `<folder>/.weft/`. Its agents may work in other folders too. A board
//! tells the others what it is doing through a small file beside its daemon record
//! (`<id>.live.json`), so the page of any board lists them all, with what needs the person there,
//! and no daemon ever calls another.

use crate::client::{self, Info};
use crate::model::{self, home, load_json, project_id, state_dir, write_atomic};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

/// What a board is doing, as the other boards see it.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
pub struct Live {
    /// its name: the folder's, unless it was given another
    #[serde(default)]
    pub name: String,
    /// cards on the canvas (notes left out)
    #[serde(default)]
    pub cards: usize,
    /// agents whose terminal runs, those of them at work, cards that wait for the person
    #[serde(default)]
    pub agents: usize,
    #[serde(default)]
    pub working: usize,
    #[serde(default)]
    pub needs: usize,
    /// the other folders its agents work in: `weft` started in one of them opens this board
    #[serde(default)]
    pub dirs: Vec<PathBuf>,
    /// when that last changed (seconds since the epoch)
    #[serde(default)]
    pub at: u64,
}

impl Live {
    /// The same picture, whenever it was taken.
    pub fn same(&self, o: &Live) -> bool {
        Live { at: 0, ..self.clone() } == Live { at: 0, ..o.clone() }
    }
}

/// (`.live.json`, not `.live`: `write_atomic` writes `<id>.tmp` first, and so does the daemon record)
pub fn live_path(dir: &Path) -> PathBuf {
    state_dir().join(format!("{}.live.json", project_id(dir)))
}

pub fn write_live(dir: &Path, live: &Live) {
    if let Ok(b) = serde_json::to_vec(live) {
        let _ = write_atomic(&live_path(dir), &b);
    }
}

/// One board as the list shows it.
#[derive(Clone, Debug)]
pub struct Board {
    pub dir: PathBuf,
    /// its daemon runs
    pub up: bool,
    pub live: Live,
}

impl Board {
    pub fn name(&self) -> String {
        if !self.live.name.trim().is_empty() {
            return self.live.name.clone();
        }
        self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.dir.display().to_string())
    }

    pub fn json(&self, me: &Path) -> Value {
        let l = &self.live;
        json!({"dir": self.dir, "name": self.name(), "me": self.dir == me, "up": self.up, "cards": l.cards,
               "agents": if self.up { l.agents } else { 0 }, "working": if self.up { l.working } else { 0 },
               "needs": if self.up { l.needs } else { 0 }, "at": l.at})
    }
}

/// Its daemon runs: the pid is alive and is a weft serving (after a reboot an old record's pid may
/// belong to anything; a daemon that quit but was not waited for yet is a zombie with no command line).
fn serving(pid: u32) -> bool {
    if !client::pid_alive(pid) {
        return false;
    }
    match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(c) => c.split(|b| *b == 0).any(|a| a == b"serve"),
        // no /proc (macOS): the pid is all there is to go by
        Err(_) => !Path::new("/proc/self").exists(),
    }
}

/// What a board that has not written its live file yet (one from before boards) shows: its cards.
fn from_flow(dir: &Path) -> Live {
    let flow: Option<Value> = load_json(&dir.join(".weft").join("flow.json"));
    let nodes = flow.as_ref().and_then(|f| f["nodes"].as_array().cloned()).unwrap_or_default();
    let cards = nodes.iter().filter(|n| n["agent"] != "note").count();
    let name = flow.as_ref().and_then(|f| f["name"].as_str()).unwrap_or("").to_string();
    Live { name, cards, ..Live::default() }
}

/// Every board there is, the ones opened last first: the folders whose daemon runs, or that have
/// cards to come back to. `recent` is the order `launch::recent` keeps.
pub fn all_in(states: &Path, recent: &[PathBuf]) -> Vec<Board> {
    let mut found: Vec<(std::time::SystemTime, Board)> = Vec::new();
    for e in std::fs::read_dir(states).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") || name.ends_with(".live.json") {
            continue;
        }
        let Some(info) = load_json::<Info>(&e.path()) else { continue };
        let dir = PathBuf::from(&info.dir);
        if !dir.is_dir() || found.iter().any(|(_, b)| b.dir == dir) {
            continue;
        }
        let up = serving(info.pid);
        if !up && !dir.join(".weft").join("flow.json").is_file() {
            continue;
        }
        let live = load_json::<Live>(&states.join(format!("{}.live.json", project_id(&dir)))).unwrap_or_else(|| from_flow(&dir));
        let at = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        found.push((at, Board { dir, up, live }));
    }
    // opened last first, then the rest by when their daemon last started
    found.sort_by(|a, b| {
        let rank = |d: &Path| recent.iter().position(|r| r == d).unwrap_or(usize::MAX);
        rank(&a.1.dir).cmp(&rank(&b.1.dir)).then(b.0.cmp(&a.0))
    });
    found.into_iter().map(|(_, b)| b).collect()
}

pub fn all() -> Vec<Board> {
    all_in(&state_dir(), &crate::launch::recent())
}

/// The list a page shows: this board first, then the others, opened last first.
pub fn listing(me: &Path) -> Vec<Value> {
    let mut list = all();
    if let Some(i) = list.iter().position(|b| b.dir == me) {
        let b = list.remove(i);
        list.insert(0, b);
    }
    list.iter().take(12).map(|b| b.json(me)).collect()
}

/// The board `weft` started in `dir` opens: the folder's own, else a board whose agents work in it,
/// else the nearest board above it (never one in the home folder or at the root: everything is
/// under those). `None`: a new board.
pub fn owner_in(boards: &[Board], dir: &Path, home: &Path) -> Option<PathBuf> {
    if boards.iter().any(|b| b.dir == dir) {
        return Some(dir.to_path_buf());
    }
    let works_there = |b: &&Board| b.live.dirs.iter().any(|d| dir.starts_with(d));
    // (a board that runs knows where its agents are now; `boards` is in the order they were opened)
    if let Some(b) = boards.iter().filter(works_there).find(|b| b.up).or_else(|| boards.iter().find(works_there)) {
        return Some(b.dir.clone());
    }
    boards
        .iter()
        .filter(|b| b.dir != home && b.dir.parent().is_some() && dir.starts_with(&b.dir))
        .max_by_key(|b| b.dir.components().count())
        .map(|b| b.dir.clone())
}

pub fn owner(dir: &Path) -> Option<PathBuf> {
    owner_in(&all(), dir, &home())
}

/// The address that opens a board's canvas; `at` = the folder its new agents should start in.
pub fn url(i: &Info, at: Option<&Path>) -> String {
    let mut u = format!("http://127.0.0.1:{}/#t={}", i.port, i.token);
    if let Some(a) = at {
        u += "&at=";
        for b in a.to_string_lossy().bytes() {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                u.push(b as char);
            } else {
                u += &format!("%{b:02X}");
            }
        }
    }
    u
}

/// The daemon of the board in `dir`, started when none runs. `replace`: a daemon of another build
/// is replaced by `exe` (what `weft` in a terminal does after an install; agents keep running,
/// port and token stay). A board switching to another leaves it as it is.
pub async fn ensure(dir: &Path, exe: &Path, replace: bool) -> Result<Info, String> {
    let running = client::find_running(dir).filter(|i| Path::new(&i.dir) == dir);
    match running {
        Some(i) if i.build == model::BUILD || !replace => return Ok(i),
        Some(old) => {
            let was = if old.build.is_empty() { "an older build".to_string() } else { format!("build {}", old.build) };
            eprintln!("weft: this folder's daemon is {was}; restarting it with this one. Your agents keep running.");
            // (its pages come back to the new daemon; a daemon from before that knew no `replace`
            // says goodbye to them instead)
            let _ = client::call_at(old.port, &old.token, json!({"op": "quit", "replace": true}));
            for _ in 0..80 {
                if !client::pid_alive(old.pid) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        None => {}
    }
    let log = state_dir().join(format!("{}.log", project_id(dir)));
    std::fs::create_dir_all(state_dir()).ok();
    let logf = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(exe)
        .arg("serve")
        .arg(dir)
        .arg("--no-open")
        // (a daemon started by another one is the board's own, not the other's agent)
        .env_remove("WEFT_URL")
        .env_remove("WEFT_TOKEN")
        .env_remove("WEFT_PROJECT")
        .env_remove("WEFT_NODE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(logf.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(logf))
        .process_group(0)
        .spawn()
        .map_err(|e| format!("could not start daemon: {e}"))?;
    // a board woken by another board's daemon is its child: once it quits it must be waited for,
    // or it stays behind as a zombie whose pid still looks alive
    std::thread::spawn(move || child.wait());
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(80)).await;
        if let Some(i) = client::find_running(dir).filter(|i| Path::new(&i.dir) == dir) {
            return Ok(i);
        }
    }
    Err(format!("daemon did not come up - see {}", log.display()))
}

/// Leave a board that does not run off the list: its record and log go, its cards stay in its folder.
pub fn forget(dir: &Path) -> Result<(), String> {
    if client::find_running(dir).is_some_and(|i| Path::new(&i.dir) == dir) {
        return Err("it is running: quit it first".into());
    }
    let id = project_id(dir);
    for ext in ["json", "live.json", "log"] {
        let _ = std::fs::remove_file(state_dir().join(format!("{id}.{ext}")));
    }
    crate::launch::forget(dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(dir: &str, up: bool, dirs: &[&str]) -> Board {
        Board { dir: dir.into(), up, live: Live { dirs: dirs.iter().map(PathBuf::from).collect(), ..Live::default() } }
    }

    #[test]
    fn a_folder_opens_its_own_board_then_the_one_working_there_then_the_one_above() {
        let home = Path::new("/h");
        let bs = vec![
            board("/h/rm", false, &[]),
            board("/h/shop", true, &["/h/rm/crawler"]),
            board("/h/rm/site", false, &[]),
            board("/h", true, &[]),
        ];
        assert_eq!(owner_in(&bs, Path::new("/h/rm/site"), home), Some("/h/rm/site".into()), "its own board");
        assert_eq!(owner_in(&bs, Path::new("/h/rm/crawler/src"), home), Some("/h/shop".into()), "agents of another board work there");
        assert_eq!(owner_in(&bs, Path::new("/h/rm/other"), home), Some("/h/rm".into()), "the board above it");
        assert_eq!(owner_in(&bs, Path::new("/h/rm/site/deep"), home), Some("/h/rm/site".into()), "the nearest one above");
        assert_eq!(owner_in(&bs, Path::new("/h/new"), home), None, "a board in the home folder takes nothing in");
        assert_eq!(owner_in(&bs, Path::new("/h"), home), Some("/h".into()), "but it is still its own folder's");
        assert_eq!(owner_in(&bs, Path::new("/h/rmx"), home), None, "a name that merely begins the same is not inside");
    }

    #[test]
    fn a_running_board_wins_where_two_have_agents() {
        let bs = vec![board("/w/a", false, &["/w/x"]), board("/w/b", true, &["/w/x"])];
        assert_eq!(owner_in(&bs, Path::new("/w/x"), Path::new("/h")), Some("/w/b".into()));
    }

    #[test]
    fn the_list_has_running_boards_and_boards_with_cards_opened_last_first() {
        let tmp = std::env::temp_dir().join(format!("weft-boards-{}", std::process::id()));
        let states = tmp.join("state");
        std::fs::create_dir_all(&states).unwrap();
        let mk = |name: &str, pid: u32, cards: bool| {
            let dir = tmp.join(name);
            std::fs::create_dir_all(dir.join(".weft")).unwrap();
            if cards {
                std::fs::write(dir.join(".weft/flow.json"), r#"{"nodes":[{"id":"n1"},{"id":"n2","agent":"note"}],"name":"Shop"}"#).unwrap();
            }
            let info = Info { pid, port: 1, token: "t".into(), dir: dir.to_string_lossy().into(), session: "s".into(), sock: "k".into(), build: "".into() };
            std::fs::write(states.join(format!("{}.json", project_id(&dir))), serde_json::to_vec(&info).unwrap()).unwrap();
            dir
        };
        // a stand-in for a daemon: alive, and "serve" on its command line
        let mut daemon = std::process::Command::new("sh").args(["-c", "sleep 30", "serve"]).spawn().unwrap();
        let a = mk("a", daemon.id(), false);
        let b = mk("b", u32::MAX - 7, true);
        mk("c", u32::MAX - 8, false);
        let live = Live { name: "".into(), cards: 3, agents: 2, working: 1, needs: 1, dirs: vec![], at: 5 };
        std::fs::write(states.join(format!("{}.live.json", project_id(&a))), serde_json::to_vec(&live).unwrap()).unwrap();
        let list = all_in(&states, &[b.clone()]);
        let dirs: Vec<&PathBuf> = list.iter().map(|x| &x.dir).collect();
        assert_eq!(dirs, vec![&b, &a], "c neither runs nor has cards; b was opened last");
        assert!(!list[0].up && list[0].live.cards == 1 && list[0].name() == "Shop", "a board from before live files: its cards and name");
        let j = list[1].json(&a);
        assert_eq!((j["me"].as_bool(), j["up"].as_bool(), j["agents"].as_u64(), j["needs"].as_u64()), (Some(true), Some(true), Some(2), Some(1)));
        assert_eq!(list[0].json(&a)["needs"], 0, "a board that sleeps waits for nobody");
        mk("d", std::process::id(), false);
        assert!(!all_in(&states, &[]).iter().any(|x| x.dir.ends_with("d")), "a live pid that is no weft daemon is not a running board");
        let _ = daemon.kill();
        let _ = daemon.wait();
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn the_address_carries_the_folder_to_start_in() {
        let i = Info { pid: 1, port: 7, token: "ab".into(), dir: "/x".into(), session: "s".into(), sock: "k".into(), build: "".into() };
        assert_eq!(url(&i, None), "http://127.0.0.1:7/#t=ab");
        assert_eq!(url(&i, Some(Path::new("/w/my site&x"))), "http://127.0.0.1:7/#t=ab&at=/w/my%20site%26x");
    }
}
