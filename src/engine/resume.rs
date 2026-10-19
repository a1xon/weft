//! A card keeps what its agent knows when the agent's process ends: rested after its work, stopped,
//! filed and brought back. weft gives a new session an id of its choosing (`session_arg`), or reads the
//! one the CLI names when it ends (`resume_hint`, after `exit_keys` end it the way a person would), and
//! starts the card's next process on the same conversation (`resume_arg`). So "then", a commit request
//! or a hand-over note reach an agent that remembers its work. Where that cannot be (a CLI that cannot
//! resume, a session that is gone), the first prompt of the new session says what the card was doing.
//! A twin (`Node.twin`) starts on a copy of another card's conversation (`fork_arg`) and goes its own way.

use super::*;

/// How long a resting agent gets to end by itself before its window is closed.
const RETIRE_MS: u64 = 6_000;

/// A random version 4 UUID: the form the CLIs want a session id in.
pub(super) fn new_session_id() -> String {
    let mut b = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b));
    if read.is_err() {
        // (no /dev/urandom: the clock and the process are unique enough for one card)
        let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default() ^ (std::process::id() as u128) << 64;
        b = t.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// The session id the CLI named on its screen, after `hint` ("codex resume 01a1…", "--conversation=f416…").
pub(super) fn read_session(text: &str, hint: &str) -> Option<String> {
    let hint = hint.trim_start();
    if hint.is_empty() {
        return None;
    }
    // the last mention that names one ("Or run codex resume and select it" names none)
    text.rmatch_indices(hint).find_map(|(at, _)| {
        let rest = text[at + hint.len()..].trim_start_matches([' ', '"', '\'']);
        let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        (id.len() >= 8 && id.chars().any(|c| c.is_ascii_digit())).then_some(id)
    })
}

/// `cmd` with the session's arguments right after the agent's own command (`base`, as configured, with
/// weft's `command ` in front or not) and before the flags weft adds: Codex takes `resume <id>` as a
/// subcommand, and a command like `python3 agent.py --name x` keeps its program and script together.
/// A command line that does not start with `base` gets them at the end.
pub(super) fn with_session(cmd: &str, base: &str, args: &str) -> String {
    let (args, base) = (args.trim(), base.trim());
    if args.is_empty() {
        return cmd.to_string();
    }
    let at = [format!("command {base}"), base.to_string()]
        .into_iter()
        .filter(|b| !base.is_empty() && cmd.starts_with(b.as_str()) && cmd[b.len()..].chars().next().is_none_or(|c| c == ' '))
        .map(|b| b.len())
        .next();
    match at {
        Some(i) => format!("{} {args}{}", &cmd[..i], &cmd[i..]),
        None => format!("{cmd} {args}"),
    }
}

/// A template with the session id in it; an id that could break the command line is not used.
fn fill(tpl: &str, id: &str) -> Option<String> {
    plain_word(id).then(|| tpl.trim().replace("{}", id))
}

impl Engine {
    /// The agent CLI behind a card that weft runs as an agent (not a command line of its own).
    pub(super) fn session_cfg(&self, id: &str) -> Option<AgentCfg> {
        let n = self.node(id).filter(|n| n.kind() == Kind::Agent && n.finish == Finish::Idle && n.cmd.trim().is_empty())?;
        self.cfg.agents.get(&n.agent).cloned()
    }

    /// The card's process has ended and its next one would take up the same conversation.
    pub(super) fn resumable(&self, id: &str) -> bool {
        let Some(rt) = self.rt.get(id) else { return false };
        !rt.alive
            && !rt.spawning
            && !rt.resuming
            && rt.runs > 0
            && rt.session.is_some()
            && self.session_cfg(id).is_some_and(|a| !a.resume_arg.trim().is_empty())
    }

    /// The agent behind the card knows what the card did: its process is the one that did it (or a
    /// resumed one), or the next one will be. An agent that never ran has nothing to forget.
    pub(super) fn remembers(&self, id: &str) -> bool {
        let Some(rt) = self.rt.get(id) else { return false };
        if rt.runs == 0 {
            return true;
        }
        if rt.spawning {
            return rt.resuming;
        }
        if rt.alive {
            return !rt.blank;
        }
        self.resumable(id)
    }

    /// The arguments that put the next process of this card on its conversation: the same one again
    /// (`resume_arg`), a copy of another card's for a twin's first start (`fork_arg`), or a new one with
    /// an id weft picks (`session_arg`). Called when the process is started.
    pub(super) fn session_args(&mut self, id: &str) -> String {
        let Some(a) = self.session_cfg(id) else { return String::new() };
        let twin = self.node(id).map(|n| n.twin.clone()).unwrap_or_default();
        let from = (!twin.is_empty()).then(|| self.rt.get(&twin).and_then(|r| r.session.clone())).flatten();
        let Some(rt) = self.rt.get_mut(id) else { return String::new() };
        if std::mem::take(&mut rt.resuming) {
            // the last start took the conversation up again, and that process ended before it took
            // anything: it cannot be resumed (gone, or kept for another folder)
            rt.session = None;
        }
        rt.twin_due = false;
        let mut args: Vec<String> = Vec::new();
        if rt.runs > 0 {
            if let Some(s) = rt.session.clone().and_then(|s| fill(&a.resume_arg, &s)).filter(|s| !s.is_empty()) {
                rt.resuming = true;
                return s;
            }
        } else if let Some(s) = from.and_then(|s| fill(&a.fork_arg, &s)).filter(|s| !s.is_empty()) {
            args.push(s);
            rt.twin_due = true;
        }
        let own = new_session_id();
        match fill(&a.session_arg, &own).filter(|s| !s.is_empty()) {
            Some(s) => {
                args.push(s);
                rt.session = Some(own);
            }
            // the CLI names it when it ends
            None => rt.session = None,
        }
        args.join(" ")
    }

    /// A finished agent rests: it is ended the way a person would end it, so it names its session
    /// on the way out (when weft does not know it yet); `tick_retire` closes the window after that.
    pub(super) fn retire(&mut self, id: &str) {
        let a = self.session_cfg(id);
        let (keys, hint) = a.map(|a| (a.exit_keys, a.resume_hint)).unwrap_or_default();
        let (Some(rt), Some(tm)) = (self.rt.get_mut(id), self.tmux.clone()) else { return self.kill(id) };
        if rt.session.is_some() || keys.is_empty() || hint.trim().is_empty() || rt.pane.is_none() {
            return self.kill(id);
        }
        let pane = rt.pane.clone().unwrap_or_default();
        for k in &keys {
            tm.send_key(&pane, k);
        }
        rt.retiring = Some(now_ms());
    }

    /// Agents that were asked to end: once the process is gone (or did not go in time), the session
    /// it named is kept and the window closes. A pane whose program ended by itself names its session too.
    pub(super) fn tick_retire(&mut self) {
        let now = now_ms();
        let ended: Vec<String> =
            self.rt.iter().filter(|(_, r)| r.session.is_none() && !r.alive && r.pane.is_some()).map(|(i, _)| i.clone()).collect();
        for id in ended {
            self.note_session(&id);
        }
        let mut close: Vec<String> = Vec::new();
        for (id, rt) in self.rt.iter_mut() {
            let ended = !rt.alive && rt.pane.is_some();
            let late = rt.retiring.is_some_and(|t| now >= t + RETIRE_MS);
            if rt.retiring.is_some() && (ended || late) {
                rt.retiring = None;
                close.push(id.clone());
            }
        }
        for id in close {
            self.kill(&id);
            // (what was queued on it meanwhile, or what it was ended for, goes to a new process)
            self.restart_if_queued(&id);
        }
    }

    /// The session the card's ended process named on its screen, when weft does not know it yet.
    pub(super) fn note_session(&mut self, id: &str) {
        let hint = self.node(id).and_then(|n| self.cfg.agents.get(&n.agent)).map(|a| a.resume_hint.clone());
        let Some(rt) = self.rt.get_mut(id).filter(|r| r.session.is_none()) else { return };
        if let Some(s) = hint.and_then(|h| read_session(&rt.parser.screen().contents(), &h)) {
            rt.session = Some(s);
            self.state_dirty = true;
        }
    }

    /// What goes in front of the first prompt of a new process: for a twin, that it is one; for a card
    /// whose old conversation could not be taken up again, what it was doing.
    pub(super) fn session_note(&mut self, id: &str) -> Option<String> {
        let node = self.node(id)?.clone();
        let src_name = self.node(&node.twin).map(|n| n.name.clone()).unwrap_or_else(|| node.twin.clone());
        let rt = self.rt.get_mut(id)?;
        if std::mem::take(&mut rt.twin_due) {
            rt.blank = false;
            return Some(words::twin_intro(&node.twin, &src_name, id));
        }
        if !std::mem::take(&mut rt.blank) {
            return None;
        }
        let task = if node.prompt.trim().is_empty() { rt.asked.clone() } else { node.prompt.clone() };
        let said = rt.output.as_deref().map(|o| tail_bytes(o.trim(), 3000).to_string()).unwrap_or_default();
        Some(words::recap(&head_chars(task.trim(), 2000), &head_chars(rt.asked.trim(), 1200), &said))
    }

    /// A twin of `from`: a new card on a copy of its conversation, asked `text`. It works where `from`
    /// works, with the same agent, model and reasoning, and starts at once.
    pub(super) fn cmd_twin(&mut self, from: String, text: String, x: f64, y: f64) -> Value {
        let Some(src) = self.node(&from).filter(|n| n.kind() == Kind::Agent).cloned() else {
            return json!({"ok": false, "error": "no such agent"});
        };
        if !self.can_twin(&from) {
            return json!({"ok": false, "error": "this agent's conversation cannot be copied (its CLI cannot, or weft does not know the session yet)"});
        }
        if text.trim().is_empty() {
            return json!({"ok": false, "error": "say what the twin should do"});
        }
        // where the copied conversation did its work: the source's own worktree, if it has one
        let cwd = self.live_worktree(&from).map_or_else(|| src.cwd.clone(), str::to_string);
        let spec = json!({
            "agent": src.agent, "model": src.model, "effort": src.effort, "yolo": src.yolo, "cwd": cwd,
            "addenda": src.addenda, "x": x, "y": y, "prompt": text, "twin": from,
        });
        let r = self.cmd_node_add(spec, None, false);
        let Some(id) = r["id"].as_str().map(str::to_string) else { return r };
        self.trigger(&id, true);
        json!({"ok": true, "id": id})
    }

    /// A twin can start from this card now: its CLI copies conversations and weft knows its session.
    pub(super) fn can_twin(&self, id: &str) -> bool {
        self.rt.get(id).is_some_and(|r| r.session.is_some() && r.runs > 0)
            && self.session_cfg(id).is_some_and(|a| !a.fork_arg.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    fn claude(e: &mut Engine, id: &str) {
        add(e, id, id, "build the parser");
        e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap().agent = "claude".into();
    }

    /// The card worked once and its process ended (it rested).
    fn rested(e: &mut Engine, id: &str) {
        let r = e.rt.get_mut(id).unwrap();
        r.runs = 1;
        r.finished = true;
        r.ok = Some(true);
        r.status = Status::Done;
        r.output = Some("Parser done: src/parse.rs, 4 tests.".into());
        r.asked = "build the parser".into();
        r.alive = false;
    }

    #[test]
    fn the_session_is_read_from_what_the_cli_says_and_goes_right_after_the_program() {
        let codex = "Token usage: total=9,575\nTo continue this session, run:\n  codex resume 01a1484a-df2e-7d90-bb52-7046bfa6c715\nOr run codex resume and select it.";
        assert_eq!(read_session(codex, "codex resume ").as_deref(), Some("01a1484a-df2e-7d90-bb52-7046bfa6c715"));
        let agy = "Resume with -c (or command below):\nagy --conversation=f416193e-576c-436b-86dc-2af30b56ba2a\n";
        assert_eq!(read_session(agy, "--conversation=").as_deref(), Some("f416193e-576c-436b-86dc-2af30b56ba2a"));
        assert_eq!(read_session("codex resume and select", "codex resume "), None, "no id there");
        assert_eq!(read_session(codex, ""), None);
        assert_eq!(
            with_session("command codex --dangerously-bypass-approvals-and-sandbox -m o4", "codex", "resume abc12345"),
            "command codex resume abc12345 --dangerously-bypass-approvals-and-sandbox -m o4"
        );
        assert_eq!(
            with_session("FOO=1 claude --model opus", "FOO=1 claude", "--resume abc12345"),
            "FOO=1 claude --resume abc12345 --model opus"
        );
        // a program and its script stay together (the test suites run the simulated agent so)
        assert_eq!(
            with_session(
                "python3 /x/sim.py --name codex --dangerously-skip-permissions",
                "python3 /x/sim.py --name codex",
                "resume abc12345"
            ),
            "python3 /x/sim.py --name codex resume abc12345 --dangerously-skip-permissions"
        );
        assert_eq!(with_session("npx claude", "npx claude", "--session-id abc12345"), "npx claude --session-id abc12345");
        // a command line of another shape: at the end
        assert_eq!(with_session("my-wrapper --x", "claude", "--resume abc12345"), "my-wrapper --x --resume abc12345");
        assert_eq!(with_session("claude", "claude", ""), "claude");
        let id = new_session_id();
        assert!(id.len() == 36 && id.as_bytes()[14] == b'4' && plain_word(&id), "{id}");
        assert_ne!(id, new_session_id());
    }

    #[test]
    fn a_rested_card_takes_its_conversation_up_again_and_a_failed_resume_starts_anew() {
        let mut e = engine();
        claude(&mut e, "a");
        // first start: weft picks the id
        let first = e.session_args("a");
        let sid = e.rt["a"].session.clone().unwrap();
        assert_eq!(first, format!("--session-id {sid}"));
        assert!(e.remembers("a"), "nothing to forget yet");
        rested(&mut e, "a");
        assert!(e.resumable("a") && e.remembers("a"));
        assert_eq!(e.session_args("a"), format!("--resume {sid}"));
        assert!(e.rt["a"].resuming);
        // that process ended before it took a prompt: the conversation cannot be taken up again
        let again = e.session_args("a");
        assert!(again.starts_with("--session-id ") && !again.contains(&sid), "{again}");
        assert!(!e.rt["a"].resuming);
        // and a card on its own command line is left alone
        e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().cmd = "claude --print".into();
        assert_eq!(e.session_args("a"), "");
        assert!(!e.resumable("a"));
    }

    #[test]
    fn a_new_conversation_is_told_what_the_card_did_and_a_twin_that_it_is_one() {
        let mut e = engine();
        claude(&mut e, "a");
        rested(&mut e, "a");
        e.rt.get_mut("a").unwrap().blank = true;
        let note = e.session_note("a").unwrap();
        assert!(note.starts_with("[weft] Note:") && note.contains("build the parser") && note.contains("Parser done"), "{note}");
        assert!(e.session_note("a").is_none(), "once");
        // a twin starts on a copy of the other card's conversation, with an id of its own
        e.rt.get_mut("a").unwrap().session = Some("11111111-2222-4333-8444-555555555555".into());
        assert!(e.can_twin("a"));
        let r = e.cmd_twin("a".into(), "try it with a hand-written lexer".into(), 900.0, 0.0);
        let t = r["id"].as_str().unwrap().to_string();
        assert_eq!(e.node(&t).unwrap().twin, "a");
        assert_eq!(e.node(&t).unwrap().agent, "claude");
        let args = e.session_args(&t);
        let own = e.rt[&t].session.clone().unwrap();
        assert_eq!(args, format!("--resume 11111111-2222-4333-8444-555555555555 --fork-session --session-id {own}"));
        assert!(e.rt[&t].twin_due);
        let note = e.session_note(&t).unwrap();
        assert!(note.contains("separate agent") && note.contains(&t) && note.contains("copy of a's"), "{note}");
        // codex copies a conversation only once weft knows its id; agy cannot
        add(&mut e, "c", "c", "x");
        e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().agent = "agy".into();
        rested(&mut e, "c");
        e.rt.get_mut("c").unwrap().session = Some("f416193e-576c-436b-86dc-2af30b56ba2a".into());
        assert!(!e.can_twin("c") && e.resumable("c"));
        assert!(e.cmd_twin("c".into(), "x".into(), 0.0, 0.0)["ok"] == false);
    }

    #[test]
    fn a_queued_prompt_to_an_agent_that_forgot_gets_its_own_answer_again() {
        let mut e = engine();
        add(&mut e, "a", "a", "build the parser");
        // a CLI weft cannot resume
        e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "aider".into();
        rested(&mut e, "a");
        add_kind(&mut e, "s", "then", "step", "now add tests");
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().host = "a".into();
        link(&mut e, "e1", "a", "s", On::Done);
        let given = |e: &mut Engine| {
            let i = Input {
                from: "a".into(),
                name: "a".into(),
                ok: true,
                text: "Parser done: src/parse.rs, 4 tests.".into(),
                kind: InputKind::Result,
            };
            e.rt.get_mut("s").unwrap().inputs.push(("e1".into(), i));
        };
        given(&mut e);
        assert!(!e.remembers("a"));
        e.trigger("s", true);
        let q = &e.rt["a"].queue;
        assert!(q.iter().any(|q| q.text.contains("now add tests") && q.text.contains("Parser done")), "its answer is not in a new session");
        // one that rests on a conversation weft can take up again remembers it
        let mut e = engine();
        claude(&mut e, "a");
        rested(&mut e, "a");
        e.rt.get_mut("a").unwrap().session = Some("11111111-2222-4333-8444-555555555555".into());
        add_kind(&mut e, "s", "then", "step", "now add tests");
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().host = "a".into();
        link(&mut e, "e1", "a", "s", On::Done);
        given(&mut e);
        e.trigger("s", true);
        let q = &e.rt["a"].queue;
        assert!(q.iter().any(|q| q.text.contains("now add tests") && !q.text.contains("Parser done")));
    }

    #[test]
    fn a_rested_agent_is_still_asked_to_commit_its_own_work() {
        let mut e = engine();
        claude(&mut e, "a");
        rested(&mut e, "a");
        e.rt.get_mut("a").unwrap().session = Some("11111111-2222-4333-8444-555555555555".into());
        add_kind(&mut e, "c", "commit", "git", "");
        e.flow.nodes.iter_mut().find(|n| n.id == "c").unwrap().git_op = GitOp::Commit;
        link(&mut e, "e1", "a", "c", On::Done);
        e.git_arrival("c", "e1", "a", "a", true, "Parser done");
        assert!(e.rt["a"].queue.iter().any(|q| q.text.contains("Commit the work you just did")), "asked, not `git add -A`");
        assert_eq!(e.rt["c"].git.wait.as_deref(), Some("a"));
    }

    #[test]
    fn an_agent_that_ended_names_its_session_and_a_resting_one_is_closed() {
        let mut e = engine();
        add(&mut e, "x", "x", "go");
        e.flow.nodes.iter_mut().find(|n| n.id == "x").unwrap().agent = "codex".into();
        let r = e.rt.get_mut("x").unwrap();
        r.pane = Some("%9".into());
        r.alive = false;
        r.retiring = Some(1);
        r.parser
            .process(b"Token usage: total=12\r\nTo continue this session, run:\r\n  codex resume 01a1484a-df2e-7d90-bb52-7046bfa6c715\r\n");
        e.tick_retire();
        assert_eq!(e.rt["x"].session.as_deref(), Some("01a1484a-df2e-7d90-bb52-7046bfa6c715"));
        assert!(e.rt["x"].retiring.is_none() && e.rt["x"].pane.is_none(), "its window is closed");
    }
}
