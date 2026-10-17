//! A fresh agent takes over another one's work: the note, the facts, the start.

use super::*;

/// What the new agent is given: the task as first stated, the note, and the state on disk.
fn handover_doc(old: &str, agent: &str, task: &str, brief: Option<&str>, last: &str, facts: &str) -> String {
    let who = if agent.is_empty() { format!("\"{old}\"") } else { format!("\"{old}\" ({agent})") };
    let mut d = format!(
        "You are taking over unfinished work from {who}. You start with a fresh context; the files are exactly as it left them, in your working folder."
    );
    if !task.trim().is_empty() {
        d.push_str("\n\n### The original task\n");
        d.push_str(&head_chars(task.trim(), 3_000));
    }
    match brief {
        Some(b) => {
            d.push_str(&format!("\n\n### Handover note from {old}\n{}", b.trim()));
        }
        None => {
            d.push_str(&format!("\n\n### No handover note\n{old} could not write one. This is what it last reported:\n"));
            d.push_str(if last.trim().is_empty() { "(nothing)" } else { last.trim() });
        }
    }
    if !facts.trim().is_empty() {
        d.push_str("\n\n### State of the repository\n");
        d.push_str(facts.trim());
    }
    d
}

/// Branch, uncommitted files and the latest commits of a folder: what a newcomer needs first.
async fn repo_facts(dir: &Path) -> String {
    let probe = async {
        let (inside, _) = git_run(dir, &["rev-parse", "--is-inside-work-tree"]).await?;
        if !inside {
            return Ok::<String, String>(String::new());
        }
        let (_, b) = git_run(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        let (_, st) = git_run(dir, &["status", "--short", "--", ".", ":(exclude).weft"]).await?;
        let (_, lg) = git_run(dir, &["log", "-3", "--format=%h %s"]).await.unwrap_or((false, String::new()));
        let mut out = format!("branch: {}\n", b.trim());
        let lines: Vec<&str> = st.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            out.push_str("working tree: clean (everything is committed)\n");
        } else {
            out.push_str("uncommitted changes:\n");
            for l in lines.iter().take(40) {
                out.push_str("  ");
                out.push_str(l);
                out.push('\n');
            }
            if lines.len() > 40 {
                out.push_str(&format!("  \u{2026} and {} more\n", lines.len() - 40));
            }
        }
        if !lg.trim().is_empty() && !lg.starts_with("fatal") {
            out.push_str("latest commits:\n");
            for l in lg.lines() {
                out.push_str("  ");
                out.push_str(l);
                out.push('\n');
            }
        }
        Ok(out)
    };
    match tokio::time::timeout(Duration::from_secs(5), probe).await {
        Ok(Ok(s)) => s,
        _ => String::new(),
    }
}

impl Engine {
    pub(super) fn out_path(&self, id: &str) -> PathBuf {
        self.boot.dir.join(".weft").join("run").join(format!("{id}.out.md"))
    }

    /// `to` was just created to take over from `from`. Ask the old agent for a note when it can
    /// answer now, wait for it to finish its step when it is busy, or go without when it is gone.
    pub(super) fn takeover_begin(&mut self, to: &str, from: &str) {
        // a shell command cannot write a note: whatever it printed is all there is
        let askable = self.node(from).is_some_and(|n| n.kind() == Kind::Agent && n.agent != "sh" && n.finish != Finish::Exit);
        let (alive, busy) =
            self.rt.get(from).map_or((false, false), |r| (r.alive && askable, r.cur.is_some() || r.status == Status::Busy || r.attn));
        if let Some(rt) = self.rt.get_mut(to) {
            rt.hand.from = Some(from.to_string());
            rt.ui = true;
        }
        if self.rt.get(from).is_some_and(|r| r.limit.is_some()) {
            // at its usage limit it cannot write a note: what it said and what git says will do
            self.limit_hand_over(to, from);
            self.takeover_compose(to, None);
        } else if !alive {
            self.takeover_compose(to, None);
        } else if busy {
            if let Some(rt) = self.rt.get_mut(to) {
                rt.hand.phase = HandPhase::Settle;
            }
            self.set_status(to, Status::Waiting);
        } else {
            self.takeover_ask(to);
        }
    }

    pub(super) fn takeover_ask(&mut self, to: &str) {
        let Some(from) = self.rt.get(to).and_then(|r| r.hand.from.clone()) else { return };
        if !self.rt.get(&from).is_some_and(|r| r.alive) {
            return self.takeover_compose(to, None);
        }
        let exe = self.boot.exe.clone();
        let new_name = self.node(to).map(|n| n.name.clone()).unwrap_or_default();
        let text = words::handover_request(&exe, &new_name);
        self.enqueue(&from, text, format!("handover note for {new_name}"), Src::Reply(to.to_string()), true);
        if let Some(rt) = self.rt.get_mut(to) {
            rt.hand.phase = HandPhase::Asking;
            rt.ui = true;
        }
        self.set_status(to, Status::Waiting);
        self.state_dirty = true;
    }

    /// The old agent answered the request for a note.
    pub(super) fn takeover_response(&mut self, to: &str, from: &str, ok: bool, text: &str) {
        if self.rt.get(to).is_none_or(|r| r.hand.phase != HandPhase::Asking || r.hand.from.as_deref() != Some(from)) {
            return;
        }
        let note = text.trim();
        self.takeover_compose(to, if ok && !note.is_empty() { Some(note.to_string()) } else { None });
    }

    /// Gather what git itself knows (it is cheap and exact), then start the new agent.
    pub(super) fn takeover_compose(&mut self, to: &str, brief: Option<String>) {
        let Some(from) = self.rt.get(to).and_then(|r| r.hand.from.clone()) else { return };
        if let Some(rt) = self.rt.get_mut(to) {
            rt.hand.phase = HandPhase::None;
        }
        let dir = self
            .rt
            .get(&from)
            .and_then(|r| r.worktree.clone())
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.node(&from).map_or_else(|| self.boot.dir.clone(), |n| self.node_dir(n)));
        let (tx, to, from2) = (self.tx.clone(), to.to_string(), from);
        tokio::spawn(async move {
            let facts = repo_facts(&dir).await;
            let _ = tx.send(Ev::Hand { to, from: from2, brief, facts });
        });
    }

    pub(super) fn on_hand(&mut self, to: &str, from: &str, brief: Option<String>, facts: String) {
        if self.node(to).is_none() {
            return;
        }
        let chrome = self.cfg.chrome.clone();
        let (old_name, old_agent, task) = match self.node(from) {
            Some(n) => (n.name.clone(), n.agent.clone(), n.prompt.clone()),
            None => ("the previous agent".to_string(), String::new(), String::new()),
        };
        // without a note, show what the old agent last said so the new one is not blind
        let last = match (&brief, self.rt.get_mut(from)) {
            (None, Some(r)) => match r.output.clone() {
                Some(o) if !o.trim().is_empty() => tail_bytes(o.trim(), 3_000).to_string(),
                _ => term::transcript_clean(&mut r.parser, 60, &chrome),
            },
            _ => String::new(),
        };
        let doc = handover_doc(&old_name, &old_agent, &task, brief.as_deref(), &last, &facts);
        {
            let Some(rt) = self.rt.get_mut(to) else { return };
            rt.hand.phase = HandPhase::None;
            rt.inputs.retain(|(eid, _)| eid != "hand");
            rt.inputs
                .push(("hand".into(), Input { from: from.to_string(), name: old_name, ok: true, text: doc, kind: InputKind::Handover }));
            rt.ui = true;
        }
        self.trigger(to, true);
    }

    pub(super) fn cmd_takeover(&mut self, from: String, node: Value, text: String) -> Value {
        let from = self.resolve_host(&from);
        let Some(src) = self.node(&from).cloned() else {
            return json!({"ok": false, "error": "no such agent"});
        };
        if src.kind() != Kind::Agent {
            return json!({"ok": false, "error": "only an agent can hand its work over"});
        }
        let instruction = if text.trim().is_empty() {
            format!("Continue the work of {}: finish what is still open, verify it, then report.", src.name)
        } else {
            text.trim().to_string()
        };
        let mut v = if node.is_object() { node } else { json!({}) };
        {
            // same files: its worktree if it has one, else its folder
            let cwd =
                self.rt.get(&from).and_then(|r| r.worktree.clone()).filter(|p| Path::new(p).is_dir()).unwrap_or_else(|| src.cwd.clone());
            let o = v.as_object_mut().unwrap();
            o.entry("agent").or_insert(json!(src.agent));
            o.insert("prompt".into(), json!(instruction));
            o.insert("draft".into(), json!(false));
            o.insert("takeover".into(), json!(from));
            o.insert("worktree".into(), json!(false));
            if !cwd.is_empty() {
                o.insert("cwd".into(), json!(cwd));
            }
        }
        let r = self.cmd_node_add(v, None, false);
        let Some(new_id) = r["id"].as_str().map(String::from) else { return r };
        self.takeover_begin(&new_id, &from);
        json!({"ok": true, "id": new_id})
    }
}
