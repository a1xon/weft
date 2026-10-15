//! The flow graph as the engine edits it: ids, names, links.

use super::*;

impl Engine {
    /// Put a prompt in `agent`'s queue (at the front: before what is waiting) and say it changed.
    /// `shown` is what people see in the queue (the prompt without the boilerplate).
    pub(super) fn enqueue(&mut self, agent: &str, text: String, shown: String, src: Src, front: bool) -> u64 {
        let id = self.next_q;
        self.next_q += 1;
        if let Some(rt) = self.rt.get_mut(agent) {
            let item = QItem { id, text, shown, src };
            if front {
                rt.queue.push_front(item);
            } else {
                rt.queue.push_back(item);
            }
            rt.ui = true;
        }
        self.state_dirty = true;
        id
    }

    /// The folder a node works in: its own `cwd` (`~` and relative paths understood), else the project's.
    pub(super) fn node_dir(&self, n: &Node) -> PathBuf {
        if n.cwd.trim().is_empty() {
            return self.boot.dir.clone();
        }
        let p = PathBuf::from(n.cwd.replacen('~', &home().to_string_lossy(), 1));
        if p.is_absolute() {
            p
        } else {
            self.boot.dir.join(p)
        }
    }

    pub(super) fn node(&self, id: &str) -> Option<&Node> {
        self.flow.nodes.iter().find(|n| n.id == id)
    }

    pub(super) fn node_by_name(&self, name: &str) -> Option<&Node> {
        self.flow.nodes.iter().find(|n| n.name == name)
    }

    pub(super) fn next_id(&self, prefix: &str, taken: impl Iterator<Item = String>) -> String {
        let max = taken.filter_map(|i| i.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0);
        format!("{prefix}{}", max + 1)
    }

    pub(super) fn unique_name(&self, base: &str, except: Option<&str>) -> String {
        let taken = |n: &str| self.flow.nodes.iter().any(|m| m.name == n && Some(m.id.as_str()) != except);
        if !taken(base) {
            return base.to_string();
        }
        let mut k = 2;
        while taken(&format!("{base} {k}")) {
            k += 1;
        }
        format!("{base} {k}")
    }

    /// Add (or update) a link. A result that already exists is delivered at once,
    /// like `.then()` on a resolved promise.
    pub(super) fn add_edge(&mut self, from: &str, to: &str, on: On) -> Result<(), String> {
        if from == to || self.node(from).is_none() || self.node(to).is_none() {
            return Err("invalid link".into());
        }
        if self.node(to).is_some_and(|n| n.kind() == Kind::Note) {
            return Err("a note has no input".into());
        }
        // a commit is one agent's work: another agent gets its own commit, and the commits meet in a push
        if self.node(to).is_some_and(|n| n.kind() == Kind::Git && n.git_op == GitOp::Commit)
            && self.flow.edges.iter().any(|e| e.to == to && e.from != from)
        {
            return Err(
                "a commit takes the work of one agent: give the other agent its own commit and link both commits to one push".into()
            );
        }
        let eid = if let Some(e) = self.flow.edges.iter_mut().find(|e| e.from == from && e.to == to) {
            e.on = on;
            e.id.clone()
        } else {
            let id = self.next_id("e", self.taken_ids(true).into_iter());
            self.flow.edges.push(Edge { id: id.clone(), from: from.into(), to: to.into(), on });
            id
        };
        // a queued prompt continues the session of whatever it was just linked after
        if let Some(agent) = self.agent_behind(from) {
            self.rehost(to, &agent);
        }
        self.flow_changed();
        // an agent that has a result hands it over, also while it is busy answering a queued prompt
        // (that answer goes to its step; its own result stays what it was)
        let settled = self.rt.get(from).and_then(|r| {
            if r.finished || r.reply_to.is_some() {
                r.output.clone().map(|o| (o, r.ok.unwrap_or(true)))
            } else {
                None
            }
        });
        let arrived = self.rt.get(to).is_some_and(|r| r.inputs.iter().any(|(i, _)| *i == eid));
        if let (Some((out, ok)), false) = (settled, arrived) {
            if on.matches(ok) {
                let name = self.node(from).map(|n| n.name.clone()).unwrap_or_default();
                let e = self.flow.edges.iter().find(|e| e.id == eid).cloned().unwrap();
                if let Some(r) = self.rt.get_mut(from) {
                    r.fresh = false;
                    r.propagated = true;
                }
                self.deliver_input(&e, from, &name, ok, &out);
            }
        }
        Ok(())
    }

    /// The agent whose session goes on after `from`: an agent, the agent a queued prompt is asked
    /// of, or the agent behind a gate. None after a git step, a note or a command: no session.
    pub(super) fn agent_behind(&self, from: &str) -> Option<String> {
        let mut at = from.to_string();
        let mut seen = std::collections::HashSet::new();
        while seen.insert(at.clone()) {
            let n = self.node(&at)?;
            match n.kind() {
                Kind::Agent if n.finish == Finish::Idle => return Some(at),
                Kind::Step => at = n.host.clone(),
                k if k.is_gate() => at = self.flow.edges.iter().find(|e| e.to == at)?.from.clone(),
                _ => return None,
            }
        }
        None
    }

    /// A queued prompt is asked of the agent it is linked after, and so is the one behind it: when a
    /// step moves to another agent the steps (and gates) after it go with it. `host` is a copy made
    /// when a link was drawn; this is where it is kept true to the graph. A step with several links
    /// in keeps its agent while one of them still leads to it (the link drawn last decides, see
    /// `add_edge`); one with none keeps what it had. Returns how many steps moved.
    pub(super) fn reconcile_hosts(&mut self) -> usize {
        let mut moved = 0;
        // a chain of steps is settled front to back: one pass per link of the longest chain
        for _ in 0..self.flow.nodes.len().max(1) {
            let steps: Vec<String> = self.flow.nodes.iter().filter(|n| n.kind() == Kind::Step).map(|n| n.id.clone()).collect();
            let mut changed = false;
            for step in steps {
                let host = self.node(&step).map(|n| n.host.clone()).unwrap_or_default();
                let behind: Vec<String> =
                    self.flow.edges.iter().filter(|e| e.to == step).filter_map(|e| self.agent_behind(&e.from)).collect();
                if let Some(agent) = behind.first().filter(|_| !behind.contains(&host)) {
                    let agent = agent.clone();
                    self.rehost(&step, &agent);
                    changed = true;
                    moved += 1;
                }
            }
            if !changed {
                break;
            }
        }
        moved
    }

    /// A queued prompt is asked of the agent it is linked after: rewired, it follows.
    fn rehost(&mut self, step: &str, agent: &str) {
        let who = self.node(agent).map_or(agent.to_string(), |a| a.name.clone());
        let Some(old) = self.node(step).filter(|n| n.kind() == Kind::Step).map(|n| n.host.clone()) else { return };
        if old == agent {
            return;
        }
        // (a new card made from a gate names the gate: no news that it ends up at its agent)
        let moved = self.node(&old).is_some_and(|h| h.kind() == Kind::Agent);
        let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == step) else { return };
        n.host = agent.to_string();
        let msg = format!("\"{}\" is now asked of {who}.", n.name);
        if moved {
            self.toast("info", &msg);
        }
    }

    fn rename_refs(&mut self, old: &str, new: &str) {
        let (a, b) = (format!("{{{{from:{old}}}}}"), format!("{{{{from:{new}}}}}"));
        for n in &mut self.flow.nodes {
            if n.prompt.contains(&a) {
                n.prompt = n.prompt.replace(&a, &b);
            }
        }
    }

    pub(super) fn has_work(&self, n: &Node) -> bool {
        !n.prompt.trim().is_empty() || (n.finish == Finish::Exit && !n.cmd.trim().is_empty())
    }

    pub(super) fn flow_changed(&mut self) {
        self.reconcile_hosts();
        save_json(&self.flow_path, &self.flow);
        let m = json!({"t": "flow", "nodes": self.flow.nodes, "edges": self.flow.edges});
        self.bcast(&m);
        for rt in self.rt.values_mut() {
            rt.ui = true;
            rt.last_msg.clear();
        }
    }

    pub(super) fn cmd_node_add(&mut self, node: Value, from: Option<String>, run: bool) -> Value {
        let id = self.next_id("n", self.taken_ids(false).into_iter());
        let mut v = node;
        if let Some(o) = v.as_object_mut() {
            o.insert("id".into(), json!(id));
        }
        let n_json = v.clone();
        let has = |k: &str| v.get(k).is_some();
        let (has_finish, has_add, has_name) = (has("finish"), has("addenda"), has("name"));
        let mut n: Node = match serde_json::from_value(v) {
            Ok(n) => n,
            Err(e) => return json!({"ok": false, "error": e.to_string()}),
        };
        let kind = n.kind();
        if !has_finish {
            n.finish = if n.agent == "sh" || (n.agent.is_empty() && !n.cmd.is_empty()) { Finish::Exit } else { Finish::Idle };
        }
        if !has_add {
            n.addenda = if n.finish == Finish::Exit || kind != Kind::Agent {
                Vec::new()
            } else {
                self.cfg.library.iter().filter(|s| s.kind == SnippetKind::Addendum && s.default_on).map(|s| s.id.clone()).collect()
            };
        }
        if kind == Kind::Git && n_json.get("draft").is_none() {
            // a push is outward-facing: nothing happens until the user arms the node.
            // A commit only asks the agents to commit what they did: it starts at once.
            n.draft = n.git_op != GitOp::Commit;
        }
        if !has_name || n.name.trim().is_empty() {
            let (src, fallback) = match kind {
                Kind::Note => (n.prompt.clone(), "note".to_string()),
                Kind::Gate => (String::new(), "approve".to_string()),
                Kind::Shot => (String::new(), "screenshot".to_string()),
                Kind::Tests => (String::new(), "tests".to_string()),
                Kind::Git => (String::new(), n.git_op.label().to_string()),
                Kind::Step => (n.prompt.clone(), "prompt".to_string()),
                Kind::Agent => (
                    if n.finish == Finish::Exit && !n.cmd.trim().is_empty() { n.cmd.clone() } else { n.prompt.clone() },
                    if n.agent.is_empty() { "Agent".to_string() } else { n.agent.clone() },
                ),
            };
            let as_cmd = kind == Kind::Agent && n.finish == Finish::Exit && !n.cmd.trim().is_empty();
            let t = if as_cmd { title_cmd(&src, &fallback) } else { title_from(&src, &fallback) };
            n.name = self.unique_name(&t, None);
            n.auto_name = kind == Kind::Agent || kind == Kind::Note;
        } else {
            n.name = self.unique_name(n.name.trim(), None);
        }
        self.rt.insert(n.id.clone(), Rt::new(self.cfg.cols, self.cfg.rows));
        let nid = n.id.clone();
        let is_note = n.kind() == Kind::Note;
        let is_draft = n.draft;
        self.flow.nodes.push(n);
        if is_note {
            self.refresh_note(&nid);
        }
        self.flow_changed();
        if let Some(f) = from {
            if let Err(e) = self.add_edge(&f, &nid, On::Done) {
                return json!({"ok": false, "error": e, "id": nid});
            }
        }
        let incoming = self.flow.edges.iter().any(|e| e.to == nid);
        if run && !incoming && !is_draft {
            self.trigger(&nid, true);
        }
        json!({"ok": true, "id": nid})
    }

    pub(super) fn cmd_node_set(&mut self, id: String, patch: Value) -> Value {
        let Some(pos) = self.flow.nodes.iter().position(|n| n.id == id) else {
            return json!({"ok": false, "error": "no such node"});
        };
        let old_name = self.flow.nodes[pos].name.clone();
        let mut cur = serde_json::to_value(&self.flow.nodes[pos]).unwrap();
        let renamed_by_user = patch.get("name").is_some();
        if let (Some(c), Some(p)) = (cur.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                if k != "id" {
                    c.insert(k.clone(), v.clone());
                }
            }
            if renamed_by_user {
                c.insert("auto_name".into(), json!(false));
            }
        }
        match serde_json::from_value::<Node>(cur) {
            Ok(mut n) => {
                if n.kind() == Kind::Git
                    && n.git_op == GitOp::Commit
                    && self.flow.nodes[pos].git_op != GitOp::Commit
                    && self.flow.edges.iter().filter(|e| e.to == id).count() > 1
                {
                    return json!({"ok": false, "error": "a commit takes the work of one agent: this one has several links in"});
                }
                if n.auto_name && (patch.get("prompt").is_some() || patch.get("cmd").is_some()) && !renamed_by_user {
                    let fallback = if n.agent.is_empty() { "Agent".to_string() } else { n.agent.clone() };
                    let src = if n.finish == Finish::Exit && !n.cmd.trim().is_empty() { n.cmd.clone() } else { n.prompt.clone() };
                    let as_cmd = n.finish == Finish::Exit && !n.cmd.trim().is_empty();
                    let t = if as_cmd { title_cmd(&src, &fallback) } else { title_from(&src, &fallback) };
                    n.name = self.unique_name(&t, Some(&id));
                } else if renamed_by_user {
                    n.name = self.unique_name(n.name.trim(), Some(&id));
                }
                let new_name = n.name.clone();
                let note_edit = n.kind() == Kind::Note && patch.get("prompt").is_some();
                self.flow.nodes[pos] = n;
                if new_name != old_name {
                    self.rename_refs(&old_name, &new_name);
                }
                if note_edit {
                    self.refresh_note(&id);
                }
                self.flow_changed();
                json!({"ok": true, "name": new_name})
            }
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        }
    }

    pub(super) fn cmd_node_move(&mut self, moves: Vec<Mv>) -> Value {
        for m in moves {
            if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == m.id) {
                n.x = m.x;
                n.y = m.y;
            }
        }
        self.flow_changed();
        json!({"ok": true})
    }

    pub(super) fn cmd_node_del(&mut self, id: String) -> Value {
        self.drop_node(&id, true);
        self.flow_changed();
        json!({"ok": true})
    }

    /// Take a card off the canvas: its process ends, nobody waits on it any more, its links go.
    /// `worktree`: its git worktree goes too (filed work keeps it, so it can come back to it).
    pub(super) fn drop_node(&mut self, id: &str, worktree: bool) {
        // an agent that took over works in this agent's worktree: it must not vanish under it
        let shared = self.worktree_shared(id);
        // a note somebody asked this agent for, or an agent waiting on it, must not hang
        for r in self.rt.values_mut() {
            r.forget_requests_from(id);
        }
        let waiting: Vec<String> = self
            .rt
            .iter()
            .filter(|(i, r)| *i != id && r.hand.phase != HandPhase::None && r.hand.from.as_deref() == Some(id))
            .map(|(i, _)| i.clone())
            .collect();
        for w in waiting {
            self.takeover_compose(&w, None);
        }
        self.kill(id);
        if let Some(wt) = self.rt.get(id).and_then(|r| r.worktree.clone()).filter(|_| worktree && !shared) {
            let dir = self.boot.dir.clone();
            tokio::spawn(async move {
                let _ = tokio::process::Command::new("git").args(["-C"]).arg(dir).args(["worktree", "remove", &wt]).output().await;
            });
        }
        self.rt.remove(id);
        self.flow.nodes.retain(|n| n.id != id);
        self.flow.edges.retain(|e| e.from != id && e.to != id);
        let live: Vec<String> = self.flow.edges.iter().map(|e| e.id.clone()).collect();
        for rt in self.rt.values_mut() {
            rt.inputs.retain(|(eid, _)| live.contains(eid));
        }
    }

    pub(super) fn cmd_edge_add(&mut self, from: String, to: String, on: On) -> Value {
        match self.add_edge(&from, &to, on) {
            Ok(()) => json!({"ok": true}),
            Err(e) => json!({"ok": false, "error": e}),
        }
    }

    pub(super) fn cmd_edge_set(&mut self, id: String, on: On) -> Value {
        let Some(e) = self.flow.edges.iter().find(|e| e.id == id).cloned() else {
            return json!({"ok": false, "error": "no such link"});
        };
        match self.add_edge(&e.from, &e.to, on) {
            Ok(()) => json!({"ok": true}),
            Err(er) => json!({"ok": false, "error": er}),
        }
    }

    pub(super) fn cmd_edge_del(&mut self, id: String) -> Value {
        let to = self.flow.edges.iter().find(|e| e.id == id).map(|e| e.to.clone());
        self.flow.edges.retain(|e| e.id != id);
        // a queued prompt whose agent is no longer behind it follows the link that is left
        if let Some(step) = to.filter(|t| self.node(t).is_some_and(|n| n.kind() == Kind::Step)) {
            let host = self.node(&step).map(|n| n.host.clone()).unwrap_or_default();
            let behind: Vec<String> =
                self.flow.edges.iter().filter(|e| e.to == step).filter_map(|e| self.agent_behind(&e.from)).collect();
            if let Some(agent) = behind.first().filter(|_| !behind.contains(&host)) {
                let agent = agent.clone();
                self.rehost(&step, &agent);
            }
        }
        for rt in self.rt.values_mut() {
            rt.inputs.retain(|(eid, _)| *eid != id);
            rt.ui = true;
        }
        self.flow_changed();
        json!({"ok": true})
    }

    pub(super) fn cmd_handover(&mut self, from: String, to: String) -> Value {
        let Some((out, ok)) = self.rt.get(&from).and_then(|r| r.output.clone().map(|o| (o, r.ok.unwrap_or(true)))) else {
            return json!({"ok": false, "error": "that agent has no result yet"});
        };
        if let Err(e) = self.add_edge(&from, &to, On::Done) {
            return json!({"ok": false, "error": e});
        }
        // an existing link may already have delivered: hand over again, explicitly
        let arrived = self
            .flow
            .edges
            .iter()
            .find(|e| e.from == from && e.to == to)
            .map(|e| self.rt.get(&to).is_some_and(|r| r.inputs.iter().any(|(i, _)| *i == e.id)));
        let consumed_already = self.rt.get(&to).is_some_and(|r| r.runs > 0 || !r.queue.is_empty());
        if arrived != Some(true) && consumed_already {
            if let Some(e) = self.flow.edges.iter().find(|e| e.from == from && e.to == to).cloned() {
                let name = self.node(&from).map(|n| n.name.clone()).unwrap_or_default();
                self.deliver_input(&e, &from, &name, ok, &out);
            }
        }
        if let Some(r) = self.rt.get_mut(&from) {
            r.fresh = false;
            r.propagated = true;
            r.ui = true;
        }
        json!({"ok": true})
    }
}
