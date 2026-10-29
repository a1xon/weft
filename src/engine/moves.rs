//! Moving cards to another board: a card put on the wrong board goes to the right one as it is. The
//! board it leaves packs it as the archive does (the card, what it knew, the links among the cards that
//! go) and hands it to the other board's daemon, which takes it in like filed work that comes back.
//! An agent at work goes on working: every board's tmux session is on the one tmux server, so its
//! window moves into the other board's session and is adopted there as after a restart. Its
//! environment still names the old board and card, so the board that has it notes where the card went
//! (`boards::note_moved`) and its `weft` commands find it there. The agent hears of the move with its
//! next prompt, never in a turn of its own (that would only make an answer that replaces its result).
//! Queued prompts go with their agent; links to cards that stay are cut. Checks, git steps and deploys
//! belong to the links they sit on: they stay. A board that does not take the cards leaves them here.

use super::*;
use crate::boards::{self as bd, Moved};

/// A running agent's window on its way to another board.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Win {
    pub win: String,
    pub pane: String,
}

/// tmux tells every client whose session holds a window that the window left another session, as
/// if it had closed: for a window that just came over, that is not the agent ending.
const ARRIVAL_MS: u64 = 5_000;
/// A card that is just starting or being given a prompt goes once that is over: looked at this
/// often, this many times.
const MOVE_AGAIN: Duration = Duration::from_millis(500);
const MOVE_TRIES: u32 = 20;

impl Engine {
    /// The cards that go when `ids` move (the prompts queued on an agent go with it), the ones that
    /// stay with why, and whether one of those can go in a moment (it is starting, being given a prompt).
    fn move_set(&self, ids: &[String]) -> (Vec<String>, Vec<String>, bool) {
        let (mut take, mut stay, mut soon): (Vec<String>, Vec<String>, bool) = (Vec::new(), Vec::new(), false);
        for id in ids {
            let (Some(n), Some(rt)) = (self.node(id), self.rt.get(id)) else { continue };
            let why = match n.kind() {
                Kind::Agent if n.deploy.is_some() => Some("a deploy belongs to the push before it"),
                Kind::Agent | Kind::Note => None,
                // (it goes with its agent, or stays with it)
                Kind::Step if ids.contains(&n.host) => continue,
                Kind::Step => Some("a queued prompt goes with its agent"),
                _ => Some("checks and git steps belong to the links they sit on"),
            };
            let busy = if rt.spawning {
                Some("it is starting")
            } else if rt.typing.is_some() {
                Some("it is being given a prompt")
            } else if rt.retiring.is_some() {
                Some("it is being put to rest")
            } else {
                None
            };
            soon |= why.is_none() && busy.is_some();
            let why = why.or(busy).or((rt.hand.phase != HandPhase::None).then_some("it is handing over"));
            // (what it answers stays here, and so would the answer)
            let asker = rt.reply_to.as_ref().filter(|r| !self.node(r).is_some_and(|s| s.kind() == Kind::Step && s.host == *id));
            match (why, asker) {
                (Some(w), _) => stay.push(format!("{}: {w}", n.name)),
                (None, Some(a)) => {
                    let a = self.node(a).map_or(a.clone(), |s| s.name.clone());
                    stay.push(format!("{}: it answers {a} here; let it finish first", n.name));
                }
                (None, None) => take.push(id.clone()),
            }
        }
        let steps: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() == Kind::Step && take.contains(&n.host) && !take.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        take.extend(steps);
        (take, stay, soon)
    }

    /// Move cards to the board in `dir` (a folder that is no board yet becomes one; its daemon is
    /// woken when it sleeps). Answered once that board has them, or could not take them.
    pub(super) fn cmd_card_move(&mut self, ids: Vec<String>, dir: String, reply: Option<oneshot::Sender<Value>>) {
        let d = self.abs_dir(&dir);
        let d = d.canonicalize().unwrap_or(d);
        if d == self.boot.dir {
            return answer(reply, json!({"ok": false, "error": "that is this board"}));
        }
        if !d.is_dir() {
            return answer(reply, json!({"ok": false, "error": format!("{} is not a folder", d.display())}));
        }
        let (take, stay, soon) = self.move_set(&ids);
        if take.is_empty() && !soon {
            let e = if stay.is_empty() { "nothing to move".to_string() } else { format!("it stays: {}", stay.join("; ")) };
            return answer(reply, json!({"ok": false, "error": e}));
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return answer(reply, json!({"ok": false, "error": "not running"}));
        };
        let (exe, tx) = (PathBuf::from(&self.boot.exe), self.tx.clone());
        handle.spawn(async move {
            let to = bd::ensure(&d, &exe, false).await;
            let _ = tx.send(Ev::MoveTo { ids, dir: d, to, reply, tries: 0 });
        });
    }

    /// The board is up: the cards leave this one and are handed over.
    pub(super) fn on_move_to(
        &mut self,
        ids: Vec<String>,
        dir: PathBuf,
        to: Result<crate::client::Info, String>,
        reply: Option<oneshot::Sender<Value>>,
        tries: u32,
    ) {
        let to = match to {
            Ok(i) => i,
            Err(e) => return answer(reply, json!({"ok": false, "error": e})),
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return answer(reply, json!({"ok": false, "error": "not running"}));
        };
        // (they may have changed while that board woke up)
        let (take, stay, soon) = self.move_set(&ids);
        if soon && tries < MOVE_TRIES {
            let tx = self.tx.clone();
            handle.spawn(async move {
                tokio::time::sleep(MOVE_AGAIN).await;
                let _ = tx.send(Ev::MoveTo { ids, dir, to: Ok(to), reply, tries: tries + 1 });
            });
            return;
        }
        if take.is_empty() {
            return answer(reply, json!({"ok": false, "error": format!("it stays: {}", stay.join("; "))}));
        }
        let board = self
            .boards
            .list()
            .iter()
            .find(|b| b["dir"].as_str() == Some(&*dir.to_string_lossy()))
            .and_then(|b| b["name"].as_str().map(str::to_string))
            .unwrap_or_else(|| dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned()));
        let (pack, wins) = self.hand_over(&take);
        let ask = json!({"op": "card_take", "pack": pack, "wins": wins, "from": self.boot.dir, "board": self.board_name()});
        let tx = self.tx.clone();
        handle.spawn_blocking(move || {
            let res = crate::client::call_at(to.port, &to.token, ask);
            let _ = tx.send(Ev::Moved { pack, wins, board, stay, res, reply });
        });
    }

    /// The cards leave this board, packed as the archive packs them; the windows of the agents at work
    /// are let go, not closed.
    fn hand_over(&mut self, take: &[String]) -> (Value, HashMap<String, Win>) {
        let mut nodes: Vec<Node> = self.flow.nodes.iter().filter(|n| take.contains(&n.id)).cloned().collect();
        // a card of this board's own folder works there on the other board too
        for n in &mut nodes {
            if matches!(n.kind(), Kind::Agent | Kind::Note) {
                n.cwd = self.node_dir(n).to_string_lossy().into_owned();
            }
        }
        let edges: Vec<Edge> = self.flow.edges.iter().filter(|e| take.contains(&e.from) && take.contains(&e.to)).cloned().collect();
        let state: HashMap<String, Saved> = take.iter().filter_map(|id| Some((id.clone(), self.rt.get(id)?.to_saved()))).collect();
        let pack = serde_json::to_value(Pack { nodes, edges, state }).unwrap_or_default();
        let mut wins: HashMap<String, Win> = HashMap::new();
        for id in take {
            let Some(rt) = self.rt.get_mut(id) else { continue };
            let (Some(pane), Some(win)) = (rt.pane.clone(), rt.win.clone()) else { continue };
            if !rt.alive {
                continue;
            }
            // (dropping the card would close the window)
            rt.pane = None;
            rt.win = None;
            rt.alive = false;
            self.panes.remove(&pane);
            self.wins.remove(&win);
            wins.insert(id.clone(), Win { win, pane });
        }
        for id in take {
            self.drop_node(id, false);
        }
        self.state_dirty = true;
        self.flow_changed();
        (pack, wins)
    }

    /// The other board has the cards; or it did not take them, and they come back.
    pub(super) fn on_moved(
        &mut self,
        pack: Value,
        wins: HashMap<String, Win>,
        board: String,
        stay: Vec<String>,
        res: Result<Value, String>,
        reply: Option<oneshot::Sender<Value>>,
    ) {
        let r = match res {
            Ok(r) => {
                let names: Vec<String> = pack["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|n| n["agent"] != "step")
                    .map(|n| {
                        let (old, name) = (n["id"].as_str().unwrap_or(""), n["name"].as_str().unwrap_or(""));
                        match r["ids"][old].as_str() {
                            Some(new) if new != old => format!("{name} ({old}, {new} there)"),
                            _ => format!("{name} ({old})"),
                        }
                    })
                    .collect();
                self.toast("ok", &format!("{} went to {board}", names.join(", ")));
                json!({"ok": true, "board": board, "ids": r["ids"], "stay": stay})
            }
            Err(e) => {
                let me = (self.boot.dir.to_string_lossy().into_owned(), self.board_name());
                match serde_json::from_value::<Pack>(pack) {
                    Ok(p) => {
                        self.take_in(p, wins, &me.0, &me.1);
                    }
                    // (cannot be: it was written here a moment ago)
                    Err(_) => eprintln!("weft: the cards that did not go to {board} could not be read back"),
                }
                json!({"ok": false, "error": format!("{board} did not take them ({e}); they stay here")})
            }
        };
        answer(reply, r);
    }

    /// Cards another board hands over: they are taken in (a screenshot they showed comes along).
    pub(super) fn cmd_card_take(&mut self, pack: Value, wins: HashMap<String, Win>, from: String, board: String) -> Value {
        let Ok(pack) = serde_json::from_value::<Pack>(pack) else {
            return json!({"ok": false, "error": "these are no cards this weft can read"});
        };
        if pack.nodes.is_empty() {
            return json!({"ok": false, "error": "no cards"});
        }
        let shots = Path::new(&from).join(".weft").join("shots");
        for f in pack.state.values().flat_map(|s| &s.feed).filter(|f| !f.img.is_empty()) {
            let to = self.shots_dir().join(&f.img);
            if Path::new(&f.img).file_name() == Some(f.img.as_ref()) && !to.exists() {
                let _ = std::fs::create_dir_all(self.shots_dir());
                let _ = std::fs::copy(shots.join(&f.img), &to);
            }
        }
        let board = if board.trim().is_empty() { from.clone() } else { board };
        let ids = self.take_in(pack, wins, &from, &board);
        let names: Vec<String> = ids.values().filter_map(|id| self.node(id).map(|n| format!("{} ({id})", n.name))).collect();
        self.toast("info", &format!("{} came over from {board}", names.join(", ")));
        json!({"ok": true, "ids": ids})
    }

    /// Cards that come over from the board in `from` (or back, after a move that failed): taken in
    /// like filed work, placed where nothing is in the way, and the windows of the agents that were at
    /// work move into this board's session. Returns the ids they had and have here.
    fn take_in(&mut self, pack: Pack, wins: HashMap<String, Win>, from: &str, from_board: &str) -> HashMap<String, String> {
        let back = Path::new(from) == self.boot.dir;
        let (x0, y0) = pack.nodes.iter().fold((f64::INFINITY, f64::INFINITY), |(x, y), n| (x.min(n.x), y.min(n.y)));
        let (dx, dy) = if x0.is_finite() && y0.is_finite() {
            let (x, y) = self.free_spot(x0, y0);
            (x - x0, y - y0)
        } else {
            (0.0, 0.0)
        };
        let (order, ids) = self.bring_in(pack, dx, dy, true);
        let here = self.board_name();
        // an agent that worked is told with its next prompt where its card is now
        for (was, id) in &ids {
            let worked = self.node(id).is_some_and(Node::reads_prompts) && self.rt.get(id).is_some_and(|r| r.runs > 0);
            if let (false, true, Some(rt)) = (back, worked, self.rt.get_mut(id)) {
                rt.moved = Some(words::moved(from_board, was, &here, id));
            }
        }
        let now = now_ms();
        for (was, w) in wins {
            let Some(id) = ids.get(&was).cloned() else { continue };
            let (Some(t), Some(rt)) = (self.tmux.clone(), self.rt.get_mut(&id)) else { continue };
            if !tmux::valid_id(&w.win, '@') || !tmux::valid_pane(&w.pane) {
                continue;
            }
            // (until its window is here, nothing starts another process for it)
            rt.spawning = true;
            if !rt.finished {
                rt.status = Status::Starting;
            }
            rt.ui = true;
            if !back {
                self.arriving.insert(w.win.clone(), now);
            }
            let (tx, from, me) = (self.tx.clone(), from.to_string(), self.boot.dir.to_string_lossy().into_owned());
            tokio::spawn(async move {
                let got = come_over(&t, &w, &id, Moved { from_dir: from, from: was, dir: me, id: id.clone() }).await;
                let _ = tx.send(Ev::Arrived { id, w, got });
            });
        }
        for id in &order {
            if self.rt.get(id).is_some_and(|r| !r.spawning) {
                self.resume_step(id);
            }
        }
        ids
    }

    /// The window of an agent that came over is here: adopted as after a restart.
    pub(super) fn on_arrived(&mut self, id: String, w: Win, got: Option<(Snap, bool)>) {
        let now = now_ms();
        self.arriving.retain(|_, t| now < *t + ARRIVAL_MS);
        let waits = self.rt.get(&id).is_some_and(|r| r.spawning && r.spawn.is_none() && r.pane.is_none());
        if !waits {
            // deleted or stopped meanwhile: the window is nobody's
            if let (Some(_), Some(t)) = (&got, &self.tmux) {
                t.kill_window(&w.win);
            }
            return;
        }
        let Some(rt) = self.rt.get_mut(&id) else { return };
        rt.spawning = false;
        rt.ui = true;
        let Some((snap, dead)) = got else {
            rt.cur = None;
            rt.reply_to = None;
            if !rt.finished {
                self.set_status(&id, Status::Stopped);
            }
            return;
        };
        rt.parser = snap.screen();
        rt.pane = Some(w.pane.clone());
        rt.win = Some(w.win.clone());
        rt.alive = !dead;
        rt.dirty = true;
        rt.last_change = Instant::now();
        if !rt.finished {
            rt.status = if dead { Status::Exited } else { Status::Starting };
        }
        self.orphans.remove(&w.pane);
        self.panes.insert(w.pane, id.clone());
        self.wins.insert(w.win, id.clone());
        self.resume_step(&id);
        self.reread(&id);
        self.state_dirty = true;
    }

    /// The window just came over from another board: tmux's word that it closed is about the other session.
    pub(super) fn came_over(&mut self, win: &str) -> bool {
        self.arriving.remove(win).is_some_and(|t| now_ms() < t + ARRIVAL_MS)
    }
}

/// The answer to a request that waited (a page asks with one; another board's daemon never does).
fn answer(reply: Option<oneshot::Sender<Value>>, v: Value) {
    if let Some(r) = reply {
        let _ = r.send(v);
    }
}

async fn tmux_say(t: &Tmux, args: &[&str]) -> (bool, String) {
    tmux::run(&t.sock, &args.iter().map(|a| a.to_string()).collect::<Vec<_>>()).await
}

/// Bring a window into this board's session under the card's id, note where its agent's card is now
/// (`card`: what its environment says it is is looked up), and read what it shows.
async fn come_over(t: &Tmux, w: &Win, id: &str, card: Moved) -> Option<(Snap, bool)> {
    let (ok, session) = tmux_say(t, &["display-message", "-p", "-t", &w.win, "#{session_name}"]).await;
    if !ok {
        return None;
    }
    // (one that did not leave, after a move that failed, is here already)
    if session.trim() != t.session && !tmux_say(t, &["move-window", "-d", "-s", &w.win, "-t", &format!("{}:", t.session)]).await.0 {
        return None;
    }
    tmux_say(t, &["rename-window", "-t", &w.win, id]).await;
    let (ok, meta) = tmux_say(t, &["display-message", "-p", "-t", &w.pane, "#{pane_dead} #{socket_path},#{pid}"]).await;
    if !ok {
        return None;
    }
    let (dead, server) = meta.trim().split_once(' ')?;
    // the board and card in its environment: where it was started, if it had moved before
    let (dir, was) = (card.from_dir.clone(), card.from.clone());
    let started = bd::moved_of(server, &w.pane).filter(|m| m.dir == dir && m.id == was).map_or((dir, was), |m| (m.from_dir, m.from));
    let home = started.0 == card.dir && started.1 == card.id;
    bd::note_moved(server, &w.pane, (!home).then_some(Moved { from_dir: started.0, from: started.1, ..card }).as_ref());
    let snap = snapshot(t, &w.pane, true).await?;
    Some((snap, dead == "1"))
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    /// An agent that worked once, on a conversation weft can take up again.
    fn worked(e: &mut Engine, id: &str) {
        add(e, id, id, "build the parser");
        e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap().agent = "claude".into();
        let r = e.rt.get_mut(id).unwrap();
        r.runs = 1;
        r.finished = true;
        r.ok = Some(true);
        r.status = Status::Done;
        r.output = Some("Parser done: src/parse.rs, 4 tests.".into());
        r.session = Some("11111111-2222-4333-8444-555555555555".into());
    }

    #[test]
    fn a_card_moved_to_another_board_is_there_as_it_was_with_what_is_queued_on_it() {
        let mut a = engine();
        worked(&mut a, "n1");
        add_kind(&mut a, "n2", "then", "step", "now add tests");
        a.flow.nodes.iter_mut().find(|n| n.id == "n2").unwrap().host = "n1".into();
        link(&mut a, "e1", "n1", "n2", On::Done);
        add(&mut a, "n3", "reviewer", "review it");
        link(&mut a, "e2", "n1", "n3", On::Done);
        add_kind(&mut a, "n4", "approve", "gate", "");
        let (take, stay, _) = a.move_set(&["n1".into(), "n4".into()]);
        assert_eq!(take, vec!["n1".to_string(), "n2".to_string()], "the prompt queued on it goes along");
        assert!(stay.len() == 1 && stay[0].starts_with("approve: "), "{stay:?}");
        let (pack, wins) = a.hand_over(&take);
        assert!(wins.is_empty(), "no window: it rests");
        assert!(a.node("n1").is_none() && a.node("n2").is_none() && a.node("n3").is_some());
        assert!(a.flow.edges.is_empty(), "its link to a card that stays is cut");

        // the other board has an n1 of its own
        let mut b = engine();
        add(&mut b, "n1", "theirs", "something else");
        let r = b.cmd_card_take(pack, wins, a.boot.dir.to_string_lossy().into(), "shop".into());
        assert_eq!(r["ok"], true, "{r}");
        let id = r["ids"]["n1"].as_str().unwrap().to_string();
        let step = r["ids"]["n2"].as_str().unwrap().to_string();
        assert_ne!(id, "n1");
        let n = b.node(&id).unwrap();
        assert_eq!((n.prompt.as_str(), n.agent.as_str()), ("build the parser", "claude"));
        assert_eq!(Path::new(&n.cwd), a.boot.dir, "it works in its folder, not in the other board's");
        assert_eq!(b.node(&step).unwrap().host, id);
        assert!(b.flow.edges.iter().any(|e| e.from == id && e.to == step));
        let rt = &b.rt[&id];
        assert_eq!(rt.output.as_deref(), Some("Parser done: src/parse.rs, 4 tests."));
        assert!(rt.finished && b.resumable(&id), "its next process takes the conversation up again");
        let note = rt.moved.clone().unwrap();
        assert!(note.starts_with("[weft] Note:") && note.contains("\"shop\"") && note.contains(&format!("it is {id} there (it was n1)")), "{note}");
        assert!(b.rt[&step].moved.is_none(), "a queued prompt has no conversation to tell");

        // the next prompt it gets says so, once
        let (t, _typed) = Tmux::sink();
        b.tmux = Some(t);
        let r = b.rt.get_mut(&id).unwrap();
        r.alive = true;
        r.pane = Some("%9".into());
        b.cmd_send(id.clone(), "and a benchmark".into(), SendMode::Auto, true);
        b.tmux = None;
        let c = b.rt[&id].cur.as_ref().expect("it works on it");
        assert!(c.prompt.contains(&note) && c.prompt.contains("and a benchmark"), "{}", c.prompt);
        assert!(b.rt[&id].moved.is_none());
    }

    #[test]
    fn what_belongs_to_a_link_or_is_answered_here_stays() {
        let mut e = engine();
        worked(&mut e, "a");
        add_kind(&mut e, "s", "then", "step", "x");
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().host = "a".into();
        add(&mut e, "d", "deploy", "");
        e.flow.nodes.iter_mut().find(|n| n.id == "d").unwrap().deploy = Some(DeployAt::Pushed);
        add_kind(&mut e, "g", "approve", "gate", "");
        let (take, stay, _) = e.move_set(&["s".into(), "d".into(), "g".into()]);
        assert!(take.is_empty() && stay.len() == 3, "{stay:?}");
        // a queued prompt picked along with its agent is not counted twice
        assert_eq!(e.move_set(&["a".into(), "s".into()]).0, vec!["a".to_string(), "s".to_string()]);
        // an agent that answers a check here, or is just starting, stays
        e.rt.get_mut("a").unwrap().reply_to = Some("g".into());
        assert!(e.move_set(&["a".into()]).1[0].contains("answers approve here"));
        e.rt.get_mut("a").unwrap().reply_to = Some("s".into());
        assert_eq!(e.move_set(&["a".into()]).0.len(), 2, "the step it answers goes with it");
        e.rt.get_mut("a").unwrap().spawning = true;
        let (take, _, soon) = e.move_set(&["a".into()]);
        assert!(take.is_empty() && soon, "a card that is just starting goes in a moment");
        assert_eq!(e.cmd_card_take(json!({"nodes": []}), HashMap::new(), "/x".into(), "".into())["ok"], false);
    }

    #[test]
    fn cards_the_other_board_did_not_take_come_back() {
        let mut e = engine();
        worked(&mut e, "n1");
        let (take, _, _) = e.move_set(&["n1".into()]);
        let (pack, wins) = e.hand_over(&take);
        assert!(e.node("n1").is_none());
        e.on_moved(pack, wins, "beta".into(), Vec::new(), Err("daemon refused request (HTTP/1.1 422)".into()), None);
        let n = e.node("n1").expect("it is back, under its own id");
        assert_eq!(Path::new(&n.cwd), e.boot.dir);
        assert_eq!(e.rt["n1"].output.as_deref(), Some("Parser done: src/parse.rs, 4 tests."));
        assert!(e.rt["n1"].moved.is_none(), "it never left: nothing to tell");
    }

    #[test]
    fn a_window_that_came_over_is_not_taken_for_one_that_closed() {
        let mut e = engine();
        e.arriving.insert("@7".into(), now_ms());
        assert!(e.came_over("@7"), "tmux's word about the other session");
        assert!(!e.came_over("@7"), "only once");
        e.arriving.insert("@8".into(), now_ms() - ARRIVAL_MS - 1);
        assert!(!e.came_over("@8"), "long after it came, a close is a close");
    }
}
