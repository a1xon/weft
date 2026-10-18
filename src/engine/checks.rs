//! Watches on an agent: screenshots, tests, changes, the ETA countdown and the evidence feed.

use super::*;

/// What the agent has touched so far, from git: a header and one line per file.
async fn changes_summary(dir: &Path) -> Result<String, String> {
    let probe = async {
        let (inside, _) = git_run(dir, &["rev-parse", "--is-inside-work-tree"]).await?;
        if !inside {
            return Err("not a git repository".to_string());
        }
        let (_, st) = git_run(dir, &["status", "--porcelain", "--", ".", ":(exclude).weft"]).await?;
        let (_, num) = git_run(dir, &["diff", "--numstat", "HEAD", "--", ".", ":(exclude).weft"]).await.unwrap_or((false, String::new()));
        let mut counts: HashMap<String, (String, String)> = HashMap::new();
        for l in num.lines() {
            let mut p = l.splitn(3, '\t');
            if let (Some(a), Some(d), Some(path)) = (p.next(), p.next(), p.next()) {
                counts.insert(path.to_string(), (a.to_string(), d.to_string()));
            }
        }
        let (mut add, mut del, mut files) = (0u64, 0u64, 0usize);
        let mut rows: Vec<String> = Vec::new();
        for l in st.lines().filter(|l| l.len() > 3) {
            let (xy, path) = (l[..2].trim().to_string(), l[3..].trim().trim_matches('"').to_string());
            files += 1;
            let tag = if xy == "??" { "?".to_string() } else { xy.chars().next().unwrap_or('M').to_string() };
            match counts.get(path.as_str()) {
                Some((a, d)) => {
                    add += a.parse::<u64>().unwrap_or(0);
                    del += d.parse::<u64>().unwrap_or(0);
                    rows.push(format!("{tag} {path}  +{a} \u{2212}{d}"));
                }
                None => rows.push(format!("{tag} {path}")),
            }
        }
        if files == 0 {
            return Ok("no changes yet".to_string());
        }
        let mut out = format!("{files} file{} \u{b7} +{add} \u{2212}{del}", if files == 1 { "" } else { "s" });
        for r in rows.iter().take(40) {
            out.push('\n');
            out.push_str(r);
        }
        if rows.len() > 40 {
            out.push_str(&format!("\n\u{2026} and {} more", rows.len() - 40));
        }
        Ok(out)
    };
    match tokio::time::timeout(Duration::from_secs(8), probe).await {
        Ok(r) => r,
        Err(_) => Err("git took too long".into()),
    }
}

impl Engine {
    /// Once a second: a watch with an interval collects again while its agent is working.
    pub(super) fn tick_checks(&mut self) {
        let now = now_ms();
        if now / 1000 == self.last_check_sec {
            return;
        }
        self.last_check_sec = now / 1000;
        let due: Vec<(String, Check)> = self
            .flow
            .nodes
            .iter()
            .filter(|n| !n.checks.is_empty())
            .flat_map(|n| n.checks.iter().map(move |c| (n.id.clone(), c.clone())))
            .filter(|(id, c)| {
                let Some(rt) = self.rt.get(id) else { return false };
                c.every_s > 0
                    && rt.alive
                    && rt.status == Status::Busy
                    && !rt.checks.busy.contains(&c.id)
                    && now.saturating_sub(rt.checks.at.get(&c.id).copied().unwrap_or(0)) >= c.every_s as u64 * 1000
            })
            .collect();
        for (id, c) in due {
            if self.rt.get(&id).is_none_or(|r| !r.checks.at.contains_key(&c.id)) {
                // the first interval starts when the agent starts working, not at once
                if let Some(rt) = self.rt.get_mut(&id) {
                    rt.checks.at.insert(c.id.clone(), now);
                }
                continue;
            }
            self.check_run(&id, &c);
        }
        self.tick_eta(now);
    }

    /// An agent with an ETA that has gone a minute without giving one, or whose estimate ran out
    /// while it is still working, is asked once for a new one.
    pub(super) fn tick_eta(&mut self, now: u64) {
        let exe = self.boot.exe.clone();
        let asks: Vec<(String, String)> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.checks.iter().any(|c| c.kind == CheckKind::Eta))
            .filter_map(|n| {
                let rt = self.rt.get(&n.id)?;
                if !(rt.alive && rt.status == Status::Busy && rt.armed && !rt.finished && !rt.attn) {
                    return None;
                }
                match &rt.checks.eta {
                    None if rt.checks.eta_asked == 0 && now.saturating_sub(rt.run_ms) >= 60_000 => {
                        Some((n.id.clone(), words::eta_request(&exe, None)))
                    }
                    Some(e) if now > e.at + 5_000 && now.saturating_sub(e.asked.max(e.set)) >= (e.total * 500).max(90_000) => {
                        Some((n.id.clone(), words::eta_request(&exe, Some(e.total))))
                    }
                    _ => None,
                }
            })
            .collect();
        for (id, text) in asks {
            if let Some(rt) = self.rt.get_mut(&id) {
                rt.checks.eta_asked = now;
                if let Some(e) = rt.checks.eta.as_mut() {
                    e.asked = now;
                }
            }
            self.steer(&id, &text);
        }
    }

    /// What the agent still has to be told (and has not been, in this session) for its watches.
    pub(super) fn take_orders(&mut self, id: &str) -> String {
        let Some(node) = self.node(id).cloned() else { return String::new() };
        let exe = self.boot.exe.clone();
        let mut out: Vec<String> = Vec::new();
        let told = self.rt.get(id).map(|r| r.checks.told.clone()).unwrap_or_default();
        let mut mark: Vec<String> = Vec::new();
        for c in &node.checks {
            if told.contains(&c.id) {
                continue;
            }
            match c.kind {
                CheckKind::Shot if c.cmd.trim().is_empty() => out.push(words::shot_standing(&exe, &c.subject)),
                CheckKind::Eta => out.push(words::eta_standing(&exe)),
                _ => continue,
            }
            mark.push(c.id.clone());
        }
        if let Some(rt) = self.rt.get_mut(id) {
            for m in mark {
                rt.checks.told.insert(m);
            }
        }
        out.join("\n\n")
    }

    /// A watch that keeps one live item (the changes so far) replaces it instead of piling up.
    fn feed_upsert(&mut self, node: &str, check: &str, note: &str) {
        let note = head_chars(note.trim(), 1800);
        let same = self.rt.get(node).and_then(|r| r.checks.feed.iter().find(|f| f.check == check)).map(|f| f.note == note);
        match same {
            Some(true) => {}
            Some(false) => {
                if let Some(f) = self.rt.get_mut(node).and_then(|r| r.checks.feed.iter_mut().find(|f| f.check == check)) {
                    f.note = note;
                    f.at = now_ms();
                    f.state = FeedState::New;
                    f.reply.clear();
                }
                if let Some(rt) = self.rt.get_mut(node) {
                    rt.ui = true;
                }
                self.state_dirty = true;
            }
            None => self.feed_push(node, check, "", &note, None),
        }
    }

    pub(super) fn feed_push(&mut self, node: &str, check: &str, img: &str, note: &str, ok: Option<bool>) {
        let Some(rt) = self.rt.get_mut(node) else { return };
        rt.checks.feed_seq += 1;
        let id = rt.checks.feed_seq;
        rt.checks.feed.push(FeedItem {
            id,
            check: check.to_string(),
            at: now_ms(),
            img: img.to_string(),
            note: head_chars(note.trim(), 900),
            ok,
            state: FeedState::New,
            reply: String::new(),
        });
        while rt.checks.feed.len() > 60 {
            rt.checks.feed.remove(0);
        }
        rt.ui = true;
        self.state_dirty = true;
    }

    /// Collect one piece of evidence: weft runs the check's command itself (the agent is not
    /// disturbed), or, without a command, the agent is asked once and shows it with `weft shot`.
    pub(super) fn check_run(&mut self, node_id: &str, ch: &Check) {
        let now = now_ms();
        if let Some(rt) = self.rt.get_mut(node_id) {
            rt.checks.at.insert(ch.id.clone(), now);
        }
        if ch.kind == CheckKind::Changes {
            // git knows: what the agent has touched so far. Nobody is asked, nothing is disturbed.
            if self.rt.get(node_id).is_none_or(|r| r.checks.busy.contains(&ch.id)) {
                return;
            }
            let Ok(rt_h) = tokio::runtime::Handle::try_current() else { return };
            if let Some(rt) = self.rt.get_mut(node_id) {
                rt.checks.busy.insert(ch.id.clone());
            }
            let dir = self.work_dir(node_id);
            let tx = self.tx.clone();
            let (nid, cid) = (node_id.to_string(), ch.id.clone());
            rt_h.spawn(async move {
                let (ok, out) = match changes_summary(&dir).await {
                    Ok(s) => (true, s),
                    Err(e) => (false, e),
                };
                let _ = tx.send(Ev::Check { node: nid, check: cid, kind: CheckKind::Changes, ok, out, img: String::new() });
            });
            return;
        }
        if matches!(ch.kind, CheckKind::Eta | CheckKind::Ping) {
            return;
        }
        if !ch.cmd.trim().is_empty() {
            let Ok(rt_h) = tokio::runtime::Handle::try_current() else { return };
            if let Some(rt) = self.rt.get_mut(node_id) {
                rt.checks.busy.insert(ch.id.clone());
                rt.ui = true;
            }
            let _ = std::fs::create_dir_all(self.shots_dir());
            let dir = self.work_dir(node_id);
            let tx = self.tx.clone();
            let path = self.shots_dir().join(format!("{node_id}-{now}.png"));
            let (nid, cid, kind, cmd) = (node_id.to_string(), ch.id.clone(), ch.kind, ch.cmd.clone());
            rt_h.spawn(async move {
                if kind == CheckKind::Tests {
                    let (ok, out) = run_tests(&dir, &cmd).await;
                    let _ = tx.send(Ev::Check { node: nid, check: cid, kind, ok, out, img: String::new() });
                } else {
                    let (ok, out) = run_capture(&dir, &cmd, &path).await;
                    // (an image the command left is shown even when the command failed)
                    let img = if path.is_file() { path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { String::new() };
                    let _ = tx.send(Ev::Check { node: nid, check: cid, kind, ok, out, img });
                }
            });
            return;
        }
        if ch.kind != CheckKind::Shot {
            self.toast("warn", "Set a test command in this check's options, then run it.");
            return;
        }
        if !self.rt.get(node_id).is_some_and(|r| r.alive) {
            self.toast("warn", "The agent is not running, so it cannot take a screenshot. Paste or drop one, or set a capture command.");
            return;
        }
        let text = words::shot_request(&self.boot.exe, &ch.subject);
        self.steer(node_id, &text);
    }

    pub(super) fn on_check(&mut self, node: &str, check: &str, kind: CheckKind, ok: bool, out: &str, img: &str) {
        if let Some(rt) = self.rt.get_mut(node) {
            rt.checks.busy.remove(check);
            rt.ui = true;
        }
        if kind == CheckKind::Changes {
            if ok {
                self.feed_upsert(node, check, out);
            } else {
                let name = self.node(node).map(|n| n.name.clone()).unwrap_or_default();
                self.toast("warn", &format!("Cannot list the changes of {name}: {}", head_chars(out.trim(), 100)));
            }
            return;
        }
        if kind == CheckKind::Tests {
            let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
            let tail = lines[lines.len().saturating_sub(14)..].join("\n");
            self.feed_push(node, check, "", &tail, Some(ok));
        } else if !img.is_empty() {
            let note = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
            self.feed_push(node, check, img, &note, None);
        } else {
            let name = self.node(node).map(|n| n.name.clone()).unwrap_or_default();
            self.toast("warn", &format!("The capture command for {name} produced no image: {}", head_chars(out.trim(), 120)));
        }
    }

    pub(super) fn cmd_eta(&mut self, node: String, seconds: u64, note: String) -> Value {
        let Some(n) = self.node(&node) else {
            return json!({"ok": false, "error": "no such agent"});
        };
        if !n.checks.iter().any(|c| c.kind == CheckKind::Eta) {
            return json!({"ok": false, "error": "no ETA is attached to this agent: drop the ETA check on its card first"});
        }
        let Some(rt) = self.rt.get_mut(&node) else {
            return json!({"ok": false, "error": "no such agent"});
        };
        if rt.finished {
            return json!({"ok": true, "ignored": "this agent has already finished"});
        }
        let secs = seconds.min(7 * 86_400);
        let now = now_ms();
        rt.checks.eta = Some(Eta { at: now + secs * 1000, set: now, total: secs, note: head_chars(note.trim(), 160), asked: 0 });
        rt.ui = true;
        self.state_dirty = true;
        json!({"ok": true, "seconds": secs})
    }

    pub(super) fn cmd_check_add(&mut self, node: String, kind: CheckKind, subject: String, cmd: String, every_s: u32) -> Value {
        let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == node && n.kind() == Kind::Agent) else {
            return json!({"ok": false, "error": "checks watch an agent"});
        };
        // an ETA, the changes and a ping are single: asking again keeps the one there is
        if matches!(kind, CheckKind::Eta | CheckKind::Changes | CheckKind::Ping) {
            if let Some(c) = n.checks.iter().find(|c| c.kind == kind) {
                return json!({"ok": true, "check": c.id});
            }
        }
        let every_s = if kind == CheckKind::Changes && every_s == 0 { 20 } else { every_s };
        let mut k = 1;
        while n.checks.iter().any(|c| c.id == format!("c{k}")) {
            k += 1;
        }
        let ch = Check { id: format!("c{k}"), kind, subject, cmd, every_s };
        n.checks.push(ch.clone());
        // an agent that has not been given its task yet is told with it: the first prompt carries
        // the orders (typing them now would race the task into a program that is still starting)
        let working = self.rt.get(&node).is_some_and(|r| r.alive && (r.armed || r.cur.is_some() || !r.steps.is_empty()));
        self.flow_changed();
        if working {
            // tell the running agent once what is wanted of it
            let orders = self.take_orders(&node);
            if !orders.is_empty() {
                self.steer(&node, &orders);
            }
        }
        if kind == CheckKind::Changes {
            if let Some(ch) = self.node(&node).and_then(|n| n.checks.iter().find(|c| c.id == ch.id).cloned()) {
                self.check_run(&node, &ch);
            }
        }
        json!({"ok": true, "check": ch.id})
    }

    pub(super) fn cmd_check_set(&mut self, node: String, check: String, patch: Value) -> Value {
        let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == node) else {
            return json!({"ok": false, "error": "no such agent"});
        };
        let Some(c) = n.checks.iter_mut().find(|c| c.id == check) else {
            return json!({"ok": false, "error": "no such check"});
        };
        if let Some(v) = patch.get("subject").and_then(|v| v.as_str()) {
            c.subject = v.chars().take(300).collect();
        }
        if let Some(v) = patch.get("cmd").and_then(|v| v.as_str()) {
            c.cmd = v.chars().take(1000).collect();
        }
        if let Some(v) = patch.get("every_s").and_then(|v| v.as_u64()) {
            c.every_s = v.min(86_400) as u32;
        }
        self.flow_changed();
        json!({"ok": true})
    }

    pub(super) fn cmd_check_del(&mut self, node: String, check: String) -> Value {
        if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == node) {
            n.checks.retain(|c| c.id != check);
        }
        if let Some(rt) = self.rt.get_mut(&node) {
            rt.checks.feed.retain(|f| f.check != check);
            rt.checks.at.remove(&check);
            rt.checks.busy.remove(&check);
            rt.ui = true;
        }
        self.flow_changed();
        json!({"ok": true})
    }

    pub(super) fn cmd_check_now(&mut self, node: String, check: String) -> Value {
        let Some(ch) = self.node(&node).and_then(|n| n.checks.iter().find(|c| c.id == check).cloned()) else {
            return json!({"ok": false, "error": "no such check"});
        };
        self.check_run(&node, &ch);
        json!({"ok": true})
    }

    pub(super) fn cmd_feed_file(&mut self, node: String, path: String, note: String) -> Value {
        let Some(n) = self.node(&node) else { return json!({"ok": false, "error": "no such agent"}) };
        let Some(ch) = n.checks.iter().find(|c| c.kind == CheckKind::Shot).map(|c| c.id.clone()) else {
            return json!({"ok": false, "error": "no screenshot watch is attached to this agent: drop the screenshot tool on its card first"});
        };
        let src = PathBuf::from(&path);
        let mut head = [0u8; 16];
        let n_read = std::fs::File::open(&src).and_then(|mut f| std::io::Read::read(&mut f, &mut head));
        let Ok(n_read) = n_read else {
            return json!({"ok": false, "error": format!("cannot read {path}")});
        };
        let Some(ext) = image_ext(&head[..n_read]) else {
            return json!({"ok": false, "error": "that file is not a png, jpg, webp or gif image"});
        };
        if std::fs::metadata(&src).map_or(true, |m| m.len() > 16 * 1024 * 1024) {
            return json!({"ok": false, "error": "image is larger than 16 MB"});
        }
        let _ = std::fs::create_dir_all(self.shots_dir());
        let name = format!("{node}-{}.{ext}", now_ms());
        if std::fs::copy(&src, self.shots_dir().join(&name)).is_err() {
            return json!({"ok": false, "error": "could not save the image"});
        }
        self.feed_push(&node, &ch, &name, &note, None);
        json!({"ok": true, "name": name})
    }

    pub(super) fn cmd_feed_add(&mut self, node: String, name: String, note: String) -> Value {
        let Some(ch) = self.node(&node).and_then(|n| n.checks.iter().find(|c| c.kind == CheckKind::Shot).map(|c| c.id.clone())) else {
            return json!({"ok": false, "error": "no screenshot watch is attached to this agent"});
        };
        self.feed_push(&node, &ch, &name, &note, None);
        json!({"ok": true})
    }

    pub(super) fn cmd_feed_act(&mut self, node: String, item: u64, text: String) -> Value {
        let text = text.trim().to_string();
        let Some(it) = self.rt.get(&node).and_then(|r| r.checks.feed.iter().find(|f| f.id == item).cloned()) else {
            return json!({"ok": false, "error": "no such item"});
        };
        if text.is_empty() {
            if let Some(f) = self.rt.get_mut(&node).and_then(|r| r.checks.feed.iter_mut().find(|f| f.id == item)) {
                f.state = FeedState::Ok;
            }
        } else {
            // steer: the agent hears it right away, whatever it is doing
            let what = if it.img.is_empty() {
                format!("your last test run ({})", if it.ok == Some(false) { "it failed" } else { "it passed" })
            } else if it.note.trim().is_empty() {
                "the screenshot you showed me".to_string()
            } else {
                format!("the screenshot you showed me (\"{}\")", head_chars(it.note.trim(), 100))
            };
            let mut msg = format!("About {what}: {text}");
            if it.img.is_empty() && !it.note.trim().is_empty() {
                msg.push_str(&format!("\n\nTest output:\n{}", it.note.trim()));
            }
            msg.push_str("\n\nPlease adjust that and carry on.");
            if self.rt.get(&node).is_some_and(|r| r.finished) {
                // a finished agent set to work again: a new run, like a message from the bar, and what
                // it says then is its result for the cards after it (a check there asks again)
                let full = self.node(&node).map_or_else(|| msg.clone(), |n| self.with_addenda(n, &msg));
                self.enqueue(&node, full, msg.clone(), Src::You, false);
                self.restart_if_queued(&node);
            } else {
                self.steer(&node, &msg);
            }
            if let Some(f) = self.rt.get_mut(&node).and_then(|r| r.checks.feed.iter_mut().find(|f| f.id == item)) {
                f.state = FeedState::Steered;
                f.reply = head_chars(&text, 300);
            }
        }
        if let Some(rt) = self.rt.get_mut(&node) {
            rt.ui = true;
        }
        self.state_dirty = true;
        json!({"ok": true})
    }

    pub(super) fn cmd_feed_del(&mut self, node: String, item: u64) -> Value {
        if let Some(rt) = self.rt.get_mut(&node) {
            rt.checks.feed.retain(|f| f.id != item);
            rt.ui = true;
        }
        self.state_dirty = true;
        json!({"ok": true})
    }
}
