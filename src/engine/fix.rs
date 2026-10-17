//! What failed, fixed: one click on the top bar's *failed* (or "fix it" on a failed card) asks a
//! fresh agent, a reasoning level above the agents that failed, to find out why, fix the cause and
//! run the cards again (`weft retry`), so the flow goes on from where it stopped. The fixer is a
//! card like any other; it knows the cards it fixes (`Node.fixes`), and asking again while it works
//! shows it instead of starting a second one.

use super::*;

/// How much of what a failed card was asked and said goes into the brief (the rest is one `weft
/// read` / `weft result` away); shared out when several cards failed.
const ASKED_MAX: usize = 2_500;
const SAID_MAX: usize = 4_000;

impl Engine {
    /// A fresh agent fixes the cards that failed (`ids`; none: every failed card on the board).
    pub(super) fn cmd_fix(&mut self, ids: Vec<String>) -> Value {
        let blocked = self.blocked_map();
        let failed: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() != Kind::Note && (ids.is_empty() || ids.contains(&n.id)))
            // (one that is about to run again has failed no more)
            .filter(|n| self.eff_status(&n.id, &blocked) == Status::Failed && self.rt[&n.id].queue.is_empty())
            .map(|n| n.id.clone())
            .collect();
        if failed.is_empty() {
            return json!({"ok": false, "error": "nothing has failed"});
        }
        // the ones an agent already works on are its own: that agent is shown
        let mut fixing = None;
        let todo: Vec<String> = failed
            .into_iter()
            .filter(|f| match self.fixer_of(f) {
                Some(x) => {
                    fixing.get_or_insert(x);
                    false
                }
                None => true,
            })
            .collect();
        if todo.is_empty() {
            return json!({"ok": true, "id": fixing.unwrap_or_default(), "again": true});
        }
        // (the person's choices may have changed on another board since this one read them)
        self.sync_user_config();
        let Some((agent, model, effort)) = self.fixer_agent(&todo) else {
            return json!({"ok": false, "error": "no agent is set up on this board"});
        };
        let share = todo.len() + 2;
        let cards: Vec<words::Failed> = todo.iter().filter_map(|id| self.failed_card(id, share, &blocked)).collect();
        let name = |id: &str| self.node(id).map(|n| n.name.clone()).unwrap_or_default();
        let title = match todo.len() {
            1 => format!("fix: {}", name(&todo[0])),
            n => format!("fix: {} + {} more", name(&todo[0]), n - 1),
        };
        let what = if todo.len() == 1 { format!("{} \"{}\"", todo[0], name(&todo[0])) } else { todo.join(", ") };
        let ac = &self.cfg.agents[&agent];
        let mut node = json!({"agent": agent, "name": title, "prompt": words::fix_request(&self.boot.exe, &cards),
                              "addenda": self.chosen_addenda(), "fixes": todo});
        if !ac.model_arg.is_empty() {
            node["model"] = json!(model);
        }
        if !ac.effort_arg.is_empty() {
            node["effort"] = json!(effort);
        }
        if let (false, Some(y)) = (ac.yolo_arg.is_empty(), self.cfg.prefs["yolo"].as_bool()) {
            node["yolo"] = json!(y);
        }
        // it works where the first of them did (in its worktree, when it had one of its own)
        let dir = todo
            .iter()
            .find_map(|i| self.agent_upstream(i).and_then(|a| self.rt.get(&a)?.worktree.clone()).filter(|w| Path::new(w).is_dir()))
            .map(PathBuf::from)
            .or_else(|| self.node(&todo[0]).map(|n| self.node_dir(n)));
        if let Some(d) = dir.filter(|d| *d != self.boot.dir) {
            node["cwd"] = json!(d.to_string_lossy());
        }
        // under the cards it looks into
        let under: Vec<&Node> = todo.iter().filter_map(|i| self.node(i)).collect();
        let x = under.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let y = under.iter().map(|n| n.y + super::marks::card_h(n)).fold(f64::NEG_INFINITY, f64::max) + 120.0;
        let (x, y) = self.free_spot(x, y);
        node["x"] = json!(x);
        node["y"] = json!(y);
        let r = self.cmd_node_add(node, None, true);
        if let Some(id) = r["id"].as_str() {
            self.toast("info", &format!("{id} looks into what failed: {what}"));
        }
        r
    }

    /// The agent at work on fixing `id`, if there is one.
    pub(super) fn fixer_of(&self, id: &str) -> Option<String> {
        self.flow
            .nodes
            .iter()
            .filter(|n| n.fixes.iter().any(|f| f == id))
            .find(|n| {
                self.rt
                    .get(&n.id)
                    .is_some_and(|r| !r.finished && !matches!(r.status, Status::Done | Status::Failed | Status::Stopped | Status::Exited))
            })
            .map(|n| n.id.clone())
    }

    /// Who fixes: the CLI most of the failed agents ran (the person's own on a tie, or when no agent
    /// was behind them), their model, and a reasoning level above the highest of theirs.
    fn fixer_agent(&self, todo: &[String]) -> Option<(String, String, String)> {
        let prefs = &self.cfg.prefs;
        let usable = |a: &str| self.cfg.agents.contains_key(a) && a != "sh";
        let behind: Vec<&Node> = todo
            .iter()
            .filter_map(|i| self.agent_upstream(i))
            .filter_map(|a| self.node(&a))
            .filter(|n| usable(&n.agent) && n.finish == Finish::Idle)
            .collect();
        let mine = prefs["agent"].as_str().unwrap_or("");
        let count = |a: &str| behind.iter().filter(|n| n.agent == a).count();
        let most = behind.iter().map(|n| count(&n.agent)).max().unwrap_or(0);
        let agent = if most > 0 && count(mine) == most {
            Some(mine.to_string())
        } else {
            behind.iter().find(|n| count(&n.agent) == most).map(|n| n.agent.clone())
        };
        let agent = agent
            .or_else(|| [mine, "claude"].into_iter().find(|a| usable(a)).map(str::to_string))
            .or_else(|| self.cfg.agents.keys().find(|a| usable(a)).cloned())?;
        let ac = &self.cfg.agents[&agent];
        let rank = |e: &str| ac.efforts.iter().position(|l| l == e);
        let medium = rank("medium").unwrap_or(0);
        // the one of them that thought hardest sets the bar (the CLI's own default counts as medium)
        let hardest = behind.iter().filter(|n| n.agent == agent).max_by_key(|n| rank(&n.effort).unwrap_or(medium));
        let model = hardest
            .map(|n| n.model.clone())
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| prefs["model"][agent.as_str()].as_str().unwrap_or("").to_string());
        let effort = match hardest.map(|n| rank(&n.effort).unwrap_or(medium)) {
            _ if ac.effort_arg.is_empty() || ac.efforts.is_empty() || !effort_applies(ac, &model) => String::new(),
            had => ac.efforts[(had.unwrap_or(medium) + 1).max(rank("high").unwrap_or(0)).min(ac.efforts.len() - 1)].clone(),
        };
        Some((agent, model, effort))
    }

    /// What the fixer is told about one card that failed.
    fn failed_card(&self, id: &str, share: usize, blocked: &HashMap<String, String>) -> Option<words::Failed> {
        let n = self.node(id)?;
        let rt = self.rt.get(id)?;
        let label = |i: &str| {
            self.node(i).map(|x| {
                let st = json!(self.eff_status(i, blocked));
                format!("{} \"{}\" ({})", x.id, x.name, st.as_str().unwrap_or(""))
            })
        };
        let what = match n.kind() {
            Kind::Agent if n.deploy == Some(DeployAt::Pushed) => format!(
                "a deploy step running `{}` on what was pushed (a clean checkout of the pushed commit): a fix counts only once it is committed and pushed, then `weft retry` it",
                n.cmd.trim()
            ),
            Kind::Agent if n.finish == Finish::Exit => format!("the command `{}`", n.cmd.trim()),
            Kind::Agent => [n.agent.as_str(), n.model.trim(), n.effort.trim()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" \u{b7} "),
            Kind::Step => format!("a queued prompt, answered by {}", label(&n.host).unwrap_or_else(|| "an agent that is gone".into())),
            Kind::Tests if n.cmd.trim().is_empty() => "a tests gate".into(),
            Kind::Tests => format!("a tests gate running `{}`", n.cmd.trim()),
            Kind::Shot => "a screenshot gate".into(),
            Kind::Gate => "an approval gate".into(),
            Kind::Git => format!("a git step ({})", n.git_op.label()),
            Kind::Note => "a note".into(),
        };
        let asked = match n.kind() {
            Kind::Agent => Some(rt.asked.trim()).filter(|a| !a.is_empty()).unwrap_or(n.prompt.trim()),
            Kind::Step => n.prompt.trim(),
            k if k.is_gate() => n.subject.trim(),
            _ => "",
        };
        let mut said = rt.output.clone().unwrap_or_default();
        let tests = rt.gate.out.trim();
        if n.kind() == Kind::Tests && !tests.is_empty() && !said.contains(tests) {
            said = format!("{}\n\nThe tests printed:\n{tests}", said.trim());
        }
        let said = said.trim();
        let max = SAID_MAX * 3 / share;
        let said = if said.len() > max { format!("\u{2026}{}", tail_bytes(said, max)) } else { said.to_string() };
        let links = |into: bool| -> Vec<String> {
            self.flow
                .edges
                .iter()
                .filter(|e| if into { e.to == id && !self.is_const_edge(e) } else { e.from == id })
                .filter_map(|e| label(if into { &e.from } else { &e.to }))
                .collect()
        };
        let dir = match rt.worktree.as_deref().filter(|w| n.kind() == Kind::Agent && Path::new(w).is_dir()) {
            Some(w) => PathBuf::from(w),
            None => self.node_dir(n),
        };
        Some(words::Failed {
            id: id.to_string(),
            name: n.name.clone(),
            what,
            dir: dir.to_string_lossy().into_owned(),
            asked: head_chars(asked, ASKED_MAX * 3 / share),
            said,
            after: links(true),
            before: links(false),
            screen: n.kind() == Kind::Agent,
        })
    }

    /// Run a finished card again with what it had (`weft retry`; "run it again" on the card): an
    /// agent, a queued prompt or a gate gets the results of the cards before it once more, plus `note`
    /// (what was fixed since, by `by`); a git step takes them up again. A gate with an open round
    /// checks again. What it hands on goes down its links as the first time.
    pub(super) fn cmd_retry(&mut self, id: String, note: String, by: String) -> Value {
        let Some(node) = self.node(&id).cloned() else { return json!({"ok": false, "error": "no such node"}) };
        if node.kind().is_gate() && self.rt[&id].gate.held.is_some() {
            return self.cmd_gate_retry(id, false);
        }
        let rt = &self.rt[&id];
        if node.kind() == Kind::Note {
            return json!({"ok": false, "error": "a note does not run"});
        }
        // (one that could not even start has failed without finishing)
        if (!rt.finished && rt.status != Status::Failed) || rt.turn || !rt.queue.is_empty() || rt.git.busy {
            return json!({"ok": false, "error": format!("{id} has not finished: there is nothing to run again yet")});
        }
        // what came in along its links last time: the latest results of the cards before it
        let back: Vec<Edge> = self
            .flow
            .edges
            .iter()
            .filter(|e| e.to == id && !self.is_const_edge(e))
            .filter(|e| self.rt.get(&e.from).is_some_and(|s| s.finished && s.output.is_some() && e.on.matches(s.ok.unwrap_or(true))))
            .cloned()
            .collect();
        if node.kind() == Kind::Git {
            if back.is_empty() {
                return json!({"ok": false, "error": format!("nothing came into {id} to take up again")});
            }
            for e in &back {
                // (after a failed round the first one takes the others up with it)
                if self.rt.get(&id).is_some_and(|r| !r.finished && r.git.seen.contains(&e.id)) {
                    continue;
                }
                let (name, ok, out) = self.input_from(&e.from);
                self.deliver_input(e, &e.from, &name, ok, &out);
            }
            // (one that is not armed ran because it was told to: so it is now)
            self.git_pump(&id, true);
            return json!({"ok": true});
        }
        for e in &back {
            let (name, ok, out) = self.input_from(&e.from);
            if let Some(t) = self.rt.get_mut(&id) {
                if !t.inputs.iter().any(|(i, _)| *i == e.id) {
                    t.inputs.push((e.id.clone(), Input { from: e.from.clone(), name, ok, text: out, kind: InputKind::Result }));
                }
            }
        }
        if !note.trim().is_empty() && !node.kind().is_gate() {
            let who = self.node(&by).map(|n| n.name.clone()).unwrap_or_default();
            if let Some(t) = self.rt.get_mut(&id) {
                t.inputs.retain(|(i, _)| !i.starts_with("fix:"));
                t.inputs.push((format!("fix:{by}"), Input { from: by, name: who, ok: true, text: note, kind: InputKind::Fixed }));
            }
        }
        self.trigger(&id, true);
        json!({"ok": true})
    }

    /// A card's latest result as an input: its name, whether it succeeded, what it said.
    pub(super) fn input_from(&self, from: &str) -> (String, bool, String) {
        let name = self.node(from).map(|n| n.name.clone()).unwrap_or_default();
        let r = self.rt.get(from);
        (name, r.and_then(|r| r.ok).unwrap_or(true), r.and_then(|r| r.output.clone()).unwrap_or_default())
    }
}
