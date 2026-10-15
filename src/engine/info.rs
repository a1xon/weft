//! Facts about the outside world the engine asks for: the project folder and each CLI's models.

use super::*;

/// how often the folder is looked at again while a browser is open (on top of the moments it surely changed)
const PROJ_EVERY: Duration = Duration::from_secs(5);

/// Branch and uncommitted files of the project folder: the "am I in the right place" line. Also the command
/// that checks the work done in it (`check`), when one can be guessed.
async fn project_info(dir: &Path) -> Value {
    let mut v = git_info(dir).await;
    let check = guess_check(dir);
    if let (Some(o), false) = (v.as_object_mut(), check.is_empty()) {
        o.insert("check".into(), json!(check));
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
    let probe = async {
        let (inside, _) = git_run(dir, &["rev-parse", "--is-inside-work-tree"]).await?;
        if !inside {
            return Ok::<Value, String>(json!({"repo": false}));
        }
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
        Ok(json!({"repo": true, "branch": branch, "dirty": dirty, "last": last, "refs": refs}))
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

    /// Look at the project folder again: its branch and uncommitted files reach the browsers as `Ev::Proj`.
    /// One look at a time; asking during one queues exactly one more.
    pub(super) fn refresh_project(&mut self) {
        if self.proj_busy {
            self.proj_due = true;
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        self.proj_busy = true;
        self.proj_at = Instant::now();
        let (dir, tx) = (self.boot.dir.clone(), self.tx.clone());
        handle.spawn(async move {
            let _ = tx.send(Ev::Proj(project_info(&dir).await));
        });
    }

    pub(super) fn on_project(&mut self, v: Value) {
        self.proj_busy = false;
        if v != self.proj {
            self.proj = v.clone();
            self.bcast(&json!({"t": "proj", "git": v}));
        }
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

    pub(super) fn cmd_proj(&mut self) -> Value {
        self.refresh_project();
        json!({"ok": true})
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
