//! Loose ends. Pushes run by themselves now (a check on the link is where a person looks first), but a
//! flow from an older weft can still have a push waiting to be armed, and that would keep its whole
//! flow on the canvas for good: the archive files a flow only once every card in it is finished. So git
//! is asked what such a push has left to do. Its commits on the remote already (a later push sent the
//! whole branch, or someone pushed by hand) and nothing linked after it: it is done by itself. Commits
//! still to send: the card and the top bar say "ready to push", and arming it is one click. A draft that
//! never ran and was given nothing to do goes to the archive with its flow instead of holding it; any
//! other draft that holds a finished flow is named on the flow's cards.

use super::git::carry_hashes;
use super::*;

/// What a push nobody armed has left to do, as git says (`GitRound.left`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Left {
    /// its commits are on the remote: the remote branch that has them ("origin/master")
    Pushed(String),
    /// this many of its commits are not on the remote yet
    Push(usize),
}

impl Left {
    /// How it travels in a git job's result (`Ev::Git` with `GitJobKind::Look`).
    fn wire(&self) -> String {
        match self {
            Left::Pushed(on) => format!("on {on}"),
            Left::Push(n) => format!("push {n}"),
        }
    }

    fn from_wire(s: &str) -> Option<Left> {
        if let Some(on) = s.strip_prefix("on ") {
            return Some(Left::Pushed(on.to_string()));
        }
        s.strip_prefix("push ").and_then(|n| n.parse().ok()).map(Left::Push)
    }

    pub(super) fn to_json(&self) -> Value {
        match self {
            Left::Pushed(on) => json!({"on": on}),
            Left::Push(n) => json!({"push": n}),
        }
    }
}

/// A push nobody armed is looked at this often. Only refs are read: the git tree is what fetches.
const LOOK_EVERY_MS: u64 = 30_000;

impl Engine {
    /// The pushes nobody armed whose links are all in and that owe no commit: what is left for them?
    pub(super) fn tick_loose(&mut self) {
        let now = now_ms();
        let due: Vec<String> = self
            .flow
            .nodes
            .iter()
            .filter(|n| n.kind() == Kind::Git && n.draft && n.git_op != GitOp::Pr && n.pushes())
            .filter(|n| {
                self.rt
                    .get(&n.id)
                    .is_some_and(|r| !r.finished && !r.git.busy && r.git.jobs.is_empty() && now >= r.git.looked + LOOK_EVERY_MS)
            })
            .filter(|n| self.git_all_in(&n.id))
            .map(|n| n.id.clone())
            .collect();
        for id in due {
            self.look_loose(&id);
        }
    }

    fn look_loose(&mut self, id: &str) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        let Some(node) = self.node(id).cloned() else { return };
        let repos = self.git_repos(id);
        let several = repos.len() > 1;
        let all = self.rt.get(id).map(|r| carry_hashes(&r.git.carry)).unwrap_or_default();
        let work: Vec<(PathBuf, String, Vec<String>)> = repos
            .into_iter()
            .map(|d| {
                let hashes = if several { self.git_hashes_in(id, &d) } else { all.clone() };
                let branch = self.git_branch_in(id, &d);
                (d, branch, hashes)
            })
            .collect();
        let (tx, nid, remote) = (self.tx.clone(), id.to_string(), node.remote.trim().to_string());
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.git.looked = now_ms();
        let gen = rt.git.gen;
        handle.spawn(async move {
            let mut res = Ok(String::new());
            let mut missing = 0;
            for (dir, branch, hashes) in work {
                match left_to_push(&dir, &remote, &branch, &hashes).await {
                    Ok(Left::Push(n)) => missing += n,
                    Ok(Left::Pushed(on)) => {
                        if let Ok(s) = &mut res {
                            if s.is_empty() {
                                *s = on;
                            }
                        }
                    }
                    Err(e) => res = Err(e),
                }
                if res.is_err() {
                    break;
                }
            }
            let res = res.map(|on| if missing > 0 { Left::Push(missing) } else { Left::Pushed(on) }.wire());
            let _ = tx.send(Ev::Git { id: nid, gen, job: GitJobKind::Look, res, facts: None });
        });
    }

    /// What git said a push nobody armed has left. Nothing, and nothing linked after it: done. A step
    /// after it is not started by a push the person never armed (arming it is their go-ahead for what
    /// follows), so such a push only says that its commits are there.
    pub(super) fn on_loose(&mut self, id: &str, gen: u64, res: Result<String, String>) {
        let left = res.ok().as_deref().and_then(Left::from_wire);
        let fed = self.flow.edges.iter().any(|e| e.from == id);
        let still = self.node(id).is_some_and(|n| n.draft) && self.git_all_in(id);
        let Some(rt) = self.rt.get_mut(id) else { return };
        if rt.git.gen != gen || rt.finished || rt.git.busy || !rt.git.jobs.is_empty() {
            return;
        }
        if rt.git.left != left {
            rt.git.left = left.clone();
            rt.ui = true;
        }
        if let (Some(Left::Pushed(on)), false, true) = (left, fed, still) {
            let on = if on.is_empty() { "the remote".to_string() } else { on };
            rt.git.log.push(format!("pushed before: {on} has these commits already, nothing was left to push"));
            let out = self.git_report(id);
            self.git_round_done(id, true, out);
        }
    }

    /// A draft that never ran and was given nothing to do (a queued prompt with no text, say): it holds
    /// nothing up, so it goes to the archive with its flow.
    pub(super) fn idle_draft(&self, id: &str) -> bool {
        let Some(n) = self.node(id) else { return false };
        n.draft
            && matches!(n.kind(), Kind::Agent | Kind::Step)
            && n.prompt.trim().is_empty()
            && self.rt.get(id).is_some_and(|r| r.runs == 0 && !r.finished && !r.alive && !r.spawning && r.queue.is_empty())
    }

    /// The cards of flows that are finished but for a draft nobody armed: which card holds them, and why.
    pub(super) fn held_map(&self, blocked: &HashMap<String, String>) -> HashMap<String, (String, String)> {
        let mut out = HashMap::new();
        for flow in self.flows() {
            let mut holder: Option<String> = None;
            let mut open = false;
            for id in flow.iter().filter(|id| self.node(id).is_some_and(|n| n.kind() != Kind::Note)) {
                let Some(rt) = self.rt.get(id) else { continue };
                match self.eff_status(id, blocked) {
                    Status::Done if rt.at_rest() => {}
                    Status::Blocked => {}
                    Status::Draft if self.idle_draft(id) => {}
                    Status::Draft => {
                        if holder.is_none() {
                            holder = Some(id.clone());
                        }
                    }
                    _ => open = true,
                }
            }
            let (Some(h), false) = (holder, open) else { continue };
            if flow.len() < 2 || flow.iter().any(|id| self.node(id).is_some_and(|n| n.keep)) {
                continue;
            }
            let why = match self.node(&h) {
                Some(n) if n.kind() == Kind::Git && n.git_op == GitOp::Pr => "pull request not armed",
                Some(n) if n.kind() == Kind::Git && n.pushes() => "push not armed",
                Some(n) if n.deploy_unset() => "deploy has no command yet",
                Some(n) if n.deploy.is_some() => "deploy not armed",
                _ => "not armed",
            };
            for id in &flow {
                if *id != h {
                    out.insert(id.clone(), (h.clone(), why.to_string()));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    fn git(e: &mut Engine, id: &str, op: GitOp, draft: bool) {
        add_kind(e, id, id, "git", "");
        let n = e.flow.nodes.iter_mut().find(|n| n.id == id).unwrap();
        n.git_op = op;
        n.draft = draft;
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn git_says_what_a_push_has_left() {
        let root = std::env::temp_dir().join(format!("weft-left-{}-{}", std::process::id(), now_ms()));
        let dir = root.join("w");
        std::fs::create_dir_all(&dir).unwrap();
        sh(&root, &["init", "-q", "--bare", "r.git"]);
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["config", "user.name", "t"]);
        sh(&dir, &["config", "user.email", "t@t"]);
        sh(&dir, &["remote", "add", "origin", &root.join("r.git").to_string_lossy()]);
        let commit = |name: &str| {
            std::fs::write(dir.join(name), name).unwrap();
            sh(&dir, &["add", name]);
            sh(&dir, &["commit", "-q", "-m", name]);
            sh(&dir, &["rev-parse", "--short", "HEAD"])
        };
        let a = commit("a");
        sh(&dir, &["push", "-q", "-u", "origin", "main"]);
        let b = commit("b");
        let on = Ok(Left::Pushed("origin/main".into()));
        assert_eq!(left_to_push(&dir, "", "", std::slice::from_ref(&a)).await, on);
        assert_eq!(left_to_push(&dir, "origin", "main", &[a.clone(), b.clone()]).await, Ok(Left::Push(1)));
        // the commits before it had nothing new: the branch against the remote's
        assert_eq!(left_to_push(&dir, "", "", &[]).await, Ok(Left::Push(1)));
        // someone else pushed the branch since
        sh(&dir, &["push", "-q", "origin", "main"]);
        assert_eq!(left_to_push(&dir, "", "", &[b]).await, on);
        assert_eq!(left_to_push(&dir, "", "", &[]).await, on);
        assert!(left_to_push(&dir, "upstream", "", &[a]).await.is_err(), "no such remote");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_push_or_pull_request_is_armed_when_it_is_made() {
        let mut e = engine();
        for op in ["commit", "push", "pr"] {
            let r = e.cmd_node_add(json!({"agent": "git", "git_op": op}), None, false);
            let id = r["id"].as_str().unwrap().to_string();
            assert!(!e.node(&id).unwrap().draft, "{op} waits for nobody: a check on the link is where a person looks first");
        }
    }

    #[test]
    fn a_push_nobody_armed_is_done_when_its_commits_are_on_the_remote_and_nothing_follows() {
        let mut e = engine();
        alive_agent(&mut e, "a", "slug");
        git(&mut e, "c", GitOp::Commit, false);
        git(&mut e, "p", GitOp::Push, true);
        link(&mut e, "e1", "a", "c", On::Done);
        link(&mut e, "e2", "c", "p", On::Done);
        e.git_arrival("p", "e2", "c", "commit", true, "abc1234 slug: done");
        assert!(e.git_all_in("p") && !e.rt["p"].finished, "it waits for the person");
        let gen = e.rt["p"].git.gen;
        // commits still to send: it says so and waits
        e.on_loose("p", gen, Ok("push 2".into()));
        assert_eq!(e.rt["p"].git.left, Some(Left::Push(2)));
        assert!(!e.rt["p"].finished);
        // all on the remote: done, so its flow can go to the archive
        e.on_loose("p", gen, Ok("on origin/main".into()));
        assert!(e.rt["p"].finished && e.rt["p"].ok == Some(true));
        assert!(e.rt["p"].git.log.iter().any(|l| l.contains("origin/main has these commits already")), "{:?}", e.rt["p"].git.log);
        assert!(e.rt["p"].output.as_deref().is_some_and(|o| o.starts_with("Pushed")), "{:?}", e.rt["p"].output);
    }

    #[test]
    fn a_push_nobody_armed_does_not_start_what_comes_after_it_or_skip_a_commit_it_owes() {
        let mut e = engine();
        alive_agent(&mut e, "a", "slug");
        git(&mut e, "c", GitOp::Commit, false);
        git(&mut e, "p", GitOp::Push, true);
        add(&mut e, "t", "test it live", "try it");
        link(&mut e, "e1", "a", "c", On::Done);
        link(&mut e, "e2", "c", "p", On::Done);
        link(&mut e, "e3", "p", "t", On::Done);
        e.git_arrival("p", "e2", "c", "commit", true, "abc1234 slug: done");
        let gen = e.rt["p"].git.gen;
        e.on_loose("p", gen, Ok("on origin/main".into()));
        assert!(!e.rt["p"].finished, "arming it is the go-ahead for what follows");
        assert_eq!(e.rt["p"].git.left, Some(Left::Pushed("origin/main".into())));
        assert!(e.rt["t"].inputs.is_empty() && e.rt["t"].queue.is_empty());
        // linked straight from the agent, the push still owes the commit: git's view of the branch says nothing yet
        git(&mut e, "q", GitOp::Push, true);
        link(&mut e, "e4", "a", "q", On::Done);
        e.git_arrival("q", "e4", "a", "slug", true, "done");
        assert!(!e.rt["q"].git.jobs.is_empty());
        let gen = e.rt["q"].git.gen;
        e.on_loose("q", gen, Ok("on origin/main".into()));
        assert!(!e.rt["q"].finished && e.rt["q"].git.left.is_none());
    }

    #[test]
    fn an_empty_draft_goes_with_its_flow_and_any_other_draft_is_named_as_what_holds_it() {
        let mut e = engine();
        add(&mut e, "a", "build", "go");
        {
            let r = e.rt.get_mut("a").unwrap();
            r.finished = true;
            r.ok = Some(true);
            r.runs = 1;
            r.status = Status::Done;
            r.since = now_ms() - 3_600_000;
        }
        add_kind(&mut e, "s", "prompt", "step", "");
        {
            let n = e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap();
            n.draft = true;
            n.host = "a".into();
        }
        link(&mut e, "e1", "a", "s", On::Done);
        let blocked = e.blocked_map();
        let plan = e.filing_plan(&blocked);
        assert!(plan.iter().any(|(f, _)| f.len() == 2), "the empty draft goes along: {plan:?}");
        assert!(e.held_map(&blocked).is_empty());
        // a draft with something to do holds the flow, and the flow says who
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().prompt = "also do x".into();
        assert!(e.filing_plan(&blocked).is_empty());
        let held = e.held_map(&blocked);
        assert_eq!(held.get("a"), Some(&("s".to_string(), "not armed".to_string())));
        assert!(!held.contains_key("s"));
        // a lone draft just placed is not filed (nothing finished there)
        let mut e = engine();
        add_kind(&mut e, "d", "prompt", "step", "");
        e.flow.nodes[0].draft = true;
        assert!(e.filing_plan(&e.blocked_map()).is_empty());
    }
}
