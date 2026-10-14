//! Gates on a link: approve, screenshot, tests. Each is a barrier the flow waits at.

use super::*;

/// The request a gate puts to the agent upstream of it.
fn gate_prompt(node: &Node, stage: GateStage, extra: &str, path: &str, exe: &str, notes: &str) -> String {
    match stage {
        GateStage::Shot => words::gate_shot(exe, &node.subject, extra, path),
        GateStage::Write => words::gate_write_tests(exe, &node.subject),
        GateStage::Fix => words::gate_fix(exe, if extra.trim().is_empty() { notes } else { extra }),
        GateStage::RunByAgent => words::gate_run_tests(exe),
        GateStage::None => String::new(),
    }
}

impl Engine {
    pub(super) fn shots_dir(&self) -> PathBuf {
        self.boot.dir.join(".weft").join("shots")
    }

    pub(super) fn shot_path(&self, gate: &str, round: u32, src: &str) -> PathBuf {
        self.shots_dir().join(format!("{gate}-r{round}-{src}.png"))
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
            // a queued prompt is answered by its agent: that is who the gate talks to
            let t = self.resolve_host(&i.from);
            if self.node(&t).is_some_and(|n| n.kind() == Kind::Agent) && !targets.contains(&t) {
                targets.push(t);
            }
        }
        {
            let Some(rt) = self.rt.get_mut(id) else { return };
            rt.gate.held = Some(held);
            rt.gate.round = 1;
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

    fn gate_review_with(&mut self, id: &str, note: &str) {
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
            let (ok, out) = run_shell(&dir, &cmd, &[("WEFT_SHOT", path.to_string_lossy().as_ref())], 120).await;
            let ok = ok && path.is_file();
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

    /// Run the suite: with the node's command if it has one, else by asking the agent to run it.
    fn gate_run_tests(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
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
            let (ok, out) = run_shell(&dir, &cmd, &[], 900).await;
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
        if round <= node.retries && can_fix && !node.cmd.trim().is_empty() {
            if let Some(rt) = self.rt.get_mut(id) {
                rt.gate.round += 1;
            }
            let out = self.rt[id].gate.out.clone();
            self.gate_ask(id, GateStage::Fix, &format!("The test suite failed:\n\n{out}"));
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
            if node.kind() == Kind::Tests {
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
                self.gate_ask(&id, GateStage::Fix, &format!("The test suite failed:\n\n{out}"));
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
