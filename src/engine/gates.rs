//! Gates on a link: approve, screenshot, tests. Each is a barrier the flow waits at.

use super::*;

/// The request a gate puts to the agent upstream of it.
fn gate_prompt(node: &Node, stage: GateStage, extra: &str, path: &str, exe: &str, notes: &str) -> String {
    match stage {
        GateStage::Shot => words::gate_shot(exe, &node.subject, extra, path),
        GateStage::Write => words::gate_write_tests(exe, &node.subject),
        GateStage::Fix if node.ci => words::ci_fix(exe, if extra.trim().is_empty() { notes } else { extra }),
        GateStage::Fix => words::gate_fix(exe, if extra.trim().is_empty() { notes } else { extra }),
        GateStage::RunByAgent => words::gate_run_tests(exe),
        GateStage::None => String::new(),
    }
}

/// What goes back to the agent when the suite (or CI, which says it in its own words) went red.
fn fix_note(node: &Node, out: &str) -> String {
    if node.ci {
        out.to_string()
    } else {
        format!("The test suite failed:\n\n{out}")
    }
}

impl Engine {
    pub(super) fn shots_dir(&self) -> PathBuf {
        self.boot.dir.join(".weft").join("shots")
    }

    /// Where a round's screenshot goes. The name says when the round opened: a gate that runs again
    /// asks for a new file, and a page never shows the image it keeps of an earlier round instead.
    pub(super) fn shot_path(&self, gate: &str, round: u32, src: &str) -> PathBuf {
        match self.rt.get(gate).map_or(0, |r| r.gate.opened) {
            // (a round an older build opened told the agent the name without it)
            0 => self.shots_dir().join(format!("{gate}-r{round}-{src}.png")),
            opened => self.shots_dir().join(format!("{gate}-{opened}-r{round}-{src}.png")),
        }
    }

    /// After a restart (or a revive): a gate that was asking agents goes on waiting for those that
    /// came back and still have its request, queued or being answered. With none left, it is the
    /// person's turn, as after a capture or a test run the restart cut short.
    pub(super) fn resume_gates(&mut self, ids: &[String]) {
        for id in ids {
            let Some(r) = self.rt.get(id).filter(|r| r.gate.phase == GatePhase::Asking) else { continue };
            let holds = |t: &String| {
                self.rt.get(t).is_some_and(|a| {
                    a.alive && (a.reply_to.as_deref() == Some(id) || a.queue.iter().any(|q| matches!(&q.src, Src::Reply(g) if g == id)))
                })
            };
            let wait: Vec<String> = r.gate.targets.iter().filter(|t| holds(t)).cloned().collect();
            let Some(r) = self.rt.get_mut(id) else { continue };
            if wait.is_empty() {
                r.gate.cut_short();
                r.set_status(Status::Attention);
            } else {
                r.gate.wait = wait;
            }
            r.ui = true;
        }
    }

    /// A round opens: upstream results arrived at a gate.
    pub(super) fn gate_begin(&mut self, id: &str, node: &Node, inputs: &[Input]) {
        let held = inputs
            .iter()
            .map(|i| format!("### Output of \"{}\"\n{}", i.name, tail_bytes(i.text.trim(), 20_000)))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut targets: Vec<String> = Vec::new();
        for i in inputs {
            // a queued prompt is answered by its agent, and a gate before this one holds that agent's
            // work: that is who this gate talks to (a command is its own target)
            let t = self.agent_behind(&i.from).unwrap_or_else(|| self.resolve_host(&i.from));
            if self.node(&t).is_some_and(|n| n.kind() == Kind::Agent) && !targets.contains(&t) {
                targets.push(t);
            }
        }
        if node.ci {
            // after a push nobody's session goes on: what CI finds goes to the agents behind the commits
            for t in self.ci_targets(id) {
                if !targets.contains(&t) {
                    targets.push(t);
                }
            }
        }
        {
            let Some(rt) = self.rt.get_mut(id) else { return };
            rt.gate.held = Some(held);
            rt.gate.round = 1;
            rt.gate.opened = now_ms().max(rt.gate.opened + 1);
            rt.gate.shots.clear();
            rt.gate.out.clear();
            rt.gate.wait.clear();
            rt.gate.failed = false;
            rt.gate.targets = targets;
            rt.gate.gen += 1;
            rt.finished = false;
            rt.ok = None;
            rt.cause = Cause::Flow;
            rt.ui = true;
        }
        self.state_dirty = true;
        match node.kind() {
            Kind::Gate => self.gate_review(id),
            Kind::Shot => self.gate_capture(id, ""),
            Kind::Tests => {
                if node.write_tests {
                    self.gate_ask(id, GateStage::Write, "");
                } else {
                    self.gate_run_tests(id);
                }
            }
            _ => {}
        }
    }

    /// Everything is in: the round waits for the human.
    fn gate_review(&mut self, id: &str) {
        if let Some(rt) = self.rt.get_mut(id) {
            rt.gate.phase = GatePhase::Review;
            rt.gate.wait.clear();
            rt.ui = true;
        }
        self.set_status(id, Status::Attention);
        self.state_dirty = true;
    }

    pub(super) fn gate_review_with(&mut self, id: &str, note: &str) {
        if let Some(rt) = self.rt.get_mut(id) {
            if !rt.gate.out.is_empty() {
                rt.gate.out.push('\n');
            }
            rt.gate.out.push_str(note);
        }
        self.gate_review(id);
    }

    /// Take a screenshot: with the node's own command if it has one, else by asking the agent.
    fn gate_capture(&mut self, id: &str, changes: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        if node.cmd.trim().is_empty() {
            self.gate_ask(id, GateStage::Shot, changes);
            return;
        }
        let round = self.rt[id].gate.round;
        let path = self.shot_path(id, round, "cmd");
        let _ = std::fs::create_dir_all(self.shots_dir());
        let _ = std::fs::remove_file(&path);
        let dir = self.node_dir(&node);
        let (tx, gen, nid) = (self.tx.clone(), self.rt[id].gate.gen, id.to_string());
        let cmd = node.cmd.clone();
        if let Some(rt) = self.rt.get_mut(id) {
            rt.gate.phase = GatePhase::Running;
            rt.gate.stage = GateStage::Shot;
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        tokio::spawn(async move {
            let (ok, out) = run_capture(&dir, &cmd, &path).await;
            let _ = tx.send(Ev::Gate { id: nid, gen, tests: false, ok, out });
        });
    }

    /// Queue a request on every running upstream agent and wait for their answers.
    fn gate_ask(&mut self, id: &str, stage: GateStage, extra: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        let exe = self.boot.exe.clone();
        let round = self.rt[id].gate.round;
        let alive: Vec<String> = self.rt[id].gate.targets.iter().filter(|t| self.rt.get(*t).is_some_and(|r| r.alive)).cloned().collect();
        // a tests gate sends fixes to the agent that finished last: one fixer, not a crowd
        let alive: Vec<String> = if node.kind() == Kind::Tests && alive.len() > 1 { vec![alive[alive.len() - 1].clone()] } else { alive };
        if alive.is_empty() {
            let why = match stage {
                GateStage::Shot => "No running agent upstream to ask for a screenshot. Paste or drop an image onto this gate, or set a capture command in its options.",
                _ => "No running agent upstream to ask. Fix things yourself, then run the check again or continue.",
            };
            self.gate_review_with(id, why);
            return;
        }
        let _ = std::fs::create_dir_all(self.shots_dir());
        for t in &alive {
            let path = self.shot_path(id, round, t);
            // a stale file at this path would pass for the new screenshot
            let _ = std::fs::remove_file(&path);
            let text = gate_prompt(&node, stage, extra, &path.to_string_lossy(), &exe, &self.rt[id].gate.out);
            let shown = text.lines().next().unwrap_or("").to_string();
            self.enqueue(t, text, shown, Src::Reply(id.to_string()), true);
        }
        if let Some(rt) = self.rt.get_mut(id) {
            rt.gate.wait = alive;
            rt.gate.stage = stage;
            rt.gate.phase = GatePhase::Asking;
            rt.gate.failed = false;
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
    }

    /// An agent answered the gate's request.
    pub(super) fn gate_response(&mut self, gate: &str, from: &str, ok: bool, text: &str) {
        let name = self.node(from).map(|n| n.name.clone()).unwrap_or_default();
        let Some(kind) = self.node(gate).map(|n| n.kind()) else { return };
        let (done, stage) = {
            let Some(rt) = self.rt.get_mut(gate) else { return };
            // (an answer to a request the gate took back, or that a pasted image made moot, is not this round's)
            if !rt.gate.wait.iter().any(|w| w == from) {
                return;
            }
            rt.gate.wait.retain(|w| w != from);
            if !ok {
                rt.gate.failed = true;
            }
            let line = if ok { text.trim().to_string() } else { format!("{name} could not do it: {}", text.trim()) };
            if !line.is_empty() {
                if !rt.gate.out.is_empty() {
                    rt.gate.out.push('\n');
                }
                rt.gate.out.push_str(&line);
            }
            rt.ui = true;
            (rt.gate.wait.is_empty(), rt.gate.stage)
        };
        if !done {
            return;
        }
        match (kind, stage) {
            (Kind::Shot, _) => {
                let round = self.rt[gate].gate.round;
                let targets = self.rt[gate].gate.targets.clone();
                let mut found = Vec::new();
                for t in &targets {
                    let p = self.shot_path(gate, round, t);
                    if p.is_file() {
                        if let Some(n) = p.file_name() {
                            found.push(n.to_string_lossy().into_owned());
                        }
                    }
                }
                if found.is_empty() {
                    self.gate_review_with(gate, "No screenshot file appeared. Ask again, paste or drop an image, or continue without one.");
                } else {
                    if let Some(rt) = self.rt.get_mut(gate) {
                        rt.gate.shots = found;
                    }
                    self.gate_review(gate);
                }
            }
            (Kind::Tests, GateStage::RunByAgent) => {
                let failed = self.rt[gate].gate.failed;
                let out = self.rt[gate].gate.out.clone();
                self.gate_decide_tests(gate, !failed, out);
            }
            (Kind::Tests, _) => self.gate_run_tests(gate),
            _ => self.gate_review(gate),
        }
    }

    /// A round shows what came in, and a new result is on its way from before it (the agent was set
    /// to work again, a prompt was queued on it, a card before it works): what it shows or asks for is
    /// about to change. The round is taken back: its screenshot and requests are withdrawn, and the
    /// gate waits with what it held until the cards before it are done, then starts over (and asks
    /// for a new screenshot). A CI check watches what was pushed, which a new push replaces itself.
    pub(super) fn tick_gates(&mut self) {
        let stale: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind().is_gate() && !n.ci)
            // (an image pasted onto an idle gate, or a gate fired with nothing, holds nothing from its links)
            .filter(|n| self.rt.get(&n.id).is_some_and(|r| !r.finished && r.gate.held.as_deref().is_some_and(|h| !h.is_empty())))
            .filter(|n| self.round_links(&n.id).iter().any(|e| self.pending(e)))
            .map(|n| n.id.clone())
            .collect();
        for id in stale {
            self.gate_take_back(&id);
        }
    }

    /// The links whose results a gate's open round holds: all of them for "all", only those it took
    /// for "any" (one that did not come yet is not in the round).
    fn round_links(&self, id: &str) -> Vec<Edge> {
        let (Some(node), Some(rt)) = (self.node(id), self.rt.get(id)) else { return Vec::new() };
        let took: Option<Vec<&str>> = rt.unstarted.as_ref().map(|(_, t)| t.iter().map(|(e, _)| e.as_str()).collect());
        self.flow
            .edges
            .iter()
            .filter(|e| e.to == id && !self.is_const_edge(e))
            .filter(|e| node.join == Join::All || took.as_ref().is_none_or(|t| t.contains(&e.id.as_str())))
            .cloned()
            .collect()
    }

    fn gate_take_back(&mut self, id: &str) {
        let Some(join) = self.node(id).map(|n| n.join) else { return };
        // what the round held goes back to the gate as what came in, so it starts over with it: a new
        // result coming down a link takes its place, and one whose card only answered what was queued
        // on it counts again once that is done. (A round from before a restart says what it took only
        // in its text: the links' results are taken again.)
        let took = self.rt.get_mut(id).and_then(|r| r.unstarted.take()).map(|(_, t)| t);
        let mut back: Vec<(String, Input)> = Vec::new();
        for e in self.flow.edges.iter().filter(|e| e.to == id && !self.is_const_edge(e)) {
            // ("any" goes on with what is not about to change, and waits for the rest)
            if join == Join::Any && self.pending(e) {
                continue;
            }
            let had = match &took {
                Some(t) => t.iter().find(|(eid, _)| *eid == e.id).map(|(_, i)| i.clone()),
                None => {
                    let has = self.rt.get(&e.from).is_some_and(|s| s.output.is_some() && e.on.matches(s.ok.unwrap_or(true)));
                    has.then(|| self.input_along(e))
                }
            };
            if let Some(i) = had {
                back.push((e.id.clone(), i));
            }
        }
        let asked = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let targets = std::mem::take(&mut rt.gate.targets);
            let opened = rt.gate.opened;
            rt.gate.reset();
            // (the card still says whose work it waits for; the next round's screenshots get new names)
            rt.gate.targets = targets.clone();
            rt.gate.opened = opened;
            for (eid, i) in back {
                if !rt.inputs.iter().any(|(x, _)| *x == eid) {
                    rt.inputs.push((eid, i));
                }
            }
            rt.ok = None;
            rt.set_status(Status::Waiting);
            targets
        };
        // a request still waiting in an agent's queue is withdrawn; one it is answering now is let be
        // (its answer is not this round's: `gate_response`)
        for t in asked {
            if let Some(r) = self.rt.get_mut(&t) {
                r.queue.retain(|q| !matches!(&q.src, Src::Reply(g) if g == id));
                r.ui = true;
            }
        }
        self.state_dirty = true;
        self.touch_downstream(id);
        // ("any" may have something to go on with already)
        self.check_join(id);
    }

    /// Run the suite: with the node's command if it has one, else by asking the agent to run it.
    fn gate_run_tests(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        if node.ci {
            return self.ci_start(id);
        }
        if node.cmd.trim().is_empty() {
            self.gate_ask(id, GateStage::RunByAgent, "");
            return;
        }
        let dir = self.node_dir(&node);
        let (tx, gen, nid) = (self.tx.clone(), self.rt[id].gate.gen, id.to_string());
        let cmd = node.cmd.clone();
        if let Some(rt) = self.rt.get_mut(id) {
            rt.gate.phase = GatePhase::Running;
            rt.gate.stage = GateStage::None;
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
        tokio::spawn(async move {
            let (ok, out) = run_tests(&dir, &cmd).await;
            let _ = tx.send(Ev::Gate { id: nid, gen, tests: true, ok, out });
        });
    }

    pub(super) fn on_gate(&mut self, id: String, gen: u64, tests: bool, ok: bool, out: String) {
        let Some(rt) = self.rt.get_mut(&id) else { return };
        if rt.gate.gen != gen || rt.gate.held.is_none() {
            return; // a reset or a new round happened meanwhile
        }
        rt.ui = true;
        if tests {
            self.gate_decide_tests(&id, ok, out);
            return;
        }
        // a capture command
        let round = rt.gate.round;
        let p = self.shot_path(&id, round, "cmd");
        if ok && p.is_file() {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(rt) = self.rt.get_mut(&id) {
                rt.gate.shots = vec![name];
            }
            self.gate_review(&id);
        } else {
            self.gate_review_with(&id, &format!("The capture command did not produce an image.\n{}", tail_bytes(out.trim(), 1500)));
        }
    }

    /// The suite ran. Green passes (or asks you); red goes back to the agent a few times first.
    pub(super) fn gate_decide_tests(&mut self, id: &str, ok: bool, out: String) {
        let Some(node) = self.node(id).cloned() else { return };
        {
            let Some(rt) = self.rt.get_mut(id) else { return };
            rt.gate.out = tail_bytes(out.trim(), 6000).to_string();
            rt.ui = true;
        }
        if ok {
            if node.confirm {
                self.gate_review(id);
            } else {
                self.gate_finish(id, true, "");
            }
            return;
        }
        let round = self.rt[id].gate.round;
        let can_fix = self.rt[id].gate.targets.iter().any(|t| self.rt.get(t).is_some_and(|r| r.alive));
        if round <= node.retries && can_fix && (node.ci || !node.cmd.trim().is_empty()) {
            if let Some(rt) = self.rt.get_mut(id) {
                rt.gate.round += 1;
            }
            let out = self.rt[id].gate.out.clone();
            self.gate_ask(id, GateStage::Fix, &fix_note(&node, &out));
        } else {
            self.gate_review(id);
        }
    }

    /// Close the round: the barrier opens (ok) or the fail links fire.
    fn gate_finish(&mut self, id: &str, ok: bool, note: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        let (held, shots, out) = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let held = rt.gate.held.take().unwrap_or_default();
            rt.gate.phase = GatePhase::Idle;
            rt.gate.wait.clear();
            rt.cause = Cause::Flow;
            (held, rt.gate.shots.clone(), rt.gate.out.clone())
        };
        let mut text = held;
        if ok {
            for s in &shots {
                text.push_str(&format!("\n\nApproved screenshot: {}", self.shots_dir().join(s).to_string_lossy()));
            }
            if node.ci {
                text.push_str(&format!("\n\n{}", head_chars(out.trim(), 1500)));
            } else if node.kind() == Kind::Tests {
                let tail: Vec<&str> = out.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
                text.push_str(&format!("\n\nTests passed.\n{}", tail.join("\n")));
            }
            if text.trim().is_empty() {
                text = "approved".to_string();
            }
        } else {
            text = if note.is_empty() { "rejected".to_string() } else { note.to_string() };
        }
        self.settle(id, ok, text.trim().to_string());
    }

    pub(super) fn cmd_approve(&mut self, id: String, ok: bool) -> Value {
        if self.rt.get(&id).is_none_or(|r| r.gate.held.is_none()) {
            return json!({"ok": false, "error": "nothing is waiting for a decision here"});
        }
        self.gate_finish(&id, ok, if ok { "" } else { "rejected" });
        json!({"ok": true})
    }

    pub(super) fn cmd_gate_changes(&mut self, id: String, text: String) -> Value {
        let Some(kind) = self.node(&id).map(|n| n.kind()) else {
            return json!({"ok": false, "error": "no such node"});
        };
        if !matches!(kind, Kind::Shot | Kind::Tests) || self.rt.get(&id).is_none_or(|r| r.gate.held.is_none()) {
            return json!({"ok": false, "error": "no open round to change"});
        }
        if let Some(r) = self.rt.get_mut(&id) {
            r.gate.round += 1;
            r.gate.shots.clear();
            r.gate.failed = false;
        }
        let stage = if kind == Kind::Shot { GateStage::Shot } else { GateStage::Fix };
        self.gate_ask(&id, stage, &text);
        json!({"ok": true})
    }

    pub(super) fn cmd_gate_retry(&mut self, id: String, fix: bool) -> Value {
        let Some(kind) = self.node(&id).map(|n| n.kind()) else {
            return json!({"ok": false, "error": "no such node"});
        };
        if self.rt.get(&id).is_none_or(|r| r.gate.held.is_none()) {
            return json!({"ok": false, "error": "no open round to retry"});
        }
        if let Some(r) = self.rt.get_mut(&id) {
            r.gate.round += 1;
            r.gate.failed = false;
        }
        match (kind, fix) {
            (Kind::Shot, _) => {
                if let Some(r) = self.rt.get_mut(&id) {
                    r.gate.shots.clear();
                }
                self.gate_capture(&id, "");
            }
            (Kind::Tests, true) => {
                let out = self.rt.get(&id).map(|r| r.gate.out.clone()).unwrap_or_default();
                let note = self.node(&id).map(|n| fix_note(n, &out)).unwrap_or_default();
                self.gate_ask(&id, GateStage::Fix, &note);
            }
            (Kind::Tests, false) => self.gate_run_tests(&id),
            _ => {}
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_gate_shot(&mut self, id: String, name: String) -> Value {
        let ok_kind = self.node(&id).is_some_and(|n| n.kind() == Kind::Shot);
        let safe = !name.contains('/') && !name.contains("..") && self.shots_dir().join(&name).is_file();
        if !ok_kind || !safe {
            return json!({"ok": false, "error": "cannot attach that image"});
        }
        if let Some(r) = self.rt.get_mut(&id) {
            if r.gate.held.is_none() {
                r.gate.held = Some(String::new());
                r.gate.round = r.gate.round.max(1);
            }
            r.gate.shots.push(name);
            r.gate.wait.clear();
            r.gate.phase = GatePhase::Review;
            r.ui = true;
        }
        self.set_status(&id, Status::Attention);
        self.state_dirty = true;
        json!({"ok": true})
    }
}
