//! A node's run: reading its screen, finishing a step, settling it, feeding it the next prompt.

use super::*;
use crate::words::Verdict;

impl Engine {
    /// Read a node's screen and decide what it means: still working, waiting for a person, quiet
    /// long enough to be finished, or ready for the next queued prompt.
    pub(super) fn eval(&mut self, id: &str, now: Instant) {
        let Some(node) = self.flow.nodes.iter().find(|n| n.id == id) else { return };
        let (finish, agent) = (node.finish, node.agent.clone());
        let quiet_s = if node.quiet_s > 0.0 { node.quiet_s } else { self.cfg.quiet_s };
        let (quiet_d, ready_d) = (secs(quiet_s), secs(self.cfg.ready_s));
        {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let changed = rt.dirty && rt.read_screen(self.cfg.agents.get(&agent), now);
            if changed {
                rt.accumulate(&self.cfg.chrome);
            }
            if changed && rt.work {
                // something still runs in the background: has the step already given its last word?
                let Rt { cur, parser, .. } = rt;
                if let Some(c) = cur.as_mut().filter(|c| c.rule) {
                    c.closed = new_since(parser, &c.before, &c.typed(), &self.cfg.chrome)
                        .and_then(|t| words::read_status(&t, c.asked))
                        .is_some_and(|(v, _)| v != Verdict::Waiting);
                }
            }
        }
        // a pasted message gets its Enter, and is checked to have been taken, before the screen is judged
        self.drive_typing(id, now);
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.hold_for_background(finish == Finish::Idle, secs(self.cfg.background_s), now);

        let quiet = now.duration_since(rt.last_change);
        // An agent that was asked to `weft done` gets a grace period to do so
        // before plain silence is taken as "finished".
        let fin_d = if rt.waiting.as_ref().is_some_and(|w| w.holding) {
            // it said it waits for something: look again after a while, not after every pause
            secs(self.cfg.recheck_s)
        } else if rt.cur.as_ref().is_some_and(|c| c.expect_signal) {
            secs(quiet_s.max(self.cfg.signal_grace_s))
        } else {
            quiet_d
        };
        let next = rt.implied_status(finish, quiet, quiet_d, ready_d);
        let can_type = rt.can_take_input(finish, quiet, ready_d);
        // a dialog on the screen, or an ISSUE the agent raised: nothing moves until the person has answered
        let (armed, finished, attn) = (rt.armed, rt.finished, rt.attn || rt.issue.is_some());
        let (has_q, has_cur) = (!rt.queue.is_empty(), rt.cur.is_some());
        // a prompt that has not been seen taken is not a step that went quiet: nothing is judged yet
        let typing = rt.typing.is_some();
        if next != rt.status {
            rt.status = next;
            rt.since = now_ms();
            rt.ui = true;
        }
        // a turn marker on a screen that has not moved for a while: is it on the pane too?
        if rt.needs_check(now) {
            self.verify_screen(id, now);
        }

        if typing {
            // wait for drive_typing
        } else if armed && !finished && finish == Finish::Idle && !attn && quiet >= fin_d && (has_cur || !has_q) {
            self.on_silence(id, has_q);
        } else if has_q && !attn && can_type && (finish == Finish::Exit || (!has_cur && (finished || !armed || next == Status::Idle))) {
            self.pump(id);
        }
    }

    /// The screen has been quiet as long as this step needs. Silence alone is not an answer: an agent
    /// can have stopped because it waits for something it announced ("I will get back to you"), or
    /// because it has a question. A prompt to an agent was asked to end with a `STATUS:` line, and
    /// that line is read from its output:
    /// * DONE: the result goes on; FAILED: the failure links fire;
    /// * ISSUE: the card asks the person and nothing moves on;
    /// * WAITING: weft checks on the agent now and then, and wakes it;
    /// * no line: a step of a chain is asked once for it, and after that silence is the end; a lone
    ///   card (nothing waits for its answer) is done right away, as it always was.
    ///
    /// An agent that was told to report with a command is reminded once. Anything else (a person
    /// chatting with an agent, a request that weft verifies itself) is over when the screen says so.
    fn on_silence(&mut self, id: &str, has_q: bool) {
        enum Act {
            Settle,
            Done,
            Fail(String),
            Issue(String),
            Hold,
            Say(String),
        }
        let chrome = self.cfg.chrome.clone();
        let (nudge, max_checks) =
            (words::nudge(&self.boot.exe), (self.cfg.background_s / self.cfg.recheck_s.max(1.0)).ceil().max(1.0) as u32);
        let act = {
            let Some(Rt { cur, parser, waiting, .. }) = self.rt.get_mut(id) else { return };
            match cur.as_mut() {
                Some(c) if c.expect_signal && !c.asked && !has_q => {
                    c.asked = true;
                    c.extra.push_str(&nudge);
                    Act::Say(nudge)
                }
                Some(c) if c.rule && !has_q => {
                    // only what came after the prompt: an old status line of an earlier turn is not this one's
                    let said = new_since(parser, &c.before, &c.typed(), &chrome);
                    match said.as_deref().and_then(|t| words::read_status(t, c.asked)) {
                        Some((Verdict::Done, _)) => Act::Done,
                        Some((Verdict::Failed, why)) => Act::Fail(why),
                        Some((Verdict::Issue, q)) => Act::Issue(q),
                        Some((Verdict::Waiting, what)) => match waiting {
                            None => {
                                *waiting = Some(Waiting { what, checks: 0, holding: true });
                                Act::Hold
                            }
                            Some(w) if w.holding && w.checks < max_checks => {
                                // the quiet it asked for is over: wake it
                                w.checks += 1;
                                w.holding = false;
                                let text = words::recheck(&w.what);
                                c.extra.push('\n');
                                c.extra.push_str(&text);
                                Act::Say(text)
                            }
                            Some(w) if w.holding => Act::Issue(format!("still waiting for {} after {} checks", w.what, w.checks)),
                            Some(w) => {
                                // it answered the check and is still waiting: another quiet spell
                                w.what = what;
                                w.holding = true;
                                Act::Hold
                            }
                        },
                        // only a step whose answer a flow takes is asked again: a lone card is just quiet
                        None if !c.asked && !c.lone => {
                            c.asked = true;
                            let text = words::missing_status();
                            c.extra.push('\n');
                            c.extra.push_str(&text);
                            Act::Say(text)
                        }
                        None => Act::Settle,
                    }
                }
                _ => Act::Settle,
            }
        };
        match act {
            Act::Settle | Act::Done => self.step_done(id, true, None, How::Quiet),
            Act::Fail(why) => {
                let said = self.rt.get_mut(id).and_then(|r| answer_in(&mut r.parser, r.cur.as_ref()?, &chrome));
                let why = if why.is_empty() { "the agent said it could not do it".to_string() } else { why };
                let out = match said.map(|t| words::strip_status(&t)).filter(|t| !t.is_empty()) {
                    Some(t) => format!("{why}\n\n{t}"),
                    None => why,
                };
                self.step_done(id, false, Some(out), How::Quiet);
            }
            Act::Issue(q) => {
                self.raise_issue(id, q);
            }
            Act::Hold => {
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.ui = true;
                }
            }
            Act::Say(text) => {
                self.raw_type(id, &text, true);
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.ui = true;
                }
            }
        }
    }

    /// Ask tmux what the pane really shows (the answer comes back as `Ev::Snapshot`).
    fn verify_screen(&mut self, id: &str, now: Instant) {
        let Some(t) = self.tmux.clone() else { return };
        let Some(rt) = self.rt.get_mut(id) else { return };
        let Some(pane) = rt.pane.clone() else { return };
        rt.verified = Some(now);
        let (tx, id) = (self.tx.clone(), id.to_string());
        tokio::spawn(async move {
            let snap = snapshot(&t, &pane, false).await;
            let _ = tx.send(Ev::Snapshot { id, snap });
        });
    }

    /// tmux's copy of an agent's screen arrived. weft's own copy is rebuilt from the bytes tmux
    /// sent, so it should be the same; when it is not (whatever the reason) tmux is right: the
    /// screen is replaced, and a marker or a stale row it held on to is gone with it.
    pub(super) fn on_snapshot(&mut self, id: String, snap: Option<Snap>) {
        let Some(snap) = snap else { return };
        let agent = self.node(&id).map(|n| n.agent.clone()).unwrap_or_default();
        let now = Instant::now();
        let Some(rt) = self.rt.get_mut(&id) else { return };
        let fresh = snap.screen();
        if fresh.screen().contents() == rt.parser.screen().contents() {
            return;
        }
        rt.parser = fresh;
        rt.hash = 0;
        rt.dirty = true;
        rt.read_screen(self.cfg.agents.get(&agent), now);
        // the step's start was found on the old copy: look for it on this one
        if let Some(c) = rt.cur.as_mut() {
            let lines = term::lines(&mut rt.parser);
            if c.echoed_in(&lines) {
                c.rebase(lines);
            }
        }
        rt.ui = true;
        rt.scr = true;
        self.state_dirty = true;
    }

    pub(super) fn set_status(&mut self, id: &str, s: Status) {
        if let Some(rt) = self.rt.get_mut(id) {
            if rt.status != s {
                rt.status = s;
                rt.since = now_ms();
                rt.ui = true;
            }
        }
    }

    /// A queued prompt exists for an agent whose process is gone: bring it back.
    pub(super) fn restart_if_queued(&mut self, id: &str) {
        let go = self.rt.get(id).is_some_and(|r| !r.alive && !r.spawning && !r.queue.is_empty());
        if go {
            self.start(id);
        }
    }

    /// One step finished. Continue the script, or settle the agent.
    pub(super) fn step_done(&mut self, id: &str, ok: bool, text: Option<String>, how: How) {
        let Some(node) = self.node(id) else { return };
        let exit_node = node.finish == Finish::Exit;
        let chrome = self.cfg.chrome.clone();
        let out = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let cur = rt.cur.take();
            let prompt = cur.as_ref().map(|c| c.prompt.clone()).unwrap_or_default();
            let given = text.filter(|t| !t.trim().is_empty());
            let out = match given {
                Some(t) => t,
                None if how == How::Exit && exit_node => term::transcript_clean(&mut rt.parser, 80, &chrome),
                None => {
                    // what weft typed into the session afterwards (a reminder, a question) is not part of the answer
                    let said = match cur.as_ref() {
                        Some(c) => answer_in(&mut rt.parser, c, &chrome),
                        None => new_since(&mut rt.parser, &[], "", &chrome),
                    };
                    said.map(|t| words::strip_status(&t))
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| term::transcript_clean(&mut rt.parser, 80, &chrome))
                }
            };
            let out = tail_bytes(&out, 60_000).to_string();
            rt.steps.push(StepRec { prompt, text: out.clone(), ok });
            if rt.steps.len() > 12 {
                rt.steps.remove(0);
            }
            rt.ui = true;
            out
        };
        let more = ok && how != How::Exit && self.rt[id].queue.iter().any(|_| true) && self.rt[id].reply_to.is_none();
        if more {
            // the script continues: stay busy, the next prompt goes out when the screen is calm
            self.set_status(id, Status::Busy);
            self.state_dirty = true;
            return;
        }
        self.settle(id, ok, out);
    }

    /// The agent has nothing left to do: its result is final for this run.
    pub(super) fn settle(&mut self, id: &str, ok: bool, out: String) {
        // a turn that ended has likely changed files
        self.refresh_project();
        let reply = self.rt.get_mut(id).and_then(|r| {
            r.issue = None;
            r.waiting = None;
            r.reply_to.take()
        });
        if let Some(gate) = reply {
            // an answer to a gate's request: the agent's own result stays as it was and
            // nothing fires down its links; the gate decides what happens next
            {
                let Some(rt) = self.rt.get_mut(id) else { return };
                rt.finished = true;
                rt.armed = true;
                rt.cur = None;
                rt.ui = true;
            }
            self.set_status(id, Status::Done);
            self.state_dirty = true;
            match self.node(&gate).map(|n| n.kind()) {
                Some(Kind::Git) => self.git_response(&gate, id, ok, &out),
                Some(Kind::Step) => self.step_response(&gate, ok, &out),
                Some(Kind::Agent) => self.takeover_response(&gate, id, ok, &out),
                _ => self.gate_response(&gate, id, ok, &out),
            }
            return;
        }
        let (propagate, unsent) = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            rt.finished = true;
            rt.armed = true;
            rt.cur = None;
            rt.ok = Some(ok);
            rt.output = Some(out.clone());
            rt.ui = true;
            let go = rt.cause == Cause::Flow || !rt.propagated;
            if go {
                rt.propagated = true;
                rt.fresh = false;
            } else {
                rt.fresh = true;
            }
            (go, std::mem::take(&mut rt.unsent))
        };
        self.set_status(id, if ok { Status::Done } else { Status::Failed });
        self.state_dirty = true;
        if let Some(rt) = self.rt.get_mut(id) {
            rt.checks.eta = None;
        }
        let last_look: Vec<Check> =
            self.node(id).map(|n| n.checks.iter().filter(|c| c.kind == CheckKind::Changes).cloned().collect()).unwrap_or_default();
        for c in last_look {
            self.check_run(id, &c);
        }
        if out.len() > PIPE_MAX {
            // what links hand on is capped; the whole text stays reachable by path
            let _ = write_atomic(&self.out_path(id), out.as_bytes());
        }
        if propagate {
            self.propagate(id, ok, &out);
        } else {
            // links made while it worked have had nothing yet: they get this result
            let name = self.node(id).map(|n| n.name.clone()).unwrap_or_default();
            let edges: Vec<Edge> =
                self.flow.edges.iter().filter(|e| e.from == id && e.on.matches(ok) && unsent.contains(&e.id)).cloned().collect();
            let all = !edges.is_empty() && self.flow.edges.iter().filter(|e| e.from == id && e.on.matches(ok)).count() == edges.len();
            for e in edges {
                self.deliver_input(&e, id, &name, ok, &out);
            }
            if all {
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.fresh = false;
                }
            }
        }
        // agents that were waiting for this one to finish can have their handover note now
        let waiting: Vec<String> = self
            .rt
            .iter()
            .filter(|(_, r)| r.hand.phase == HandPhase::Settle && r.hand.from.as_deref() == Some(id))
            .map(|(i, _)| i.clone())
            .collect();
        for w in waiting {
            self.takeover_ask(&w);
        }
    }

    /// Hand the next queued prompt to the agent.
    fn pump(&mut self, id: &str) {
        let Some(finish) = self.node(id).map(|n| n.finish) else { return };
        let Some(item) = self.rt.get_mut(id).and_then(|r| r.queue.pop_front()) else { return };
        if finish == Finish::Exit {
            // a running command: whatever is queued is input for its stdin
            self.raw_type(id, &item.text, true);
        } else {
            self.begin_step(id, item);
        }
        if let Some(rt) = self.rt.get_mut(id) {
            rt.ui = true;
        }
    }

    /// Paste a prompt, press Enter, and start tracking it as a step.
    fn begin_step(&mut self, id: &str, mut item: QItem) {
        let Some(tm) = self.tmux.clone() else { return };
        let exe = self.boot.exe.clone();
        if self.rt.get(id).is_none_or(|r| r.pane.is_none()) {
            if let Some(rt) = self.rt.get_mut(id) {
                rt.queue.push_front(item);
            }
            return;
        }
        let agent = self.node(id).is_some_and(|n| n.kind() == Kind::Agent && n.finish == Finish::Idle && n.agent != "sh");
        let plain = !item.text.trim_start().starts_with(['/', '!']);
        let expect_signal = item.text.contains(&format!("{exe} done"));
        let (consumed, rule) = self.told(id, &item.text, &item.src);
        let (first, ruled) = self.rt.get(id).map_or((false, true), |r| (!r.briefed, r.ruled));
        if consumed && first {
            item.text = format!("{}\n\n{}", words::intro(), item.text);
        }
        // standing orders (an estimate, screenshots) are given once per session, with the first prompt
        if !matches!(item.src, Src::Reply(_)) {
            let orders = self.take_orders(id);
            if !orders.is_empty() {
                item.text.push_str("\n\n");
                item.text.push_str(&orders);
            }
        }
        // the agents it talks with: who they are the first time, and what they said before it could take it
        let weft_asks = matches!(&item.src, Src::Reply(t) if self.node(t).is_none_or(|n| n.kind() != Kind::Step));
        if agent && plain && !weft_asks {
            let talk = self.talk_with_prompt(id);
            if !talk.is_empty() {
                item.text.push_str("\n\n");
                item.text.push_str(&talk);
            }
        }
        if rule {
            item.text.push_str("\n\n");
            item.text.push_str(&words::status_rule(!ruled));
        }
        let Some(rt) = self.rt.get_mut(id) else { return };
        let Some(pane) = rt.pane.clone() else {
            rt.queue.push_front(item);
            return;
        };
        let cause = if item.src == Src::Flow { Cause::Flow } else { Cause::User };
        rt.reply_to = match &item.src {
            Src::Reply(node) => Some(node.clone()),
            // the answer to a question it asked while answering a request: the request still gets the result
            _ if rt.issue.is_some() => rt.reply_to.take(),
            _ => None,
        };
        // an answer to the agent's own question continues the run it raised the question in
        let new_run = rt.issue.is_none() && (rt.finished || rt.steps.is_empty());
        if new_run {
            rt.begin_run(cause);
        } else if cause == Cause::Flow {
            rt.cause = Cause::Flow;
        }
        // the Enter that submits is sent by `drive_typing`, which also sees to it that the program took it
        tm.send_bytes(&pane, &term::paste_payload(rt.parser.screen(), &item.text));
        rt.typing = Some(Typing { turn: rt.turn, ..Typing::new(&item.text, true, rt.parser.screen(), Instant::now()) });
        let before = term::lines(&mut rt.parser);
        rt.cur = Some(Step { expect_signal, rule, lone: !consumed, ..Step::new(item.text.clone(), before) });
        rt.issue = None;
        rt.waiting = None;
        if consumed {
            rt.briefed = true;
        }
        rt.ruled |= rule;
        rt.asked = item.shown.clone();
        rt.armed = true;
        rt.finished = false;
        rt.attn = false;
        rt.last_change = Instant::now();
        rt.ui = true;
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
    }

    /// What a prompt for `id` is told besides its task: `(chain, rule)`. A step whose result a flow
    /// takes without a person in between (`chain`) is told what weft is, in the first line of its
    /// first message. Every prompt to an agent is asked to end with a status line (`rule`), so a card
    /// that goes quiet with a question for the person is not taken for done; weft's own requests
    /// (which weft verifies itself), an agent told to report with commands, slash commands and shell
    /// escapes get nothing. (A shell command is not a model: nothing is explained to it.)
    pub(super) fn told(&self, id: &str, text: &str, src: &Src) -> (bool, bool) {
        let agent = self.node(id).is_some_and(|n| n.kind() == Kind::Agent && n.finish == Finish::Idle && n.agent != "sh");
        let plain = agent && !text.trim_start().starts_with(['/', '!']);
        let weft_asks = matches!(src, Src::Reply(t) if self.node(t).is_none_or(|n| n.kind() != Kind::Step));
        let commands = text.contains(&format!("{} done", self.boot.exe));
        (plain && self.step_is_consumed(id, src), plain && !weft_asks && !commands && self.cfg.chain_status)
    }

    /// Does a flow take the result of this step without a person in between? A step of a running
    /// flow does if links leave the agent; a queued prompt does if its own node's links carry the
    /// answer; what you type to a flow's agent while the flow runs belongs to the flow too.
    pub(super) fn step_is_consumed(&self, id: &str, src: &Src) -> bool {
        let from = match src {
            Src::Flow => id,
            Src::Reply(step) if self.node(step).is_some_and(|n| n.kind() == Kind::Step) => step.as_str(),
            Src::Reply(_) => return false,
            Src::You if self.rt.get(id).is_some_and(|r| r.cause == Cause::Flow && !r.finished) => id,
            Src::You => return false,
        };
        self.flow.edges.iter().any(|e| e.from == from)
    }

    pub(super) fn cmd_send(&mut self, id: String, text: String, mode: SendMode, submit: bool) -> Value {
        let id = self.resolve_host(&id);
        let Some(node) = self.node(&id).cloned() else {
            return json!({"ok": false, "error": "no such node"});
        };
        let text = expand(&text, &|k| match k.strip_prefix("from:") {
            Some(n) => Some(self.result_of(n.trim(), &[])),
            None => self.node_var(&node, k),
        });
        if text.trim().is_empty() {
            return json!({"ok": true});
        }
        let (alive, working, dialog) = {
            let r = &self.rt[&id];
            (r.alive, r.alive && r.armed && !r.finished && r.status == Status::Busy, r.alive && r.attn)
        };
        let queue_it = match mode {
            SendMode::Queue => true,
            // a dialog would take a paste as its answer: the message waits until it is closed
            SendMode::Now => !alive || dialog,
            SendMode::Auto => !alive || working || dialog,
        };
        if alive && mode == SendMode::Now && working && submit {
            // steering a busy agent: part of its current step
            self.raw_type(&id, &text, true);
        } else if queue_it || !submit {
            let full = if submit { self.with_addenda(&node, &text) } else { text.clone() };
            self.enqueue(&id, full, text, Src::You, false);
            if !alive && self.rt.get(&id).is_some_and(|r| !r.spawning) {
                self.start(&id);
            }
        } else {
            // idle / ready / settled: start a step right away
            let qid = self.next_q;
            self.next_q += 1;
            let full = self.with_addenda(&node, &text);
            let item = QItem { id: qid, shown: text.clone(), text: full, src: Src::You };
            if node.finish == Finish::Exit {
                self.raw_type(&id, &item.text, true);
            } else {
                self.begin_step(&id, item);
            }
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_queue_add(&mut self, id: String, text: String, front: bool) -> Value {
        let Some(node) = self.node(&id).cloned() else {
            return json!({"ok": false, "error": "no such node"});
        };
        let full = self.with_addenda(&node, &text);
        let qid = self.enqueue(&id, full, text, Src::You, front);
        if self.rt.get(&id).is_some_and(|r| !r.alive && !r.spawning) {
            self.start(&id);
        }
        json!({"ok": true, "qid": qid})
    }

    pub(super) fn cmd_queue_del(&mut self, id: String, qid: u64) -> Value {
        if let Some(rt) = self.rt.get_mut(&id) {
            rt.queue.retain(|q| q.id != qid);
            rt.ui = true;
        }
        self.state_dirty = true;
        json!({"ok": true})
    }

    pub(super) fn cmd_queue_move(&mut self, id: String, qid: u64, to: usize) -> Value {
        if let Some(rt) = self.rt.get_mut(&id) {
            if let Some(pos) = rt.queue.iter().position(|q| q.id == qid) {
                let item = rt.queue.remove(pos).unwrap();
                let at = to.min(rt.queue.len());
                rt.queue.insert(at, item);
                rt.ui = true;
            }
        }
        self.state_dirty = true;
        json!({"ok": true})
    }

    pub(super) fn cmd_queue_now(&mut self, id: String, qid: u64) -> Value {
        if let Some(rt) = self.rt.get_mut(&id) {
            if let Some(pos) = rt.queue.iter().position(|q| q.id == qid) {
                let item = rt.queue.remove(pos).unwrap();
                rt.queue.push_front(item);
                rt.ui = true;
            }
        }
        self.state_dirty = true;
        json!({"ok": true})
    }

    pub(super) fn cmd_queue_clear(&mut self, id: String) -> Value {
        if let Some(rt) = self.rt.get_mut(&id) {
            rt.queue.clear();
            rt.ui = true;
        }
        self.state_dirty = true;
        json!({"ok": true})
    }

    /// The agent's report: DONE (`ok`), FAILURE (`!ok`) or ISSUE (it cannot go on without the person).
    pub(super) fn cmd_done(&mut self, id: String, ok: bool, text: String, issue: bool) -> Value {
        if self.node(&id).is_none() {
            return json!({"ok": false, "error": "no such node"});
        }
        if let Some(r) = self.rt.get_mut(&id) {
            // a report is the agent's own word that it has the message: no reason to paste it again
            r.typing = None;
        }
        if issue {
            return self.raise_issue(&id, text);
        }
        let (running, finished) = {
            let r = &self.rt[&id];
            (r.cur.is_some() || (r.armed && !r.finished), r.finished)
        };
        if running || !finished {
            self.step_done(&id, ok, Some(text), How::Signal);
        } else if !text.trim().is_empty() {
            // a late report after the screen already went quiet: keep the better text
            let Some(rt) = self.rt.get_mut(&id) else { return json!({"ok": false, "error": "no such node"}) };
            rt.output = Some(text);
            rt.ui = true;
            self.state_dirty = true;
        }
        json!({"ok": true})
    }

    /// ISSUE: the step stays open, the card says what the agent needs, and nothing moves on until
    /// the person has answered in the same session.
    pub(super) fn raise_issue(&mut self, id: &str, text: String) -> Value {
        let note = if text.trim().is_empty() {
            "the agent needs something from you but did not say what".to_string()
        } else {
            text.trim().to_string()
        };
        if let Some(rt) = self.rt.get_mut(id) {
            rt.issue = Some(head_chars(&note, 4_000));
            rt.waiting = None;
            rt.ui = true;
        }
        self.set_status(id, Status::Attention);
        self.state_dirty = true;
        json!({"ok": true})
    }
}

/// What the agent said in step `c`. While the echo of what was typed is still on the screen, the
/// whole answer is there as the agent left it. Once it scrolled away, the rows that left the window
/// are only in the frames collected on the way (see `Step::rebuilt`).
fn answer_in(parser: &mut vt100::Parser, c: &Step, chrome: &[String]) -> Option<String> {
    let tail = new_since(parser, &c.before, &c.typed(), chrome);
    if c.echoed_in(&term::lines(parser)) {
        return tail;
    }
    c.rebuilt(tail.as_deref()).or(tail)
}

/// What the agent said since `before`, without the echo of what was typed to it. `None` when
/// nothing new is on the screen (so an old line of an earlier turn is never mistaken for this one's).
fn new_since(parser: &mut vt100::Parser, before: &[String], typed: &str, chrome: &[String]) -> Option<String> {
    let after = term::lines(parser);
    let said = term::extract_response(before, &after, typed, chrome);
    (!said.trim().is_empty()).then_some(said)
}
