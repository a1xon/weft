//! Card names. A card with a prompt is named by the smallest model of its own agent's CLI at the
//! lowest reasoning (`title_cmd`: claude haiku, ...); until that answers, and when it cannot, the
//! first words of the prompt stand in. A git step is named after the work linked into it
//! ("commit: Fix slugify", "push: Fix slugify + Wrap text"). A name you give yourself stays.

use super::*;

/// How long a prompt must rest before it is named: a card's prompt is patched while it is typed.
const SETTLE: Duration = Duration::from_millis(1200);
/// A naming call that has not answered by then is given up.
const TIMEOUT: Duration = Duration::from_secs(90);
/// What a name may take up on a card.
const MAX_CHARS: usize = 40;

/// The bare name of a git step, as one gets it with nothing linked in ("commit"), or as older
/// builds numbered them ("commit 2").
pub(super) fn is_plain_git_name(name: &str, op: GitOp) -> bool {
    let l = op.label();
    name == l || name.strip_prefix(l).and_then(|r| r.strip_prefix(' ')).is_some_and(|k| k.parse::<u32>().is_ok())
}

/// A git step named after the tasks of the agents whose work it carries.
pub(super) fn git_title(op: GitOp, tasks: &[String]) -> String {
    let verb = if op == GitOp::Pr { "PR" } else { op.label() };
    match tasks {
        [] => op.label().to_string(),
        [a] => format!("{verb}: {a}"),
        // both when they fit on the card, else the first and how many more
        [a, b] if a.chars().count() + b.chars().count() <= 44 => format!("{verb}: {a} + {b}"),
        [a, rest @ ..] => format!("{verb}: {a} + {} more", rest.len()),
    }
}

/// A model's answer as a card name: its last line, without quotes, labels or a final period.
pub(super) fn clean_title(out: &str) -> Option<String> {
    let line = out.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    // ("Title:", "Card name:", "**Name:**")
    let label = |l: &str| {
        let words: Vec<&str> = l.split_whitespace().collect();
        let last = words.last().map(|w| w.trim_matches(['*', '_']).to_lowercase()).unwrap_or_default();
        words.len() <= 3 && (last == "name" || last == "title")
    };
    let line = match line.split_once(':') {
        Some((l, rest)) if label(l) => rest,
        _ => line,
    };
    let t = line.trim_matches(|c: char| c.is_whitespace() || "\"'`*_#.\u{201C}\u{201D}".contains(c));
    let n = t.chars().count();
    // a sentence is not a name (an error, or the model talking)
    if n == 0 || n > 60 {
        return None;
    }
    if n <= MAX_CHARS {
        return Some(t.to_string());
    }
    // whole words only
    let mut out = String::new();
    for w in t.split_whitespace() {
        if out.chars().count() + 1 + w.chars().count() > MAX_CHARS {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    (!out.is_empty()).then_some(out)
}

/// What a card is named after: its prompt. A command is its own name.
fn title_source(n: &Node) -> Option<String> {
    if !n.auto_name || n.runs_command() {
        return None;
    }
    match n.kind() {
        Kind::Agent | Kind::Step | Kind::Note => Some(n.prompt.trim().to_string()).filter(|p| !p.is_empty()),
        _ => None,
    }
}

/// `sh -c <cmd>` outside the project with the request in `$WEFT_TITLE_REQ`; only stdout counts
/// (Codex reports its progress on stderr).
async fn run_title(cmd: &str, req: &str) -> Option<String> {
    let mut c = tokio::process::Command::new("sh");
    c.arg("-c")
        .arg(cmd)
        .current_dir(std::env::temp_dir())
        .env("WEFT_TITLE_REQ", req)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let o = tokio::time::timeout(TIMEOUT, c.output()).await.ok()?.ok()?;
    if !o.status.success() {
        return None;
    }
    clean_title(&String::from_utf8_lossy(&o.stdout))
}

impl Engine {
    /// The CLI that names a card: the card's own agent, a queued prompt's agent, else (a note) the
    /// agent you used last.
    fn title_cmd(&self, n: &Node) -> Option<String> {
        let a = match n.kind() {
            Kind::Agent => n.agent.clone(),
            Kind::Step => self.node(&n.host).map(|h| h.agent.clone()).unwrap_or_default(),
            _ => String::new(),
        };
        let a = if a.is_empty() { self.cfg.prefs.get("agent").and_then(Value::as_str).unwrap_or("claude").to_string() } else { a };
        self.cfg.agents.get(&a).map(|c| c.title_cmd.trim().to_string()).filter(|c| !c.is_empty())
    }

    /// Have the card named once its prompt has rested.
    pub(super) fn ask_title(&self, id: &str) {
        if !self.cfg.titles {
            return;
        }
        let Some(text) = self.node(id).and_then(title_source) else { return };
        let (tx, id) = (self.tx.clone(), id.to_string());
        tokio::spawn(async move {
            tokio::time::sleep(SETTLE).await;
            let _ = tx.send(Ev::TitleDue { id, text });
        });
    }

    /// The prompt rested: ask the small model, unless it changed meanwhile (a newer ask is on its way).
    pub(super) fn title_due(&mut self, id: &str, text: String) {
        let Some(n) = self.node(id) else { return };
        if title_source(n).as_deref() != Some(text.as_str()) {
            return;
        }
        let Some(cmd) = self.title_cmd(n) else { return };
        let (tx, id) = (self.tx.clone(), id.to_string());
        tokio::spawn(async move {
            let title = run_title(&cmd, &words::title_request(&text)).await;
            let _ = tx.send(Ev::Title { id, text, title });
        });
    }

    /// The name came back: it applies while the card still has that prompt and you did not name it.
    pub(super) fn on_title(&mut self, id: &str, text: &str, title: Option<String>) {
        let Some(t) = title else { return };
        let Some(pos) = self.flow.nodes.iter().position(|n| n.id == id) else { return };
        if title_source(&self.flow.nodes[pos]).as_deref() != Some(text) {
            return;
        }
        let new = self.unique_name(&t, Some(id));
        if new == self.flow.nodes[pos].name {
            return;
        }
        let old = std::mem::replace(&mut self.flow.nodes[pos].name, new.clone());
        self.rename_refs(&old, &new);
        self.flow_changed();
    }

    /// The names of the agents whose work reaches this git step, through gates, queued prompts
    /// and the git steps before it, in link order.
    pub(super) fn git_tasks(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut todo = VecDeque::from([id.to_string()]);
        while let Some(at) = todo.pop_front() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for e in self.flow.edges.iter().filter(|e| e.to == at) {
                match self.node(&e.from) {
                    Some(n) if n.kind() == Kind::Git => todo.push_back(n.id.clone()),
                    Some(_) => {
                        let who = self.agent_behind(&e.from).unwrap_or_else(|| e.from.clone());
                        if let Some(n) = self.node(&who).filter(|n| !out.contains(&n.name)) {
                            out.push(n.name.clone());
                        }
                    }
                    None => {}
                }
            }
        }
        out
    }

    /// Name every git step that was not named by hand after what it carries. Returns how many changed.
    pub(super) fn retitle_git(&mut self) -> usize {
        let want: Vec<(usize, String)> = self
            .flow
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind() == Kind::Git && n.auto_name)
            .map(|(i, n)| (i, git_title(n.git_op, &self.git_tasks(&n.id))))
            .filter(|(i, t)| self.flow.nodes[*i].name != *t)
            .collect();
        let mut changed = 0;
        for (i, t) in want {
            let id = self.flow.nodes[i].id.clone();
            let new = self.unique_name(&t, Some(&id));
            if new != self.flow.nodes[i].name {
                let old = std::mem::replace(&mut self.flow.nodes[i].name, new.clone());
                self.rename_refs(&old, &new);
                changed += 1;
            }
        }
        changed
    }
}
