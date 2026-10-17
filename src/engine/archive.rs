//! The archive: finished work leaves the canvas for `.weft/archive.json`. A person drops cards on it,
//! or a flow goes there by itself once nothing in it is left to happen: a lone card `archive_s` after
//! it was done, a flow of several cards `archive_flow_s` after its last step. That clock runs only
//! while someone works on the canvas: two minutes without a sign of them and it stops until they are
//! back, and not at all while archiving is held (`Flow::hold`, the lock in the top bar). A kept card stays, and so does its flow. Filed work can come back with its results and links; its process ended when it was
//! filed (the next prompt starts a new one), its worktree is kept.

use super::*;

/// Everything filed, oldest first: what the list shows and what it holds on to (ids, worktrees).
/// The cards themselves, their links and what they knew are in one gzipped pack per drop
/// (`.weft/archive/<id>.json.gz`), read only when a drop is opened or comes back.
#[derive(Serialize, Deserialize, Default)]
pub(super) struct Archive {
    #[serde(default)]
    pub(super) groups: Vec<Filed>,
}

/// One drop into the archive: a card, a selection or a whole flow.
#[derive(Serialize, Deserialize)]
pub(super) struct Filed {
    pub(super) id: String,
    /// when it was filed (ms since the epoch)
    pub(super) at: u64,
    /// filed by itself, not by a person
    #[serde(default)]
    pub(super) auto: bool,
    #[serde(default)]
    pub(super) cards: Vec<Card>,
    /// the ids of every link that touched one of its cards
    #[serde(default)]
    pub(super) links: Vec<String>,
    /// An archive from before the packs kept all of it here: it moves into its pack on load.
    #[serde(default, skip_serializing)]
    nodes: Vec<Node>,
    #[serde(default, skip_serializing)]
    edges: Vec<Edge>,
    #[serde(default, skip_serializing)]
    state: HashMap<String, Saved>,
}

/// A filed card as the list shows it.
#[derive(Serialize, Deserialize, Clone)]
pub(super) struct Card {
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    git_op: GitOp,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    w: f64,
    #[serde(default)]
    h: f64,
    /// the folder it worked in, and its worktree
    #[serde(default, skip_serializing_if = "String::is_empty")]
    cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<String>,
    /// how it ended (None: it did not)
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    since: u64,
    /// the start of what it was asked and said
    #[serde(default)]
    asked: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    out: Option<String>,
}

/// All of a drop: its cards, every link that touched one of them, what each card knew (`Rt::to_saved`).
#[derive(Serialize, Deserialize, Default)]
pub(super) struct Pack {
    pub(super) nodes: Vec<Node>,
    #[serde(default)]
    pub(super) edges: Vec<Edge>,
    #[serde(default)]
    pub(super) state: HashMap<String, Saved>,
}

impl Filed {
    fn new(id: String, at: u64, auto: bool, pack: &Pack) -> Filed {
        let cards = pack
            .nodes
            .iter()
            .map(|n| {
                let s = pack.state.get(&n.id);
                let asked = s.map(|s| s.asked.trim()).filter(|a| !a.is_empty()).unwrap_or(n.prompt.trim());
                Card {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    agent: n.agent.clone(),
                    git_op: n.git_op,
                    x: n.x,
                    y: n.y,
                    w: n.w,
                    h: n.h,
                    cwd: n.cwd.clone(),
                    worktree: s.and_then(|s| s.worktree.clone()),
                    ok: s.filter(|s| s.finished).map(|s| s.ok.unwrap_or(true)),
                    since: s.map_or(0, |s| s.since),
                    asked: head_chars(asked, 200),
                    out: s.and_then(|s| s.output.as_deref()).map(|o| head_chars(o.trim(), 400)),
                }
            })
            .collect();
        Filed {
            id,
            at,
            auto,
            cards,
            links: pack.edges.iter().map(|e| e.id.clone()).collect(),
            nodes: Vec::new(),
            edges: Vec::new(),
            state: HashMap::new(),
        }
    }
}

impl Filed {
    /// What a drop from before the packs holds, while its pack could not be written.
    fn unpacked(&self) -> Option<Pack> {
        if self.nodes.is_empty() {
            return None;
        }
        let state = serde_json::from_value(serde_json::to_value(&self.state).ok()?).ok()?;
        Some(Pack { nodes: self.nodes.clone(), edges: self.edges.clone(), state })
    }
}

impl Archive {
    /// The archive of a folder (`path`: its index). One from before the packs is moved into them,
    /// and the index is written again only once every pack is on disk.
    pub(super) fn load(path: &Path) -> Archive {
        let mut a: Archive = load_json(path).unwrap_or_default();
        let dir = packs_dir(path);
        let mut moved = 0;
        for g in a.groups.iter_mut().filter(|g| !g.nodes.is_empty()) {
            let pack =
                Pack { nodes: std::mem::take(&mut g.nodes), edges: std::mem::take(&mut g.edges), state: std::mem::take(&mut g.state) };
            if write_pack(&dir.join(pack_name(&g.id)), &pack).is_ok() {
                *g = Filed::new(std::mem::take(&mut g.id), g.at, g.auto, &pack);
                moved += 1;
            } else {
                (g.nodes, g.edges, g.state) = (pack.nodes, pack.edges, pack.state);
            }
        }
        if moved > 0 && a.groups.iter().all(|g| g.nodes.is_empty()) {
            a.save(path);
        }
        a
    }

    /// The index is read by weft alone: no indentation.
    fn save(&self, path: &Path) {
        if let Ok(s) = serde_json::to_vec(self) {
            let _ = write_atomic(path, &s);
        }
    }
}

/// Where the packs of the archive with this index are.
fn packs_dir(index: &Path) -> PathBuf {
    index.with_extension("")
}

fn pack_name(gid: &str) -> String {
    format!("{gid}.json.gz")
}

fn write_pack(path: &Path, pack: &Pack) -> std::io::Result<()> {
    use std::io::Write;
    let json = serde_json::to_vec(pack)?;
    let mut z = flate2::write::GzEncoder::new(Vec::with_capacity(json.len() / 4), flate2::Compression::best());
    z.write_all(&json)?;
    write_atomic(path, &z.finish()?)
}

fn read_pack(path: &Path) -> Option<Pack> {
    use std::io::Read;
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).ok()?).read_to_end(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

/// The archive keeps this many drops; the oldest make room.
const KEEP_FILED: usize = 100;
/// Filed steps keep this much of each prompt and answer (the result itself is kept whole).
const FILED_STEP: usize = 4_000;
/// How often the canvas is looked over for work that is due.
const LOOK_EVERY_MS: u64 = 10_000;

/// The archive clock stops this long after the last sign of someone at the canvas.
pub(super) const IDLE_MS: u64 = 120_000;

/// When someone worked on the canvas: the spans the archive clock ran (ms since the epoch, oldest
/// first). Each sign of them keeps it running `IDLE_MS` on.
pub(super) struct Active {
    pub(super) spans: Vec<(u64, u64)>,
    /// when weft started: the clock stands there until someone comes
    born: u64,
}

impl Active {
    pub(super) fn new(now: u64) -> Self {
        Active { spans: Vec::new(), born: now }
    }

    /// Someone is at the canvas at `t`. Spans beyond the last `keep` ms of work are dropped: a card
    /// done before them is due anyway.
    pub(super) fn touch(&mut self, t: u64, keep: u64) {
        match self.spans.last_mut() {
            Some(last) if last.1 >= t => last.1 = last.1.max(t + IDLE_MS),
            _ => self.spans.push((t, t + IDLE_MS)),
        }
        let mut sum = 0;
        let mut from = 0;
        for (i, (a, b)) in self.spans.iter().enumerate().rev() {
            if sum >= keep {
                from = i + 1;
                break;
            }
            sum += b - a;
        }
        self.spans.drain(..from);
    }

    /// The clock stops now (archiving is held).
    pub(super) fn halt(&mut self, now: u64) {
        if let Some(last) = self.spans.last_mut() {
            last.1 = last.1.min(now);
        }
    }

    /// Where the clock stands: now while it runs, else the moment it stopped.
    pub(super) fn stand(&self, now: u64) -> u64 {
        self.spans.last().map_or(self.born, |s| s.1.min(now))
    }

    /// How long it ran between `from` and `to`.
    pub(super) fn ran(&self, from: u64, to: u64) -> u64 {
        self.spans.iter().map(|&(a, b)| b.min(to).saturating_sub(a.max(from))).sum()
    }
}

impl Rt {
    /// Nothing runs, starts, types or waits for an answer here: it can be filed.
    pub(super) fn at_rest(&self) -> bool {
        self.cur.is_none()
            && self.queue.is_empty()
            && !self.spawning
            && self.typing.is_none()
            && self.step_wait.is_none()
            && !self.git.busy
            && self.git.jobs.is_empty()
            && self.git.wait.is_none()
            && !matches!(self.gate.phase, GatePhase::Asking | GatePhase::Running)
            && self.hand.phase == HandPhase::None
            && !(self.alive && self.armed && !self.finished)
    }
}

/// What the archive keeps of a card's run: all of it, with its steps cut short (the result is whole).
fn filed_state(mut s: Saved) -> Saved {
    for st in &mut s.steps {
        st.prompt = head_chars(&st.prompt, FILED_STEP);
        st.text = tail_bytes(&st.text, FILED_STEP).to_string();
    }
    s
}

impl Engine {
    /// Ids in use on the canvas or in the archive (`edges`: link ids, else card ids). Filed work keeps
    /// its ids, so it comes back as it was and its links find it again.
    pub(super) fn taken_ids(&self, edges: bool) -> Vec<String> {
        let filed = self.archive.groups.iter();
        if edges {
            let legacy = filed.clone().flat_map(|g| g.edges.iter().map(|e| e.id.clone()));
            self.flow.edges.iter().map(|e| e.id.clone()).chain(filed.flat_map(|g| g.links.iter().cloned())).chain(legacy).collect()
        } else {
            let legacy = filed.clone().flat_map(|g| g.nodes.iter().map(|n| n.id.clone()));
            self.flow
                .nodes
                .iter()
                .map(|n| n.id.clone())
                .chain(filed.flat_map(|g| g.cards.iter().map(|c| c.id.clone())))
                .chain(legacy)
                .collect()
        }
    }

    fn pack_path(&self, gid: &str) -> PathBuf {
        packs_dir(&self.archive_path).join(pack_name(gid))
    }

    /// All of a drop, from its pack (or, if its pack could not be written yet, from memory).
    fn open_pack(&self, g: &Filed) -> Option<Pack> {
        if !g.nodes.is_empty() {
            return g.unpacked();
        }
        read_pack(&self.pack_path(&g.id))
    }

    /// A drop leaves the archive: its pack goes with it.
    fn take_group(&mut self, pos: usize) -> Filed {
        let g = self.archive.groups.remove(pos);
        let _ = std::fs::remove_file(self.pack_path(&g.id));
        g
    }

    /// The canvas as flows: cards tied by a link, a takeover's lineage, a queued prompt's agent or a
    /// talk belong together. Notes are context, not work: a note goes along with the one flow it feeds, and one that
    /// feeds none or several belongs to none.
    pub(super) fn flows(&self) -> Vec<Vec<String>> {
        fn root(up: &mut [usize], mut i: usize) -> usize {
            while up[i] != i {
                up[i] = up[up[i]];
                i = up[i];
            }
            i
        }
        let ids: Vec<&str> = self.flow.nodes.iter().map(|n| n.id.as_str()).collect();
        let note: Vec<bool> = self.flow.nodes.iter().map(|n| n.kind() == Kind::Note).collect();
        let at: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let mut up: Vec<usize> = (0..ids.len()).collect();
        let ties = self
            .flow
            .edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .chain(self.flow.nodes.iter().flat_map(|n| [(n.id.as_str(), n.takeover.as_str()), (n.id.as_str(), n.host.as_str())]))
            .chain(self.flow.talks.iter().map(|t| (t.a.as_str(), t.b.as_str())));
        for (a, b) in ties {
            if let (Some(&x), Some(&y)) = (at.get(a), at.get(b)) {
                if !note[x] && !note[y] {
                    let (rx, ry) = (root(&mut up, x), root(&mut up, y));
                    up[rx] = ry;
                }
            }
        }
        let mut flows: HashMap<usize, Vec<String>> = HashMap::new();
        for i in (0..ids.len()).filter(|&i| !note[i]) {
            flows.entry(root(&mut up, i)).or_default().push(ids[i].to_string());
        }
        for i in (0..ids.len()).filter(|&i| note[i]) {
            let mut fed: Vec<usize> = Vec::new();
            for e in self.flow.edges.iter().filter(|e| e.from == ids[i]) {
                if let Some(&t) = at.get(e.to.as_str()) {
                    let r = root(&mut up, t);
                    if !fed.contains(&r) {
                        fed.push(r);
                    }
                }
            }
            if let [one] = fed[..] {
                if let Some(f) = flows.get_mut(&one) {
                    f.push(ids[i].to_string());
                }
            }
        }
        flows.into_values().collect()
    }

    /// The flows that go to the archive by themselves, and when (ms since the epoch, as if the clock
    /// ran from where it stands: see `Active::stand`). A flow goes when every card in it is finished
    /// (done, or a branch whose condition did not come true), none failed, waits for a person or is
    /// kept, and it stayed so for `archive_s` (one card) or `archive_flow_s` (several) of work on the
    /// canvas after its last step. Coming back from the archive starts the clock again.
    pub(super) fn filing_plan(&self, blocked: &HashMap<String, String>) -> Vec<(Vec<String>, u64)> {
        let stand = self.active.stand(now_ms());
        let mut plan = Vec::new();
        for flow in self.flows() {
            let work: Vec<&String> = flow.iter().filter(|id| self.node(id).is_some_and(|n| n.kind() != Kind::Note)).collect();
            let wait = if work.len() == 1 { self.cfg.archive_s } else { self.cfg.archive_flow_s };
            if work.is_empty() || wait <= 0.0 || flow.iter().any(|id| self.node(id).is_some_and(|n| n.keep)) {
                continue;
            }
            let mut last = Some(0);
            for id in work {
                let Some(rt) = self.rt.get(id) else {
                    last = None;
                    break;
                };
                last = match self.eff_status(id, blocked) {
                    Status::Done if rt.at_rest() => last.map(|l: u64| l.max(rt.since).max(rt.revived)),
                    // it will not run: nothing more happens here
                    Status::Blocked => last.map(|l| l.max(rt.revived)),
                    _ => None,
                };
                if last.is_none() {
                    break;
                }
            }
            if let Some(l) = last.filter(|l| *l > 0) {
                plan.push((flow, (stand + (wait * 1000.0) as u64).saturating_sub(self.active.ran(l, stand))));
            }
        }
        plan
    }

    /// When each card goes to the archive by itself (a card that does not is not in it).
    pub(super) fn filing_map(&self, blocked: &HashMap<String, String>) -> HashMap<String, u64> {
        self.filing_plan(blocked).into_iter().flat_map(|(flow, at)| flow.into_iter().map(move |id| (id, at))).collect()
    }

    /// Someone did something on the canvas: the archive clock runs (on).
    pub(super) fn worked(&mut self) {
        if self.flow.hold {
            return;
        }
        let keep = (self.cfg.archive_s.max(self.cfg.archive_flow_s).max(0.0) * 1000.0) as u64;
        self.active.touch(now_ms(), keep);
    }

    /// Hold archiving or let it go: while it is held the clock stands, whoever works here.
    pub(super) fn cmd_archive_hold(&mut self, on: bool) -> Value {
        if on != self.flow.hold {
            self.flow.hold = on;
            save_json(&self.flow_path, &self.flow);
            if on {
                self.active.halt(now_ms());
            } else {
                self.worked();
            }
            self.bcast(&json!({"t": "hold", "on": on}));
            self.tick_idle();
        }
        json!({"ok": true, "hold": on})
    }

    /// The archive clock stopped or runs again: the browsers show it, and every countdown moves.
    pub(super) fn tick_idle(&mut self) {
        let now = now_ms();
        let stand = self.active.stand(now);
        let idle = (self.flow.hold || stand < now).then_some(stand);
        if idle != self.idle_shown {
            self.idle_shown = idle;
            self.bcast(&json!({"t": "idle", "since": idle}));
            for rt in self.rt.values_mut() {
                rt.ui = true;
            }
        }
    }

    /// File what is due. Not while someone has one of its terminals open: that goes once it is closed.
    pub(super) fn tick_archive(&mut self) {
        let now = now_ms();
        if now < self.archive_look {
            return;
        }
        self.archive_look = now + LOOK_EVERY_MS;
        let stand = self.active.stand(now);
        let watched: Vec<String> = self.clients.values().filter_map(|c| c.watch.as_ref().map(|w| w.node.clone())).collect();
        let blocked = self.blocked_map();
        let due: Vec<Vec<String>> = self
            .filing_plan(&blocked)
            .into_iter()
            .filter(|(flow, at)| *at <= stand && !flow.iter().any(|id| watched.contains(id)))
            .map(|(flow, _)| flow)
            .collect();
        for flow in due {
            self.file_away(&flow, true);
        }
        self.rest_idle(&watched);
    }

    /// An agent that finished and has sat idle for `rest_s` ends its process: an idle agent CLI
    /// holds hundreds of MB, and one left on the canvas overnight would hold them till morning. Its
    /// card stays as it was, with its result; a prompt to it starts a new process. Not one that waits
    /// for the person (an issue, a dialog), one that is kept (or in a kept flow), or one whose terminal
    /// is open.
    pub(super) fn rest_idle(&mut self, watched: &[String]) {
        if self.cfg.rest_s <= 0.0 {
            return;
        }
        let rest = (self.cfg.rest_s * 1000.0) as u64;
        let now = now_ms();
        let kept: Vec<String> =
            self.flows().into_iter().filter(|f| f.iter().any(|id| self.node(id).is_some_and(|n| n.keep))).flatten().collect();
        let tired: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() == Kind::Agent && !n.keep && !kept.contains(&n.id) && !watched.contains(&n.id))
            .filter(|n| {
                self.rt.get(&n.id).is_some_and(|r| {
                    r.alive
                        && r.finished
                        && r.at_rest()
                        && r.issue.is_none()
                        && r.waiting.is_none()
                        && !r.attn
                        && now.saturating_sub(r.since) >= rest
                        && r.screen_changed.elapsed() >= Duration::from_millis(rest)
                })
            })
            .map(|n| n.id.clone())
            .collect();
        for id in tired {
            self.kill(&id);
        }
    }

    /// File cards away: their processes end, their links go with them, and what they knew is kept so
    /// they can come back. The queued prompts asked of a filed agent go along (they are nothing without
    /// it). Returns the drop's id.
    pub(super) fn file_away(&mut self, ids: &[String], auto: bool) -> Option<String> {
        let mut take: Vec<String> = Vec::new();
        for id in ids {
            if self.node(id).is_some() && !take.contains(id) {
                take.push(id.clone());
            }
        }
        let steps: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() == Kind::Step && take.contains(&n.host) && !take.contains(&n.id))
            .filter(|n| self.rt.get(&n.id).is_some_and(|r| r.at_rest()))
            .map(|n| n.id.clone())
            .collect();
        take.extend(steps);
        if take.is_empty() {
            return None;
        }
        let nodes: Vec<Node> = self.flow.nodes.iter().filter(|n| take.contains(&n.id)).cloned().collect();
        let edges: Vec<Edge> = self.flow.edges.iter().filter(|e| take.contains(&e.from) || take.contains(&e.to)).cloned().collect();
        let state: HashMap<String, Saved> =
            take.iter().filter_map(|id| Some((id.clone(), filed_state(self.rt.get(id)?.to_saved())))).collect();
        let gid = self.next_id("a", self.archive.groups.iter().map(|g| g.id.clone()));
        let pack = Pack { nodes, edges, state };
        // nothing leaves the canvas that the archive could not keep
        if let Err(err) = write_pack(&self.pack_path(&gid), &pack) {
            eprintln!("weft: could not file {}: {err}", take.join(", "));
            return None;
        }
        for id in &take {
            self.drop_node(id, false);
        }
        self.archive.groups.push(Filed::new(gid.clone(), now_ms(), auto, &pack));
        while self.archive.groups.len() > KEEP_FILED {
            let old = self.take_group(0);
            self.forget_worktrees(&old);
        }
        self.state_dirty = true;
        self.flow_changed();
        self.archive_changed(Some(&gid));
        Some(gid)
    }

    /// Filed work that leaves the archive for good takes its worktrees along, as a deleted card does,
    /// unless something on the canvas or in the archive still works there (and git keeps a worktree
    /// that has uncommitted changes).
    fn forget_worktrees(&self, gone: &Filed) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        let legacy = gone.nodes.iter().map(|n| (n.cwd.clone(), gone.state.get(&n.id).and_then(|s| s.worktree.clone())));
        let cards: Vec<(String, Option<String>)> = gone.cards.iter().map(|c| (c.cwd.clone(), c.worktree.clone())).chain(legacy).collect();
        for (cwd, wt) in cards.into_iter().filter_map(|(cwd, wt)| Some((cwd, wt?))) {
            let used = self.rt.values().any(|r| r.worktree.as_deref() == Some(wt.as_str()))
                || self.flow.nodes.iter().any(|n| n.cwd == wt)
                || self.archive.groups.iter().any(|g| {
                    g.cards.iter().any(|c| c.cwd == wt || c.worktree.as_deref() == Some(wt.as_str()))
                        || g.nodes.iter().any(|n| n.cwd == wt)
                        || g.state.values().any(|s| s.worktree.as_deref() == Some(wt.as_str()))
                });
            if used {
                continue;
            }
            // (the repository it was cut from: the folder the agent worked in)
            let dir = if cwd.trim().is_empty() { self.boot.dir.clone() } else { self.abs_dir(&cwd) };
            handle.spawn(async move {
                let _ = tokio::process::Command::new("git").args(["-C"]).arg(dir).args(["worktree", "remove", &wt]).output().await;
            });
        }
    }

    /// The archive changed: keep it on disk and show it. `filed`: the drop that just went in.
    fn archive_changed(&mut self, filed: Option<&str>) {
        self.archive.save(&self.archive_path);
        let m = json!({"t": "archive", "groups": self.archive_json(), "filed": filed});
        self.bcast(&m);
    }

    /// What the browsers list: per drop its cards, how each ended, and the start of what it was asked
    /// and said.
    pub(super) fn archive_json(&self) -> Vec<Value> {
        self.archive
            .groups
            .iter()
            .map(|g| {
                // (one that could not be packed yet still holds it all)
                let legacy = g.unpacked().map(|p| Filed::new(g.id.clone(), g.at, g.auto, &p));
                let nodes: Vec<Value> = legacy
                    .as_ref()
                    .unwrap_or(g)
                    .cards
                    .iter()
                    .map(|c| {
                        json!({"id": c.id, "name": c.name, "agent": c.agent, "git_op": c.git_op, "x": c.x, "y": c.y, "w": c.w, "h": c.h,
                               "ok": c.ok, "since": c.since, "asked": c.asked, "out": c.out})
                    })
                    .collect();
                json!({"id": g.id, "at": g.at, "auto": g.auto, "nodes": nodes})
            })
            .collect()
    }

    /// Cards dropped on the archive. What is still at work stays where it is, and the reply says which.
    pub(super) fn cmd_archive(&mut self, ids: Vec<String>) -> Value {
        let (rest, busy): (Vec<String>, Vec<String>) =
            ids.into_iter().filter(|id| self.node(id).is_some()).partition(|id| self.rt.get(id).is_some_and(|r| r.at_rest()));
        let busy: Vec<String> = busy.iter().filter_map(|id| self.node(id).map(|n| n.name.clone())).collect();
        match self.file_away(&rest, false) {
            Some(group) => json!({"ok": true, "group": group, "busy": busy}),
            None if busy.is_empty() => json!({"ok": false, "error": "nothing to file"}),
            None => json!({"ok": false, "busy": busy,
                           "error": format!("{} is still at work: stop it or let it finish first", busy.join(", "))}),
        }
    }

    /// Bring filed work back, moved by (dx, dy): its cards as they were filed, with their results and
    /// the links among them and to cards still on the canvas, but no process (the next prompt starts
    /// one). Nothing is handed on again. Its archive clock starts over.
    pub(super) fn cmd_revive(&mut self, group: String, dx: f64, dy: f64) -> Value {
        let Some(pos) = self.archive.groups.iter().position(|g| g.id == group) else {
            return json!({"ok": false, "error": "that is no longer in the archive"});
        };
        let Some(mut g) = self.open_pack(&self.archive.groups[pos]) else {
            return json!({"ok": false, "error": "its cards could not be read from the archive"});
        };
        self.take_group(pos);
        let (dx, dy) = (if dx.is_finite() { dx } else { 0.0 }, if dy.is_finite() { dy } else { 0.0 });
        let now = now_ms();
        // the same ids where they are free (they are, unless the files were edited by hand)
        let mut taken = self.taken_ids(false);
        taken.extend(g.nodes.iter().map(|n| n.id.clone()));
        let mut ids: HashMap<String, String> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for mut n in std::mem::take(&mut g.nodes) {
            let old = n.id.clone();
            if self.node(&old).is_some() {
                n.id = self.next_id("n", taken.iter().cloned());
                taken.push(n.id.clone());
            }
            n.name = self.unique_name(n.name.trim(), None);
            n.x += dx;
            n.y += dy;
            let mut rt = Rt::new(self.cfg.cols, self.cfg.rows);
            if let Some(s) = g.state.remove(&old) {
                rt.restore(s);
            }
            rt.revived = now;
            ids.insert(old, n.id.clone());
            order.push(n.id.clone());
            self.rt.insert(n.id.clone(), rt);
            self.flow.nodes.push(n);
        }
        for n in self.flow.nodes.iter_mut().filter(|n| order.contains(&n.id)) {
            if let Some(t) = ids.get(&n.takeover) {
                n.takeover = t.clone();
            }
            if let Some(h) = ids.get(&n.host) {
                n.host = h.clone();
            }
        }
        // links come back without delivering anything: what arrived before was used then
        let mut taken = self.taken_ids(true);
        for mut e in std::mem::take(&mut g.edges) {
            e.from = ids.get(&e.from).cloned().unwrap_or(e.from);
            e.to = ids.get(&e.to).cloned().unwrap_or(e.to);
            if self.node(&e.from).is_none() || self.node(&e.to).is_none() || self.flow.edges.iter().any(|x| x.from == e.from && x.to == e.to)
            {
                continue;
            }
            if self.flow.edges.iter().any(|x| x.id == e.id) {
                let old = std::mem::replace(&mut e.id, self.next_id("e", taken.iter().cloned()));
                taken.push(e.id.clone());
                if let Some(rt) = self.rt.get_mut(&e.to) {
                    for (eid, _) in rt.inputs.iter_mut().filter(|(eid, _)| *eid == old) {
                        *eid = e.id.clone();
                    }
                }
            }
            self.flow.edges.push(e);
        }
        let live: Vec<String> = self.flow.edges.iter().map(|e| e.id.clone()).collect();
        for id in &order {
            if let Some(rt) = self.rt.get_mut(id) {
                rt.inputs.retain(|(eid, _)| live.contains(eid));
            }
            if self.node(id).is_some_and(|n| n.kind() == Kind::Note) {
                self.refresh_note(id);
            }
            // a long result is handed on by path: the file is where the links will say it is
            if let Some(out) = self.rt.get(id).and_then(|r| r.output.clone()).filter(|o| o.len() > PIPE_MAX) {
                let _ = write_atomic(&self.out_path(id), out.as_bytes());
            }
        }
        self.state_dirty = true;
        self.flow_changed();
        self.archive_changed(None);
        json!({"ok": true, "ids": order})
    }

    /// What filed work was asked and said, in full.
    pub(super) fn cmd_archive_get(&mut self, group: String) -> Value {
        let Some(g) = self.archive.groups.iter().find(|g| g.id == group) else {
            return json!({"ok": false, "error": "that is no longer in the archive"});
        };
        let Some(g) = self.open_pack(g) else {
            return json!({"ok": false, "error": "its cards could not be read from the archive"});
        };
        let nodes: Vec<Value> = g
            .nodes
            .iter()
            .map(|n| {
                let s = g.state.get(&n.id);
                json!({"id": n.id, "name": n.name, "agent": n.agent, "prompt": n.prompt,
                       "asked": s.map(|s| s.asked.clone()), "out": s.and_then(|s| s.output.clone()),
                       "ok": s.filter(|s| s.finished).map(|s| s.ok.unwrap_or(true))})
            })
            .collect();
        json!({"ok": true, "nodes": nodes})
    }

    /// Forget filed work for good (its worktrees go too, where nothing else works in them).
    pub(super) fn cmd_archive_del(&mut self, group: String) -> Value {
        let Some(pos) = self.archive.groups.iter().position(|g| g.id == group) else {
            return json!({"ok": false, "error": "that is no longer in the archive"});
        };
        let g = self.take_group(pos);
        self.forget_worktrees(&g);
        self.archive_changed(None);
        json!({"ok": true})
    }
}
