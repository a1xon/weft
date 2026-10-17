//! Facts about the outside world the engine asks for: the project folder and each CLI's models.

use super::*;

/// how often the folder is looked at again while a browser is open (on top of the moments it surely changed)
const PROJ_EVERY: Duration = Duration::from_secs(5);
/// folders a browser asked about that no agent works in yet: the last few are kept
const PROJ_ASKED: usize = 4;

/// Branch and uncommitted files of a folder: the "am I in the right place" line. Also the command
/// that checks the work done in it (`check`), when one can be guessed.
async fn project_info(dir: &Path) -> Value {
    let mut v = git_info(dir).await;
    let check = guess_check(dir);
    if let (Some(o), false) = (v.as_object_mut(), check.is_empty()) {
        o.insert("check".into(), json!(check));
    }
    // and the one that puts it to use, for a deploy step (deploy.rs)
    let deploy = guess_deploy(dir);
    if let (Some(o), false) = (v.as_object_mut(), deploy.is_empty()) {
        o.insert("deploy".into(), json!(deploy));
    }
    v
}

/// The command a network's check gate runs in a folder unless the person typed another: build and
/// tests, guessed from the build files there. Empty: none found (the agent then runs the checks).
pub(super) fn guess_check(dir: &Path) -> String {
    let has = |f: &str| dir.join(f).exists();
    if has("Cargo.toml") {
        return "cargo build && cargo test".into();
    }
    if has("package.json") {
        let scripts = std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|p| p.get("scripts").cloned())
            .unwrap_or(Value::Null);
        let pm = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        // (npm's placeholder test script fails on purpose: it is no test)
        let real = |k: &str| scripts.get(k).and_then(|x| x.as_str()).is_some_and(|x| !x.contains("no test specified"));
        return ["build", "lint", "test"]
            .iter()
            .filter(|k| real(k))
            .map(|k| if *k == "test" && pm != "bun" { format!("{pm} test") } else { format!("{pm} run {k}") })
            .collect::<Vec<_>>()
            .join(" && ");
    }
    if has("go.mod") {
        return "go build ./... && go test ./...".into();
    }
    let py_tests = || {
        std::fs::read_dir(dir.join("tests"))
            .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".py")))
            .unwrap_or(false)
    };
    // pytest where the project uses it, else the standard library's runner (pytest may not be installed)
    let says = |f: &str| std::fs::read_to_string(dir.join(f)).is_ok_and(|t| t.contains("pytest"));
    if has("pytest.ini") || has("conftest.py") || ["pyproject.toml", "setup.cfg", "tox.ini"].iter().any(|f| says(f)) {
        return "python3 -m pytest -q".into();
    }
    if py_tests() {
        return "python3 -m unittest discover -s tests -q".into();
    }
    if has("gradlew") {
        return "./gradlew build".into();
    }
    if has("pom.xml") {
        return "mvn -q verify".into();
    }
    if has("mix.exs") {
        return "mix test".into();
    }
    match std::fs::read_to_string(dir.join("Makefile")) {
        Ok(m) if m.lines().any(|l| l.starts_with("test:")) => "make test".into(),
        _ => String::new(),
    }
}

async fn git_info(dir: &Path) -> Value {
    if !dir.is_dir() {
        return json!({"repo": false, "missing": true});
    }
    let probe = async {
        let (inside, out) = git_run(dir, &["rev-parse", "--is-inside-work-tree", "--git-common-dir"]).await?;
        if !inside {
            return Ok::<Value, String>(json!({"repo": false}));
        }
        // which repository it is: git's shared folder, the same for every folder of a checkout and
        // for the checkout's worktrees (git prints it relative to the folder asked about, or absolute)
        let common = out.lines().nth(1).map(|c| dir.join(c.trim())).map(|c| c.canonicalize().unwrap_or(c)).unwrap_or_default();
        let (ok, b) = git_run(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).await?;
        let branch = if ok && !b.trim().is_empty() {
            b.trim().to_string()
        } else {
            let (_, h) = git_run(dir, &["rev-parse", "--short", "HEAD"]).await?;
            format!("detached {}", h.trim())
        };
        let (_, st) = git_run(dir, &["status", "--porcelain", "--", ".", ":(exclude).weft"]).await?;
        let dirty = st.lines().filter(|l| !l.trim().is_empty()).count();
        let (_, last) = git_run(dir, &["log", "-1", "--format=%h %s"]).await.unwrap_or((false, String::new()));
        let last =
            if last.contains("does not have any commits") || last.starts_with("fatal") { String::new() } else { last.trim().to_string() };
        // (a commit on any branch, a fetch, a new branch: the git tree in the corner looks again)
        let (_, refs) = git_run(dir, &["for-each-ref", "--format=%(objectname)%(refname)", "refs/heads", "refs/remotes"]).await?;
        let refs = fingerprint(&refs);
        Ok(json!({"repo": true, "branch": branch, "dirty": dirty, "last": last, "refs": refs, "common": common}))
    };
    match tokio::time::timeout(Duration::from_secs(4), probe).await {
        Ok(Ok(v)) => v,
        _ => json!({"repo": false, "unknown": true}),
    }
}

/// A short stand-in for a long text: equal texts, equal fingerprints.
fn fingerprint(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:x}", h.finish())
}

/// A folder that could be worked in.
struct Sub {
    path: PathBuf,
    /// it is (the top of) a git repository
    repo: bool,
    touched: Option<SystemTime>,
}

/// The folders in `base` whose names begin with `prefix` (any case); hidden ones only when the
/// prefix asks for them. Quick: one directory, and only so many entries of it.
fn subdirs(base: &Path, prefix: &str) -> Vec<Sub> {
    let Ok(rd) = std::fs::read_dir(base) else { return Vec::new() };
    let pre = prefix.to_lowercase();
    let mut out = Vec::new();
    for e in rd.flatten().take(2000) {
        let name = e.file_name().to_string_lossy().into_owned();
        if (name.starts_with('.') && !pre.starts_with('.')) || !name.to_lowercase().starts_with(&pre) {
            continue;
        }
        let path = e.path();
        // (a link to a folder counts as one)
        let Ok(md) = std::fs::metadata(&path) else { continue };
        if !md.is_dir() {
            continue;
        }
        let repo = path.join(".git").exists();
        out.push(Sub { path, repo, touched: md.modified().ok() });
    }
    out
}

/// Run an agent's model-listing command the way its terminal would (aliases apply).
async fn list_models(cmd: &str) -> Result<Vec<(String, String)>, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let run = tokio::process::Command::new(&shell)
        .arg("-ic")
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = match tokio::time::timeout(Duration::from_secs(25), run).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("timed out".into()),
    };
    let list = parse_models(&String::from_utf8_lossy(&out.stdout));
    if list.is_empty() {
        Err(format!("`{cmd}` listed no models"))
    } else {
        Ok(list)
    }
}

/// `id<TAB>label` lines, bare ids, or a JSON catalog (an array of objects with a
/// slug/id/model/name, skipping entries the CLI itself hides).
pub(super) fn parse_models(text: &str) -> Vec<(String, String)> {
    let t = text.trim();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |id: &str, label: &str| {
        if plain_word(id) && !out.iter().any(|(i, _)| i == id) {
            out.push((id.to_string(), if label.trim().is_empty() { id.to_string() } else { label.trim().to_string() }));
        }
    };
    if t.starts_with('{') || t.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            let arr = match &v {
                Value::Array(a) => Some(a),
                Value::Object(o) => o.values().find_map(|x| x.as_array()),
                _ => None,
            };
            for m in arr.into_iter().flatten() {
                if let Some(s) = m.as_str() {
                    push(s, s);
                    continue;
                }
                let Some(o) = m.as_object() else { continue };
                if matches!(o.get("visibility").and_then(|x| x.as_str()), Some(vis) if vis != "list") {
                    continue;
                }
                if o.get("hidden").and_then(|x| x.as_bool()) == Some(true) {
                    continue;
                }
                let pick = |keys: &[&str]| keys.iter().find_map(|k| o.get(*k).and_then(|x| x.as_str()));
                if let Some(id) = pick(&["slug", "id", "model", "name"]) {
                    push(id, pick(&["display_name", "label", "title"]).unwrap_or(id));
                }
            }
        }
    } else {
        let tabbed = t.lines().any(|l| l.contains('\t'));
        for l in t.lines() {
            if tabbed {
                if let Some((a, b)) = l.split_once('\t') {
                    push(a.trim(), b);
                }
            } else {
                push(l.trim(), "");
            }
        }
    }
    out.truncate(60);
    out
}

impl Engine {
    pub(super) fn cmd_pref_set(&mut self, prefs: Value) -> Value {
        if !prefs.is_object() || prefs.to_string().len() > 8192 {
            return json!({"ok": false, "error": "preferences must be a small object"});
        }
        self.cfg.prefs = prefs;
        self.persist_config("prefs", self.cfg.prefs.clone());
        let m = json!({"t": "prefs", "prefs": self.cfg.prefs});
        self.bcast(&m);
        json!({"ok": true})
    }

    /// The folders whose branch and uncommitted files the browsers show: the project's, every
    /// other one an agent works in, and the ones a browser asked about.
    fn project_dirs(&self) -> Vec<PathBuf> {
        let mut out = vec![self.boot.dir.clone()];
        let agents = self.flow.nodes.iter().filter(|n| n.kind() == Kind::Agent && !n.cwd.trim().is_empty()).map(|n| self.node_dir(n));
        for d in agents.chain(self.proj_ask.iter().rev().cloned()) {
            if out.len() < 12 && !out.contains(&d) {
                out.push(d);
            }
        }
        out
    }

    /// Look at the folders again: their branches and uncommitted files reach the browsers as `Ev::Proj`.
    /// One look at a time; asking during one queues exactly one more.
    pub(super) fn refresh_project(&mut self) {
        if self.proj_busy {
            self.proj_due = true;
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        self.proj_busy = true;
        self.proj_at = Instant::now();
        let (dirs, tx) = (self.project_dirs(), self.tx.clone());
        handle.spawn(async move {
            let mut out = Vec::with_capacity(dirs.len());
            for d in dirs {
                let v = project_info(&d).await;
                out.push((d, v));
            }
            let _ = tx.send(Ev::Proj(out));
        });
    }

    pub(super) fn on_project(&mut self, list: Vec<(PathBuf, Value)>) {
        self.proj_busy = false;
        let mut seen = HashMap::with_capacity(list.len());
        for (d, v) in list {
            if self.proj.get(&d) != Some(&v) {
                self.bcast(&json!({"t": "proj", "dir": d, "git": v}));
                // a push, a fetch, a commit: what a push nobody armed has left is looked at again (loose.rs)
                for rt in self.rt.values_mut() {
                    rt.git.looked = 0;
                }
            }
            seen.insert(d, v);
        }
        // a folder nobody works in any more is not looked at again
        self.proj = seen;
        if std::mem::take(&mut self.proj_due) {
            self.refresh_project();
        }
    }

    /// Agents edit files without telling anyone: while a browser shows the line, look now and then.
    pub(super) fn tick_project(&mut self) {
        if !self.clients.is_empty() && self.proj_at.elapsed() >= PROJ_EVERY {
            self.refresh_project();
        }
    }

    /// Look again now; `dir` (a folder the person is about to start agents in) is looked at from now on.
    pub(super) fn cmd_proj(&mut self, dir: String) -> Value {
        // (a page came to the front: another board may have changed the settings meanwhile)
        self.sync_user_config();
        let mut known = Value::Null;
        if !dir.trim().is_empty() {
            let d = self.abs_dir(&dir);
            known = self.proj.get(&d).cloned().unwrap_or(Value::Null);
            if d != self.boot.dir {
                self.proj_ask.retain(|x| *x != d);
                self.proj_ask.push(d);
                if self.proj_ask.len() > PROJ_ASKED {
                    self.proj_ask.remove(0);
                }
            }
        }
        self.refresh_project();
        json!({"ok": true, "git": known})
    }

    /// Folders to start agents in. Nothing typed: the folders beside the project (its sibling
    /// repositories), those whose name begins like the project's first, then git repositories, then
    /// the ones touched last; `like` keeps those whose name has it. A path typed so far (`q`): the
    /// folders that complete it.
    pub(super) fn cmd_dirs(&self, q: &str, like: &str) -> Value {
        let (q, like) = (q.trim(), like.trim().to_lowercase());
        let list = if q.is_empty() {
            let Some(parent) = self.boot.dir.parent() else { return json!({"ok": true, "dirs": []}) };
            let lower = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let name = lower(&self.boot.dir);
            let mut l: Vec<Sub> =
                subdirs(parent, "").into_iter().filter(|s| s.path != self.boot.dir && lower(&s.path).contains(&like)).collect();
            let kin = |s: &Sub| {
                let n = lower(&s.path);
                // (a name the search begins with comes before one that merely has it)
                let first = !like.is_empty() && n.starts_with(&like);
                let k = n.chars().zip(name.chars()).take_while(|(a, b)| a == b).count();
                (first, if k >= 3 { k } else { 0 })
            };
            l.sort_by(|a, b| kin(b).cmp(&kin(a)).then(b.repo.cmp(&a.repo)).then(b.touched.cmp(&a.touched)));
            l.truncate(if like.is_empty() { 12 } else { 30 });
            l
        } else {
            let p = self.abs_dir(q);
            let (base, pre) = if q.ends_with('/') || p.file_name().is_none() {
                (p, String::new())
            } else {
                (
                    p.parent().map(Path::to_path_buf).unwrap_or_default(),
                    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                )
            };
            let mut l = subdirs(&base, &pre);
            l.sort_by(|a, b| a.path.cmp(&b.path));
            l.truncate(30);
            l
        };
        let dirs: Vec<Value> = list.iter().map(|s| json!({"path": s.path, "repo": s.repo})).collect();
        json!({"ok": true, "dirs": dirs})
    }

    pub(super) fn cmd_models(&mut self, agent: String, force: bool) -> Value {
        let Some(a) = self.cfg.agents.get(&agent) else {
            return json!({"ok": false, "error": "unknown agent"});
        };
        let cmd = a.models_cmd.trim().to_string();
        if cmd.is_empty() {
            return json!({"ok": true, "models": []});
        }
        if !force {
            if let Some(l) = self.models.get(&agent) {
                let items: Vec<Value> = l.iter().map(|(id, label)| json!({"id": id, "label": label})).collect();
                return json!({"ok": true, "models": items});
            }
        }
        if self.models_busy.insert(agent.clone()) {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let res = list_models(&cmd).await;
                let _ = tx.send(Ev::Models { agent, res });
            });
        }
        json!({"ok": true, "pending": true})
    }

    pub(super) fn cmd_lib_set(&mut self, library: Vec<Snippet>) -> Value {
        self.cfg.library = library;
        self.persist_config("library", serde_json::to_value(&self.cfg.library).unwrap_or_default());
        let m = json!({"t": "lib", "library": self.cfg.library});
        self.bcast(&m);
        json!({"ok": true})
    }

    pub(super) fn cmd_net_set(&mut self, networks: Vec<Network>) -> Value {
        let value = serde_json::to_value(&networks).unwrap_or_default();
        if value.to_string().len() > 512 * 1024 {
            return json!({"ok": false, "error": "that is too much to keep: remove a network first"});
        }
        self.cfg.networks = networks;
        self.persist_config("networks", value);
        let m = json!({"t": "nets", "networks": self.cfg.networks});
        self.bcast(&m);
        json!({"ok": true})
    }

    /// Write one setting back to the user's config file, if there is one.
    fn persist_config(&self, key: &str, value: Value) {
        if let Some(path) = &self.boot.config_file {
            save_config_key(path, key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[tokio::test]
    async fn folders_of_one_repository_and_its_worktrees_name_the_same_repository() {
        let tmp = std::env::temp_dir().join(format!("weft-info-{}", now_ms()));
        let (site, crawler) = (tmp.join("site"), tmp.join("crawler"));
        for d in [&site, &crawler] {
            std::fs::create_dir_all(d.join("src")).unwrap();
            sh(d, &["init", "-q", "-b", "main"]);
            sh(d, &["commit", "-q", "--allow-empty", "-m", "first"]);
        }
        sh(&site, &["worktree", "add", "-q", "-b", "side", tmp.join("site-wt").to_str().unwrap()]);
        let common = |v: Value| v["common"].as_str().unwrap_or("").to_string();
        let top = common(git_info(&site).await);
        assert!(top.ends_with("site/.git"), "{top}");
        assert_eq!(common(git_info(&site.join("src")).await), top, "a folder inside it");
        assert_eq!(common(git_info(&tmp.join("site-wt")).await), top, "a worktree of it");
        assert_ne!(common(git_info(&crawler).await), top, "another repository");
        assert_eq!(git_info(&tmp).await["repo"], false, "the folder that holds both is none");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
