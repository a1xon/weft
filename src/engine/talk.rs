//! Agents that talk: the person links two agents at the tops of their cards (more join through any
//! of them), and they coordinate while they work. Each is told once who the others are and what to
//! coordinate on; it says something to them with `weft tell`, and weft types that into theirs.
//! Nobody else hears it, and nothing fires along a talk: their results and their flows stay as they
//! are.

use super::*;

/// One agent may say this many things to the others in `TALK_WINDOW_MS`: two agents that answer
/// each other forever would otherwise never stop.
const TALK_BURST: usize = 10;
const TALK_WINDOW_MS: u64 = 10 * 60 * 1000;
/// How much of what an agent said the canvas keeps.
const TALK_KEEP: usize = 60;
/// Changes to the talks settle this long before the agents are told (three lines drawn are one change).
const TALK_SETTLE: Duration = Duration::from_millis(1200);
/// While the person writes a talk's topic the agents wait for it, but not forever (a tab closed mid-word).
const TALK_TYPING: Duration = Duration::from_secs(30);

/// Something one agent said to the others.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct TalkMsg {
    pub(super) at: u64,
    pub(super) to: Vec<String>,
    pub(super) text: String,
}

/// An agent's side of its talks.
#[derive(Default)]
pub(super) struct TalkRt {
    /// what this session was told about the others (`peers,...\ntopic`; "" = nothing)
    pub(super) told: String,
    /// its group changed: tell it as soon as it can take a message
    pub(super) due: bool,
    /// what the others said while it could not take it (no task yet, a dialog, a request of weft's)
    pub(super) inbox: Vec<String>,
    /// what it said, newest last
    pub(super) said: Vec<TalkMsg>,
}

/// Only an agent that reads what it is told can talk: not a command, a queued prompt or a check.
fn can_talk(n: Option<&Node>) -> bool {
    n.is_some_and(Node::reads_prompts)
}

/// A topic as the agents are told it: one line, not endless.
fn clean_topic(t: &str) -> String {
    head_chars(&t.split_whitespace().collect::<Vec<_>>().join(" "), 300)
}

impl Engine {
    /// The agents `id` talks with: everyone its talks reach, through the others too, in canvas order.
    pub(super) fn talk_peers(&self, id: &str) -> Vec<String> {
        let mut seen = vec![id.to_string()];
        let mut i = 0;
        while i < seen.len() {
            let at = seen[i].clone();
            for t in &self.flow.talks {
                let other = if t.a == at {
                    &t.b
                } else if t.b == at {
                    &t.a
                } else {
                    continue;
                };
                if !seen.contains(other) && self.node(other).is_some() {
                    seen.push(other.clone());
                }
            }
            i += 1;
        }
        let order = |p: &String| self.flow.nodes.iter().position(|n| n.id == *p).unwrap_or(usize::MAX);
        let mut peers = seen.split_off(1);
        peers.sort_by_key(order);
        peers
    }

    /// What a group coordinates on: the topics of its talks, each once.
    fn talk_topic(&self, id: &str, peers: &[String]) -> String {
        let mut out: Vec<&str> = Vec::new();
        for t in self.flow.talks.iter().filter(|t| t.a == id || peers.contains(&t.a)) {
            let tp = t.topic.trim();
            if !tp.is_empty() && !out.contains(&tp) {
                out.push(tp);
            }
        }
        out.join("; ")
    }

    /// One line per peer: its id, its name, what it was asked, and where, if not where `id` works.
    fn talk_lines(&self, id: &str, peers: &[String]) -> String {
        let mine = self.work_dir(id);
        let mut lines = Vec::new();
        for p in peers {
            let Some(n) = self.node(p) else { continue };
            let task = if n.prompt.trim().is_empty() { self.rt.get(p).map(|r| r.asked.clone()).unwrap_or_default() } else { n.prompt.clone() };
            let task = task.split_whitespace().collect::<Vec<_>>().join(" ");
            let task = if task.is_empty() { "(no task yet)".to_string() } else { head_chars(&task, 160) };
            let d = self.work_dir(p);
            let at = if d != mine { format!(" (works in {})", d.display()) } else { String::new() };
            lines.push(format!("- {p} \"{}\"{at}: {task}", n.name));
        }
        lines.join("\n")
    }

    /// What `id` has to be told about its talks now, if anything: everything the first time in a
    /// session, later who is new and the topic. Fewer of them with the same topic, or nobody left,
    /// needs no message (what it says to someone who left is refused). Marks it told.
    pub(super) fn talk_news(&mut self, id: &str) -> Option<String> {
        let peers = self.talk_peers(id);
        let topic = self.talk_topic(id, &peers);
        let sig = if peers.is_empty() { String::new() } else { format!("{}\n{topic}", peers.join(",")) };
        let before = {
            let rt = self.rt.get_mut(id)?;
            rt.talk.due = false;
            if rt.talk.told == sig {
                return None;
            }
            std::mem::replace(&mut rt.talk.told, sig)
        };
        if peers.is_empty() {
            return None;
        }
        let (old_peers, old_topic) = before.split_once('\n').unwrap_or(("", ""));
        let old: Vec<&str> = old_peers.split(',').filter(|s| !s.is_empty()).collect();
        let new: Vec<&str> = peers.iter().map(String::as_str).filter(|p| !old.contains(p)).collect();
        if !before.is_empty() && new.is_empty() && old_topic == topic {
            return None;
        }
        let exe = self.boot.exe.clone();
        let lines = self.talk_lines(id, &peers);
        Some(if before.is_empty() {
            words::talk_brief(&exe, &lines, &peers.join(","), &topic)
        } else if new.is_empty() {
            words::talk_refocus(&peers.join(","), &topic)
        } else {
            words::talk_joined(&exe, &lines, &new.join(","), &topic)
        })
    }

    /// Can `id` be typed to by the others right now? Not before it has its task (the task carries
    /// what is waiting), not into a dialog, not while it asks the person something, and not in the
    /// middle of a request of weft's own (a commit, a handover note: their answer is read).
    fn talk_ready(&self, id: &str) -> bool {
        let Some(r) = self.rt.get(id) else { return false };
        let weft_asks = r.reply_to.as_deref().and_then(|t| self.node(t)).is_some_and(|n| n.kind() != Kind::Step);
        r.alive && r.pane.is_some() && (r.armed || r.cur.is_some()) && !r.attn && r.issue.is_none() && r.typing.is_none() && !weft_asks
    }

    /// Type into `id` what it has to hear about its talks, and `msg` after it. False: it cannot take
    /// it now, and `msg` waits for it.
    fn talk_flush(&mut self, id: &str, msg: Option<String>) -> bool {
        if !self.talk_ready(id) {
            if let (Some(m), Some(r)) = (msg, self.rt.get_mut(id)) {
                r.talk.inbox.push(m);
            }
            return false;
        }
        let mut parts: Vec<String> = Vec::new();
        if self.rt.get(id).is_some_and(|r| r.talk.due) {
            parts.extend(self.talk_news(id));
        }
        if let Some(r) = self.rt.get_mut(id) {
            parts.append(&mut r.talk.inbox);
        }
        parts.extend(msg);
        if !parts.is_empty() {
            self.raw_type(id, &parts.join("\n\n"), true);
        }
        true
    }

    /// What goes with a prompt the agent is given: what it has not been told about its talks yet,
    /// and what the others said before it could take it. Nothing while the talks are still being
    /// drawn or their topic written: the task does not wait, and all of it follows in one message.
    pub(super) fn talk_with_prompt(&mut self, id: &str) -> String {
        if self.talk_due.is_some() {
            return String::new();
        }
        let mut parts: Vec<String> = self.talk_news(id).into_iter().collect();
        if let Some(r) = self.rt.get_mut(id) {
            parts.append(&mut r.talk.inbox);
        }
        parts.join("\n\n")
    }

    /// The talks changed: the agents are told once the person has finished drawing, or writing the
    /// topic (`typing`: its field has the keyboard).
    fn talk_changed(&mut self, typing: bool) {
        self.talk_due = Some(Instant::now() + if typing { TALK_TYPING } else { TALK_SETTLE });
        self.flow_changed();
    }

    /// A talk whose agent is gone (deleted, filed, turned into something else) goes with it.
    pub(super) fn prune_talks(&mut self) {
        let before = std::mem::take(&mut self.flow.talks);
        let n = before.len();
        self.flow.talks = before.into_iter().filter(|t| can_talk(self.node(&t.a)) && can_talk(self.node(&t.b))).collect();
        if self.flow.talks.len() != n {
            self.talk_due = Some(Instant::now() + TALK_SETTLE);
        }
    }

    /// Housekeeping: once the talks have settled, everyone whose group changed is due to hear it;
    /// what waits for an agent that can take it now is typed.
    pub(super) fn tick_talk(&mut self) {
        if self.talk_due.is_some_and(|t| Instant::now() >= t) {
            self.talk_due = None;
            let ids: Vec<String> = self.flow.nodes.iter().filter(|n| n.kind() == Kind::Agent).map(|n| n.id.clone()).collect();
            for id in ids {
                let linked = !self.talk_peers(&id).is_empty();
                if let Some(r) = self.rt.get_mut(&id) {
                    if linked || !r.talk.told.is_empty() {
                        r.talk.due = true;
                    }
                    if !linked {
                        // what the others said is not for an agent that no longer talks with them
                        r.talk.inbox.clear();
                    }
                }
            }
        }
        let ids: Vec<String> = self.rt.iter().filter(|(_, r)| r.talk.due || !r.talk.inbox.is_empty()).map(|(i, _)| i.clone()).collect();
        for id in ids {
            // (one that has not started yet hears it all with its task)
            self.talk_flush(&id, None);
        }
    }

    /// Everything the agents said, oldest first: the canvas shows it on their lines.
    pub(super) fn talk_log(&self) -> Vec<Value> {
        let mut all: Vec<(&String, &TalkMsg)> =
            self.rt.iter().flat_map(|(id, r)| r.talk.said.iter().map(move |m| (id, m))).collect();
        all.sort_by_key(|(_, m)| m.at);
        all.iter().map(|(id, m)| json!({"from": id, "to": m.to, "text": m.text, "at": m.at})).collect()
    }

    pub(super) fn cmd_talk_add(&mut self, a: String, b: String, topic: String, typing: bool) -> Value {
        if a == b || !can_talk(self.node(&a)) || !can_talk(self.node(&b)) {
            return json!({"ok": false, "error": "only agents can talk to each other (not a command, a queued prompt, a check or a git step)"});
        }
        if let Some(t) = self.flow.talks.iter().find(|t| (t.a == a && t.b == b) || (t.a == b && t.b == a)) {
            return json!({"ok": true, "id": t.id});
        }
        let id = self.next_id("t", self.flow.talks.iter().map(|t| t.id.clone()));
        self.flow.talks.push(Talk { id: id.clone(), a, b, topic: clean_topic(&topic) });
        self.talk_changed(typing);
        json!({"ok": true, "id": id})
    }

    /// A new topic (`None`: the person started writing one: the agents wait for it).
    pub(super) fn cmd_talk_set(&mut self, id: String, topic: Option<String>, typing: bool) -> Value {
        let Some(t) = self.flow.talks.iter_mut().find(|t| t.id == id) else {
            return json!({"ok": false, "error": "no such talk"});
        };
        let topic = topic.map(|v| clean_topic(&v));
        if topic.as_ref().is_some_and(|v| *v != t.topic) {
            t.topic = topic.unwrap_or_default();
            self.talk_changed(typing);
        } else if typing {
            self.talk_due = Some(Instant::now() + TALK_TYPING);
        } else if self.talk_due.is_some() {
            // done writing (it stayed as it was): what was waiting for it goes now
            self.talk_due = Some(Instant::now() + TALK_SETTLE);
        }
        json!({"ok": true})
    }

    pub(super) fn cmd_talk_del(&mut self, id: String) -> Value {
        let n = self.flow.talks.len();
        self.flow.talks.retain(|t| t.id != id);
        if self.flow.talks.len() != n {
            self.talk_changed(false);
        }
        json!({"ok": true})
    }

    /// An agent says something to the agents it talks with (`to`: ids or names, comma or space
    /// separated; "all" or nothing = all of them). It is typed into each of them now, or as soon as
    /// it can take it; the answer says which.
    pub(super) fn cmd_tell(&mut self, from: String, to: String, text: String) -> Value {
        let from = self.resolve_host(&from);
        let Some(name) = self.node(&from).map(|n| n.name.clone()) else {
            return json!({"ok": false, "error": format!("no agent {from}")});
        };
        let peers = self.talk_peers(&from);
        if peers.is_empty() {
            return json!({"ok": false, "error": format!(
                "{from} is not linked with any agent to talk to: the person links agents at the top of their cards. Nothing was sent.")});
        }
        let text = text.trim();
        if text.is_empty() {
            return json!({"ok": false, "error": "nothing to say: weft tell <id|all> \"<message>\""});
        }
        let who = || {
            peers.iter().map(|p| format!("{p} \"{}\"", self.node(p).map(|n| n.name.as_str()).unwrap_or(""))).collect::<Vec<_>>().join(", ")
        };
        let mut targets: Vec<String> = Vec::new();
        for w in to.split([',', ' ']).map(str::trim).filter(|w| !w.is_empty()) {
            if w.eq_ignore_ascii_case("all") || w == "*" {
                targets = peers.clone();
                break;
            }
            let hit = peers.iter().find(|p| p.as_str() == w || self.node(p).is_some_and(|n| n.name.eq_ignore_ascii_case(w)));
            match hit {
                Some(p) if !targets.contains(p) => targets.push(p.clone()),
                Some(_) => {}
                None => {
                    return json!({"ok": false, "error": format!(
                        "{w} is not one of the agents you talk with ({}). Nothing was sent.", who())});
                }
            }
        }
        if targets.is_empty() {
            targets = peers.clone();
        }
        let now = now_ms();
        let recent = self.rt.get(&from).map_or(0, |r| r.talk.said.iter().filter(|m| m.at + TALK_WINDOW_MS > now).count());
        if recent >= TALK_BURST {
            return json!({"ok": false, "error": format!(
                "you have said {recent} things to them in the last 10 minutes; nothing was sent. Go on with your task.")});
        }
        let text: String = text.chars().take(4000).collect();
        let msg = words::talk_message(&self.boot.exe, &from, &name, &text);
        let mut told = Vec::new();
        let mut later = Vec::new();
        for t in &targets {
            if self.talk_flush(t, Some(msg.clone())) {
                told.push(t.clone());
            } else {
                let r = &self.rt[t];
                later.push(if !r.alive && r.runs > 0 {
                    format!("{t} is not running: it hears it when it is started again")
                } else if !r.alive || !(r.armed || r.cur.is_some()) {
                    format!("{t} has not started yet: it hears it with its task")
                } else {
                    format!("{t} cannot take it this moment: it hears it as soon as it can")
                });
            }
        }
        if let Some(r) = self.rt.get_mut(&from) {
            r.talk.said.push(TalkMsg { at: now, to: targets.clone(), text: text.clone() });
            let over = r.talk.said.len().saturating_sub(TALK_KEEP);
            r.talk.said.drain(..over);
        }
        self.state_dirty = true;
        self.bcast(&json!({"t": "talk", "from": from, "to": targets, "text": text, "at": now}));
        let mut said = Vec::new();
        if !told.is_empty() {
            said.push(format!("told {}", told.join(", ")));
        }
        said.extend(later);
        json!({"ok": true, "said": said.join(". ")})
    }
}
