//! Results travelling along links: joins, triggers, queued prompts and how a prompt is composed.

use super::*;

impl Engine {
    /// Fire every outgoing link whose condition matches. A link whose result is not final yet (the
    /// card still has prompts queued on it to answer, a card before it works) keeps it in `unsent`
    /// until it is: `recheck_joins` hands it on then.
    pub(super) fn propagate(&mut self, id: &str, ok: bool, out: &str) {
        let Some(name) = self.node(id).map(|n| n.name.clone()) else { return };
        let mut edges: Vec<Edge> = self.flow.edges.iter().filter(|e| e.from == id && e.on.matches(ok)).cloned().collect();
        // the prompts queued on the same agent go first: they are its work, and what is linked after
        // it waits for them. They are asked one at a time, the one highest on the canvas first: the
        // others wait for it (`answers_queued`)
        let agent = self.resolve_host(id);
        edges.sort_by(|a, b| self.asked_first(&agent, a).partial_cmp(&self.asked_first(&agent, b)).unwrap_or(std::cmp::Ordering::Equal));
        for e in edges {
            if self.pending(&e) {
                if let Some(r) = self.rt.get_mut(id).filter(|r| !r.unsent.contains(&e.id)) {
                    r.unsent.push(e.id.clone());
                }
                continue;
            }
            if let Some(r) = self.rt.get_mut(id) {
                r.unsent.retain(|u| *u != e.id);
            }
            self.deliver_input(&e, id, &name, ok, out);
        }
    }

    /// Where a link stands in the order results go out from `agent`'s card: a prompt queued on the agent
    /// before anything else, top to bottom (left to right on one line) as the cards are on the canvas.
    fn asked_first(&self, agent: &str, e: &Edge) -> (u8, f64, f64) {
        match self.node(&e.to) {
            Some(n) if n.kind() == Kind::Step && n.host == agent => (0, n.y, n.x),
            _ => (1, 0.0, 0.0),
        }
    }

    pub(super) fn deliver_input(&mut self, e: &Edge, from: &str, name: &str, ok: bool, out: &str) {
        // (a copy behind a "for each" link is given its share when the card it copies takes the list: each.rs)
        if self.held_for_share(e) {
            return;
        }
        if let Some(t) = self.rt.get_mut(&e.to) {
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
            // a deploy waits only for something to run (its command, or an agent it is handed to):
            // with that, the push coming in starts it
            if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == e.to && n.draft && n.deploy.is_some() && !n.deploy_unset()) {
                n.draft = false;
                self.flow_changed();
            }
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
            rt.ui = true;
            rt.set_status(Status::Done);
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

    /// The junction: all inputs arrived (`Promise.all`) or the first one (`Promise.any`). A link whose
    /// card already handed on its result, and is not about to hand on a new one, counts as arrived:
    /// it is a promise that resolved before (`Promise.all([done, done, new])` waits for `new` only).
    pub(super) fn check_join(&mut self, to: &str) {
        let Some(node) = self.node(to) else { return };
        if node.draft {
            return; // collects inputs, reacts once armed
        }
        let incoming: Vec<&Edge> = self.flow.edges.iter().filter(|e| e.to == to && !self.is_const_edge(e)).collect();
        if incoming.is_empty() {
            return; // fed only by notes: starts with "run flow" or when fired by hand
        }
        let rt = &self.rt[to];
        let fresh = |e: &Edge| rt.inputs.iter().any(|(i, _)| *i == e.id);
        let have = incoming.iter().filter(|e| fresh(e)).count();
        // (what came down a link whose card has more to do is not final: it waits for that)
        let ready = match node.join {
            Join::All => incoming.iter().all(|e| (fresh(e) && !self.pending(e)) || self.resolved(e)),
            Join::Any => incoming.iter().any(|e| fresh(e) && !self.pending(e)),
        };
        if have > 0 && ready {
            self.trigger(to, false);
        }
    }

    /// Is what came down this link final for now: its card has a result the link takes, and neither
    /// it nor a card before it has more to do (`pending`)?
    pub(super) fn resolved(&self, e: &Edge) -> bool {
        let Some(src) = self.rt.get(&e.from) else { return false };
        let ok = src.ok.unwrap_or(true);
        // (a git step builds on what came in: a failed one there is nothing to commit or push)
        let git = self.node(&e.to).is_some_and(|n| n.kind() == Kind::Git);
        src.output.is_some() && e.on.matches(ok) && (ok || !git) && !self.pending(e)
    }

    /// Is what this link carries in at its card, as the card's status sees it (the link is drawn lit,
    /// the junction counts it)? A card that ran with it keeps it until a new result is on its way down
    /// it: what its agent answers for a prompt queued on it goes elsewhere. A card that has still to
    /// run (or holds part of a round) has it once it is final for it, as its junction decides.
    pub(super) fn link_in(&self, e: &Edge) -> bool {
        let (Some(to), Some(rt)) = (self.node(&e.to), self.rt.get(&e.to)) else { return false };
        if self.is_const_edge(e) {
            return true;
        }
        let git = to.kind() == Kind::Git;
        if rt.finished && !self.part_in(&e.to) {
            let Some(src) = self.rt.get(&e.from) else { return false };
            let ok = src.ok.unwrap_or(true);
            return src.output.is_some() && e.on.matches(ok) && (ok || !git) && !self.coming(e) && !self.held(e);
        }
        // (a git step's inputs stay after its round: what it has this round is `seen`)
        let fresh = if git { rt.git.seen.contains(&e.id) } else { rt.inputs.iter().any(|(i, _)| *i == e.id) };
        (fresh && !self.pending(e)) || self.resolved(e)
    }

    /// Is a result held back at the link's card until that has done everything it was given?
    fn held(&self, e: &Edge) -> bool {
        // (one whose card ended the other way never gets it)
        self.rt.get(&e.from).is_some_and(|r| r.unsent.contains(&e.id) && (!r.finished || e.on.matches(r.ok.unwrap_or(true))))
    }

    /// Does a card wait for its links: it holds only some of what a new round needs (a junction at
    /// 3/4), a new result is on its way down one of them (or held back at its card until that has
    /// answered what is queued on it), or a card before it has not finished (one added there, or a
    /// junction further up that waits)? Then what it had is not final: it is not shown done, a card
    /// linked after it gets its next result, not this one, and what is typed into it waits with it (a
    /// deploy before its push). Before it has run, it waits until what it needs has all come.
    pub(super) fn waits_for_links(&self, id: &str) -> bool {
        let (Some(node), Some(rt)) = (self.node(id), self.rt.get(id)) else { return false };
        if node.draft || node.kind() == Kind::Note {
            return false;
        }
        let incoming: Vec<&Edge> = self.flow.edges.iter().filter(|e| e.to == id && !self.is_const_edge(e)).collect();
        // (a git step keeps what came in until its round is over)
        let git = node.kind() == Kind::Git;
        let fresh = |e: &Edge| if git { rt.git.seen.contains(&e.id) } else { rt.inputs.iter().any(|(i, _)| *i == e.id) };
        if !incoming.is_empty() && self.part_in(id) {
            return true;
        }
        if incoming.iter().any(|e| (!fresh(e) && self.coming(e)) || self.held(e)) {
            return true;
        }
        let ins = |e: &&Edge| fresh(e) || self.resolved(e);
        // one whose card ended the other way never comes: the card is blocked, which is no wait
        let never = |e: &&Edge| self.rt.get(&e.from).is_some_and(|s| s.finished && !e.on.matches(s.ok.unwrap_or(true)));
        let (joined, blocked) = match node.join {
            Join::All => (incoming.iter().all(ins), incoming.iter().any(never)),
            Join::Any => (incoming.iter().any(ins), incoming.iter().all(never)),
        };
        !incoming.is_empty() && rt.runs == 0 && !rt.finished && !joined && !blocked
    }

    /// Will a new result come down this link: its card works on one (or one is queued for it), or a
    /// card before it does, or one of them has not finished (`unfinished`). A loop back through the
    /// card it leads to does not count.
    fn coming(&self, e: &Edge) -> bool {
        self.ahead(e, false)
    }

    /// Is what this link carries not final yet: a new result is `coming`, or its card (or one before
    /// it) still has prompts queued on it to answer? What follows waits until it has done everything
    /// it was given: that work changes the files too, and a commit or a test run after it would miss
    /// it. (A prompt queued after the link's own end waits for this card, not the other way round.)
    pub(super) fn pending(&self, e: &Edge) -> bool {
        self.ahead(e, true)
    }

    fn ahead(&self, e: &Edge, queued: bool) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut todo = vec![e.from.clone()];
        while let Some(at) = todo.pop() {
            if at == e.to || !seen.insert(at.clone()) {
                continue;
            }
            if self.at_work(&at) || (queued && self.answers_queued(&at, &e.to)) {
                return true;
            }
            // (one that waits for what this link leads to is after it, in a loop, not before it)
            if self.unfinished(&at) && !self.leads_to(&e.to, &at) {
                return true;
            }
            // a draft collects what arrives and goes on only when someone arms it
            if self.node(&at).is_none_or(|n| n.draft) {
                continue;
            }
            todo.extend(self.flow.edges.iter().filter(|x| x.to == at).map(|x| x.from.clone()));
        }
        false
    }

    /// Does the agent answer, or have queued, a prompt of a queued-prompt card other than `to` and the
    /// cards after it? A prompt queued on the same agent waits for it too: they are asked one at a
    /// time, not all at once, so each joins the agent in turn (merge.rs).
    fn answers_queued(&self, id: &str, to: &str) -> bool {
        let Some(r) = self.rt.get(id) else { return false };
        let other = |s: &str| s != to && self.node(s).is_some_and(|n| n.kind() == Kind::Step) && !self.leads_to(to, s);
        r.reply_to.as_deref().is_some_and(other) || r.queue.iter().any(|q| matches!(&q.src, Src::Reply(s) if other(s)))
    }

    /// Has the card no final result yet: it has never run (a task added before the cards after it),
    /// or it holds part of a round it has not run?
    fn unfinished(&self, id: &str) -> bool {
        self.rt.get(id).is_some_and(|r| r.output.is_none()) || self.part_in(id)
    }

    /// Does the card hold some of the inputs of a round it has not run yet: a junction partly in, or a
    /// git step whose round is open?
    fn part_in(&self, id: &str) -> bool {
        let (Some(n), Some(r)) = (self.node(id), self.rt.get(id)) else { return false };
        match n.kind() {
            Kind::Note => false,
            Kind::Git => !r.finished && !r.git.seen.is_empty(),
            _ => !r.inputs.is_empty(),
        }
    }

    /// Does a path of links lead from the card back to it?
    fn in_loop(&self, id: &str) -> bool {
        self.flow.edges.iter().any(|e| e.from == id && self.leads_to(&e.to, id))
    }

    /// Does a path of links lead from `from` to `to`?
    fn leads_to(&self, from: &str, to: &str) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut todo = vec![from.to_string()];
        while let Some(at) = todo.pop() {
            if at == to {
                return true;
            }
            if seen.insert(at.clone()) {
                todo.extend(self.flow.edges.iter().filter(|e| e.from == at).map(|e| e.to.clone()));
            }
        }
        false
    }

    /// Is the card working on a result that will go down its links (or is one queued)? Whatever it
    /// was given counts, the flow's task as much as a message from the person that set it to work
    /// again. What an agent answers for a queued prompt or for weft (a commit, a check, a handover
    /// note) goes to who asked, not down its links.
    fn at_work(&self, id: &str) -> bool {
        let (Some(n), Some(r)) = (self.node(id), self.rt.get(id)) else { return false };
        match n.kind() {
            Kind::Note => false,
            Kind::Step => r.step_wait.is_some(),
            Kind::Git => {
                !r.finished
                    && (r.git.busy || r.git.wait.is_some() || r.git.finishing || (!r.git.jobs.is_empty() && (!n.draft || r.git.force)))
            }
            k if k.is_gate() => !r.finished && r.gate.held.is_some(),
            _ => {
                let works = r.armed && !r.finished && r.reply_to.is_none();
                let queued = r.queue.iter().any(|q| !matches!(q.src, Src::Reply(_)));
                works || queued || r.hand.phase != HandPhase::None
            }
        }
    }

    /// A card settled. It may have been what a junction still waited for, also when nothing came
    /// down its links (a failure on an "only if it succeeds" link, or a card before it that did not
    /// run on): results held back at a card that had more to do go on, and the junctions that hold
    /// inputs look again.
    pub(super) fn recheck_joins(&mut self) {
        let mut due: Vec<Edge> = self
            .flow
            .edges
            .iter()
            .filter(|e| self.rt.get(&e.from).is_some_and(|r| r.unsent.contains(&e.id)) && !self.pending(e))
            .cloned()
            .collect();
        // (prompts queued on one agent in their order: the first one asked holds the others back again)
        due.sort_by(|a, b| {
            let (x, y) = (self.asked_first(&self.resolve_host(&a.from), a), self.asked_first(&self.resolve_host(&b.from), b));
            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
        });
        for e in due {
            if self.pending(&e) {
                continue;
            }
            let Some(r) = self.rt.get_mut(&e.from) else { continue };
            r.unsent.retain(|u| *u != e.id);
            let (Some(out), ok) = (r.output.clone(), r.ok.unwrap_or(true)) else { continue };
            if e.on.matches(ok) {
                let name = self.node(&e.from).map(|n| n.name.clone()).unwrap_or_default();
                self.deliver_input(&e, &e.from, &name, ok, &out);
            }
        }
        let held: Vec<String> = self.rt.iter().filter(|(_, r)| !r.inputs.is_empty()).map(|(i, _)| i.clone()).collect();
        for id in held {
            if self.node(&id).is_some_and(|n| n.kind() != Kind::Git) {
                self.check_join(&id);
            }
        }
        let git: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() == Kind::Git)
            .filter(|n| {
                self.rt.get(&n.id).is_some_and(|r| {
                    let g = &r.git;
                    !r.finished && !g.seen.is_empty() && !g.busy && !g.finishing && g.wait.is_none() && g.jobs.is_empty()
                })
            })
            .map(|n| n.id.clone())
            .collect();
        for id in git {
            self.git_pump(&id, false);
        }
    }

    /// The cards after `id` show which of their links are still to come: tell the browsers again.
    pub(super) fn touch_downstream(&mut self, id: &str) {
        let mut seen = std::collections::HashSet::new();
        let mut todo = vec![id.to_string()];
        while let Some(at) = todo.pop() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for e in self.flow.edges.iter().filter(|e| e.from == at) {
                if let Some(r) = self.rt.get_mut(&e.to) {
                    r.ui = true;
                }
                todo.push(e.to.clone());
            }
        }
    }

    /// Turn the collected inputs into a queued prompt and make sure the agent runs.
    pub(super) fn trigger(&mut self, id: &str, manual: bool) {
        let Some(node) = self.node(id).cloned() else { return };
        if node.draft || node.kind() == Kind::Note || node.kind() == Kind::Git {
            return;
        }
        let consts = self.const_inputs(id);
        // Only what is new goes in: the agent's session has the results it was given before. A new
        // session (its process ended) knows none of them, so it is given them again.
        let again: Vec<(String, Input)> = match self.rt.get(id) {
            Some(r) if node.kind() == Kind::Agent && r.runs > 0 && !r.spawning && !self.remembers(id) => self
                .flow
                .edges
                .iter()
                .filter(|e| e.to == id && !self.is_const_edge(e) && !r.inputs.iter().any(|(i, _)| *i == e.id) && self.resolved(e))
                .map(|e| (e.id.clone(), self.input_along(e)))
                .collect(),
            _ => Vec::new(),
        };
        let runs = self.rt.get(id).map_or(0, |r| r.runs);
        // (only a card on a loop can be set off again and again by the flow alone; one that is not
        // runs as often as the cards before it give it something new)
        let looping = node.kind() == Kind::Agent && !manual && runs >= self.cfg.max_runs && self.in_loop(id);
        // what arrives before the card has taken up what it was given last goes out with that, as one
        let (joined, at) = if looping { (Vec::new(), None) } else { self.take_unstarted(id) };
        let taken = {
            let Some(rt) = self.rt.get_mut(id) else { return };
            let order: Vec<&str> = self.flow.edges.iter().filter(|e| e.to == id).map(|e| e.id.as_str()).collect();
            let mut taken: Vec<(String, Input)> = std::mem::take(&mut rt.inputs);
            for (eid, i) in joined.into_iter().chain(again) {
                if !taken.iter().any(|(e, _)| *e == eid) {
                    taken.push((eid, i));
                }
            }
            for (cid, ci) in consts {
                taken.retain(|(eid, _)| *eid != cid);
                taken.push((cid, ci));
            }
            taken.sort_by_key(|(eid, _)| order.iter().position(|o| *o == eid).unwrap_or(usize::MAX));
            rt.ui = true;
            taken
        };
        let inputs: Vec<Input> = taken.iter().map(|(_, i)| i.clone()).collect();
        self.touch_downstream(id);
        if node.kind() == Kind::Step {
            // the agent has what it already produced, in its own session: its own result and the
            // answers it gave to other queued prompts of the same session are not sent back to it
            let host = node.host.clone();
            // (unless that session is gone for good: then the agent gets them like any other input)
            let kept = self.remembers(&host);
            let inputs: Vec<Input> = inputs.into_iter().filter(|i| !(kept && self.in_session_of(&i.from, &host))).collect();
            if let Some(qid) = self.step_run(id, &node, &inputs) {
                self.unstarted(id, &host, qid, at, taken);
            }
            return;
        }
        if node.kind().is_gate() {
            self.gate_begin(id, &node, &inputs);
            if let Some(rt) = self.rt.get_mut(id) {
                rt.unstarted = Some((0, taken));
            }
            return;
        }
        if looping {
            self.toast("warn", &format!("Loop guard: \"{}\" already ran {} times. Use Run flow or Reset to continue.", node.name, runs));
            return;
        }
        // a "for each" link brought a list: this card takes the first share, a copy of it each other one
        let taken = self.share_out(id, &node, taken);
        let inputs: Vec<Input> = taken.iter().map(|(_, i)| i.clone()).collect();
        let (body, full) = self.render_parts(&node, &inputs);
        if !full.is_empty() {
            // what was typed into it while it waited for this goes in after it
            let first = self.rt.get(id).is_some_and(|r| !r.alive && !r.spawning && !r.queue.is_empty() && r.queue.iter().all(|q| q.src == Src::You));
            let qid = self.enqueue(id, full, body, Src::Flow, first);
            self.unstarted(id, id, qid, at, taken);
        }
        self.state_dirty = true;
        if self.rt.get(id).is_some_and(|r| !r.alive && !r.spawning) {
            self.start(id);
        }
    }

    /// What the card's last trigger took and has not taken up yet: its prompt still waits in a
    /// queue (it leaves it now; returned is where it waited), or a gate's round is still open.
    fn take_unstarted(&mut self, id: &str) -> (Vec<(String, Input)>, Option<(String, usize)>) {
        let gate = self.node(id).is_some_and(|n| n.kind().is_gate());
        let Some(rt) = self.rt.get_mut(id) else { return (Vec::new(), None) };
        let Some((qid, ins)) = rt.unstarted.take() else { return (Vec::new(), None) };
        if gate {
            return (if rt.gate.held.is_some() && !rt.finished { ins } else { Vec::new() }, None);
        }
        // (a queued prompt that moved to another agent meanwhile waited in the old one's queue)
        let Some((host, at)) = self.rt.iter().find_map(|(h, r)| r.queue.iter().position(|q| q.id == qid).map(|p| (h.clone(), p)))
        else {
            return (Vec::new(), None);
        };
        if let Some(h) = self.rt.get_mut(&host) {
            h.queue.remove(at);
            h.ui = true;
        }
        if let Some(rt) = self.rt.get_mut(id).filter(|_| host != id) {
            rt.runs = rt.runs.saturating_sub(1); // a queued prompt: the same run, not another
        }
        (ins, Some((host, at)))
    }

    /// The prompt `qid` in `host`'s queue carries what `id` took: it takes the place of the one it
    /// joined, and what arrives before it goes out joins it in turn.
    fn unstarted(&mut self, id: &str, host: &str, qid: u64, at: Option<(String, usize)>, taken: Vec<(String, Input)>) {
        if let (Some((was, pos)), Some(h)) = (at, self.rt.get_mut(host)) {
            if was == host {
                if let Some(item) = h.queue.iter().position(|q| q.id == qid).and_then(|p| h.queue.remove(p)) {
                    let pos = pos.min(h.queue.len());
                    h.queue.insert(pos, item);
                }
            }
        }
        if let Some(rt) = self.rt.get_mut(id) {
            rt.unstarted = Some((qid, taken));
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
    /// Returns the queued prompt's id.
    fn step_run(&mut self, id: &str, node: &Node, inputs: &[Input]) -> Option<u64> {
        let host = node.host.clone();
        if self.node(&host).is_none_or(|h| h.kind() != Kind::Agent) {
            self.toast("warn", "This queued prompt has no agent to answer it: the agent it was meant for is gone.");
            if let Some(rt) = self.rt.get_mut(id) {
                rt.cause = Cause::User;
            }
            self.settle(id, false, "the agent this prompt was meant for is gone".into());
            return None;
        }
        let (body, full) = self.render_parts(node, inputs);
        if full.trim().is_empty() {
            self.toast("info", "Write the prompt first: this step has nothing to ask yet.");
            return None;
        }
        let qid = self.enqueue(&host, full, body, Src::Reply(id.to_string()), false);
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
        Some(qid)
    }

    /// The agent answered the queued prompt: that answer is this node's result and goes on from here.
    pub(super) fn step_response(&mut self, id: &str, ok: bool, text: &str) {
        if let Some(rt) = self.rt.get_mut(id) {
            rt.step_wait = None;
            rt.cause = Cause::Flow;
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
                InputKind::Part => format!("### Your part of the list from \"{}\"\n{}", i.name, i.text.trim()),
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
        // its answer goes down a "for each" link: the next step needs the jobs as a list
        if self.lists_jobs(&node.id) {
            full.push_str("\n\n");
            full.push_str(&words::each_order(self.each_max()));
        }
        full
    }

    /// A message typed to an agent carries the agent's addenda like every other prompt: that is how
    /// it learns to report when it is really done. Slash commands and shell escapes go through as typed.
    pub(super) fn with_addenda(&self, node: &Node, text: &str) -> String {
        if node.kind() != Kind::Agent || node.finish != Finish::Idle || node.addenda.is_empty() || is_cli_command(text) {
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
            let cwd = self.live_worktree(&from).map(str::to_string);
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
        // a draft is set going as its run button does: armed, it waits for what its links bring
        // (trigger runs no draft: fired, it stayed one and nothing said so)
        if self.node(&id).is_some_and(|n| n.draft) {
            return self.cmd_arm(id);
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
