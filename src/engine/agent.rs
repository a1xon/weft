//! The process side: starting an agent in tmux, typing to it, stopping it.

use super::*;

/// The command line for an agent card: the configured command with yolo, model and reasoning
/// as plain flags. A command with a flag of its own (`node.cmd`) is never touched.
///
/// For agents weft manages (`yolo_arg` set) shell aliases are skipped (`command claude`), so the
/// switch is authoritative: an alias that bakes the flag in cannot keep yolo on after it was
/// switched off, nor double it when on (Codex refuses a repeated flag).
pub(super) fn agent_command(a: &AgentCfg, node: &Node, yolo_default: bool) -> String {
    let flag = a.yolo_arg.trim();
    let mut c = a.cmd.trim().to_string();
    if !flag.is_empty() {
        let first = c.split_whitespace().next().unwrap_or("");
        let plain = !first.is_empty() && first.chars().all(|ch| ch.is_ascii_alphanumeric() || "._/-".contains(ch));
        if plain && first != "command" {
            c = format!("command {c}");
        }
        if node.yolo.unwrap_or(yolo_default) && !a.cmd.split_whitespace().any(|w| w == flag) {
            c.push(' ');
            c.push_str(flag);
        }
    }
    c.push_str(&tuning_args(a, &node.model, &node.effort));
    c
}

/// ` --model opus --effort high` for the choices made on the card. Values that could break
/// the command line are dropped rather than quoted: model names are plain words.
pub(super) fn tuning_args(a: &AgentCfg, model: &str, effort: &str) -> String {
    let mut s = String::new();
    let applies = effort_applies(a, model);
    for (i, (tpl, v)) in [(&a.model_arg, model), (&a.effort_arg, effort)].into_iter().enumerate() {
        let v = v.trim();
        if tpl.is_empty() || v.is_empty() || !plain_word(v) || (i == 1 && !applies) {
            continue;
        }
        s.push(' ');
        s.push_str(&tpl.replace("{}", v));
    }
    s
}

/// Does this agent take a reasoning level together with this model?
pub(super) fn effort_applies(a: &AgentCfg, model: &str) -> bool {
    let m = model.trim();
    a.effort_models.is_empty() || a.effort_models.iter().any(|x| if x.is_empty() { m.is_empty() } else { m.contains(x.as_str()) })
}

pub(super) fn plain_word(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || "._:/@+-".contains(c))
}

/// What the process side needs to bring a node's window up: gathered on the actor, run off it.
struct SpawnPlan {
    tm: Tmux,
    id: String,
    cmd: String,
    cwd: PathBuf,
    /// the agent's folder before any worktree: worktrees are cut from its repository
    project: PathBuf,
    project_id: String,
    env: Vec<(String, String)>,
    /// plain commands receive their prompt as a file
    prompt_file: Option<String>,
    /// the node wants its own worktree (`existing_worktree` is the one it already has)
    want_worktree: bool,
    existing_worktree: Option<String>,
    cols: u16,
    rows: u16,
}

/// A new git worktree on its own branch, cut from the HEAD of the repository `project` is in.
async fn add_worktree(project: &Path, id: &str, project_id: &str) -> Result<PathBuf, String> {
    let git =
        |args: Vec<String>| async move { tokio::process::Command::new("git").args(args).output().await.map_err(|e| format!("git: {e}")) };
    let top = git(vec!["-C".into(), project.to_string_lossy().into_owned(), "rev-parse".into(), "--show-toplevel".into()]).await?;
    if !top.status.success() {
        return Err(format!("worktree requested but {} is not in a git repository", project.display()));
    }
    let top = String::from_utf8_lossy(&top.stdout).trim().to_string();
    let stamp = now_ms() % 100_000_000;
    let path = state_dir().join("worktrees").join(project_id).join(format!("{id}-{stamp}"));
    let add = git(vec![
        "-C".into(),
        top,
        "worktree".into(),
        "add".into(),
        "-b".into(),
        format!("weft/{id}-{stamp}"),
        path.to_string_lossy().into_owned(),
        "HEAD".into(),
    ])
    .await?;
    if !add.status.success() {
        return Err(format!("git worktree add failed: {}", String::from_utf8_lossy(&add.stderr).trim()));
    }
    Ok(path)
}

async fn do_spawn(plan: SpawnPlan) -> Result<Spawned, String> {
    let SpawnPlan { tm, id, cmd, mut cwd, project, project_id, env, prompt_file, want_worktree, existing_worktree, cols, rows } = plan;
    tmux::ensure_session(&tm.sock, &tm.session, cols, rows).await?;

    let mut worktree = existing_worktree.filter(|p| PathBuf::from(p).is_dir());
    // (tmux would open the window somewhere else, the home folder say)
    if worktree.is_none() && !cwd.is_dir() {
        return Err(format!("the folder {} does not exist", cwd.display()));
    }
    if let Some(p) = &worktree {
        cwd = PathBuf::from(p);
    } else if want_worktree {
        let path = add_worktree(&project, &id, &project_id).await?;
        worktree = Some(path.to_string_lossy().into_owned());
        cwd = path;
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut args: Vec<String> = vec![
        "new-window".into(),
        "-d".into(),
        "-P".into(),
        "-F".into(),
        "#{pane_id} #{window_id}".into(),
        "-t".into(),
        format!("{}:", tm.session),
        "-n".into(),
        id.clone(),
        "-c".into(),
        cwd.to_string_lossy().into_owned(),
    ];
    for (k, v) in env {
        args.push("-e".into());
        args.push(format!("{k}={v}"));
    }
    if let Some(f) = prompt_file {
        args.push("-e".into());
        args.push(format!("WEFT_PROMPT_FILE={f}"));
    }
    args.push("--".into());
    args.push(shell);
    // -i so aliases and PATH from the user's rc files apply: whatever works
    // when typed into a terminal works here.
    args.push(if cmd.trim().is_empty() { "-i".into() } else { "-ic".into() });
    if !cmd.trim().is_empty() {
        args.push(cmd);
    }
    let (ok, out) = tmux::run(&tm.sock, &args).await;
    if !ok {
        return Err("tmux new-window failed".into());
    }
    let mut it = out.split_whitespace();
    match (it.next(), it.next()) {
        (Some(p), Some(w)) if tmux::valid_pane(p) && tmux::valid_id(w, '@') => {
            Ok(Spawned { pane: p.to_string(), win: w.to_string(), worktree })
        }
        _ => Err(format!("unexpected tmux reply: {out}")),
    }
}

impl Engine {
    pub(super) fn start(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        if node.kind() != Kind::Agent || self.exit.is_some() {
            return;
        }
        let Some(tm) = self.tmux.clone() else {
            // tmux is the runtime: say so, keep the prompt queued and keep trying to reach it
            if !self.tmux_retrying {
                self.toast("err", "tmux is not available (is it installed?). Prompts stay queued until it answers.");
            }
            self.tmux_reconnect();
            self.set_status(id, Status::Stopped);
            return;
        };
        self.spawn_seq += 1;
        let ticket = self.spawn_seq;
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.spawn = Some(ticket);
        // A dead pane lingers (remain-on-exit); clear it so there is one window per node.
        if let Some(w) = rt.win.take() {
            tm.kill_window(&w);
            self.wins.remove(&w);
        }
        if let Some(p) = rt.pane.take() {
            self.panes.remove(&p);
        }
        rt.spawning = true;
        rt.ui = true;
        rt.checks.told.clear();
        let existing_wt = rt.worktree.clone();
        // Plain commands receive their (rendered) prompt as a file, not as keystrokes.
        let mut prompt_file: Option<String> = None;
        if node.finish == Finish::Exit && matches!(rt.queue.front(), Some(q) if q.src == Src::Flow) {
            if let Some(q) = rt.queue.pop_front() {
                rt.asked = q.shown.clone();
                let path = self.boot.dir.join(".weft").join("run").join(format!("{}.prompt.md", node.id));
                if write_atomic(&path, q.text.as_bytes()).is_ok() {
                    prompt_file = Some(path.to_string_lossy().into_owned());
                }
            }
        }
        self.set_status(id, Status::Starting);

        let agent_cfg = self.cfg.agents.get(&node.agent);
        let agent_cmd = agent_cfg.map(|a| a.cmd.clone()).unwrap_or_else(|| node.agent.clone());
        let cmd = if node.cmd.trim().is_empty() {
            match agent_cfg {
                Some(a) if !a.cmd.trim().is_empty() => agent_command(a, &node, self.cfg.yolo),
                _ => agent_cmd,
            }
        } else {
            node.cmd.clone()
        };
        let cwd = self.node_dir(&node);
        let boot_env = vec![
            ("WEFT_NODE".to_string(), node.id.clone()),
            ("WEFT_URL".to_string(), self.boot.url.clone()),
            ("WEFT_TOKEN".to_string(), self.boot.token.clone()),
            ("WEFT_BIN".to_string(), self.boot.exe.clone()),
            ("WEFT_PROJECT".to_string(), self.boot.dir.to_string_lossy().into_owned()),
        ];
        let plan = SpawnPlan {
            tm,
            id: node.id.clone(),
            cmd,
            // an own worktree is cut from the repository of the folder the agent works in
            project: cwd.clone(),
            cwd,
            project_id: project_id(&self.boot.dir),
            env: boot_env,
            prompt_file,
            want_worktree: node.worktree,
            existing_worktree: existing_wt,
            cols: self.cfg.cols,
            rows: self.cfg.rows,
        };
        let (tx, nid) = (self.tx.clone(), node.id.clone());
        self.spawns += 1;
        tokio::spawn(async move {
            let res = do_spawn(plan).await;
            let _ = tx.send(Ev::Spawned { id: nid, ticket, res });
        });
    }

    pub(super) fn on_spawned(&mut self, id: String, ticket: u64, res: Result<Spawned, String>) {
        let wanted = self.rt.get(&id).is_some_and(|r| r.spawn == Some(ticket));
        let finish = self.node(&id).map(|n| n.finish).filter(|_| wanted);
        let Some(finish) = finish else {
            // deleted, stopped or restarted while the window was being made: it is nobody's
            if let (Ok(s), Some(t)) = (res, &self.tmux) {
                t.kill_window(&s.win);
            }
            return;
        };
        if let Some(rt) = self.rt.get_mut(&id) {
            rt.spawn = None;
        }
        match res {
            Ok(s) => {
                let (cols, rows) = (self.cfg.cols, self.cfg.rows);
                let orphan = self.orphans.remove(&s.pane);
                let painted = orphan.is_some();
                let Some(rt) = self.rt.get_mut(&id) else { return };
                rt.parser = vt100::Parser::new(rows, cols, SCROLLBACK);
                if let Some(o) = orphan {
                    rt.parser.process(&o);
                }
                rt.pane = Some(s.pane.clone());
                rt.win = Some(s.win.clone());
                rt.worktree = s.worktree;
                rt.alive = true;
                rt.spawning = false;
                rt.dirty = true;
                rt.painted = painted;
                rt.hash = 0;
                rt.attn = false;
                rt.issue = None;
                rt.typing = None;
                // a new process knows nothing of weft yet, nor of the agents it talks with
                rt.briefed = false;
                rt.ruled = false;
                rt.talk.told.clear();
                rt.last_change = Instant::now();
                rt.finished = false;
                rt.ok = None;
                rt.ui = true;
                if finish == Finish::Exit {
                    // a command: it runs as soon as it starts and settles when it exits
                    rt.armed = true;
                    rt.runs += 1;
                    rt.steps.clear();
                    rt.cause = Cause::Flow;
                    rt.fresh = false;
                    rt.cur = Some(Step::new(String::new(), Vec::new()));
                } else {
                    rt.armed = false;
                    rt.cur = None;
                }
                self.panes.insert(s.pane, id.clone());
                self.wins.insert(s.win, id.clone());
                self.set_status(&id, if finish == Finish::Exit { Status::Busy } else { Status::Starting });
            }
            Err(e) => {
                let Some(rt) = self.rt.get_mut(&id) else { return };
                rt.spawning = false;
                rt.queue.retain(|q| q.src != Src::Flow);
                self.set_status(&id, Status::Failed);
                self.toast("err", &format!("Could not start: {e}"));
            }
        }
    }

    /// Type into the terminal without touching step bookkeeping (steering, stdin). The Enter that
    /// submits follows the paste a moment later (a TUI takes a paste and the Enter right behind it
    /// as one gesture and may not submit): `drive_typing` sends it.
    pub(super) fn raw_type(&mut self, id: &str, text: &str, submit: bool) {
        let Some(tm) = self.tmux.clone() else { return };
        let Some(rt) = self.rt.get_mut(id) else { return };
        let Some(pane) = rt.pane.clone() else { return };
        tm.send_bytes(&pane, &term::paste_payload(rt.parser.screen(), text));
        if submit {
            rt.typing = Some(Typing::new(text, false, rt.parser.screen(), Instant::now()));
        }
        // what was typed during a step is no part of its answer
        if let Some(c) = rt.cur.as_mut() {
            c.extra.push('\n');
            c.extra.push_str(text);
        }
        rt.armed = true;
        rt.issue = None;
        rt.last_change = Instant::now();
        rt.ui = true;
    }

    /// Send the Enter of a pasted message, and see to it that the program took the message. A
    /// task prompt that is still in the input box gets its Enter again; one that was lost (the
    /// program was not reading yet) is pasted again, once. A message that stays untaken is not a
    /// finished agent: the card asks the person instead of saying done.
    pub(super) fn drive_typing(&mut self, id: &str, now: Instant) {
        let Some(tm) = self.tmux.clone() else { return };
        let submit = Duration::from_millis(self.cfg.submit_ms);
        let Some(rt) = self.rt.get_mut(id) else { return };
        let (Some(t), Some(pane)) = (rt.typing.as_mut(), rt.pane.clone()) else {
            rt.typing = None;
            return;
        };
        match if t.started_turn(rt.turn) { Typed::Taken } else { t.next(rt.parser.screen(), rt.last_change, now, submit) } {
            Typed::Wait => {}
            Typed::Enter => {
                tm.send_key(&pane, "Enter");
                if t.entered {
                    t.enters += 1;
                }
                t.entered = true;
                t.at = now;
            }
            Typed::Paste => {
                tm.send_bytes(&pane, &term::paste_payload(rt.parser.screen(), &t.text));
                t.pastes += 1;
                t.entered = false;
                t.at = now;
            }
            Typed::Taken => rt.typing = None,
            Typed::GaveUp => {
                rt.typing = None;
                self.raise_issue(
                    id,
                    "weft typed the task into this agent, but it did not take it: the text is still in its input box, or it was not ready. Open its terminal and press Enter, or run it again."
                        .into(),
                );
            }
        }
    }

    /// Say something to an agent right now (a screenshot request, a note about its work)
    /// without starting a step. A dialog on screen would take it as its answer (that is how a
    /// pending "update now" gets confirmed), and a program that is still starting loses it, so
    /// then it waits in the queue instead.
    pub(super) fn steer(&mut self, id: &str, text: &str) {
        let ready_d = secs(self.cfg.ready_s);
        let finish = self.node(id).map_or(Finish::Idle, |n| n.finish);
        let now = Instant::now();
        let (alive, attn, up) = self.rt.get(id).map_or((false, false, false), |r| {
            (r.alive, r.attn, r.armed || r.can_take_input(finish, now.saturating_duration_since(r.last_change), ready_d))
        });
        if alive && !attn && up {
            self.raw_type(id, text, true);
            return;
        }
        self.enqueue(id, text.to_string(), head_chars(text, 200), Src::You, false);
        if !alive {
            self.restart_if_queued(id);
        }
    }

    /// Another agent was told to work in this agent's worktree (a takeover).
    pub(super) fn worktree_shared(&self, id: &str) -> bool {
        self.rt.get(id).and_then(|r| r.worktree.clone()).is_some_and(|wt| self.flow.nodes.iter().any(|n| n.id != id && n.cwd == wt))
    }

    pub(super) fn kill(&mut self, id: &str) {
        let Some(rt) = self.rt.get_mut(id) else { return };
        if let (Some(w), Some(t)) = (rt.win.clone(), &self.tmux) {
            t.kill_window(&w);
        }
        if let Some(p) = rt.pane.take() {
            self.panes.remove(&p);
        }
        if let Some(w) = rt.win.take() {
            self.wins.remove(&w);
        }
        rt.alive = false;
        rt.spawning = false;
        rt.spawn = None;
        rt.typing = None;
        rt.cur = None;
        rt.issue = None;
        rt.waiting = None;
        rt.ui = true;
    }

    pub(super) fn cmd_start(&mut self, id: String) -> Value {
        if self.rt.get(&id).is_some_and(|r| !r.alive && !r.spawning) {
            self.start(&id);
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_stop(&mut self, id: String) -> Value {
        self.kill(&id);
        if self.rt.get(&id).is_some_and(|r| !r.finished) {
            self.set_status(&id, Status::Stopped);
        }
        // prompts queued on it for a step node will not be answered
        let tag_owner: Vec<String> =
            self.rt.iter().filter(|(_, r)| r.step_wait.as_deref() == Some(id.as_str())).map(|(i, _)| i.clone()).collect();
        for w in tag_owner {
            if let Some(a) = self.rt.get_mut(&id) {
                a.forget_requests_from(&w);
            }
            self.step_response(&w, false, "the agent was stopped before it answered");
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_restart(&mut self, id: String) -> Value {
        self.kill(&id);
        if let Some(rt) = self.rt.get_mut(&id) {
            rt.finished = false;
            rt.armed = false;
            rt.ok = None;
            rt.output = None;
            rt.steps.clear();
        }
        self.set_status(&id, Status::New);
        self.start(&id);
        json!({"ok": true})
    }

    pub(super) fn cmd_key(&mut self, id: String, k: String) -> Value {
        let sent = match (self.rt.get(&id).and_then(|r| r.pane.clone()), &self.tmux) {
            (Some(p), Some(t)) => {
                t.send_key(&p, &k);
                true
            }
            _ => false,
        };
        if sent {
            self.keyed(&id, &k);
        }
        json!({"ok": true})
    }

    /// A key the person pressed in the agent's terminal. An Enter after something typed into a
    /// finished agent is a new message to it: it works again, and what it answers is its new result
    /// (an answer to the question it asked continues the run it asked it in).
    pub(super) fn keyed(&mut self, id: &str, k: &str) {
        let idle_agent = self.node(id).is_some_and(|n| n.kind() == Kind::Agent && n.finish == Finish::Idle);
        let Some(rt) = self.rt.get_mut(id) else { return };
        match k {
            "BSpace" => {
                rt.keyed.pop();
            }
            "C-c" | "C-u" | "Escape" => rt.keyed.clear(),
            "Enter" => {
                let typed = std::mem::take(&mut rt.keyed);
                let answered = rt.issue.take().is_some();
                if !rt.armed {
                    rt.armed = true;
                    rt.finished = false;
                    return;
                }
                if !rt.finished || !idle_agent || rt.attn || rt.cur.is_some() || rt.typing.is_some() || typed.trim().is_empty() {
                    return;
                }
                if !answered {
                    rt.begin_run(Cause::User);
                }
                let before = term::lines(&mut rt.parser);
                rt.cur = Some(Step::new(typed.clone(), before));
                rt.asked = typed;
                rt.finished = false;
                rt.waiting = None;
                rt.last_change = Instant::now();
                rt.ui = true;
                self.set_status(id, Status::Busy);
                self.state_dirty = true;
            }
            _ => {}
        }
    }

    pub(super) fn cmd_text(&mut self, id: String, text: String, paste: bool) -> Value {
        if let (Some(rt), Some(t)) = (self.rt.get_mut(&id), &self.tmux) {
            if let Some(p) = rt.pane.clone() {
                rt.issue = None;
                rt.keyed.push_str(&text);
                let bytes = if paste { term::paste_payload(rt.parser.screen(), &text) } else { text.into_bytes() };
                t.send_bytes(&p, &bytes);
            }
        }
        json!({"ok": true})
    }
}
