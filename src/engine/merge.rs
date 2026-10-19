//! A queued prompt joins its agent when the agent takes it up: its card goes, the prompt is the
//! agent's next turn in its own session, and the links of the card are the agent's from then on (in
//! and out). So the answer is the agent's result and goes down every link of the agent, a commit
//! after it included. A reasoning level set on the queued prompt is the agent's from that moment: an
//! agent whose process runs at another level is started again on the same conversation first
//! (resume.rs).
//!
//! A queued prompt that waits for work done after its own agent (a review of it, an approval after
//! it, a split it planned and others built) stays a card: as the agent, that work would feed back into
//! the agent that it came from, and the agent's answer would set it off again (the networks' "fix
//! findings", "build it", "integrate"). Its reasoning level still becomes the agent's.

use super::*;

impl Engine {
    /// The agent `host` takes up the prompt at the front of its queue (or is started for it). A queued
    /// prompt's reasoning level becomes the agent's, and its card joins the agent when it can. True
    /// when that changed what the agent's process is started with.
    pub(super) fn take_up_queued(&mut self, host: &str) -> bool {
        let Some(Src::Reply(step)) = self.rt.get(host).and_then(|r| r.queue.front()).map(|q| q.src.clone()) else { return false };
        if self.node(&step).is_none_or(|s| s.kind() != Kind::Step || s.host != host) {
            return false;
        }
        let tuned = self.tune_for(&step, host);
        if self.joins(&step, host) {
            if let Some(q) = self.rt.get_mut(host).and_then(|r| r.queue.front_mut()) {
                q.src = Src::Flow;
            }
            self.merge_step(&step, host);
        } else if tuned.is_some() {
            self.flow_changed();
        }
        tuned.unwrap_or(false)
    }

    /// Does the queued prompt `step` join its agent `host`? Not when what it waits for is work done
    /// after that agent (it would loop back into the agent). A prompt queued on the same agent before
    /// it, which joins the agent first, counts as the agent.
    pub(super) fn joins(&self, step: &str, host: &str) -> bool {
        self.joins_after(step, host, &mut Vec::new())
    }

    fn joins_after(&self, step: &str, host: &str, seen: &mut Vec<String>) -> bool {
        if seen.iter().any(|s| s == step) || self.node(host).is_none_or(|h| h.kind() != Kind::Agent) {
            return false;
        }
        seen.push(step.to_string());
        let froms: Vec<String> = self.flow.edges.iter().filter(|e| e.to == step && e.from != host).map(|e| e.from.clone()).collect();
        for from in froms {
            let before = self.node(&from).is_some_and(|n| n.kind() == Kind::Step && n.host == host);
            if !(before && self.joins_after(&from, host, seen)) && self.reaches(host, &from, step) {
                return false;
            }
        }
        true
    }

    /// Does a path of links lead from `from` to `to` that does not pass through `past`?
    fn reaches(&self, from: &str, to: &str, past: &str) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut todo = vec![from.to_string()];
        while let Some(at) = todo.pop() {
            if at == to {
                return true;
            }
            if at != past && seen.insert(at.clone()) {
                todo.extend(self.flow.edges.iter().filter(|e| e.from == at).map(|e| e.to.clone()));
            }
        }
        false
    }

    /// The queued prompt's reasoning level is the agent's from now on. None when the agent has it
    /// already, else whether that changes the command line its process runs with.
    fn tune_for(&mut self, step: &str, host: &str) -> Option<bool> {
        let effort = self.node(step).map(|s| s.effort.trim().to_string()).filter(|e| !e.is_empty())?;
        let h = self.node(host).filter(|h| h.effort != effort).cloned()?;
        if let Some(n) = self.flow.nodes.iter_mut().find(|n| n.id == host) {
            n.effort = effort.clone();
        }
        self.state_dirty = true;
        Some(self.cfg.agents.get(&h.agent).is_some_and(|a| tuning_args(a, &h.model, &h.effort) != tuning_args(a, &h.model, &effort)))
    }

    /// The card of the queued prompt goes into its agent: its links leave from (or lead into) the agent
    /// instead, keeping their ids, so what is held or under way along them stays; a link the agent
    /// already has, or one to itself, goes. What arrived for the prompt and was not taken up yet waits
    /// at the agent. The page moves a selection or an open reader along (`merged`).
    fn merge_step(&mut self, step: &str, host: &str) {
        let moving: Vec<Edge> = self.flow.edges.iter().filter(|e| e.from == step || e.to == step).cloned().collect();
        let mut gone: Vec<String> = Vec::new();
        for e in moving {
            let (from, to) = if e.from == step { (host.to_string(), e.to.clone()) } else { (e.from.clone(), host.to_string()) };
            if from == to || self.flow.edges.iter().any(|x| x.id != e.id && x.from == from && x.to == to) {
                gone.push(e.id.clone());
                continue;
            }
            if let Some(x) = self.flow.edges.iter_mut().find(|x| x.id == e.id) {
                x.from = from;
                x.to = to;
            }
        }
        self.flow.edges.retain(|e| !gone.contains(&e.id));
        for r in self.rt.values_mut() {
            r.unsent.retain(|u| !gone.contains(u));
        }
        let early: Vec<(String, Input)> = self.rt.get_mut(step).map(|r| std::mem::take(&mut r.inputs)).unwrap_or_default();
        if let Some(h) = self.rt.get_mut(host) {
            h.inputs.extend(early.into_iter().filter(|(eid, _)| !gone.contains(eid)));
            h.ui = true;
        }
        self.bcast(&json!({"t": "merged", "from": step, "into": host}));
        self.drop_node(step, false);
        self.flow_changed();
        self.state_dirty = true;
    }

    /// The agent's process runs at another reasoning level than its card now says: it is ended and
    /// started again on the same conversation, and takes up what is queued then. False when it keeps
    /// running (not running at all, or a CLI whose conversation could not be taken up again): the
    /// level applies at its next start.
    pub(super) fn restart_tuned(&mut self, id: &str) -> bool {
        let Some(a) = self.session_cfg(id) else { return false };
        let Some(rt) = self.rt.get(id).filter(|r| r.alive && !r.spawning && r.retiring.is_none()) else { return false };
        if a.resume_arg.trim().is_empty() {
            return false;
        }
        if rt.session.is_some() {
            self.kill(id);
            self.start(id);
            return true;
        }
        // (it names its conversation on the way out: ended the way a person would, it is started
        // again once it has, see the end of a pane)
        if a.exit_keys.is_empty() || a.resume_hint.trim().is_empty() {
            return false;
        }
        self.retire(id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    /// `s` is a prompt queued on the agent `a`; `a` has taken it into its queue.
    fn queued(e: &mut Engine, s: &str, a: &str, prompt: &str) {
        add_kind(e, s, s, "step", prompt);
        e.flow.nodes.iter_mut().find(|n| n.id == s).unwrap().host = a.into();
        let qid = e.enqueue(a, prompt.into(), prompt.into(), Src::Reply(s.into()), false);
        let r = e.rt.get_mut(s).unwrap();
        r.step_wait = Some(a.into());
        r.unstarted = Some((qid, Vec::new()));
    }

    fn done(e: &mut Engine, id: &str, out: &str) {
        let r = e.rt.get_mut(id).unwrap();
        (r.finished, r.armed, r.ok, r.output) = (true, true, Some(true), Some(out.into()));
    }

    fn texts(e: &Engine, id: &str) -> Vec<String> {
        e.rt[id].queue.iter().map(|q| q.text.clone()).collect()
    }

    fn edges(e: &Engine) -> Vec<(String, String, String)> {
        let mut v: Vec<_> = e.flow.edges.iter().map(|x| (x.id.clone(), x.from.clone(), x.to.clone())).collect();
        v.sort();
        v
    }

    #[test]
    fn a_queued_prompt_joins_its_agent_with_its_links_when_the_agent_takes_it_up() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        done(&mut e, "a", "built");
        add(&mut e, "b", "notes", "write notes");
        done(&mut e, "b", "the notes");
        add(&mut e, "x", "test it", "test {{inputs}}");
        queued(&mut e, "s", "a", "now add tests");
        link(&mut e, "e1", "a", "s", On::Done);
        link(&mut e, "e2", "b", "s", On::Done);
        link(&mut e, "e3", "s", "x", On::Done);
        assert!(e.joins("s", "a"));
        assert!(!e.take_up_queued("a"), "no reasoning level of its own: nothing to start again");
        assert!(e.node("s").is_none() && !e.rt.contains_key("s"), "the card is gone");
        let want = |id: &str, f: &str, t: &str| (id.to_string(), f.to_string(), t.to_string());
        assert_eq!(edges(&e), vec![want("e2", "b", "a"), want("e3", "a", "x")], "its links are the agent's, the one from the agent goes");
        let q = &e.rt["a"].queue;
        assert_eq!((q.len(), q[0].src.clone(), q[0].text.as_str()), (1, Src::Flow, "now add tests"), "its prompt is the agent's own");
        // the answer is the agent's result, and goes down the link the card had
        e.rt.get_mut("a").unwrap().queue.clear();
        e.rt.get_mut("a").unwrap().finished = false;
        e.settle("a", true, "tests added".into());
        assert_eq!(e.rt["a"].output.as_deref(), Some("tests added"));
        assert!(e.rt["x"].queue.iter().any(|q| q.text.contains("tests added")), "{:?}", texts(&e, "x"));
    }

    #[test]
    fn what_follows_the_agent_takes_the_answer_to_the_prompt_that_joined_it() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "c", "check", "check {{inputs}}");
        link(&mut e, "e1", "a", "c", On::Done);
        queued(&mut e, "s", "a", "also the footer");
        link(&mut e, "e2", "a", "s", On::Done);
        // the agent's own result waits for the prompt queued on it
        e.rt.get_mut("a").unwrap().armed = true;
        e.settle("a", true, "built".into());
        assert!(e.rt["c"].queue.is_empty() && e.rt["c"].inputs.is_empty(), "c waits for the queued prompt");
        e.take_up_queued("a");
        let r = e.rt.get_mut("a").unwrap();
        r.queue.clear();
        r.finished = false;
        e.recheck_joins();
        assert!(e.rt["c"].queue.is_empty(), "still waiting while the agent works on it");
        e.settle("a", true, "footer done".into());
        assert!(e.rt["c"].queue.iter().any(|q| q.text.contains("footer done")), "{:?}", texts(&e, "c"));
    }

    #[test]
    fn prompts_queued_side_by_side_join_the_agent_one_after_the_other_from_the_top() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "x", "x", "x {{inputs}}");
        add(&mut e, "y", "y", "y {{inputs}}");
        add_kind(&mut e, "low", "low", "step", "the docs");
        add_kind(&mut e, "top", "top", "step", "the tests");
        for (id, y) in [("low", 300.0), ("top", 0.0)] {
            let n = e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap();
            (n.host, n.y) = ("a".into(), y);
        }
        link(&mut e, "e1", "a", "low", On::Done);
        link(&mut e, "e2", "a", "top", On::Done);
        link(&mut e, "e3", "low", "y", On::Done);
        link(&mut e, "e4", "top", "x", On::Done);
        e.rt.get_mut("a").unwrap().armed = true;
        e.settle("a", true, "built".into());
        let srcs = |e: &Engine| e.rt["a"].queue.iter().map(|q| q.src.clone()).collect::<Vec<_>>();
        assert_eq!(srcs(&e), vec![Src::Reply("top".into())], "the top one is asked first, alone");
        assert!(e.rt["a"].unsent.contains(&"e1".to_string()), "the lower one waits for it");
        // the top one joins the agent; the lower one still waits while the agent answers it
        e.take_up_queued("a");
        assert!(e.node("top").is_none() && e.node("low").is_some());
        let r = e.rt.get_mut("a").unwrap();
        (r.finished, r.queue) = (false, Default::default());
        e.recheck_joins();
        assert!(srcs(&e).is_empty(), "nothing else is asked meanwhile");
        e.settle("a", true, "tests written".into());
        assert_eq!(srcs(&e), vec![Src::Reply("low".into())], "then the lower one");
        assert!(e.rt["x"].queue.is_empty(), "what followed the top one waits for the agent's last answer");
        e.take_up_queued("a");
        let r = e.rt.get_mut("a").unwrap();
        (r.finished, r.queue) = (false, Default::default());
        e.settle("a", true, "docs written".into());
        let want = |id: &str, f: &str, t: &str| (id.to_string(), f.to_string(), t.to_string());
        assert_eq!(edges(&e), vec![want("e3", "a", "y"), want("e4", "a", "x")], "both joined, in turn");
        assert!(texts(&e, "x").iter().any(|t| t.contains("docs written")) && texts(&e, "y").iter().any(|t| t.contains("docs written")));
    }

    #[test]
    fn a_queued_prompt_that_waits_for_work_after_its_agent_stays_a_card() {
        // the review network: a builds, r reviews, s asks a to fix what r found
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "r", "review", "review it");
        add(&mut e, "k", "check", "check it");
        queued(&mut e, "s", "a", "fix the findings");
        link(&mut e, "e1", "a", "r", On::Done);
        link(&mut e, "e2", "r", "s", On::Done);
        link(&mut e, "e3", "a", "s", On::Done);
        link(&mut e, "e4", "s", "k", On::Done);
        assert!(!e.joins("s", "a"), "the review is after the agent: joined, it would loop back into it");
        e.take_up_queued("a");
        assert!(e.node("s").is_some() && e.flow.edges.len() == 4);
        assert_eq!(e.rt["a"].queue[0].src, Src::Reply("s".into()), "it is answered for the card, as before");
        // a loop through the card itself is no reason to stay
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "x", "x", "x");
        queued(&mut e, "s", "a", "again");
        link(&mut e, "e1", "a", "s", On::Done);
        link(&mut e, "e2", "s", "x", On::Done);
        link(&mut e, "e3", "x", "s", On::Done);
        assert!(e.joins("s", "a"));
        // one queued after another of the same agent (the bar queues them so): the first joins first
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "r", "review", "review it");
        queued(&mut e, "s", "a", "first");
        queued(&mut e, "t", "a", "then");
        link(&mut e, "e1", "a", "s", On::Done);
        link(&mut e, "e2", "s", "t", On::Done);
        assert!(e.joins("t", "a"));
        link(&mut e, "e3", "a", "r", On::Done);
        link(&mut e, "e4", "r", "s", On::Done);
        assert!(!e.joins("s", "a") && !e.joins("t", "a"), "behind one that stays a card, it stays one too");
    }

    #[test]
    fn a_link_the_agent_already_has_is_not_doubled() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add(&mut e, "b", "b", "b");
        add(&mut e, "x", "x", "x");
        queued(&mut e, "s", "a", "more");
        link(&mut e, "e1", "a", "s", On::Done);
        link(&mut e, "e2", "b", "a", On::Done);
        link(&mut e, "e3", "b", "s", On::Done);
        link(&mut e, "e4", "a", "x", On::Done);
        link(&mut e, "e5", "s", "x", On::Done);
        e.rt.get_mut("b").unwrap().unsent.push("e3".into());
        e.take_up_queued("a");
        let want = |id: &str, f: &str, t: &str| (id.to_string(), f.to_string(), t.to_string());
        assert_eq!(edges(&e), vec![want("e2", "b", "a"), want("e4", "a", "x")]);
        assert!(e.rt["b"].unsent.is_empty(), "a link that went is not held for anymore");
    }

    #[test]
    fn the_reasoning_of_a_queued_prompt_becomes_the_agents_when_it_is_taken_up() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "claude".into();
        queued(&mut e, "s", "a", "the hard part");
        link(&mut e, "e1", "a", "s", On::Done);
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().effort = "xhigh".into();
        assert_eq!(e.node("a").unwrap().effort, "", "nothing changes before it is asked");
        assert!(e.take_up_queued("a"), "its process runs at another level");
        assert_eq!(e.node("a").unwrap().effort, "xhigh");
        // claude takes its conversation up again by the id weft gave it: the process is ended and
        // started again (here: there is no tmux, so it waits to be started), the prompt still queued
        let r = e.rt.get_mut("a").unwrap();
        (r.runs, r.session) = (1, Some("6f1c0d8e-0000-4000-8000-000000000000".into()));
        assert!(e.restart_tuned("a"));
        assert!(!e.rt["a"].alive && e.rt["a"].queue.len() == 1 && e.rt["a"].queue[0].src == Src::Flow);
        // the same level again is no reason to start it again
        queued(&mut e, "t", "a", "and this");
        e.rt.get_mut("a").unwrap().queue.pop_front();
        e.flow.nodes.iter_mut().find(|n| n.id == "t").unwrap().effort = "xhigh".into();
        assert!(!e.take_up_queued("a"));
    }

    #[test]
    fn a_process_that_cannot_take_its_conversation_up_again_keeps_running() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "claude".into();
        // (claude before weft knew its session: none to take up)
        e.cfg.agents.get_mut("claude").unwrap().resume_arg.clear();
        assert!(!e.restart_tuned("a"));
        assert!(e.rt["a"].alive);
    }
}
