//! Rewind: every step an agent takes is bracketed by two snapshots of its folder, so what a step did
//! can be looked at as a diff and undone, for any CLI (an agent's own undo, where it has one, knows
//! only its own edits). A snapshot is a commit in a shadow repository under `.weft/cp/` that
//! borrows the real repository's objects: the folder, its index, its branches and its history never
//! change, and only content the real repository does not have yet is stored. Undoing restores just
//! the files those steps changed; a file someone changed after them is left as it is and named.
//! A rewind is a step on the timeline itself, so it can be undone the same way.

use super::*;

/// One step of an agent, as its folder looked before and after it.
#[derive(Clone, Serialize, Deserialize, Default)]
pub(super) struct Cp {
    /// its number on this card (counts up, never reused)
    pub(super) n: u32,
    /// what it was asked (the first line), or which steps the person rewound
    pub(super) what: String,
    pub(super) at: u64,
    /// the work tree it was taken in, and the shadow repository that keeps it
    #[serde(default)]
    pub(super) dir: PathBuf,
    #[serde(default)]
    pub(super) shadow: PathBuf,
    /// commits in the shadow repository ("" while one is being taken or the step runs)
    #[serde(default)]
    pub(super) before: String,
    #[serde(default)]
    pub(super) after: String,
    /// the files it changed (once `after` is there)
    #[serde(default)]
    pub(super) files: Vec<String>,
    /// the person undid it
    #[serde(default)]
    pub(super) undone: bool,
    /// a rewind the person did, and the steps it undid
    #[serde(default)]
    pub(super) rewind: bool,
    #[serde(default)]
    pub(super) undid: Vec<u32>,
    /// the names its snapshots are kept under in the shadow repository
    #[serde(default)]
    pub(super) refs: Vec<String>,
    /// the step ended before its first snapshot was in; the second follows it
    #[serde(skip)]
    pub(super) end_due: bool,
    #[serde(skip)]
    pub(super) ending: bool,
}

/// Which snapshot of a step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CpAt {
    Before,
    After,
}

/// What a rewind did: the snapshot just before it, the one after, the files put back and the ones left.
pub struct Rewound {
    pub(super) range: (u32, u32),
    pub(super) refs: Vec<String>,
    pub(super) before: String,
    pub(super) after: String,
    pub(super) dir: PathBuf,
    pub(super) shadow: PathBuf,
    pub(super) restored: Vec<String>,
    pub(super) kept: Vec<String>,
}

/// Steps a card keeps (the oldest go, and their snapshots with them).
const KEEP: usize = 24;
/// Untracked files larger than this are not copied into a snapshot (a video, a dataset, a build).
const BIG: u64 = 8 * 1024 * 1024;
/// The most text a step's diff shows.
const DIFF_MAX: usize = 400_000;

impl Engine {
    fn cp_base(&self) -> PathBuf {
        self.boot.dir.join(".weft").join("cp")
    }

    /// The folder an agent works in (its own worktree when it has one).
    fn cp_dir(&self, id: &str) -> Option<PathBuf> {
        let n = self.node(id).filter(|n| n.reads_prompts())?;
        Some(self.live_worktree(id).map(PathBuf::from).unwrap_or_else(|| self.node_dir(n)))
    }

    /// A step begins: the folder as it is now is its "before".
    pub(super) fn cp_begin(&mut self, id: &str, what: &str) {
        let Some(dir) = self.cp_dir(id) else { return };
        let base = self.cp_base();
        let Some(rt) = self.rt.get_mut(id) else { return };
        // a step still open (it never ended: a restart, a stop) ends here
        if rt.cps.last().is_some_and(|c| c.after.is_empty() && !c.ending && !c.rewind) {
            self.cp_end(id);
        }
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.cp_seq += 1;
        let n = rt.cp_seq;
        let what = what.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
        rt.cps.push(Cp { n, what: head_chars(&what, 160), at: now_ms(), ..Cp::default() });
        rt.ui = true;
        self.state_dirty = true;
        self.cp_snap(id, n, CpAt::Before, base, dir, String::new());
    }

    /// The step ended: the folder as it is now is its "after".
    pub(super) fn cp_end(&mut self, id: &str) {
        let base = self.cp_base();
        let Some(rt) = self.rt.get_mut(id) else { return };
        let Some(c) = rt.cps.iter_mut().rev().find(|c| c.after.is_empty() && !c.ending && !c.rewind) else { return };
        c.ending = true;
        if c.before.is_empty() {
            c.end_due = true;
            return;
        }
        let (n, dir, before) = (c.n, c.dir.clone(), c.before.clone());
        self.cp_snap(id, n, CpAt::After, base, dir, before);
    }

    fn cp_snap(&mut self, id: &str, n: u32, at: CpAt, base: PathBuf, dir: PathBuf, before: String) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let (tx, id) = (self.tx.clone(), id.to_string());
        rt.spawn(async move {
            let which = if at == CpAt::Before { "a" } else { "b" };
            let res = match snapshot(&base, &dir, &format!("{id}/{n}-{which}")).await {
                Ok(s) if at == CpAt::After => changed(&s.shadow, &before, &s.commit).await.map(|f| (s, f)),
                Ok(s) => Ok((s, Vec::new())),
                Err(e) => Err(e),
            };
            let _ = tx.send(Ev::Cp { id, n, at, res });
        });
    }

    pub(super) fn on_cp(&mut self, id: String, n: u32, at: CpAt, res: Result<(Snapshot, Vec<String>), String>) {
        let base = self.cp_base();
        let Some(rt) = self.rt.get_mut(&id) else { return };
        let Some(i) = rt.cps.iter().position(|c| c.n == n) else { return };
        rt.ui = true;
        self.state_dirty = true;
        let Ok((s, files)) = res else {
            // not a git folder, or git could not do it: no dot for this step
            rt.cps.remove(i);
            return;
        };
        match at {
            CpAt::Before => {
                let c = &mut rt.cps[i];
                c.before = s.commit;
                c.dir = s.top;
                c.shadow = s.shadow;
                c.refs.push(format!("refs/cp/{id}/{n}-a"));
                if c.end_due {
                    let (dir, before) = (c.dir.clone(), c.before.clone());
                    self.cp_snap(&id, n, CpAt::After, base, dir, before);
                }
            }
            CpAt::After => {
                if files.is_empty() {
                    // it changed nothing: nothing to look at or undo
                    let c = rt.cps.remove(i);
                    drop_refs(c.shadow, vec![format!("refs/cp/{id}/{n}-a"), format!("refs/cp/{id}/{n}-b")], false);
                    return;
                }
                let c = &mut rt.cps[i];
                c.after = s.commit;
                c.files = files;
                c.refs.push(format!("refs/cp/{id}/{n}-b"));
            }
        }
        let rt = self.rt.get_mut(&id).unwrap();
        while rt.cps.len() > KEEP {
            let c = rt.cps.remove(0);
            drop_refs(c.shadow, c.refs, false);
        }
    }

    /// What step `n` changed, as a diff (a step still running: up to now). Answered off the actor.
    pub(super) fn cmd_cp_diff(&mut self, id: String, n: u32, reply: Option<oneshot::Sender<Value>>) {
        let Some(reply) = reply else { return };
        let base = self.cp_base();
        let Some(c) = self.rt.get(&id).and_then(|r| r.cps.iter().find(|c| c.n == n)).cloned().filter(|c| !c.before.is_empty()) else {
            let _ = reply.send(json!({"ok": false, "error": "no such step, or its snapshot is not in yet"}));
            return;
        };
        tokio::spawn(async move {
            let after = if c.after.is_empty() {
                match snapshot(&base, &c.dir, &format!("{id}/now")).await {
                    Ok(s) => s.commit,
                    Err(e) => {
                        let _ = reply.send(json!({"ok": false, "error": e}));
                        return;
                    }
                }
            } else {
                c.after.clone()
            };
            let r = match sgit(&c.shadow, &c.dir, None, &["diff", "--stat=120", "--patch", "--find-renames", &c.before, &after], None).await {
                Ok(d) => json!({"ok": true, "diff": head_chars(&d, DIFF_MAX), "open": c.after.is_empty()}),
                Err(e) => json!({"ok": false, "error": format!("git cannot show it (a snapshot that old may be gone): {}", e.trim())}),
            };
            let _ = reply.send(r);
        });
    }

    /// Undo steps `from..=to` of an agent: the files they changed go back to how they were before
    /// `from`, except files that changed after them (named in the answer, unless `force`).
    pub(super) fn cmd_rewind(&mut self, id: String, from: u32, to: Option<u32>, force: bool) -> Value {
        let Some(rt) = self.rt.get(&id) else { return json!({"ok": false, "error": "no such node"}) };
        if rt.cur.is_some() || rt.typing.is_some() || rt.status == Status::Busy {
            return json!({"ok": false, "error": "it is working: wait until it is done (or stop it), then undo"});
        }
        let to = to.unwrap_or_else(|| rt.cps.last().map_or(from, |c| c.n));
        let steps: Vec<Cp> = rt.cps.iter().filter(|c| c.n >= from && c.n <= to).cloned().collect();
        if steps.is_empty() || steps.iter().any(|c| c.before.is_empty() || c.after.is_empty()) {
            return json!({"ok": false, "error": "there is no finished step to undo there"});
        }
        if steps.iter().any(|c| c.dir != steps[0].dir) {
            return json!({"ok": false, "error": "those steps worked in different folders: undo them one at a time"});
        }
        let base = self.cp_base();
        let (tx, nid) = (self.tx.clone(), id.clone());
        tokio::spawn(async move {
            let res = rewind(&base, &nid, &steps, force).await;
            let _ = tx.send(Ev::Rewound { id: nid, res });
        });
        json!({"ok": true})
    }

    pub(super) fn on_rewound(&mut self, id: String, res: Result<Rewound, String>) {
        let name = self.node(&id).map(|n| n.name.clone()).unwrap_or_default();
        let r = match res {
            Ok(r) => r,
            Err(e) => return self.toast("err", &format!("Could not undo the steps of {name}: {e}")),
        };
        let Some(rt) = self.rt.get_mut(&id) else { return };
        let (a, b) = r.range;
        let mut undid = Vec::new();
        // undoing nothing but a rewind puts the agent's own work back
        let back = rt.cps.iter().filter(|c| c.n >= a && c.n <= b).all(|c| c.rewind);
        for c in rt.cps.iter_mut().filter(|c| c.n >= a && c.n <= b) {
            if c.rewind {
                // undoing a rewind puts the steps it undid back
                c.undone = true;
                let back = c.undid.clone();
                for d in rt.cps.iter_mut().filter(|d| back.contains(&d.n)) {
                    d.undone = false;
                }
                break;
            }
        }
        for c in rt.cps.iter_mut().filter(|c| c.n >= a && c.n <= b && !c.rewind) {
            c.undone = true;
            undid.push(c.n);
        }
        let what = if back {
            "undo taken back".to_string()
        } else if a == b {
            format!("undo of step {a}")
        } else {
            format!("rewind of steps {a}\u{2013}{b}")
        };
        let asked: Vec<String> = rt.cps.iter().filter(|c| c.n >= a && c.n <= b && !c.rewind).map(|c| c.what.clone()).collect();
        if !r.restored.is_empty() {
            rt.cp_seq += 1;
            let n = rt.cp_seq;
            rt.cps.push(Cp {
                n,
                what: what.clone(),
                at: now_ms(),
                dir: r.dir,
                shadow: r.shadow,
                before: r.before,
                after: r.after,
                files: r.restored.clone(),
                rewind: true,
                undid,
                refs: r.refs,
                ..Cp::default()
            });
            rt.rewound = match (back, rt.rewound.take()) {
                // it was never told about the rewind: as far as it knows nothing happened
                (true, Some(_)) => None,
                (true, None) => Some(words::rewound_back(&r.restored)),
                (false, _) => Some(words::rewound(&asked, &r.restored)),
            };
        }
        rt.ui = true;
        self.state_dirty = true;
        self.refresh_project();
        let kept = if r.kept.is_empty() { String::new() } else { format!("; left as they are (changed since): {}", r.kept.join(", ")) };
        let msg = match r.restored.len() {
            0 => format!("Nothing to put back for {name}{kept}"),
            k => format!("{name}: {k} file{} back as {}{kept}", if k == 1 { "" } else { "s" }, if a == b { "before that step" } else { "before those steps" }),
        };
        self.toast(if r.kept.is_empty() { "ok" } else { "warn" }, &msg);
    }

    /// The card is gone: so are its snapshots (everything kept under its name, a diff's "now" too).
    pub(super) fn cp_forget(&mut self, id: &str) {
        let Some(rt) = self.rt.get(id) else { return };
        let mut shadows: Vec<PathBuf> = rt.cps.iter().map(|c| c.shadow.clone()).filter(|s| !s.as_os_str().is_empty()).collect();
        shadows.dedup();
        for shadow in shadows {
            drop_refs(shadow, vec![format!("refs/cp/{id}/")], true);
        }
    }
}

/// A snapshot as it was taken: the commit, the work tree and the shadow repository.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub(super) commit: String,
    pub(super) top: PathBuf,
    pub(super) shadow: PathBuf,
}

/// git with the shadow repository as its repository and the real folder as its work tree.
async fn sgit(shadow: &Path, top: &Path, index: Option<&Path>, args: &[&str], stdin: Option<Vec<u8>>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let mut c = tokio::process::Command::new("git");
    c.current_dir(top)
        .env("GIT_DIR", shadow)
        .env("GIT_WORK_TREE", top)
        .env("GIT_AUTHOR_NAME", "weft")
        .env("GIT_AUTHOR_EMAIL", "weft@localhost")
        .env("GIT_COMMITTER_NAME", "weft")
        .env("GIT_COMMITTER_EMAIL", "weft@localhost")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .args(args)
        .stdin(if stdin.is_some() { std::process::Stdio::piped() } else { std::process::Stdio::null() })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    match index {
        Some(i) => c.env("GIT_INDEX_FILE", i),
        None => c.env_remove("GIT_INDEX_FILE"),
    };
    let mut child = c.spawn().map_err(|e| format!("git: {e}"))?;
    if let (Some(data), Some(mut w)) = (stdin, child.stdin.take()) {
        let _ = w.write_all(&data).await;
    }
    let o = tokio::time::timeout(Duration::from_secs(120), child.wait_with_output()).await.map_err(|_| "git took too long".to_string())?.map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// The shadow repository of the repository at `common` (its git folder): made the first time, and
/// told where the real objects are and what the real repository leaves out.
fn ensure_shadow(shadow: &Path, common: &Path) -> Result<(), String> {
    if !shadow.join("HEAD").is_file() {
        std::fs::create_dir_all(shadow).map_err(|e| e.to_string())?;
        let init = std::process::Command::new("git").args(["init", "--bare", "-q"]).arg(shadow).output().map_err(|e| e.to_string())?;
        if !init.status.success() {
            return Err(String::from_utf8_lossy(&init.stderr).into_owned());
        }
        for kv in [["core.bare", "false"], ["gc.auto", "0"]] {
            let _ = std::process::Command::new("git").arg("--git-dir").arg(shadow).args(["config", kv[0], kv[1]]).output();
        }
    }
    let alt = format!("{}\n", common.join("objects").display());
    if std::fs::read_to_string(shadow.join("objects/info/alternates")).ok().as_deref() != Some(alt.as_str()) {
        write_atomic(&shadow.join("objects/info/alternates"), alt.as_bytes()).map_err(|e| e.to_string())?;
    }
    if let Ok(ex) = std::fs::read(common.join("info/exclude")) {
        let _ = write_atomic(&shadow.join("info/exclude"), &ex);
    }
    Ok(())
}

/// The folder's files as they are now (tracked and untracked, not ignored, not weft's own, no huge
/// untracked file), as a commit in its repository's shadow, kept under `refs/cp/<name>`.
pub(super) async fn snapshot(base: &Path, dir: &Path, name: &str) -> Result<Snapshot, String> {
    let info = git_run(dir, &["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir", "--git-path", "index"]).await?;
    let lines: Vec<&str> = info.1.lines().collect();
    let (true, [top, common, index]) = (info.0, lines.as_slice()) else { return Err("not in a git repository".into()) };
    let (top, common, index) = (PathBuf::from(top), PathBuf::from(common), PathBuf::from(index));
    let shadow = base.join(format!("{:016x}", fnv1a(common.to_string_lossy().as_bytes())));
    ensure_shadow(&shadow, &common)?;
    let idx = shadow.join(format!("index-{}", name.replace('/', "-")));
    let mut spec: Vec<String> = vec!["add".into(), "-A".into(), "--".into(), ".".into(), ":(exclude,glob)**/[.]weft/**".into()];
    // the real index carries what git already knows about every file: only what changed is read
    let seeded = std::fs::copy(&index, &idx).is_ok();
    if !seeded {
        if let Ok((true, t)) = git_run(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{tree}"]).await {
            sgit(&shadow, &top, Some(&idx), &["read-tree", t.trim()], None).await?;
        }
    }
    if let Ok(o) = sgit(&shadow, &top, Some(&idx), &["ls-files", "-o", "--exclude-standard", "-z"], None).await {
        for p in o.split('\0').filter(|p| !p.is_empty()).take(5_000) {
            if std::fs::metadata(top.join(p)).is_ok_and(|m| m.len() > BIG) {
                spec.push(format!(":(exclude,literal){p}"));
            }
        }
    }
    let args: Vec<&str> = spec.iter().map(String::as_str).collect();
    if let Err(e) = sgit(&shadow, &top, Some(&idx), &args, None).await {
        // an index git could not share (a split index, say): start from HEAD instead
        let _ = std::fs::remove_file(&idx);
        if let Ok((true, t)) = git_run(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{tree}"]).await {
            sgit(&shadow, &top, Some(&idx), &["read-tree", t.trim()], None).await?;
        }
        sgit(&shadow, &top, Some(&idx), &args, None).await.map_err(|e2| format!("{e}; {e2}"))?;
    }
    let tree = sgit(&shadow, &top, Some(&idx), &["write-tree"], None).await;
    let _ = std::fs::remove_file(&idx);
    let tree = tree?.trim().to_string();
    let commit = sgit(&shadow, &top, None, &["commit-tree", &tree, "-m", name], None).await?.trim().to_string();
    sgit(&shadow, &top, None, &["update-ref", &format!("refs/cp/{name}"), &commit], None).await?;
    Ok(Snapshot { commit, top, shadow })
}

/// The files that differ between two snapshots.
async fn changed(shadow: &Path, a: &str, b: &str) -> Result<Vec<String>, String> {
    let top = shadow.to_path_buf();
    let o = sgit(shadow, &top, None, &["diff", "--name-only", "-z", "--no-renames", a, b], None).await?;
    Ok(o.split('\0').filter(|p| !p.is_empty()).map(String::from).collect())
}

/// Put back what `steps` changed (see `cmd_rewind`).
async fn rewind(base: &Path, id: &str, steps: &[Cp], force: bool) -> Result<Rewound, String> {
    let (dir, shadow) = (steps[0].dir.clone(), steps[0].shadow.clone());
    let range = (steps[0].n, steps[steps.len() - 1].n);
    let names = [format!("{id}/{}-{}-now", range.0, range.1), format!("{id}/{}-{}-rewound", range.0, range.1)];
    let now = snapshot(base, &dir, &names[0]).await?;
    // every file the steps touched, and the last of them that touched it
    let mut last: Vec<(String, String)> = Vec::new();
    for c in steps {
        for f in changed(&shadow, &c.before, &c.after).await? {
            match last.iter_mut().find(|(p, _)| *p == f) {
                Some(e) => e.1 = c.after.clone(),
                None => last.push((f, c.after.clone())),
            }
        }
    }
    // a file that is not as the steps left it was changed since, by someone else (or the agent later on)
    let mut kept = Vec::new();
    if !force {
        let mut ends: Vec<String> = last.iter().map(|(_, a)| a.clone()).collect();
        ends.dedup();
        for end in ends {
            let since = changed(&shadow, &end, &now.commit).await?;
            kept.extend(last.iter().filter(|(p, a)| *a == end && since.contains(p)).map(|(p, _)| p.clone()));
        }
    }
    let todo: Vec<String> = last.into_iter().map(|(p, _)| p).filter(|p| !kept.contains(p)).collect();
    let start = &steps[0].before;
    let had = sgit(&shadow, &dir, None, &["ls-tree", "-r", "-z", "--name-only", start], None).await?;
    let had: std::collections::HashSet<&str> = had.split('\0').filter(|p| !p.is_empty()).collect();
    let (back, gone): (Vec<&String>, Vec<&String>) = todo.iter().partition(|p| had.contains(p.as_str()));
    if !back.is_empty() {
        // through a throwaway index: the real one is not touched
        let idx = shadow.join(format!("index-{id}-rewind"));
        let paths: Vec<u8> = back.iter().flat_map(|p| format!(":(literal){p}\0").into_bytes()).collect();
        let r = sgit(&shadow, &dir, Some(&idx), &["checkout", start, "--pathspec-from-file=-", "--pathspec-file-nul"], Some(paths)).await;
        let _ = std::fs::remove_file(&idx);
        r?;
    }
    for p in &gone {
        let _ = std::fs::remove_file(dir.join(p));
    }
    let after = snapshot(base, &dir, &names[1]).await?;
    let refs = names.iter().map(|n| format!("refs/cp/{n}")).collect();
    Ok(Rewound { range, refs, before: now.commit, after: after.commit, dir, shadow, restored: todo, kept })
}

/// Snapshots that are no longer kept give up their names (`prefix`: every name under the ones
/// given), and git drops, after a while, what only they held.
fn drop_refs(shadow: PathBuf, refs: Vec<String>, prefix: bool) {
    if shadow.as_os_str().is_empty() || refs.is_empty() {
        return;
    }
    let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
    rt.spawn(async move {
        let top = shadow.clone();
        let mut names = refs;
        if prefix {
            let mut args = vec!["for-each-ref", "--format=%(refname)"];
            args.extend(names.iter().map(String::as_str));
            names = sgit(&shadow, &top, None, &args, None).await.map(|o| o.lines().map(String::from).collect()).unwrap_or_default();
        }
        let input: Vec<u8> = names.iter().flat_map(|r| format!("delete {r}\n").into_bytes()).collect();
        let _ = sgit(&shadow, &top, None, &["update-ref", "--stdin"], Some(input)).await;
        // (the shadow never collects on its own: it borrows objects, and gc there packs only its own)
        let _ = sgit(&shadow, &top, None, &["-c", "gc.auto=2000", "gc", "--auto", "--quiet"], None).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("weft-rewind-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/a.txt"), "one\n").unwrap();
        std::fs::write(d.join("b.txt"), "bee\n").unwrap();
        std::fs::write(d.join(".gitignore"), "target/\n").unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-qm", "start"]);
        d
    }

    fn git(d: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git").arg("-C").arg(d).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    async fn step(base: &Path, d: &Path, n: u32, what: impl FnOnce()) -> Cp {
        let a = snapshot(base, d, &format!("t/{n}-a")).await.unwrap();
        what();
        let b = snapshot(base, d, &format!("t/{n}-b")).await.unwrap();
        let files = changed(&a.shadow, &a.commit, &b.commit).await.unwrap();
        Cp { n, dir: a.top, shadow: a.shadow, before: a.commit, after: b.commit, files, ..Cp::default() }
    }

    #[tokio::test]
    async fn a_step_is_undone_file_by_file_and_the_real_repository_never_changes() {
        let d = repo("undo");
        let base = d.join(".weft/cp");
        // the person staged something of their own: the index must stay exactly so
        std::fs::write(d.join("b.txt"), "bee, staged\n").unwrap();
        git(&d, &["add", "b.txt"]);
        let index0 = std::fs::read(d.join(".git/index")).unwrap();
        let refs0 = git(&d, &["for-each-ref"]);
        let s1 = step(&base, &d, 1, || {
            std::fs::write(d.join("src/a.txt"), "one, edited\n").unwrap();
            std::fs::write(d.join("src/new.txt"), "new\n").unwrap();
            std::fs::create_dir_all(d.join("target")).unwrap();
            std::fs::write(d.join("target/out.bin"), "ignored\n").unwrap();
        })
        .await;
        assert_eq!(s1.files, vec!["src/a.txt".to_string(), "src/new.txt".to_string()], "ignored files are no part of it");
        // someone else edits the new file after the step: that one is left alone
        std::fs::write(d.join("src/new.txt"), "new, then edited by another agent\n").unwrap();
        let r = rewind(&base, "t", std::slice::from_ref(&s1), false).await.unwrap();
        assert_eq!(r.restored, vec!["src/a.txt".to_string()]);
        assert_eq!(r.kept, vec!["src/new.txt".to_string()]);
        assert_eq!(std::fs::read_to_string(d.join("src/a.txt")).unwrap(), "one\n");
        assert!(d.join("src/new.txt").is_file() && d.join("target/out.bin").is_file());
        assert_eq!(std::fs::read(d.join(".git/index")).unwrap(), index0, "the real index is untouched");
        assert_eq!(git(&d, &["for-each-ref"]), refs0, "no branch, tag or ref appears in the real repository");
        assert!(git(&d, &["status", "--porcelain"]).contains("?? src/new.txt"));
        // the rewind is a step itself: undoing it puts the edit back
        let undo = Cp { n: 2, dir: r.dir.clone(), shadow: r.shadow.clone(), before: r.before.clone(), after: r.after.clone(), rewind: true, ..Cp::default() };
        let r2 = rewind(&base, "t", &[undo], false).await.unwrap();
        assert_eq!(r2.restored, vec!["src/a.txt".to_string()]);
        assert_eq!(std::fs::read_to_string(d.join("src/a.txt")).unwrap(), "one, edited\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn rewinding_to_an_earlier_step_takes_every_step_after_it_back_and_deletes_what_they_created() {
        let d = repo("range");
        let base = d.join(".weft/cp");
        let s1 = step(&base, &d, 1, || std::fs::write(d.join("src/a.txt"), "two\n").unwrap()).await;
        let s2 = step(&base, &d, 2, || {
            std::fs::write(d.join("src/a.txt"), "three\n").unwrap();
            std::fs::write(d.join("made.txt"), "made\n").unwrap();
            std::fs::remove_file(d.join("b.txt")).unwrap();
        })
        .await;
        let r = rewind(&base, "t", &[s1, s2], false).await.unwrap();
        assert!(r.kept.is_empty());
        assert_eq!(std::fs::read_to_string(d.join("src/a.txt")).unwrap(), "one\n");
        assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "bee\n", "a file a step deleted is back");
        assert!(!d.join("made.txt").exists(), "a file a step made is gone");
        assert_eq!(git(&d, &["status", "--porcelain", "--", ".", ":(exclude).weft"]), "", "the folder is as it was");
        let _ = std::fs::remove_dir_all(&d);
    }
}
