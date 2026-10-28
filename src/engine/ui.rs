//! What the engine tells browsers (changed nodes only), what it keeps on disk, and what it derives for both.

use super::*;

/// A CLI's model list as the page takes it.
pub(super) fn models_json(list: &[(String, String)]) -> Vec<Value> {
    list.iter().map(|(id, label)| json!({"id": id, "label": label})).collect()
}

impl Engine {
    /// Agents whose junction can no longer be satisfied without re-running
    /// something, with the name of the agent in the way.
    pub(super) fn blocked_map(&self) -> HashMap<String, String> {
        let mut blocked: HashMap<String, String> = HashMap::new();
        for _ in 0..self.flow.nodes.len().max(1) {
            let mut changed = false;
            for n in &self.flow.nodes {
                if blocked.contains_key(&n.id) {
                    continue;
                }
                let rt = &self.rt[&n.id];
                if rt.finished || rt.cur.is_some() || rt.spawning || rt.armed || !rt.queue.is_empty() || rt.gate.held.is_some() {
                    continue;
                }
                let inc: Vec<&Edge> = self.flow.edges.iter().filter(|e| e.to == n.id && !self.is_const_edge(e)).collect();
                if inc.is_empty() {
                    continue;
                }
                let (mut arrived, mut possible) = (0, 0);
                let mut blockers: Vec<String> = Vec::new();
                for e in &inc {
                    if rt.inputs.iter().any(|(i, _)| *i == e.id) {
                        arrived += 1;
                        continue;
                    }
                    let name = self.node(&e.from).map(|x| x.name.clone()).unwrap_or_default();
                    let src = &self.rt[&e.from];
                    if blocked.contains_key(&e.from) || (src.finished && !e.on.matches(src.ok.unwrap_or(true))) {
                        blockers.push(name);
                    } else {
                        possible += 1;
                    }
                }
                let is = match n.join {
                    Join::All => !blockers.is_empty(),
                    Join::Any => arrived == 0 && possible == 0 && !blockers.is_empty(),
                };
                if is {
                    blocked.insert(n.id.clone(), blockers[0].clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        blocked
    }

    pub(super) fn eff_status(&self, id: &str, blocked: &HashMap<String, String>) -> Status {
        let rt = &self.rt[id];
        let node = self.node(id);
        if node.is_some_and(|n| n.draft) && !rt.alive && !rt.spawning && !rt.git.busy && !rt.finished {
            return Status::Draft;
        }
        let idle = !rt.spawning && matches!(rt.status, Status::New | Status::Ready);
        if idle && blocked.contains_key(id) {
            return Status::Blocked;
        }
        if !rt.alive && !rt.spawning && rt.status == Status::New {
            let has_in = self.flow.edges.iter().any(|e| e.to == id);
            let has_work = node.is_some_and(|n| self.has_work(n) || n.kind() != Kind::Agent);
            if !rt.queue.is_empty() {
                return if self.waits_for_links(id) { Status::Waiting } else { Status::Starting };
            }
            if has_in && (has_work || !rt.inputs.is_empty()) {
                return Status::Waiting;
            }
        }
        // done before, and a card before it is not finished (or it holds only some of a new round, a
        // git step's open one too): it waits for that, to run again, and is not shown done
        if matches!(rt.status, Status::Done | Status::Failed) && self.waits_for_links(id) {
            return Status::Waiting;
        }
        rt.status
    }

    pub(super) fn rt_json(&self, id: &str, blocked: &HashMap<String, String>, filing: &HashMap<String, u64>) -> Value {
        let rt = &self.rt[id];
        let node = self.node(id);
        let incoming: Vec<&Edge> = self.flow.edges.iter().filter(|e| e.to == id).collect();
        let is_git = node.is_some_and(|n| n.kind() == Kind::Git);
        let arrived: Vec<&str> = incoming.iter().filter(|e| self.link_in(e)).map(|e| e.id.as_str()).collect();
        let need = match node.map(|n| n.join) {
            Some(Join::Any) => 1.min(incoming.len()),
            _ => incoming.len(),
        };
        let git = if is_git {
            let tail = &rt.git.log[rt.git.log.len().saturating_sub(8)..];
            json!({"log": tail, "seen": rt.git.seen.len(), "busy": rt.git.busy,
                   "waiting_on": rt.git.wait.as_ref().and_then(|w| self.node(w)).map(|n| n.name.clone()),
                   "commits": rt.git.facts.commits.len(), "files": rt.git.facts.files.len(), "pr": rt.git.facts.pr, "prs": rt.git.facts.prs,
                   "stage": match rt.git.stage { GitStage::Commit => "commit", GitStage::Rebase => "rebase", GitStage::Pr => "pr" },
                   "carried": rt.git.carry.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
                   "left": rt.git.left.as_ref().map(Left::to_json)})
        } else {
            Value::Null
        };
        let pv = term::preview(rt.parser.screen(), crate::model::preview_rows(node.map_or(0.0, |n| n.h)), &self.cfg.chrome);
        let queue: Vec<Value> =
            rt.queue.iter().map(|q| json!({"id": q.id, "text": head_chars(q.shown.trim(), 240), "src": q.src})).collect();
        // (one read off the screen ends with what the agent said last: that end is what the card shows)
        let out = rt.output.as_deref().map(|o| if rt.out_read { tail_chars(o.trim(), 1500) } else { head_chars(o.trim(), 1500) });
        // steps that changed files (and the one running): the dots under the card (see rewind.rs)
        let cps: Vec<Value> = rt
            .cps
            .iter()
            .filter(|c| !c.files.is_empty() || c.after.is_empty())
            .map(|c| json!({"n": c.n, "what": c.what, "at": c.at, "files": c.files.len(), "names": c.files.iter().take(8).collect::<Vec<_>>(),
                            "open": c.after.is_empty() && !c.rewind, "undone": c.undone, "rewind": c.rewind}))
            .collect();
        let ci = if rt.gate.ci_sha.is_empty() && rt.gate.ci_runs.is_empty() {
            Value::Null
        } else {
            json!({"sha": rt.gate.ci_sha, "watching": rt.gate.ci, "runs": rt.gate.ci_runs.iter().map(CiRun::to_json).collect::<Vec<_>>()})
        };
        let status = self.eff_status(id, blocked);
        // a finished card has no countdown, however it got there and whatever time was left
        let eta = rt.checks.eta.as_ref().filter(|_| !matches!(status, Status::Done | Status::Failed | Status::Stopped | Status::Exited));
        let mut v = json!({
            "t": "rt", "id": id, "status": status, "alive": rt.alive, "armed": rt.armed,
            "runs": rt.runs, "since": rt.since, "arrived": arrived, "need": need, "ok": rt.ok,
            "out": out, "out_n": rt.output.as_ref().map_or(0, |o| o.len()), "out_read": rt.out_read, "wt": rt.worktree,
            "queue": queue, "asked": head_chars(rt.asked.trim(), 300),
            "cause": rt.cause, "blocked": blocked.get(id), "steps": rt.steps.len(),
            "step_now": rt.cur.is_some(), "pv": pv,
            "gate": rt.gate.held.as_deref().map(|g| head_chars(g, 700)), "git": git,
            "bg": rt.bg,
            "archive_at": filing.get(id),
            // its countdown stands since then: someone writes a prompt into its flow
            "archive_stand": filing.get(id).and_then(|_| self.filing_stands(id)),
            // a twin can start from it; its process rests and the next one takes its conversation up again
            "twin": self.can_twin(id), "rests": self.resumable(id),
            "issue": rt.issue.as_deref().map(|i| head_chars(i, 4000)),
            "waiting": rt.waiting.as_ref().map(|w| json!({"what": head_chars(&w.what, 200), "checks": w.checks})),
            "limit": rt.limit.as_ref().map(|l| json!({"until": l.until, "said": head_chars(&l.said, 300), "known": l.known})),
            "step_wait": rt.step_wait.as_ref().and_then(|h| self.node(h)).map(|n| n.name.clone()),
            "eta": eta.map(|e| json!({"at": e.at, "set": e.set, "total": e.total, "note": e.note})),
            "feed": rt.checks.feed[rt.checks.feed.len().saturating_sub(24)..].iter().map(|f| json!({
                "id": f.id, "check": f.check, "at": f.at, "img": f.img, "note": f.note, "ok": f.ok,
                "state": f.state, "reply": f.reply,
            })).collect::<Vec<_>>(),
            "checks_busy": rt.checks.busy.iter().collect::<Vec<_>>(),
            "hand_wait": if rt.hand.phase == HandPhase::None { Value::Null } else {
                json!(rt.hand.from.as_ref().and_then(|f| self.node(f)).map(|n| n.name.clone()))
            },
            "hand_phase": match rt.hand.phase { HandPhase::None => "", HandPhase::Settle => "settle", HandPhase::Asking => "asking" },
            "gate_phase": rt.gate.phase, "gate_round": rt.gate.round, "gate_shots": rt.gate.shots,
            "gate_out": head_chars(rt.gate.out.trim(), 1800),
            "ci": ci,
            "cps": cps,
            "gate_to": rt.gate.targets.iter().filter_map(|t| self.node(t).map(|n| n.name.clone())).collect::<Vec<_>>(),
        });
        // (set afterwards: the macro above is at its recursion limit)
        v["held"] = json!(rt.held.as_ref().map(|(h, why)| json!({"id": h, "name": self.node(h).map(|n| n.name.clone()), "why": why})));
        // a queued prompt's card joins its agent when the agent takes it up, or stays (merge.rs)
        if let Some(n) = node.filter(|n| n.kind() == Kind::Step) {
            v["joins"] = json!(self.joins(id, &n.host));
        }
        // what it asks the person, with the answers it suggests: from a report it gave (`report`: it can be let
        // go as it is) or from its ISSUE, as a model read it (reads.rs), else as weft's rules find it
        if rt.issue.is_some() {
            let read = rt.cur.as_ref().and_then(|c| c.asks.clone());
            let report = rt.cur.as_ref().and_then(|c| c.report.as_deref().map(|r| read.clone().unwrap_or_else(|| c.asks_in(r))));
            v["report"] = json!(report.is_some());
            v["asks"] = json!(report.or(read).unwrap_or_else(|| asks::in_issue(rt.issue.as_deref().unwrap_or(""))));
        } else if rt.cur.as_ref().is_some_and(|c| c.reading != 0) {
            v["reading"] = json!(true);
        }
        v
    }

    pub(super) fn flush_ui(&mut self) {
        if self.clients.is_empty() {
            // nobody is looking: the next client gets the whole state, and every node counts as unsent
            for rt in self.rt.values_mut() {
                rt.ui = false;
                rt.last_msg.clear();
            }
            return;
        }
        if self.rt.values().any(|r| r.ui) {
            let blocked = self.blocked_map();
            let filing = self.filing_map(&blocked);
            let held = self.held_map(&blocked);
            // blocked-ness and the archive clock are derived across the graph, so a node can change
            // without being touched
            for (id, rt) in self.rt.iter_mut() {
                if rt.blocked_by.as_ref() != blocked.get(id) || rt.filing.as_ref() != filing.get(id) || rt.held.as_ref() != held.get(id) {
                    rt.blocked_by = blocked.get(id).cloned();
                    rt.filing = filing.get(id).copied();
                    rt.held = held.get(id).cloned();
                    rt.ui = true;
                }
            }
            let ids: Vec<String> = self.rt.iter().filter(|(_, r)| r.ui).map(|(i, _)| i.clone()).collect();
            for id in ids {
                let m = self.rt_json(&id, &blocked, &filing).to_string();
                let Some(rt) = self.rt.get_mut(&id) else { continue };
                rt.ui = false;
                if m != rt.last_msg {
                    rt.last_msg = m.clone();
                    self.bcast_raw(&m);
                }
            }
        }
        let watchers: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.watch.as_ref().is_some_and(|w| self.rt.get(&w.node).is_some_and(|r| r.scr)))
            .map(|(i, _)| *i)
            .collect();
        for cid in watchers {
            self.push_screen(cid);
        }
        for rt in self.rt.values_mut() {
            rt.scr = false;
        }
    }

    /// Send the watched node's screen to one client, only the rows that changed.
    fn push_screen(&mut self, cid: u64) {
        let Some(mut c) = self.clients.remove(&cid) else { return };
        if let Some(w) = c.watch.as_mut() {
            if let Some(rt) = self.rt.get_mut(&w.node) {
                let (rows, cols) = rt.parser.screen().size();
                if w.sent.len() < rows as usize {
                    w.sent.resize(rows as usize, u64::MAX);
                }
                rt.parser.screen_mut().set_scrollback(w.scroll);
                let scroll = rt.parser.screen().scrollback();
                w.scroll = scroll;
                let s = rt.parser.screen();
                let (cr, cc) = s.cursor_position();
                let show_cursor = scroll == 0 && !s.hide_cursor();
                let mouse = s.mouse_protocol_mode() != vt100::MouseProtocolMode::None;
                let mut changed: Vec<Value> = Vec::new();
                for r in 0..rows {
                    let cur = if show_cursor && r == cr { Some(cc.min(cols.saturating_sub(1))) } else { None };
                    let h = term::row_html(s, r, cols, cur);
                    let hh = fnv(&h);
                    if w.sent[r as usize] != hh {
                        w.sent[r as usize] = hh;
                        changed.push(json!([r, h]));
                    }
                }
                rt.parser.screen_mut().set_scrollback(0);
                let hist = {
                    rt.parser.screen_mut().set_scrollback(usize::MAX);
                    let n = rt.parser.screen().scrollback();
                    rt.parser.screen_mut().set_scrollback(0);
                    n
                };
                if !changed.is_empty() || scroll != 0 {
                    let m = json!({"t": "scr", "id": w.node, "cols": cols, "rows": rows, "scroll": scroll, "hist": hist, "mouse": mouse, "rowsd": changed});
                    let _ = c.tx.try_send(m.to_string());
                }
            }
        }
        self.clients.insert(cid, c);
    }

    pub(super) fn full_state(&self) -> Value {
        let blocked = self.blocked_map();
        let filing = self.filing_map(&blocked);
        let rts: Vec<Value> = self.flow.nodes.iter().map(|n| self.rt_json(&n.id, &blocked, &filing)).collect();
        json!({
            "t": "state",
            "build": crate::model::BUILD,
            "project": {"name": self.board_name(), "folder": self.boot.name, "dir": self.boot.dir, "session": self.boot.session, "sock": self.boot.sock,
                        "home": home(), "git": self.proj.get(&self.boot.dir).cloned().unwrap_or(Value::Null),
                        "dirs": self.proj.iter().map(|(d, v)| (d.to_string_lossy().into_owned(), v.clone())).collect::<HashMap<_, _>>()},
            "cfg": {"agents": self.cfg.agents, "quiet_s": self.cfg.quiet_s, "cols": self.cfg.cols, "rows": self.cfg.rows,
                    "prefs": self.cfg.prefs, "yolo": self.cfg.yolo,
                    "archive_s": self.cfg.archive_s, "archive_flow_s": self.cfg.archive_flow_s, "each_max": self.each_max()},
            "idle": self.idle_shown,
            "hold": self.flow.hold,
            "models": self.models.iter().map(|(a, l)| (a.clone(), models_json(l))).collect::<HashMap<_, _>>(),
            "usage": self.usage.json(),
            "library": self.cfg.library,
            "networks": self.cfg.networks,
            "flow": {"nodes": self.flow.nodes, "edges": self.flow.edges, "talks": self.flow.talks},
            "talk": self.talk_log(),
            "rt": rts,
            "archive": self.archive_json(),
            "boards": self.boards.list(),
            // (the page offers to fix what failed only to a daemon that can: see fix.rs)
            "fix": true,
        })
    }

    pub(super) fn toast(&mut self, kind: &str, msg: &str) {
        let m = json!({"t": "toast", "k": kind, "m": msg});
        self.bcast(&m);
    }

    pub(super) fn send_to(&mut self, id: u64, v: &Value) {
        if let Some(c) = self.clients.get(&id) {
            if c.tx.try_send(v.to_string()).is_err() {
                self.clients.remove(&id);
            }
        }
    }

    pub(super) fn bcast(&mut self, v: &Value) {
        let s = v.to_string();
        self.bcast_raw(&s);
    }

    fn bcast_raw(&mut self, s: &str) {
        self.clients.retain(|_, c| c.tx.try_send(s.to_string()).is_ok());
    }

    pub(super) fn save_state(&mut self) {
        self.state_dirty = false;
        self.last_save = Instant::now();
        let map: HashMap<&String, Saved> = self.rt.iter().map(|(id, r)| (id, r.to_saved())).collect();
        save_json(&self.state_path, &map);
    }

    pub(super) fn cmd_watch(&mut self, client: Option<u64>, id: Option<String>) -> Value {
        if let Some(cid) = client {
            let rows = self.cfg.rows as usize;
            if let Some(c) = self.clients.get_mut(&cid) {
                c.watch = id.map(|node| Watch { node, scroll: 0, sent: vec![u64::MAX; rows.max(200)] });
            }
            self.push_screen(cid);
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_scroll(&mut self, client: Option<u64>, id: String, offset: usize) -> Value {
        if let Some(cid) = client {
            if let Some(w) = self.clients.get_mut(&cid).and_then(|c| c.watch.as_mut()) {
                if w.node == id {
                    w.scroll = offset;
                }
            }
            self.push_screen(cid);
        }
        json!({"ok": true})
    }

    /// The wheel goes to the program when it asked for the mouse (Claude Code scrolls its own
    /// transcript: on the alternate screen there is no history to show), else it moves through
    /// weft's history of the pane, two lines a notch.
    pub(super) fn cmd_wheel(&mut self, client: Option<u64>, id: String, n: i32, row: u16, col: u16) -> Value {
        if let (Some(rt), Some(t)) = (self.rt.get(&id), &self.tmux) {
            if let (Some(p), Some(b)) = (&rt.pane, term::wheel_bytes(rt.parser.screen(), n, row, col)) {
                t.send_bytes(p, &b);
                return json!({"ok": true});
            }
        }
        let Some(cid) = client else { return json!({"ok": true}) };
        if let Some(w) = self.clients.get_mut(&cid).and_then(|c| c.watch.as_mut()).filter(|w| w.node == id) {
            w.scroll = w.scroll.saturating_add_signed(-2 * n as isize);
        }
        self.push_screen(cid);
        json!({"ok": true})
    }

    pub(super) fn cmd_preview(&mut self, id: String) -> Value {
        let Some(node) = self.node(&id).cloned() else {
            return json!({"ok": false, "error": "no such node"});
        };
        let order: Vec<Edge> = self.flow.edges.iter().filter(|e| e.to == id).cloned().collect();
        let inputs: Vec<Input> = order
            .iter()
            .filter_map(|e| {
                let src = self.node(&e.from)?;
                let got = self.rt.get(&id).and_then(|r| r.inputs.iter().find(|(i, _)| *i == e.id).map(|(_, i)| i.clone()));
                Some(got.unwrap_or(Input {
                    kind: InputKind::Result,
                    from: e.from.clone(),
                    name: src.name.clone(),
                    ok: true,
                    text: format!("<output of {} appears here>", src.name),
                }))
            })
            .collect();
        json!({"ok": true, "text": self.render(&node, &inputs)})
    }

    pub(super) fn cmd_read(&mut self, id: String, lines: usize) -> Value {
        match self.rt.get_mut(&id) {
            Some(rt) => json!({"ok": true, "text": term::transcript(&mut rt.parser, if lines == 0 { 200 } else { lines })}),
            None => json!({"ok": false, "error": "no such node"}),
        }
    }

    pub(super) fn cmd_result(&mut self, id: String) -> Value {
        match self.rt.get(&id) {
            Some(rt) => json!({
                "ok": true,
                // a report that waits for the person's answers is what they read (and, while it works on them, the one answered)
                "text": rt.cur.as_ref().and_then(|c| c.report.clone().or_else(|| c.earlier.clone())).or_else(|| rt.output.clone()).unwrap_or_default(),
                "status": rt.ok,
                "steps": rt.steps,
                "asked": rt.asked,
            }),
            None => json!({"ok": false, "error": "no such node"}),
        }
    }

    pub(super) fn cmd_ls(&mut self) -> Value {
        let blocked = self.blocked_map();
        let list: Vec<Value> = self
            .flow
            .nodes
            .iter()
            .map(|n| {
                let rt = &self.rt[&n.id];
                json!({"id": n.id, "name": n.name, "agent": n.agent, "status": self.eff_status(&n.id, &blocked),
                       "runs": rt.runs, "queued": rt.queue.len(),
                       "eta": rt.checks.eta.as_ref().map(|e| (e.at as i64 - now_ms() as i64) / 1000),
                       "changes": rt.checks.feed.iter().rev().find(|f| n.checks.iter().any(|c| c.id == f.check && c.kind == CheckKind::Changes)).map(|f| f.note.lines().next().unwrap_or("").to_string())})
            })
            .collect();
        json!({"ok": true, "nodes": list})
    }

    /// Leave. Ending stops every agent first, so what is saved says they were stopped and the next
    /// `weft` shows a calm canvas; keeping leaves them running for it to adopt.
    pub(super) fn cmd_quit(&mut self, end: bool) -> Value {
        let alive: Vec<String> = self.rt.iter().filter(|(_, r)| r.alive || r.spawning).map(|(id, _)| id.clone()).collect();
        if end {
            for id in &alive {
                self.cmd_stop(id.clone());
            }
        }
        self.exit = Some(if end { Exit::End } else { Exit::Keep });
        self.bcast(&json!({"t": "bye", "end": end, "agents": alive.len()}));
        json!({"ok": true, "end": end, "agents": alive.len()})
    }
}
