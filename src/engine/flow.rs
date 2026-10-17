//! Results travelling along links: joins, triggers, queued prompts and how a prompt is composed.

use super::*;

impl Engine {
    /// Fire every outgoing link whose condition matches.
    pub(super) fn propagate(&mut self, id: &str, ok: bool, out: &str) {
        let Some(name) = self.node(id).map(|n| n.name.clone()) else { return };
        let edges: Vec<Edge> = self.flow.edges.iter().filter(|e| e.from == id && e.on.matches(ok)).cloned().collect();
        for e in edges {
            self.deliver_input(&e, id, &name, ok, out);
        }
    }

    pub(super) fn deliver_input(&mut self, e: &Edge, from: &str, name: &str, ok: bool, out: &str) {
        if let Some(t) = self.rt.get_mut(&e.to) {
            t.used.clear(); // a new round has begun
            t.inputs.retain(|(eid, _)| *eid != e.id);
            t.inputs.push((
                e.id.clone(),
                Input { from: from.to_string(), name: name.to_string(), ok, text: out.to_string(), kind: InputKind::Result },
            ));
            t.ui = true;
        }
        // lets the UI animate the result travelling along the link
        let m = json!({"t": "hand", "edge": e.id, "from": from, "to": e.to, "ok": ok});
        self.bcast(&m);
        if self.node(&e.to).is_some_and(|n| n.kind() == Kind::Git) {
            self.git_arrival(&e.to, &e.id, from, name, ok, out);
        } else {
            self.check_join(&e.to);
        }
    }

    /// Notes are constants: a link from a note is always satisfied and always
    /// carries the note's current text.
    pub(super) fn is_const_edge(&self, e: &Edge) -> bool {
        self.node(&e.from).is_some_and(|n| n.kind() == Kind::Note)
    }

    pub(super) fn const_inputs(&self, to: &str) -> Vec<(String, Input)> {
        self.flow
            .edges
            .iter()
            .filter(|e| e.to == to && self.is_const_edge(e))
            .filter_map(|e| {
                let src = self.node(&e.from)?;
                let text = self.rt.get(&e.from)?.output.clone().unwrap_or_default();
                Some((e.id.clone(), Input { from: e.from.clone(), name: src.name.clone(), ok: true, text, kind: InputKind::Result }))
            })
            .collect()
    }

    pub(super) fn refresh_note(&mut self, id: &str) {
        let Some(text) = self.node(id).map(|n| n.prompt.clone()) else { return };
        if let Some(rt) = self.rt.get_mut(id) {
            rt.finished = true;
            rt.ok = Some(true);
            rt.output = Some(text.clone());
            rt.armed = true;
            rt.propagated = true;
            rt.ui = true;
            if rt.status != Status::Done {
                rt.status = Status::Done;
                rt.since = now_ms();
            }
        }
        // inputs a target has collected but not consumed follow the edit
        let edges: Vec<Edge> = self.flow.edges.iter().filter(|e| e.from == id).cloned().collect();
        for e in edges {
            if let Some(t) = self.rt.get_mut(&e.to) {
                if let Some(slot) = t.inputs.iter_mut().find(|(eid, _)| *eid == e.id) {
                    slot.1.text = text.clone();
                    t.ui = true;
                }
            }
        }
        self.state_dirty = true;
    }

    /// The junction: all inputs arrived (`Promise.all`) or the first one (`Promise.any`).
    fn check_join(&mut self, to: &str) {
        let Some(node) = self.node(to) else { return };
        if node.draft {
            return; // collects inputs, reacts once armed
        }
        let incoming: Vec<&str> = self.flow.edges.iter().filter(|e| e.to == to && !self.is_const_edge(e)).map(|e| e.id.as_str()).collect();
        if incoming.is_empty() {
            return; // fed only by notes: starts with "run flow" or when fired by hand
        }
        let rt = &self.rt[to];
        let have = incoming.iter().filter(|eid| rt.inputs.iter().any(|(i, _)| i == **eid)).count();
        let need = match node.join {
            Join::All => incoming.len(),
            Join::Any => 1,
        };
        if have > 0 && have >= need {
            self.trigger(to, false);
        }
    }

    /// Turn the collected inputs into a queued prompt and make sure the agent runs.
    pub(super) fn trigger(&mut self, id: &str, manual: bool) {
        let Some(node) = self.node(id).cloned() else { return };
        if node.draft || node.kind() == Kind::Note || node.kind() == Kind::Git {
            return;
        }
        let consts = self.const_inputs(id);
        let (runs, inputs) = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let order: Vec<&str> = self.flow.edges.iter().filter(|e| e.to == id).map(|e| e.id.as_str()).collect();
            let mut taken: Vec<(String, Input)> = std::mem::take(&mut rt.inputs);
            for (cid, ci) in consts {
                taken.retain(|(eid, _)| *eid != cid);
                taken.push((cid, ci));
            }
            taken.sort_by_key(|(eid, _)| order.iter().position(|o| *o == eid).unwrap_or(usize::MAX));
            rt.used = taken.iter().map(|(eid, _)| eid.clone()).collect();
            rt.ui = true;
            (rt.runs, taken.into_iter().map(|(_, i)| i).collect::<Vec<_>>())
        };
        if node.kind() == Kind::Step {
            // the agent has what it already produced, in its own session: its own result and the
            // answers it gave to other queued prompts of the same session are not sent back to it
            let host = node.host.clone();
            let inputs: Vec<Input> = inputs.into_iter().filter(|i| !self.in_session_of(&i.from, &host)).collect();
            self.step_run(id, &node, &inputs);
            return;
        }
        if node.kind().is_gate() {
            self.gate_begin(id, &node, &inputs);
            return;
        }
        if !manual && runs >= self.cfg.max_runs {
            self.toast("warn", &format!("Loop guard: \"{}\" already ran {} times. Use Run flow or Reset to continue.", node.name, runs));
            return;
        }
        let (body, full) = self.render_parts(&node, &inputs);
        if !full.is_empty() {
            self.enqueue(id, full, body, Src::Flow, false);
        }
        self.state_dirty = true;
        if self.rt.get(id).is_some_and(|r| !r.alive && !r.spawning) {
            self.start(id);
        }
    }

    /// Is what `from` produced something the agent `host` said in its own session? That is the
    /// agent itself and a queued prompt it answers. (A gate hands on what a check found out as
    /// well, so it counts as news.)
    pub(super) fn in_session_of(&self, from: &str, host: &str) -> bool {
        from == host || self.node(from).is_some_and(|n| n.kind() == Kind::Step && self.agent_behind(from).as_deref() == Some(host))
    }

    /// The agent a node speaks for: itself, or for a queued prompt the agent that answers it.
    pub(super) fn resolve_host(&self, id: &str) -> String {
        match self.node(id) {
            Some(n) if n.kind() == Kind::Step && !n.host.is_empty() => n.host.clone(),
            _ => id.to_string(),
        }
    }

    /// Queue the step's prompt on its agent, behind whatever that agent already has to do.
    fn step_run(&mut self, id: &str, node: &Node, inputs: &[Input]) {
        let host = node.host.clone();
        if self.node(&host).is_none_or(|h| h.kind() != Kind::Agent) {
            self.toast("warn", "This queued prompt has no agent to answer it: the agent it was meant for is gone.");
            if let Some(rt) = self.rt.get_mut(id) {
                rt.cause = Cause::User;
            }
            self.settle(id, false, "the agent this prompt was meant for is gone".into());
            return;
        }
        let (body, full) = self.render_parts(node, inputs);
        if full.trim().is_empty() {
            self.toast("info", "Write the prompt first: this step has nothing to ask yet.");
            return;
        }
        self.enqueue(&host, full, body, Src::Reply(id.to_string()), false);
        if let Some(rt) = self.rt.get_mut(id) {
            rt.step_wait = Some(host.clone());
            rt.finished = false;
            rt.output = None;
            rt.runs += 1;
            rt.cause = Cause::Flow;
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
        self.restart_if_queued(&host);
    }

    /// The agent answered the queued prompt: that answer is this node's result and goes on from here.
    pub(super) fn step_response(&mut self, id: &str, ok: bool, text: &str) {
        if let Some(rt) = self.rt.get_mut(id) {
            rt.step_wait = None;
            rt.cause = Cause::Flow;
            rt.propagated = false;
        }
        self.settle(id, ok, text.to_string());
    }

    /// Prompt = template (with context block) + addenda. Pure function of its inputs.
    /// Returns (without addenda, complete).
    pub(super) fn render_parts(&self, node: &Node, inputs: &[Input]) -> (String, String) {
        let block = inputs
            .iter()
            .map(|i| match i.kind {
                InputKind::Handover => format!("## Taking over from \"{}\"\n{}", i.name, i.text.trim()),
                InputKind::Fixed => words::fixed(&i.name, &i.text),
                _ => format!("### Output of \"{}\"\n{}", i.name, self.piped(&i.from, i.text.trim())),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let only_handover = !inputs.is_empty() && inputs.iter().all(|i| i.kind == InputKind::Handover);
        let mentions = node.prompt.contains("{{inputs}}") || node.prompt.contains("{{from:");
        let mut body = expand(&node.prompt, &|k| {
            if k == "inputs" {
                Some(block.clone())
            } else if let Some(n) = k.strip_prefix("from:") {
                let n = n.trim();
                Some(self.result_of(n, inputs))
            } else {
                self.node_var(node, k)
            }
        });
        if !inputs.is_empty() && !mentions {
            body = if body.trim().is_empty() {
                block
            } else if only_handover {
                format!("{}\n\n{}", body.trim_end(), block)
            } else {
                format!("{}\n\n{}\n{}", body.trim_end(), words::CONTEXT_HEADING, block)
            };
        }
        let body = body.trim().to_string();
        let full = self.append_addenda(node, body.clone()).trim().to_string();
        (body, full)
    }

    /// What `{{weft}}`, `{{node}}` and `{{cwd}}` stand for in a template written for `node`.
    pub(super) fn node_var(&self, node: &Node, key: &str) -> Option<String> {
        match key {
            "weft" => Some(self.boot.exe.clone()),
            "node" => Some(node.name.clone()),
            "cwd" => Some(self.node_dir(node).to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// `text` followed by the node's addenda, each expanded for this node.
    fn append_addenda(&self, node: &Node, text: String) -> String {
        let mut full = text;
        for aid in &node.addenda {
            if let Some(s) = self.cfg.library.iter().find(|s| s.id == *aid) {
                full.push_str("\n\n");
                full.push_str(&expand(&s.text, &|k| self.node_var(node, k)));
            }
        }
        full
    }

    /// A message typed to an agent carries the agent's addenda like every other prompt: that is how
    /// it learns to report when it is really done. Slash commands and shell escapes go through as typed.
    pub(super) fn with_addenda(&self, node: &Node, text: &str) -> String {
        let t = text.trim_start();
        if node.kind() != Kind::Agent || node.finish != Finish::Idle || node.addenda.is_empty() || t.starts_with('/') || t.starts_with('!')
        {
            return text.to_string();
        }
        self.append_addenda(node, text.trim_end().to_string())
    }

    pub(super) fn render(&self, node: &Node, inputs: &[Input]) -> String {
        self.render_parts(node, inputs).1
    }

    /// `{{from:Name}}`: an arrived input, else that agent's latest result.
    pub(super) fn result_of(&self, name: &str, inputs: &[Input]) -> String {
        if let Some(i) = inputs.iter().find(|i| i.name == name) {
            return self.piped(&i.from, i.text.trim());
        }
        match self.node_by_name(name).and_then(|n| self.rt.get(&n.id).map(|r| (n.id.clone(), r.output.clone()))) {
            Some((id, Some(o))) => self.piped(&id, o.trim()),
            _ => format!("(no output from {name} yet)"),
        }
    }

    /// What goes into another agent's prompt: the end of the output (that is where the
    /// conclusion is), with a pointer to the whole text when it had to be cut.
    pub(super) fn piped(&self, from: &str, text: &str) -> String {
        if text.len() <= PIPE_MAX {
            return text.to_string();
        }
        format!(
            "(the beginning is left out; the full output is in {})\n\u{2026}{}",
            self.out_path(from).display(),
            tail_bytes(text, PIPE_MAX)
        )
    }

    pub(super) fn reset(&mut self) {
        for rt in self.rt.values_mut() {
            rt.reset();
        }
        let ids: Vec<String> = self.rt.keys().cloned().collect();
        for id in &ids {
            let alive = self.rt[id].alive;
            self.set_status(id, if alive { Status::Ready } else { Status::New });
        }
        for id in ids {
            if self.node(&id).is_some_and(|n| n.kind() == Kind::Note) {
                self.refresh_note(&id);
            }
        }
        self.state_dirty = true;
    }

    pub(super) fn cmd_arm(&mut self, id: String) -> Value {
        let Some(kind) = self.node(&id).map(|n| n.kind()) else {
            return json!({"ok": false, "error": "no such node"});
        };
        if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == id) {
            n.draft = false;
        }
        self.flow_changed();
        // a hand-off that was drawn on the canvas starts here: the agent it takes over from
        // is asked for its note, then this one begins
        let handoff =
            self.node(&id).filter(|n| n.kind() == Kind::Agent && !n.takeover.is_empty()).map(|n| n.takeover.clone()).filter(|f| {
                self.rt.get(&id).is_some_and(|r| r.runs == 0 && r.hand.from.is_none() && r.queue.is_empty()) && self.node(f).is_some()
            });
        if let Some(from) = handoff {
            let from = self.resolve_host(&from);
            let cwd = self.rt.get(&from).and_then(|r| r.worktree.clone()).filter(|p| Path::new(p).is_dir());
            if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == id) {
                if n.prompt.trim().is_empty() {
                    n.prompt = "Continue the work: finish what is still open, verify it, then report.".into();
                }
                n.takeover = from.clone();
                n.worktree = false;
                if let Some(c) = cwd {
                    n.cwd = c;
                }
            }
            self.flow_changed();
            self.takeover_begin(&id, &from);
            return json!({"ok": true});
        }
        match kind {
            Kind::Git => self.git_pump(&id, false),
            Kind::Agent | Kind::Step => {
                let has_in = self.flow.edges.iter().any(|e| e.to == id && !self.is_const_edge(e));
                if has_in {
                    self.check_join(&id);
                } else {
                    self.trigger(&id, true);
                }
            }
            _ => {}
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_fire(&mut self, id: String) -> Value {
        let Some(kind) = self.node(&id).map(|n| n.kind()) else {
            return json!({"ok": false, "error": "no such node"});
        };
        if kind == Kind::Git {
            self.git_ship_now(&id);
            return json!({"ok": true});
        }
        if kind == Kind::Step {
            self.trigger(&id, true);
            return json!({"ok": true});
        }
        if kind != Kind::Agent {
            return json!({"ok": true});
        }
        if let Some((from, phase)) = self.rt.get(&id).and_then(|r| r.hand.from.clone().map(|f| (f, r.hand.phase))) {
            if phase != HandPhase::None {
                // stop waiting for the old agent's note: go with what is on disk
                if let Some(a) = self.rt.get_mut(&from) {
                    a.forget_requests_from(&id);
                }
                self.takeover_compose(&id, None);
                return json!({"ok": true});
            }
        }
        let empty = self.node(&id).is_none_or(|n| !self.has_work(n) && n.addenda.is_empty());
        let rt = &self.rt[&id];
        if rt.alive && empty && rt.inputs.is_empty() {
            self.toast("info", "This agent has no prompt to send. Write one on the card or use the message box.");
        }
        self.trigger(&id, true);
        json!({"ok": true})
    }

    pub(super) fn cmd_run(&mut self) -> Value {
        self.reset();
        let roots: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| {
                n.kind() == Kind::Agent
                    && !n.draft
                    && self.has_work(n)
                    && !self.flow.edges.iter().any(|e| e.to == n.id && !self.is_const_edge(e))
            })
            .map(|n| n.id.clone())
            .collect();
        if roots.is_empty() {
            self.toast("warn", "No starting agent has a prompt yet. Write one on a card first.");
        }
        for id in &roots {
            self.trigger(id, true);
        }
        json!({"ok": true, "started": roots.len()})
    }

    pub(super) fn cmd_reset(&mut self) -> Value {
        self.reset();
        json!({"ok": true})
    }

    pub(super) fn cmd_clear_inputs(&mut self, id: String) -> Value {
        if let Some(rt) = self.rt.get_mut(&id) {
            rt.inputs.clear();
            rt.ui = true;
        }
        json!({"ok": true})
    }
}
