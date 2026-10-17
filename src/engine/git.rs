//! Commit, push and open PR: the agent that did the work is asked, git is trusted, the steps carry a report.

use super::*;

pub(super) fn commit_subject(node: &Node, name: &str, text: &str) -> String {
    let first = text.lines().map(str::trim).find(|l| l.chars().any(|c| c.is_alphanumeric())).unwrap_or("update");
    let first = first.trim_start_matches(|c: char| "#>*-\u{25CF} ".contains(c));
    if !node.message.trim().is_empty() {
        return expand(node.message.trim(), &|k| match k {
            "agent" | "name" => Some(name.to_string()),
            "result" | "summary" => Some(head_chars(first, 72)),
            _ => None,
        });
    }
    head_chars(&format!("{name}: {first}"), 72)
}

pub(super) fn carry_hashes(carry: &[Carry]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in carry {
        if let Some(h) = lead_hash(&c.commit) {
            if !v.contains(&h) {
                v.push(h);
            }
        }
    }
    v
}

/// The same agent asked the same thing is one entry: the newer answer replaces the older.
fn carry_add(v: &mut Vec<Carry>, c: Carry) {
    match v.iter_mut().find(|o| o.name == c.name && o.task == c.task) {
        Some(o) => {
            let commit = std::mem::take(&mut o.commit);
            *o = c;
            if o.commit.is_empty() {
                o.commit = commit;
            }
        }
        None => v.push(c),
    }
}

/// What a commit or push step hands on, as plain text: what happened to the repository, then who
/// did the work (what each was asked, what it said, which commit came of it). The tail of a piped
/// result is what survives a cut, so the long part is budgeted to stay well under the limit.
fn git_report(node: &Node, dirs: &[PathBuf], log: &[String], facts: &Facts, carry: &[Carry]) -> String {
    let branch = if facts.branch.is_empty() { node.branch.trim() } else { facts.branch.as_str() };
    let remote = if node.remote.trim().is_empty() { "origin" } else { node.remote.trim() };
    let b = if branch.is_empty() { "the current branch".to_string() } else { format!("`{branch}`") };
    let last = log.iter().rev().find(|l| l.starts_with("pushed ") || l.starts_with("no remote"));
    let several = dirs.len() > 1;
    let mut s = if facts.prs.len() > 1 {
        format!("Opened a pull request in each repository: {}", facts.prs.join(", "))
    } else if !facts.pr.is_empty() {
        let title = if facts.pr_title.is_empty() { String::new() } else { format!("\nTitle: {}", facts.pr_title) };
        format!("Opened a pull request for {b}: {}{title}", facts.pr)
    } else {
        match last {
            Some(l) if l.starts_with("pushed ") && several => format!("Pushed each repository to `{remote}`."),
            Some(l) if l.starts_with("pushed ") => format!("Pushed {b} to `{remote}`."),
            Some(l) => format!("Committed on {b}. Not pushed: {l}."),
            None => format!("Committed on {b}."),
        }
    };
    match dirs {
        [one] => s.push_str(&format!("\nProject folder: {}", one.display())),
        _ => s.push_str(&format!("\nRepositories: {}", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", "))),
    }
    if several {
        for l in log.iter().filter(|l| l.starts_with("pushed ") || l.starts_with("no remote") || l.starts_with("opened ")) {
            s.push_str(&format!("\n- {l}"));
        }
    }
    if !facts.head.is_empty() {
        s.push_str(&format!("\nHEAD: {}", facts.head));
    }
    if !facts.commits.is_empty() {
        s.push_str("\nCommits:");
        for c in facts.commits.iter().take(12) {
            s.push_str(&format!("\n- {c}"));
        }
        if facts.commits.len() > 12 {
            s.push_str(&format!("\n- \u{2026} and {} more", facts.commits.len() - 12));
        }
    }
    if !facts.files.is_empty() {
        let shown: Vec<&str> = facts.files.iter().take(24).map(String::as_str).collect();
        s.push_str(&format!("\nFiles changed ({}): {}", facts.files.len(), shown.join(", ")));
        if facts.files.len() > 24 {
            s.push_str(&format!(" \u{2026} and {} more", facts.files.len() - 24));
        }
    }
    if !carry.is_empty() {
        s.push_str("\n\nWho did the work:");
        let per = (5000 / carry.len()).clamp(240, 1500);
        for c in carry {
            let tag = if c.agent.is_empty() { String::new() } else { format!(" ({})", c.agent) };
            s.push_str(&format!("\n- {}{}", c.name, tag));
            let task = c.task.split_whitespace().collect::<Vec<_>>().join(" ");
            if !task.is_empty() {
                s.push_str(&format!("\n  was asked: {}", head_chars(&task, 300)));
            }
            let said = tail_bytes(c.result.trim(), per).trim();
            if !said.is_empty() {
                s.push_str("\n  said:");
                for l in said.lines() {
                    s.push_str("\n    ");
                    s.push_str(l.trim_end());
                }
            }
            if !c.commit.is_empty() {
                s.push_str(&format!("\n  commit: {}", c.commit));
            }
        }
    }
    s
}

/// A repository as the log names it: its folder's name.
fn folder_name(dir: &Path) -> String {
    dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The pull request link in an agent's report, and the title it gave (the line after it).
pub(super) fn pr_link(text: &str) -> Option<(String, String)> {
    let is_pr = |u: &str| ["/pull/", "/pulls/", "/merge_requests/", "/pull-requests/"].iter().any(|m| u.contains(m));
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let clean = |t: &str| t.trim_matches(|c: char| ",.;:()[]<>\"'`*".contains(c)).to_string();
    let mut found: Option<(usize, String)> = None;
    for (i, l) in lines.iter().enumerate() {
        for tok in l.split_whitespace().filter(|t| t.contains("http://") || t.contains("https://")) {
            let t = clean(tok.trim_start_matches(|c: char| c != 'h'));
            if !(t.starts_with("http://") || t.starts_with("https://")) {
                continue;
            }
            if is_pr(&t) {
                found = Some((i, t));
                break;
            }
            if found.is_none() {
                found = Some((i, t));
            }
        }
        if found.as_ref().is_some_and(|(_, u)| is_pr(u)) {
            break;
        }
    }
    let (i, url) = found?;
    let title = lines.get(i + 1).map(|l| head_chars(l, 160)).unwrap_or_default();
    Some((url, title))
}

impl Engine {
    /// Has every link in handed on this round (`Promise.all`; one is enough for `Promise.any`)?
    pub(super) fn git_all_in(&self, id: &str) -> bool {
        let Some(rt) = self.rt.get(id) else { return false };
        if rt.git.seen.is_empty() {
            return false;
        }
        // a link that handed on in an earlier round, with nothing new coming down it, is still in
        match self.node(id).map(|n| n.join) {
            Some(Join::Any) => true,
            _ => self.flow.edges.iter().filter(|e| e.to == id).all(|e| rt.git.seen.contains(&e.id) || self.resolved(e)),
        }
    }

    /// The agent whose work this result is: a queued prompt speaks for its agent, a gate (or any
    /// check on a link) hands on what the agent behind it did.
    fn work_agent(&self, from: &str) -> String {
        let host = self.resolve_host(from);
        match self.node(&host) {
            Some(n) if n.kind().is_gate() => {
                self.rt.get(&host).and_then(|r| r.gate.targets.first().cloned()).map(|t| self.resolve_host(&t)).unwrap_or(host)
            }
            _ => host,
        }
    }

    /// Does work linked into this step still need a push? (A PR after a push needs no second one.)
    fn git_needs_push(&self, id: &str) -> bool {
        let seen = self.rt.get(id).map_or(&[][..], |r| &r.git.seen);
        self.flow
            .edges
            .iter()
            .filter(|e| e.to == id && seen.contains(&e.id))
            .any(|e| !self.node(&e.from).is_some_and(|n| n.kind() == Kind::Git && n.pushes()))
    }

    fn git_lock(&mut self, dir: &Path) -> Arc<tokio::sync::Mutex<()>> {
        self.git_locks.entry(dir.to_path_buf()).or_default().clone()
    }

    /// The branch a step works on in one repository: its own, else the one the commits before it
    /// went to there.
    pub(super) fn git_branch_in(&self, id: &str, dir: &Path) -> String {
        let Some(n) = self.node(id) else { return String::new() };
        if !n.branch.trim().is_empty() {
            return n.branch.trim().to_string();
        }
        self.flow
            .edges
            .iter()
            .filter(|e| e.to == id)
            .filter_map(|e| self.node(&e.from))
            .filter(|n| n.kind() == Kind::Git && self.node_dir(n) == dir)
            .map(|n| n.branch.trim().to_string())
            .find(|b| !b.is_empty())
            .unwrap_or_default()
    }

    /// The folder whose repository this step works on for `agent`'s work: the step's own folder if
    /// it has one, else that agent's (the work is committed and pushed where it was done).
    pub(super) fn git_dir_for(&self, id: &str, agent: &str) -> PathBuf {
        let Some(node) = self.node(id) else { return self.boot.dir.clone() };
        match self.node(agent) {
            Some(a) if node.cwd.trim().is_empty() && a.kind() == Kind::Agent => {
                let d = self.node_dir(a);
                // (a worktree weft made is a branch of the project's repository)
                if d.starts_with(state_dir().join("worktrees")) {
                    self.boot.dir.clone()
                } else {
                    d
                }
            }
            _ => self.node_dir(node),
        }
    }

    /// Every repository the work of this round is in, in the order it arrived: each gets its own
    /// push and its own pull request.
    pub(super) fn git_repos(&self, id: &str) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for c in self.rt.get(id).map(|r| r.git.carry.as_slice()).unwrap_or_default() {
            let d = self.git_dir_for(id, &c.node);
            if !out.contains(&d) {
                out.push(d);
            }
        }
        if out.is_empty() {
            out.push(self.node(id).map_or_else(|| self.boot.dir.clone(), |n| self.node_dir(n)));
        }
        out
    }

    /// The commits of this round that are in one repository.
    pub(super) fn git_hashes_in(&self, id: &str, dir: &Path) -> Vec<String> {
        let Some(rt) = self.rt.get(id) else { return Vec::new() };
        let mine: Vec<Carry> = rt.git.carry.iter().filter(|c| self.git_dir_for(id, &c.node) == dir).cloned().collect();
        carry_hashes(&mine)
    }

    /// A result arrived: commit it, and once every incoming link has delivered, push.
    pub(super) fn git_arrival(&mut self, id: &str, edge: &str, from: &str, name: &str, ok: bool, out: &str) {
        // a round after one that failed: what the other links handed on then went no further, so
        // it is taken up again (after a round that went through only the new result goes on)
        let again = ok && self.rt.get(id).is_some_and(|r| r.finished && r.git.seen.is_empty() && r.ok == Some(false));
        if !self.git_take(id, edge, from, name, ok, out) {
            return;
        }
        if again {
            let others: Vec<Edge> = self.flow.edges.iter().filter(|e| e.to == id && e.id != edge && self.resolved(e)).cloned().collect();
            for e in others {
                let (name, ok, out) = self.input_from(&e.from);
                self.git_take(id, &e.id, &e.from, &name, ok, &out);
            }
        }
        self.state_dirty = true;
        self.git_pump(id, false);
    }

    /// Take one result into the round: what it carries, and a commit of its agent's work. False when
    /// it failed: then the round is over.
    fn git_take(&mut self, id: &str, edge: &str, from: &str, name: &str, ok: bool, out: &str) -> bool {
        // a queued prompt speaks for the agent that answered it, a gate for the agent behind it
        let agent = self.work_agent(from);
        let wt = self.live_worktree(&agent).map(PathBuf::from);
        let agent_name = self.node(&agent).map(|n| n.name.clone()).unwrap_or_else(|| name.to_string());
        // the commit step before this one already committed: here it only counts
        let from_git = self.node(from).is_some_and(|n| n.kind() == Kind::Git);
        // the agent that did the work commits it; a command or a vanished agent cannot be asked (one
        // that rests is woken on its own conversation, so it still knows which files are its own)
        let askable = self.node(&agent).is_some_and(|n| n.kind() == Kind::Agent && n.agent != "sh")
            && (self.rt.get(&agent).is_some_and(|r| r.alive) || self.resumable(&agent));
        if !ok {
            // a step before this one failed (or an agent did not finish): nothing is committed or
            // pushed on top of that. "commit now" on the card still takes what is there.
            let what = match self.node(id).map(|n| n.git_op) {
                Some(GitOp::Pr) => "open a pull request for",
                Some(GitOp::Commit) => "commit",
                _ => "push",
            };
            let why = if from_git {
                format!("{name} did not succeed, so there is nothing to {what}")
            } else {
                format!("{agent_name} did not finish its work, so nothing is committed")
            };
            self.git_fail(id, why);
            return false;
        }
        // what goes on from here: this agent's task and answer, or everything the step before carried
        let upstream = if from_git { self.rt.get(from).map(|r| (r.git.carry.clone(), r.git.facts.clone())) } else { None };
        let item = if from_git {
            None
        } else {
            // a queued prompt is its own entry (it was asked something else); a gate shows the agent behind it
            let step = self.node(from).is_some_and(|n| n.kind() == Kind::Step);
            let who = if step { from } else { agent.as_str() };
            let who_name = self.node(who).map(|n| n.name.clone()).unwrap_or_else(|| name.to_string());
            let task = self
                .rt
                .get(who)
                .map(|r| r.asked.trim().to_string())
                .filter(|a| !a.is_empty())
                .or_else(|| self.node(who).map(|n| n.prompt.trim().to_string()))
                .unwrap_or_default();
            let kind = self.node(&agent).map(|n| n.agent.clone()).unwrap_or_default();
            let kind = if step { format!("{kind} \u{b7} queued prompt") } else { kind };
            Some(Carry {
                name: who_name,
                agent: kind,
                node: agent.clone(),
                task: head_chars(&task, 400),
                result: tail_bytes(out.trim(), 4000).to_string(),
                commit: String::new(),
            })
        };
        {
            let Some(rt) = self.rt.get_mut(id) else { return false };
            if rt.finished && rt.git.seen.is_empty() {
                rt.begin_git_round();
            }
            if !rt.git.seen.iter().any(|s| s == edge) {
                rt.git.seen.push(edge.to_string());
            }
            // something new came in: what a push nobody armed has left is looked at again
            rt.git.left = None;
            rt.git.looked = 0;
            if let Some((c, f)) = upstream {
                for x in c {
                    carry_add(&mut rt.git.carry, x);
                }
                rt.git.facts.absorb(f);
            }
            if let Some(x) = item {
                carry_add(&mut rt.git.carry, x);
            }
            if from_git {
                // counted as seen above; nothing to commit
            } else if askable {
                rt.git.jobs.push_back(GitJob::Ask {
                    from: agent.clone(),
                    name: agent_name.clone(),
                    text: out.to_string(),
                    wt,
                    resolve: false,
                });
            } else {
                rt.git.jobs.push_back(GitJob::Commit { agent: agent.clone(), name: agent_name.clone(), text: out.to_string(), wt });
            }
            rt.cause = Cause::Flow;
            rt.ui = true;
        }
        true
    }

    /// The card's "commit & push now": commit what is there and push, ready or not.
    pub(super) fn git_ship_now(&mut self, id: &str) {
        let ids: Vec<String> = self.flow.edges.iter().filter(|e| e.to == id).map(|e| e.id.clone()).collect();
        {
            let waiting = self.rt.get(id).and_then(|r| r.git.wait.clone());
            if let Some(a) = waiting {
                // stop waiting on an agent that is taking too long
                if let Some(ar) = self.rt.get_mut(&a) {
                    ar.forget_requests_from(id);
                }
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.git.gen += 1;
                    rt.git.wait = None;
                    rt.git.busy = false;
                    rt.git.stage = GitStage::Commit;
                    rt.git.log.push("stopped waiting; committing what is there".to_string());
                }
            }
            let Some(rt) = self.rt.get_mut(id) else { return };
            if rt.git.busy {
                return;
            }
            if rt.finished {
                rt.begin_git_round();
            }
            rt.git.force = true;
            rt.git.seen = ids;
            if rt.git.jobs.is_empty() {
                rt.git.jobs.push_back(GitJob::Commit { agent: String::new(), name: "weft".into(), text: String::new(), wt: None });
            }
            rt.ui = true;
        }
        self.git_pump(id, true);
    }

    pub(super) fn git_pump(&mut self, id: &str, force: bool) {
        let Some(node) = self.node(id).cloned() else { return };
        let all_in = self.git_all_in(id);
        let tx = self.tx.clone();
        let nid = id.to_string();
        let needs_push = self.git_needs_push(id);
        let Some(rt) = self.rt.get_mut(id) else { return };
        if (node.draft && !force && !rt.git.force) || rt.git.busy {
            return;
        }
        let gen = rt.git.gen;
        match rt.git.jobs.pop_front() {
            Some(GitJob::Ask { from, name, text, wt, resolve }) => {
                let exe = self.boot.exe.clone();
                let branch = node.branch.trim().to_string();
                let prompt = words::git_commit(&exe, &node.message, &branch, wt.is_some(), resolve);
                let log = if resolve { format!("asked {name} to resolve a merge conflict") } else { format!("asked {name} to commit") };
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.git.last = Some(GitLast { name, text, wt, agent: from.clone() });
                }
                self.git_ask(id, &from, prompt, log, GitStage::Commit);
                return;
            }
            Some(GitJob::Commit { agent, name, text, wt }) => {
                rt.git.busy = true;
                rt.ui = true;
                rt.git.last = Some(GitLast { name: name.clone(), text: text.clone(), wt: wt.clone(), agent: agent.clone() });
                let ensure_branch = rt.git.log.iter().all(|l| l.starts_with("asked "));
                let subject = commit_subject(&node, &name, &text);
                let body = text.trim().to_string();
                let branch = node.branch.trim().to_string();
                // committed in the repository of the agent that did the work
                let dir = self.git_dir_for(id, &agent);
                let lock = self.git_lock(&dir);
                tokio::spawn(async move {
                    let _g = lock.lock().await;
                    let res = commit_work(&dir, &branch, ensure_branch, &subject, &body, wt.as_deref()).await;
                    let facts = facts_of(&dir, &res).await;
                    let _ = tx.send(Ev::Git { id: nid, gen, job: GitJobKind::Commit, res, facts });
                });
                self.set_status(id, Status::Busy);
                return;
            }
            None => {}
        }
        let Some(rt) = self.rt.get_mut(id) else { return };
        // nothing queued: when every incoming link has delivered this round, the commit step is
        // resolved (and goes on to what is linked after it), a push step pushes, a pull request
        // step pushes if nothing before it did and then has the pull request opened. Work in
        // several repositories is pushed (and gets a pull request) in each of them.
        if all_in && !rt.git.finishing {
            rt.git.finishing = true;
            let op = node.git_op;
            // a pull request only needs the push when nothing before it did that
            let do_push = if op == GitOp::Pr { needs_push } else { node.pushes() };
            if do_push {
                // what the pushes really add is the truth about the commits
                rt.git.facts.commits.clear();
            }
            if do_push || op == GitOp::Pr {
                let repos = self.git_repos(id);
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.git.repos = repos.into();
                }
                if do_push {
                    self.git_push_next(id);
                } else {
                    self.git_pr_next(id);
                }
            } else {
                let out = self.git_report(id);
                self.git_round_done(id, true, out);
            }
        }
    }

    /// Push the next repository of the round. After the last one a pull request step goes on to
    /// its pull requests and a push step is done.
    fn git_push_next(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        let Some(dir) = self.rt.get_mut(id).and_then(|r| r.git.repos.pop_front()) else {
            if node.git_op == GitOp::Pr {
                let repos = self.git_repos(id);
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.git.repos = repos.into();
                }
                return self.git_pr_next(id);
            }
            let out = self.git_report(id);
            return self.git_round_done(id, true, out);
        };
        let several = self.git_repos(id).len() > 1;
        // a push with no branch of its own pushes the branch the commits before it went to
        let branch = self.git_branch_in(id, &dir);
        let hashes =
            if several { self.git_hashes_in(id, &dir) } else { self.rt.get(id).map(|r| carry_hashes(&r.git.carry)).unwrap_or_default() };
        let remote = node.remote.trim().to_string();
        let (tx, nid) = (self.tx.clone(), id.to_string());
        let Some(rt) = self.rt.get_mut(id) else { return };
        let gen = rt.git.gen;
        rt.git.busy = true;
        rt.git.at = Some(dir.clone());
        rt.ui = true;
        tokio::spawn(async move {
            let (res, facts) = push(&dir, &remote, &branch, &hashes).await;
            let _ = tx.send(Ev::Git { id: nid, gen, job: GitJobKind::Push, res, facts });
        });
        self.set_status(id, Status::Busy);
    }

    /// An agent that did the work of this round in one repository and can still be asked: the one
    /// that finished last.
    fn git_helper_in(&self, id: &str, dir: &Path) -> Option<String> {
        let rt = self.rt.get(id)?;
        rt.git.carry.iter().rev().map(|c| c.node.clone()).find(|n| {
            !n.is_empty()
                && self.node(n).is_some_and(|x| x.kind() == Kind::Agent && x.agent != "sh")
                && (self.rt.get(n).is_some_and(|a| a.alive) || self.resumable(n))
                && self.git_dir_for(id, n) == dir
        })
    }

    /// Put a request for the agent that did the work at the front of its queue. Its answer comes
    /// back to this step (`git_response`), which knows from `stage` what it was asked.
    fn git_ask(&mut self, id: &str, agent: &str, prompt: String, log: String, stage: GitStage) {
        let shown = prompt.lines().next().unwrap_or("").to_string();
        self.enqueue(agent, prompt, shown, Src::Reply(id.to_string()), true);
        // (an agent that rests is started again on its own conversation)
        self.restart_if_queued(agent);
        if let Some(rt) = self.rt.get_mut(id) {
            rt.git.busy = true;
            rt.git.wait = Some(agent.to_string());
            rt.git.stage = stage;
            rt.git.log.push(log);
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
    }

    /// The commits are on the remote (or were before this step): in the next repository of the round,
    /// an agent that knows the work there writes the pull request, with the title and description only
    /// it can write well. After the last one the step is done.
    fn git_pr_next(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        let Some(dir) = self.rt.get_mut(id).and_then(|r| r.git.repos.pop_front()) else {
            let out = self.git_report(id);
            return self.git_round_done(id, true, out);
        };
        let several = self.git_repos(id).len() > 1;
        let there = if several { format!(" in {}", folder_name(&dir)) } else { String::new() };
        let Some(agent) = self.git_helper_in(id, &dir) else {
            return self.git_fail(id, format!("no agent that did this work{there} is still running to write the pull request"));
        };
        let branch = self.git_branch_in(id, &dir);
        let branch = if branch.is_empty() && !several { self.rt[id].git.facts.branch.clone() } else { branch };
        let report = self.git_report(id);
        let extra =
            if several { format!("{}\n{}", words::git_pr_in(&dir.display().to_string()), node.prompt) } else { node.prompt.clone() };
        let prompt = words::git_pull_request(&self.boot.exe, &branch, &report, &extra);
        let name = self.node(&agent).map(|n| n.name.clone()).unwrap_or_default();
        if let Some(rt) = self.rt.get_mut(id) {
            rt.git.at = Some(dir);
        }
        self.git_ask(id, &agent, prompt, format!("asked {name} to open the pull request{there}"), GitStage::Pr);
    }

    /// The agent reported the pull request.
    fn git_pr_done(&mut self, id: &str, ok: bool, text: &str) {
        if let Some(rt) = self.rt.get_mut(id) {
            rt.git.busy = false;
            rt.git.wait = None;
            rt.ui = true;
        }
        let there = match self.rt.get(id).and_then(|r| r.git.at.clone()) {
            Some(d) if self.git_repos(id).len() > 1 => format!(" ({})", folder_name(&d)),
            _ => String::new(),
        };
        match if ok { pr_link(text) } else { None } {
            Some((url, title)) => {
                if let Some(rt) = self.rt.get_mut(id) {
                    rt.git.log.push(format!("opened {url}{there}"));
                    if rt.git.facts.pr.is_empty() {
                        rt.git.facts.pr = url.clone();
                        rt.git.facts.pr_title = title;
                    }
                    if !rt.git.facts.prs.contains(&url) {
                        rt.git.facts.prs.push(url);
                    }
                }
                // the next repository's pull request, or done
                self.git_pr_next(id);
            }
            None => {
                let why = if ok {
                    format!("the agent did not report a link: {}", head_chars(text.trim(), 160))
                } else {
                    format!("the pull request was not opened: {}", head_chars(text.trim(), 160))
                };
                self.git_fail(id, why);
            }
        }
    }

    /// The agent brought the branch up to date after a rejected push: push again, once.
    fn git_rebased(&mut self, id: &str, ok: bool, text: &str) {
        if !ok {
            return self.git_fail(id, format!("could not bring the branch up to date: {}", head_chars(text.trim(), 160)));
        }
        // the repository whose push was rejected is pushed again (the ones before it are done)
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.git.log.push("branch brought up to date; pushing again".to_string());
        rt.git.busy = false;
        rt.ui = true;
        match rt.git.at.take() {
            Some(d) => {
                rt.git.repos.push_front(d);
                self.git_push_next(id);
            }
            None => {
                rt.git.finishing = false;
                self.git_pump(id, true);
            }
        }
    }

    /// The agent answered a request of this step. Trust git, not the text: look at HEAD and the tree.
    pub(super) fn git_response(&mut self, git: &str, ok: bool, text: &str) {
        let Some(node) = self.node(git).cloned() else { return };
        let stage = self
            .rt
            .get_mut(git)
            .map(|r| {
                r.git.wait = None;
                std::mem::take(&mut r.git.stage)
            })
            .unwrap_or_default();
        match stage {
            GitStage::Pr => return self.git_pr_done(git, ok, text),
            GitStage::Rebase => return self.git_rebased(git, ok, text),
            GitStage::Commit => {}
        }
        let (tx, gen, nid) = (self.tx.clone(), self.rt[git].git.gen, git.to_string());
        let (name, wt, agent) = {
            let Some(rt) = self.rt.get_mut(git) else { return };
            let last = rt.git.last.clone();
            (
                last.as_ref().map(|l| l.name.clone()).unwrap_or_default(),
                last.as_ref().and_then(|l| l.wt.clone()),
                last.map(|l| l.agent).unwrap_or_default(),
            )
        };
        if !ok {
            return self.git_fail(git, format!("{name} could not commit: {}", head_chars(text.trim(), 160)));
        }
        // git is read where the agent works: its commit is in its repository
        let dir = self.git_dir_for(git, &agent);
        let lock = self.git_lock(&dir);
        let branch = node.branch.trim().to_string();
        let subject = commit_subject(&node, &name, text);
        let note = head_chars(text.trim().lines().next().unwrap_or(""), 90);
        tokio::spawn(async move {
            let _g = lock.lock().await;
            let res = after_reply(&dir, &branch, wt.as_deref(), &name, &note, &subject).await;
            let facts = facts_of(&dir, &res).await;
            let _ = tx.send(Ev::Git { id: nid, gen, job: GitJobKind::Commit, res, facts });
        });
    }

    /// The result a commit or push step passes on: where the work is and what each agent that did it
    /// was asked and said. An agent linked after it (one that tests the change live, say) starts from this.
    pub(super) fn git_report(&self, id: &str) -> String {
        let (Some(node), Some(rt)) = (self.node(id), self.rt.get(id)) else { return String::new() };
        git_report(node, &self.git_repos(id), &rt.git.log, &rt.git.facts, &rt.git.carry)
    }

    /// The round fails: why goes last in its log, and the log is its result.
    fn git_fail(&mut self, id: &str, why: String) {
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.git.log.push(format!("failed: {why}"));
        let out = rt.git.log.join("\n");
        self.git_round_done(id, false, out);
    }

    pub(super) fn git_round_done(&mut self, id: &str, ok: bool, out: String) {
        if let Some(rt) = self.rt.get_mut(id) {
            rt.git.seen.clear();
            rt.git.jobs.clear();
            rt.git.finishing = false;
            rt.git.force = false;
            rt.git.busy = false;
            rt.git.wait = None;
            rt.git.conflicts = 0;
        }
        self.settle(id, ok, out);
    }

    pub(super) fn on_git(&mut self, id: String, gen: u64, job: GitJobKind, res: Result<String, String>, facts: Option<Facts>) {
        if job == GitJobKind::Look {
            return self.on_loose(&id, gen, res);
        }
        let push = job == GitJobKind::Push;
        // which repository a push was for, when there are several
        let there = match self.rt.get(&id).and_then(|r| r.git.at.clone()) {
            Some(d) if push && self.git_repos(&id).len() > 1 => format!(" ({})", folder_name(&d)),
            _ => String::new(),
        };
        // the folder changed under the "uncommitted" line (even when the node was reset meanwhile)
        self.refresh_project();
        let Some(rt) = self.rt.get_mut(&id) else { return };
        if rt.git.gen != gen {
            return; // a reset happened while the job ran
        }
        rt.git.busy = false;
        rt.ui = true;
        match res {
            Ok(line) => {
                if !push {
                    // whose work that commit was, for the report
                    let who = rt.git.last.as_ref().map(|l| l.agent.clone()).unwrap_or_default();
                    if let Some(c) = rt.git.carry.iter_mut().rev().find(|c| c.node == who && c.commit.is_empty()) {
                        c.commit = line.clone();
                    }
                }
                rt.git.log.push(format!("{line}{there}"));
                if let Some(f) = facts {
                    // (a push's facts are what it really added: the commits were cleared when the pushes began)
                    rt.git.facts.absorb(f);
                }
                if push {
                    let is_pr = self.node(&id).is_some_and(|n| n.git_op == GitOp::Pr);
                    let no_remote = self.rt.get(&id).and_then(|r| r.git.log.last()).is_some_and(|l| l.starts_with("no remote"));
                    if is_pr && no_remote {
                        // nowhere to open it against
                        self.git_fail(&id, "there is no remote to open a pull request against".into());
                    } else {
                        // the next repository, then the pull requests (or done)
                        self.git_push_next(&id);
                    }
                } else {
                    self.git_pump(&id, false);
                }
            }
            Err(e) => {
                // the merge of an agent's branch conflicts: that agent knows its code, so it resolves it
                if let (Some(rest), true, Some(GitLast { name, text, wt: Some(wt), agent })) =
                    (e.strip_prefix("CONFLICT:"), rt.git.conflicts < 1, rt.git.last.clone())
                {
                    rt.git.conflicts += 1;
                    rt.git.log.push(format!("conflict while merging ({})", head_chars(rest, 80)));
                    let from = agent;
                    if !from.is_empty() && (self.rt.get(&from).is_some_and(|r| r.alive) || self.resumable(&from)) {
                        if let Some(r) = self.rt.get_mut(&id) {
                            r.git.jobs.push_front(GitJob::Ask { from, name, text, wt: Some(wt), resolve: true });
                        }
                        self.git_pump(&id, true);
                        return;
                    }
                }
                // the remote has commits this branch lacks: the agent that did the work brings the
                // branch up to date (it knows the code), then the push is tried once more
                if push {
                    if let Some(rest) = e.strip_prefix("REJECTED:") {
                        let tried = self.rt.get(&id).is_none_or(|r| r.git.conflicts >= 1);
                        // the repository that refused it, and an agent that worked there
                        let dir = self.rt.get(&id).and_then(|r| r.git.at.clone()).unwrap_or_else(|| self.git_dir_for(&id, ""));
                        if let (false, Some(agent)) = (tried, self.git_helper_in(&id, &dir)) {
                            let branch = {
                                let b = self.git_branch_in(&id, &dir);
                                if b.is_empty() {
                                    self.rt[&id].git.facts.branch.clone()
                                } else {
                                    b
                                }
                            };
                            let remote = self
                                .node(&id)
                                .map(|n| n.remote.trim().to_string())
                                .filter(|r| !r.is_empty())
                                .unwrap_or_else(|| "origin".into());
                            let prompt = words::git_rebase(&self.boot.exe, &branch, &remote, &dir.display().to_string());
                            let name = self.node(&agent).map(|n| n.name.clone()).unwrap_or_default();
                            if let Some(r) = self.rt.get_mut(&id) {
                                r.git.conflicts += 1;
                                r.git.log.push(format!("push rejected: {}", head_chars(rest.trim(), 90)));
                            }
                            self.git_ask(&id, &agent, prompt, format!("asked {name} to bring the branch up to date"), GitStage::Rebase);
                            return;
                        }
                    }
                }
                let e = e.strip_prefix("REJECTED:").unwrap_or(&e).to_string();
                self.git_fail(&id, format!("{e}{there}"));
            }
        }
    }
}
