//! A CI check: a gate after a push (or a pull request) that waits for the CI runs of the pushed
//! commit, read with GitHub's own CLI (`gh run list --commit`, `gh run view --log-failed`). Green
//! opens it; red goes back to the agent that did the work with the failing part of the log, and that
//! agent fixes it, commits and pushes, and the runs of the new commit are waited for: a few times,
//! then the person decides. It is a tests gate (`Node.ci`), so its rounds are the tests gate's. It
//! never pushes by itself: a commit that is not on the remote yet is the person's call.

use super::*;

/// One workflow run of the commit, as `gh run list --json` gives it.
#[derive(Clone, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct CiRun {
    #[serde(default, rename = "databaseId")]
    pub(super) id: u64,
    #[serde(default, rename = "workflowName")]
    pub(super) workflow: String,
    #[serde(default)]
    pub(super) name: String,
    /// queued, in_progress, completed, ...
    #[serde(default)]
    pub(super) status: String,
    /// success, failure, cancelled, skipped, ... (empty until it completes)
    #[serde(default)]
    pub(super) conclusion: String,
    #[serde(default)]
    pub(super) url: String,
}

impl CiRun {
    fn done(&self) -> bool {
        self.status == "completed"
    }

    fn green(&self) -> bool {
        matches!(self.conclusion.as_str(), "success" | "skipped" | "neutral")
    }

    fn title(&self) -> &str {
        if self.workflow.is_empty() {
            &self.name
        } else {
            &self.workflow
        }
    }

    pub(super) fn to_json(&self) -> Value {
        json!({"id": self.id, "name": self.title(), "status": self.status, "conclusion": self.conclusion, "url": self.url})
    }
}

/// How a watch of the runs ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CiEnd {
    Green,
    Red,
    /// nothing to judge (not pushed, gh cannot see the repository, it never ends): the person decides
    Stuck,
}

/// How long the runs of a commit may take to appear, and to finish; how often they are looked at.
const APPEAR_S: u64 = 180;
const LONGEST_S: u64 = 90 * 60;
const POLL_S: u64 = 15;
/// Lines of a failed run's log that go to the agent (the end, where the error is).
const LOG_LINES: usize = 120;

impl Engine {
    /// Watch the CI runs of what the push before this gate pushed (in the folder of the work).
    pub(super) fn ci_start(&mut self, id: &str) {
        let Some(node) = self.node(id).cloned() else { return };
        let dir = self.node_dir(&node);
        let branch = self.ci_branch(id);
        let gh = self.cfg.gh.clone();
        let (tx, gen, nid) = (self.tx.clone(), self.rt[id].gate.gen, id.to_string());
        if let Some(rt) = self.rt.get_mut(id) {
            rt.gate.phase = GatePhase::Running;
            rt.gate.stage = GateStage::None;
            rt.gate.ci = true;
            rt.gate.ci_runs.clear();
            rt.ui = true;
        }
        self.set_status(id, Status::Busy);
        self.state_dirty = true;
        tokio::spawn(ci_watch(dir, gh, branch, tx, nid, gen));
    }

    /// The branch the git step before this gate pushed (or opened its pull request from); "" = the
    /// folder's current one.
    fn ci_branch(&self, id: &str) -> String {
        let mut seen = std::collections::HashSet::new();
        let mut todo = VecDeque::from([id.to_string()]);
        while let Some(at) = todo.pop_front() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for e in self.flow.edges.iter().filter(|e| e.to == at) {
                match self.node(&e.from).map(|n| n.kind()) {
                    Some(Kind::Git) => {
                        if let Some(b) = self.rt.get(&e.from).map(|r| r.git.facts.branch.clone()).filter(|b| !b.is_empty()) {
                            return b;
                        }
                        todo.push_back(e.from.clone());
                    }
                    Some(k) if k.is_gate() => todo.push_back(e.from.clone()),
                    _ => {}
                }
            }
        }
        String::new()
    }

    /// The agents whose work reached this gate through git steps: who fixes what CI found.
    pub(super) fn ci_targets(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut todo = VecDeque::from([id.to_string()]);
        while let Some(at) = todo.pop_front() {
            if !seen.insert(at.clone()) {
                continue;
            }
            for e in self.flow.edges.iter().filter(|e| e.to == at) {
                match self.node(&e.from) {
                    Some(n) if n.kind() == Kind::Agent && n.agent != "sh" => {
                        if !out.contains(&n.id) {
                            out.push(n.id.clone());
                        }
                    }
                    Some(n) if n.kind() == Kind::Step && !n.host.is_empty() => {
                        if !out.contains(&n.host) {
                            out.push(n.host.clone());
                        }
                    }
                    Some(n) if n.kind() == Kind::Git || n.kind().is_gate() => todo.push_back(n.id.clone()),
                    _ => {}
                }
            }
        }
        out
    }

    pub(super) fn on_ci(&mut self, id: String, gen: u64, sha: String, runs: Vec<CiRun>, end: Option<(CiEnd, String)>) {
        let Some(rt) = self.rt.get_mut(&id) else { return };
        if rt.gate.gen != gen || rt.gate.held.is_none() {
            return; // a reset or a new round happened meanwhile
        }
        rt.gate.ci_sha = sha;
        rt.gate.ci_runs = runs;
        rt.ui = true;
        let Some((how, out)) = end else { return };
        rt.gate.ci = false;
        match how {
            CiEnd::Green => self.gate_decide_tests(&id, true, out),
            CiEnd::Red => self.gate_decide_tests(&id, false, out),
            CiEnd::Stuck => {
                if let Some(rt) = self.rt.get_mut(&id) {
                    rt.gate.out.clear();
                }
                self.gate_review_with(&id, &out)
            }
        }
    }
}

/// Look at the runs of the branch's commit until they are all done, telling the engine what they
/// show on the way, and how it ended.
async fn ci_watch(dir: PathBuf, gh: String, branch: String, tx: mpsc::UnboundedSender<Ev>, id: String, gen: u64) {
    let send = |sha: &str, runs: &[CiRun], end: Option<(CiEnd, String)>| {
        let _ = tx.send(Ev::Ci { id: id.clone(), gen, sha: sha.to_string(), runs: runs.to_vec(), end });
    };
    let br = match branch {
        b if !b.is_empty() => b,
        _ => branch_of(&dir).await,
    };
    let Some(sha) = git_out(&dir, &["rev-parse", "--verify", "--quiet", &format!("{br}^{{commit}}")]).await.filter(|s| !s.is_empty()) else {
        return send("", &[], Some((CiEnd::Stuck, format!("There is no branch \"{br}\" in {} to look at CI for.", dir.display()))));
    };
    let short = sha.get(..7).unwrap_or(&sha).to_string();
    if git_out(&dir, &["rev-parse", "--verify", "--quiet", &format!("{br}@{{u}}")]).await.as_deref() != Some(sha.as_str()) {
        return send(&sha, &[], Some((CiEnd::Stuck, format!(
            "{br} at {short} is not on the remote yet, and CI only runs on what was pushed. Push it (a push step before this check does), then run the check again."
        ))));
    }
    let env = [("GH_PROMPT_DISABLED", "1"), ("NO_COLOR", "1"), ("GIT_TERMINAL_PROMPT", "0")];
    let list = format!("{gh} run list --commit {sha} --json databaseId,workflowName,name,status,conclusion,url --limit 30");
    let t0 = Instant::now();
    let (mut runs, mut errs): (Vec<CiRun>, u32) = (Vec::new(), 0);
    loop {
        let (ok, text) = run_shell(&dir, &list, &env, 60).await;
        match parse_runs(&text).filter(|_| ok) {
            None => {
                errs += 1;
                if errs >= 3 {
                    return send(&sha, &runs, Some((CiEnd::Stuck, format!("gh cannot read the CI runs here:\n{}", tail_bytes(text.trim(), 1200)))));
                }
            }
            Some(now) => {
                errs = 0;
                if now != runs {
                    runs = now;
                    send(&sha, &runs, None);
                }
                if !runs.is_empty() && runs.iter().all(CiRun::done) {
                    break;
                }
                if runs.is_empty() && t0.elapsed() >= Duration::from_secs(APPEAR_S) {
                    let why = if has_workflows(&dir).await { "its workflows do not run for this change" } else { "the repository has no GitHub workflows" };
                    return send(&sha, &runs, Some((CiEnd::Green, format!("No CI run started for {short} within {} minutes: {why}.", APPEAR_S / 60))));
                }
            }
        }
        if t0.elapsed() >= Duration::from_secs(LONGEST_S) {
            return send(&sha, &runs, Some((CiEnd::Stuck, format!("CI for {short} is still running after {} minutes.", LONGEST_S / 60))));
        }
        tokio::time::sleep(Duration::from_secs(if runs.is_empty() { 8 } else { POLL_S })).await;
    }
    let summary = runs.iter().map(|r| format!("{} {}: {}", if r.green() { "\u{2713}" } else { "\u{2717}" }, r.title(), r.conclusion)).collect::<Vec<_>>().join("\n");
    let red: Vec<&CiRun> = runs.iter().filter(|r| !r.green()).collect();
    if red.is_empty() {
        return send(&sha, &runs, Some((CiEnd::Green, format!("CI passed for {br} at {short}:\n{summary}"))));
    }
    let mut report = format!("CI failed for {br} at {short}:\n{summary}");
    for r in red.iter().take(3) {
        let (_, log) = run_shell(&dir, &format!("{gh} run view {} --log-failed", r.id), &env, 120).await;
        report.push_str(&format!("\n\n### {} ({})\n{}", r.title(), r.url, failed_log(&log, LOG_LINES)));
    }
    send(&sha, &runs, Some((CiEnd::Red, report)))
}

/// The runs in what `gh run list --json` printed (gh may put a notice around the JSON).
pub(super) fn parse_runs(text: &str) -> Option<Vec<CiRun>> {
    let (a, b) = (text.find('[')?, text.rfind(']')?);
    serde_json::from_str(text.get(a..=b)?).ok()
}

/// The end of a failed run's log, as the agent needs it: `gh run view --log-failed` prints
/// "job<TAB>step<TAB>2026-10-17T05:01:02.1234567Z message"; the job and step are said once.
pub(super) fn failed_log(log: &str, n: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut at = (String::new(), String::new());
    for line in log.lines() {
        let mut f = line.splitn(3, '\t');
        let (job, step, msg) = match (f.next(), f.next(), f.next()) {
            (Some(j), Some(s), Some(m)) => (j, s, m),
            _ => ("", "", line),
        };
        // the timestamp in front of every line says nothing the agent needs
        let msg = match msg.split_once(' ') {
            Some((ts, rest)) if ts.len() >= 20 && ts.ends_with('Z') && ts.as_bytes()[4] == b'-' => rest,
            _ => msg,
        };
        if !job.is_empty() && (job, step) != (at.0.as_str(), at.1.as_str()) {
            at = (job.to_string(), step.to_string());
            out.push(format!("--- {job} / {step}"));
        }
        out.push(msg.trim_end().to_string());
    }
    let skip = out.len().saturating_sub(n);
    let mut tail = out.split_off(skip);
    if skip > 0 {
        tail.insert(0, format!("({skip} earlier lines left out)"));
    }
    tail.join("\n")
}

/// Does the repository the folder is in have GitHub workflows?
async fn has_workflows(dir: &Path) -> bool {
    let top = match git_run(dir, &["rev-parse", "--show-toplevel"]).await {
        Ok((true, t)) => PathBuf::from(t.trim()),
        _ => dir.to_path_buf(),
    };
    std::fs::read_dir(top.join(".github").join("workflows")).is_ok_and(|mut d| d.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn runs_and_failed_logs_are_read_the_way_gh_prints_them() {
        let text = "A new release of gh is available\n[{\"databaseId\":42,\"workflowName\":\"CI\",\"name\":\"CI\",\"status\":\"completed\",\"conclusion\":\"failure\",\"url\":\"https://github.com/o/r/actions/runs/42\"},{\"databaseId\":43,\"workflowName\":\"lint\",\"name\":\"lint\",\"status\":\"in_progress\",\"conclusion\":\"\",\"url\":\"\"}]\n";
        let runs = parse_runs(text).unwrap();
        assert_eq!(runs.len(), 2);
        assert!(runs[0].done() && !runs[0].green() && runs[0].title() == "CI");
        assert!(!runs[1].done());
        assert_eq!(parse_runs("[]"), Some(vec![]));
        assert_eq!(parse_runs("HTTP 404: Not Found"), None);
        let log = "build\tRun tests\t2026-10-17T05:01:02.1234567Z running 3 tests\n\
                   build\tRun tests\t2026-10-17T05:01:03.0000000Z test wrap ... FAILED\n\
                   build\tRun tests\t2026-10-17T05:01:03.5000000Z assert len(line) <= 10\n";
        assert_eq!(failed_log(log, 120), "--- build / Run tests\nrunning 3 tests\ntest wrap ... FAILED\nassert len(line) <= 10");
        assert_eq!(failed_log(log, 1), "(3 earlier lines left out)\nassert len(line) <= 10");
    }

    #[tokio::test]
    async fn a_commit_that_is_not_pushed_is_the_persons_call_and_is_never_pushed_by_weft() {
        let dir = std::env::temp_dir().join(format!("weft-ci-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git").arg("-C").arg(&dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "a"]);
        let (tx, mut rx) = mpsc::unbounded_channel();
        // (gh would fail here: it must not even be asked)
        ci_watch(dir.clone(), "false".into(), String::new(), tx, "g".into(), 1).await;
        match rx.try_recv() {
            Ok(Ev::Ci { end: Some((CiEnd::Stuck, why)), sha, .. }) => {
                assert!(why.contains("main at") && why.contains("not on the remote"), "{why}");
                assert_eq!(sha.len(), 40);
            }
            _ => panic!("expected the check to stop for the person"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ci_check_after_a_push_knows_its_branch_and_who_fixes() {
        let mut e = engine();
        alive_agent(&mut e, "a", "build");
        add_kind(&mut e, "c", "commit", "git", "");
        add_kind(&mut e, "p", "push", "git", "");
        add_kind(&mut e, "g", "CI", "tests", "");
        add_kind(&mut e, "s", "step", "step", "also this");
        e.flow.nodes.iter_mut().find(|n| n.id == "s").unwrap().host = "a".into();
        for (id, from, to) in [("e1", "a", "s"), ("e2", "s", "c"), ("e3", "c", "p"), ("e4", "p", "g")] {
            link(&mut e, id, from, to, On::Done);
        }
        e.rt.get_mut("p").unwrap().git.facts.branch = "issue-42".into();
        assert_eq!(e.ci_branch("g"), "issue-42");
        assert_eq!(e.ci_targets("g"), vec!["a".to_string()]);
    }
}
