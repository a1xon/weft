//! Whose uncommitted files are they? Every uncommitted file of a work tree the board's agents work in
//! is put to the cards whose steps changed it (rewind.rs snapshots the folder before and after every
//! step, so it knows which files changed while a step ran, and when that was). What no card on the
//! board changed is left over: work of a card that was filed or deleted, of another session, of the
//! person. The page shows the count on each card and the whole list under the uncommitted files in the
//! git tree, where one click has an agent commit what is left over (`cmd_strays`).
//!
//! Several agents in one folder: a step's snapshots see every change made while it ran, its own and
//! the others'. So a file that steps of several cards list, at times that overlap, goes to the one
//! that names it in what it reported (agents are asked to list the files they changed) or that alone
//! was at work when the file was last written. When nothing tells them apart it goes to all of them,
//! marked as unsure. A change that was committed since is no one's any more: only what a step did
//! after the file's last commit counts.

use super::*;

/// While a step runs its files change without telling anyone: they are put to their cards again this
/// often. Else only when something that decides it changed.
const OPEN_EVERY: Duration = Duration::from_secs(15);
/// The most files of one work tree the page is told about (a build folder nobody ignored).
const FILES_MAX: usize = 400;
/// Snapshots are taken a moment after a step starts and ends.
const SLACK_MS: u64 = 3_000;
/// How much of what a card said is searched for the names of files.
const SAID_MAX: usize = 60_000;
/// git that takes longer than this (a huge tree) gives up: the files keep the cards they had.
const GIT_WAIT: Duration = Duration::from_secs(8);
/// The most of an uncommitted file's diff the page is shown.
const DIFF_MAX: usize = 400_000;

/// A card's steps in one work tree.
#[derive(Clone, Debug)]
pub(super) struct Worker {
    pub(super) id: String,
    pub(super) steps: Vec<Stint>,
    /// what it said in its reports, lowercased (filled in only for a look that runs), and how long
    /// that is, which says whether it said more
    pub(super) said: String,
    pub(super) said_n: usize,
}

/// One step: when it ran (no end while it runs) and the files that changed meanwhile (none known
/// while it runs: the file's time says whether it was written since the step began).
#[derive(Clone, Debug)]
pub(super) struct Stint {
    pub(super) at: u64,
    pub(super) end: Option<u64>,
    pub(super) files: Option<Vec<String>>,
}

impl Engine {
    /// The work trees to look at (the folders the line above the bar can name, and every one a card
    /// worked in), and who worked in each.
    fn owner_inputs(&self) -> Vec<(PathBuf, Vec<Worker>)> {
        let mut out: Vec<(PathBuf, Vec<Worker>)> = Vec::new();
        let mut tops: Vec<PathBuf> = self.proj.values().filter_map(|v| v["top"].as_str()).map(PathBuf::from).collect();
        tops.sort();
        tops.dedup();
        for t in tops {
            out.push((t, Vec::new()));
        }
        for n in &self.flow.nodes {
            let Some(rt) = self.rt.get(&n.id) else { continue };
            let said_n = rt.steps.iter().map(|s| s.text.len()).sum::<usize>()
                + rt.output.as_ref().map_or(0, String::len)
                + rt.issue.as_ref().map_or(0, String::len);
            for (i, c) in rt.cps.iter().enumerate() {
                // (an undone step changed nothing that is still there; a rewind is the person's)
                if c.before.is_empty() || c.undone || c.rewind || c.dir.as_os_str().is_empty() {
                    continue;
                }
                let stint = if c.after.is_empty() {
                    Stint { at: c.at, end: (c.end > 0).then_some(c.end), files: None }
                } else {
                    // (a step from before steps kept their end: until the next one began, or the card finished)
                    let end = if c.end > 0 { c.end } else { rt.cps.get(i + 1).map_or(rt.since.max(c.at), |d| d.at) };
                    Stint { at: c.at, end: Some(end), files: Some(c.files.clone()) }
                };
                let i = match out.iter().position(|(t, _)| *t == c.dir) {
                    Some(i) => i,
                    None => {
                        out.push((c.dir.clone(), Vec::new()));
                        out.len() - 1
                    }
                };
                let ws = &mut out[i].1;
                match ws.iter_mut().find(|w| w.id == n.id) {
                    Some(w) => w.steps.push(stint),
                    None => ws.push(Worker { id: n.id.clone(), steps: vec![stint], said: String::new(), said_n }),
                }
            }
        }
        out.truncate(12);
        out
    }

    /// Put the uncommitted files to their cards again, when something that decides it changed (`proj`:
    /// what git says of the folders did). One look at a time; asking during one queues one more.
    pub(super) fn refresh_owners(&mut self, proj: bool) {
        if self.clients.is_empty() {
            // nobody looks: the next page to connect has them looked at again
            self.owners_due = true;
            return;
        }
        if self.owners_busy {
            self.owners_due |= proj;
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        let mut inputs = self.owner_inputs();
        let key = inputs_key(&inputs);
        let open = inputs.iter().any(|(_, ws)| ws.iter().any(|w| w.steps.iter().any(|s| s.files.is_none())));
        let due = std::mem::take(&mut self.owners_due);
        let stale = open && self.owners_at.elapsed() >= OPEN_EVERY;
        if !(proj || due || stale || key != self.owners_key) {
            return;
        }
        for w in inputs.iter_mut().flat_map(|(_, ws)| ws.iter_mut()) {
            if let Some(rt) = self.rt.get(&w.id) {
                w.said = said_by(rt);
            }
        }
        self.owners_key = key;
        self.owners_busy = true;
        self.owners_at = Instant::now();
        let tx = self.tx.clone();
        handle.spawn(async move {
            let mut out = Vec::with_capacity(inputs.len());
            for (top, ws) in inputs {
                let v = match tokio::time::timeout(GIT_WAIT, owners(&top, &ws)).await {
                    Ok(v) => v,
                    Err(_) => Value::Null,
                };
                out.push((top, v));
            }
            let _ = tx.send(Ev::Owners(out));
        });
    }

    pub(super) fn on_owners(&mut self, list: Vec<(PathBuf, Value)>) {
        self.owners_busy = false;
        let mut seen: HashMap<PathBuf, Value> = HashMap::with_capacity(list.len());
        for (top, v) in list {
            // (git could not say this time: what it said last stands, unless the folder is gone)
            let v = if v.is_null() && top.is_dir() { self.owners.get(&top).cloned().unwrap_or(Value::Null) } else { v };
            if v.is_null() {
                continue;
            }
            if self.owners.get(&top) != Some(&v) {
                self.bcast(&json!({"t": "owners", "top": top, "own": v}));
            }
            seen.insert(top, v);
        }
        let gone: Vec<PathBuf> = self.owners.keys().filter(|t| !seen.contains_key(*t)).cloned().collect();
        for top in gone {
            self.bcast(&json!({"t": "owners", "top": top, "own": Value::Null}));
        }
        self.owners = seen;
        if self.owners_due {
            self.refresh_owners(false);
        }
    }

    /// For a page that connects: every work tree's files and their cards.
    pub(super) fn owners_json(&self) -> Value {
        Value::Object(self.owners.iter().map(|(t, v)| (t.to_string_lossy().into_owned(), v.clone())).collect())
    }

    /// The uncommitted files of `top` that no card on the board changed: an agent looks at each,
    /// commits what is still valid and needed, and leaves the rest for the person (the git tree's
    /// "commit with an agent"). One that is already at it is shown instead.
    pub(super) fn cmd_strays(&mut self, top: String, x: Option<f64>, y: Option<f64>) -> Value {
        let top = PathBuf::from(top);
        let Some(own) = self.owners.get(&top).cloned() else {
            return json!({"ok": false, "error": "weft has not looked at that folder's files yet"});
        };
        let files: Vec<words::Stray> = own["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["by"].as_array().is_some_and(|b| b.is_empty()))
            .map(|f| words::Stray {
                path: f["p"].as_str().unwrap_or("").to_string(),
                st: f["st"].as_str().unwrap_or("").to_string(),
                at: f["m"].as_u64(),
            })
            .collect();
        if files.is_empty() {
            return json!({"ok": false, "error": "a card on this board changed every uncommitted file there"});
        }
        if let Some(id) = self.strays.get(&top).filter(|id| self.rt.get(*id).is_some_and(|r| !r.finished)) {
            return json!({"ok": true, "id": id, "again": true});
        }
        // (the person's choices may have changed on another board since this one read them)
        self.sync_user_config();
        let Some((agent, model, effort)) = self.fixer_agent(&[]) else {
            return json!({"ok": false, "error": "no agent is set up on this board"});
        };
        let repo = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let more = own["n"].as_u64().unwrap_or(0) as usize;
        let more = more.saturating_sub(own["files"].as_array().map_or(0, Vec::len));
        let mut node = json!({"agent": agent, "name": format!("commit what is left over in {repo}"),
                              "prompt": words::strays_request(&top.to_string_lossy(), &files, more, now_ms()),
                              "addenda": self.chosen_addenda()});
        self.tune_agent(&mut node, &agent, &model, &effort);
        if top != self.boot.dir {
            node["cwd"] = json!(top.to_string_lossy());
        }
        let (x, y) = self.free_spot(x.unwrap_or(0.0), y.unwrap_or(0.0));
        node["x"] = json!(x);
        node["y"] = json!(y);
        let r = self.cmd_node_add(node, None, true);
        if let Some(id) = r["id"].as_str() {
            self.strays.insert(top, id.to_string());
            let k = files.len();
            self.toast("info", &format!("{id} looks at the {k} uncommitted file{} no card claims", if k == 1 { "" } else { "s" }));
        }
        r
    }

    /// What is uncommitted in one of those files, as a diff (answered off the actor). Only a file
    /// the page was told about: the list is what may be read.
    pub(super) fn cmd_dirt_diff(&mut self, top: String, path: String, reply: Option<oneshot::Sender<Value>>) {
        let Some(reply) = reply else { return };
        let top = PathBuf::from(top);
        let known = self.owners.get(&top).and_then(|o| o["files"].as_array()).and_then(|fs| fs.iter().find(|f| f["p"].as_str() == Some(path.as_str())));
        let Some(f) = known else {
            let _ = reply.send(json!({"ok": false, "error": "that file is not among the uncommitted ones (any more)"}));
            return;
        };
        let new = f["st"].as_str() == Some("?");
        tokio::spawn(async move {
            let args: Vec<&str> = if new {
                vec!["diff", "--no-index", "--stat=120", "--patch", "--", "/dev/null", &path]
            } else {
                vec!["diff", "HEAD", "--stat=120", "--patch", "--find-renames", "--", &path]
            };
            let r = match git_text(&top, &args, true).await {
                Some(d) if !d.trim().is_empty() => json!({"ok": true, "diff": head_chars(&d, DIFF_MAX)}),
                Some(_) => json!({"ok": true, "diff": "", "note": "git shows no change in it (committed or put back meanwhile?)"}),
                None => json!({"ok": false, "error": "git could not show it"}),
            };
            let _ = reply.send(r);
        });
    }
}

/// What a card said in its reports, lowercased (the end of it, where the latest reports are).
fn said_by(rt: &Rt) -> String {
    let mut s = String::new();
    for st in &rt.steps {
        s.push_str(&st.text);
        s.push('\n');
    }
    for t in [rt.output.as_deref(), rt.issue.as_deref()].into_iter().flatten() {
        s.push_str(t);
        s.push('\n');
    }
    tail_chars(&s, SAID_MAX).to_lowercase()
}

/// Stands for everything that decides whose the files are, apart from the files themselves.
fn inputs_key(inputs: &[(PathBuf, Vec<Worker>)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (t, ws) in inputs {
        t.hash(&mut h);
        for w in ws {
            (&w.id, w.said_n).hash(&mut h);
            for s in &w.steps {
                (s.at, s.end, &s.files).hash(&mut h);
            }
        }
    }
    h.finish()
}

/// git's output when it succeeded (`any`: whatever it printed, like `diff --no-index`, which says
/// "there are differences" with its exit code).
async fn git_text(dir: &Path, args: &[&str], any: bool) -> Option<String> {
    let o = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    (o.status.success() || (any && o.status.code() == Some(1))).then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The uncommitted files of the work tree `top` and the cards each one is put to.
pub(super) async fn owners(top: &Path, ws: &[Worker]) -> Value {
    let Some(st) = git_text(top, &["status", "--porcelain=v1", "-z", "-uall", "--", ".", ":(exclude,glob)**/[.]weft/**"], false).await else {
        return Value::Null;
    };
    let files = parse_status(&st);
    // when each file was last committed: what a step did before that is in the history now
    let mut committed: HashMap<String, u64> = HashMap::new();
    if let Some(since) = ws.iter().flat_map(|w| w.steps.iter().map(|s| s.at)).min() {
        let since = format!("--since=@{}", (since / 1000).saturating_sub(2));
        if let Some(log) = git_text(top, &["log", "-n", "3000", "--no-renames", "--format=%x00%ct", "--name-only", &since, "HEAD", "--"], false).await {
            read_log(&log, &mut committed);
        }
    }
    let mut bases: HashMap<String, usize> = HashMap::new();
    for (_, p) in &files {
        *bases.entry(base_of(p)).or_default() += 1;
    }
    let now = now_ms();
    let list: Vec<Value> = files
        .iter()
        .take(FILES_MAX)
        .map(|(code, p)| {
            let m = mtime(&top.join(p));
            let after = committed.get(p.as_str()).copied().unwrap_or(0);
            let claims = claims_on(ws, p, m, after, now);
            let base_ok = bases.get(&base_of(p)).copied() == Some(1);
            let (by, unsure) = settle(&claims, m, |id| ws.iter().any(|w| w.id == id && names(&w.said, p, base_ok)));
            json!({"p": p, "st": code, "by": by, "unsure": unsure, "m": m})
        })
        .collect();
    json!({"n": files.len(), "files": list})
}

/// `git status --porcelain=v1 -z`: (what happened to it, the path) for each file; "?" = new to git.
fn parse_status(st: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut it = st.split('\0');
    while let Some(e) = it.next() {
        let (Some(xy), Some(p)) = (e.get(..2), e.get(3..)) else { continue };
        if p.is_empty() {
            continue;
        }
        // (a rename or copy in the index is followed by the name it had)
        if xy.contains(['R', 'C']) {
            it.next();
        }
        let code = if xy == "??" {
            "?".to_string()
        } else {
            xy.chars().find(|c| *c != ' ').map(String::from).unwrap_or_default()
        };
        out.push((code, p.to_string()));
    }
    out
}

/// `git log --format=%x00%ct --name-only`: the latest commit time (ms, rounded up) of each file.
fn read_log(log: &str, into: &mut HashMap<String, u64>) {
    for chunk in log.split('\0').skip(1) {
        let mut lines = chunk.lines();
        let Some(t) = lines.next().and_then(|t| t.trim().parse::<u64>().ok()) else { continue };
        for p in lines.filter(|l| !l.is_empty()) {
            // (newest first: the first time a file shows up is its last commit; git has whole seconds)
            into.entry(p.to_string()).or_insert(t * 1000 + 999);
        }
    }
}

fn mtime(p: &Path) -> Option<u64> {
    let t = std::fs::symlink_metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64)
}

fn base_of(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_lowercase()
}

/// The cards whose steps changed `p` after its last commit (`after`), each with the times they did
/// (a step still running counts when the file was written since it began).
fn claims_on<'a>(ws: &'a [Worker], p: &str, m: Option<u64>, after: u64, now: u64) -> Vec<(&'a str, Vec<(u64, u64)>)> {
    let mut out = Vec::new();
    for w in ws {
        let mut wins = Vec::new();
        for s in &w.steps {
            let to = s.end.unwrap_or(now);
            let hit = match &s.files {
                Some(fl) => fl.iter().any(|f| f == p),
                None => m.is_some_and(|m| m + SLACK_MS >= s.at && m <= to + SLACK_MS),
            };
            let from = s.at.max(after);
            if hit && to > from {
                wins.push((from, to));
            }
        }
        if !wins.is_empty() {
            out.push((w.id.as_str(), wins));
        }
    }
    out
}

/// Whose a file is, of the cards that claim it: cards that worked on it at times nobody else did
/// are all its owners; of cards at work at the same time, the ones that name it in a report or that
/// alone were at work when it was last written (`m`). When that tells no one apart, all of them,
/// unsure.
fn settle(claims: &[(&str, Vec<(u64, u64)>)], m: Option<u64>, named: impl Fn(&str) -> bool) -> (Vec<String>, bool) {
    if claims.len() < 2 {
        return (claims.iter().map(|c| c.0.to_string()).collect(), false);
    }
    let meets = |a: &(u64, u64), b: &(u64, u64)| a.0 < b.1 + SLACK_MS && b.0 < a.1 + SLACK_MS;
    // the cards at work at the same time as one another (directly or through a third)
    let k = claims.len();
    let mut group: Vec<usize> = (0..k).collect();
    fn root(g: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while g[r] != r {
            r = g[r];
        }
        g[i] = r;
        r
    }
    for i in 0..k {
        for j in i + 1..k {
            if claims[i].1.iter().any(|a| claims[j].1.iter().any(|b| meets(a, b))) {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[rj] = ri;
            }
        }
    }
    let at_work = |i: usize| m.is_some_and(|m| claims[i].1.iter().any(|w| m + SLACK_MS >= w.0 && m <= w.1 + SLACK_MS));
    let mut keep = vec![false; k];
    let mut unsure = false;
    for r in 0..k {
        let members: Vec<usize> = (0..k).filter(|&i| root(&mut group, i) == r).collect();
        if members.len() == 1 {
            keep[members[0]] = true;
            continue;
        }
        if members.is_empty() {
            continue;
        }
        let last: Vec<usize> = members.iter().copied().filter(|&i| at_work(i)).collect();
        let sure: Vec<usize> = members.iter().copied().filter(|&i| named(claims[i].0) || (last.len() == 1 && last[0] == i)).collect();
        if sure.is_empty() {
            unsure = true;
            members.iter().for_each(|&i| keep[i] = true);
        } else {
            sure.iter().for_each(|&i| keep[i] = true);
        }
    }
    (claims.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, c)| c.0.to_string()).collect(), unsure)
}

/// A report names the file: its path, or its name where no other uncommitted file has that name.
fn names(said: &str, p: &str, base_ok: bool) -> bool {
    let p = p.to_lowercase();
    if said.contains(&p) {
        return true;
    }
    let b = base_of(&p);
    if !base_ok || b.len() < 5 {
        return false;
    }
    let before = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.');
    let after = |c: char| c.is_alphanumeric() || c == '_';
    said.match_indices(b.as_str()).any(|(i, _)| {
        !said[..i].chars().next_back().is_some_and(before) && !said[i + b.len()..].chars().next().is_some_and(after)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a step as the tests write it: start, end, the files it changed
    type Step<'a> = (u64, Option<u64>, Option<&'a [&'a str]>);

    fn w(id: &str, steps: &[Step], said: &str) -> Worker {
        Worker {
            id: id.into(),
            steps: steps
                .iter()
                .map(|(at, end, f)| Stint { at: *at, end: *end, files: f.map(|f| f.iter().map(|s| s.to_string()).collect()) })
                .collect(),
            said: said.to_lowercase(),
            said_n: said.len(),
        }
    }

    fn whose(ws: &[Worker], p: &str, m: Option<u64>, after: u64) -> (Vec<String>, bool) {
        let claims = claims_on(ws, p, m, after, 1_000_000);
        settle(&claims, m, |id| ws.iter().any(|w| w.id == id && names(&w.said, p, true)))
    }

    #[test]
    fn a_file_goes_to_the_cards_that_changed_it_and_what_nobody_changed_is_left_over() {
        let ws = [w("n1", &[(10_000, Some(20_000), Some(&["a.rs", "b.rs"][..]))], ""), w("n2", &[(50_000, Some(60_000), Some(&["b.rs"][..]))], "")];
        assert_eq!(whose(&ws, "a.rs", Some(15_000), 0), (vec!["n1".to_string()], false));
        // one after the other: both did some of it
        assert_eq!(whose(&ws, "b.rs", Some(55_000), 0), (vec!["n1".to_string(), "n2".to_string()], false));
        assert_eq!(whose(&ws, "c.rs", Some(55_000), 0), (vec![], false));
        // n1's change of b.rs was committed since: only n2's is left
        assert_eq!(whose(&ws, "b.rs", Some(55_000), 30_000), (vec!["n2".to_string()], false));
    }

    #[test]
    fn of_cards_at_work_at_the_same_time_the_one_that_names_it_or_wrote_it_last_has_it() {
        // n1 works long; n2 works a short while in the middle: n1's snapshots see n2's file too
        let long = (10_000, Some(100_000), Some(&["x.rs", "web/js/88-tree.js"][..]));
        let short = (40_000, Some(50_000), Some(&["x.rs", "web/js/88-tree.js"][..]));
        let ws = [w("n1", &[long], "I changed src/x.rs only"), w("n2", &[short], "Changed: 88-tree.js")];
        assert_eq!(whose(&ws, "x.rs", Some(45_000), 0), (vec!["n1".to_string()], false), "a report names it (a path ending in it counts)");
        assert_eq!(whose(&ws, "web/js/88-tree.js", Some(45_000), 0), (vec!["n2".to_string()], false), "its name, unique among the files");
        // nobody names it; last written while only n1 was at work
        let ws = [w("n1", &[long], ""), w("n2", &[short], "")];
        assert_eq!(whose(&ws, "x.rs", Some(80_000), 0), (vec!["n1".to_string()], false));
        // last written while both were: either could have
        assert_eq!(whose(&ws, "x.rs", Some(45_000), 0), (vec!["n1".to_string(), "n2".to_string()], true));
    }

    #[test]
    fn a_running_step_has_the_files_written_since_it_began() {
        let ws = [w("n1", &[(10_000, None, None)], "")];
        assert_eq!(whose(&ws, "new.rs", Some(30_000), 0).0, vec!["n1".to_string()]);
        assert!(whose(&ws, "old.rs", Some(5_000), 0).0.is_empty(), "written before it began: not its");
        assert!(whose(&ws, "gone.rs", None, 0).0.is_empty(), "a deleted file has no time: it waits for the step's end");
    }

    #[test]
    fn status_and_log_are_read() {
        let st = " M src/a.rs\0?? new/b.rs\0R  c.rs\0old c.rs\0D  d.rs\0AM e.rs\0";
        let got = parse_status(st);
        let want: Vec<(String, String)> =
            [("M", "src/a.rs"), ("?", "new/b.rs"), ("R", "c.rs"), ("D", "d.rs"), ("A", "e.rs")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        assert_eq!(got, want);
        let mut m = HashMap::new();
        read_log("\x00200\n\nsrc/a.rs\nb.rs\n\x00100\n\nsrc/a.rs\nc.rs\n", &mut m);
        assert_eq!(m["src/a.rs"], 200_999);
        assert_eq!(m["c.rs"], 100_999);
        assert!(names("see `mod.rs:12`", "src/engine/mod.rs", true));
        assert!(!names("see src/engine/mod.rs", "src/model.rs", true));
        assert!(!names("the remod.rsx file", "a/mod.rs", true));
    }

    #[tokio::test]
    async fn a_work_tree_is_read_with_git() {
        let d = std::env::temp_dir().join(format!("weft-owners-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git").arg("-C").arg(&d).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap()
        };
        std::fs::write(d.join("src/a.rs"), "one\n").unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "start"]);
        std::fs::write(d.join("src/a.rs"), "two\n").unwrap();
        std::fs::create_dir_all(d.join("docs/x")).unwrap();
        std::fs::write(d.join("docs/x/notes.md"), "n\n").unwrap();
        std::fs::create_dir_all(d.join(".weft")).unwrap();
        std::fs::write(d.join(".weft/flow.json"), "{}").unwrap();
        let ws = [w("n1", &[(now_ms() - 60_000, Some(now_ms() + 60_000), Some(&["src/a.rs"][..]))], "")];
        let v = owners(&d, &ws).await;
        assert_eq!(v["n"], 2, "weft's own folder is no part of it: {v}");
        let f = v["files"].as_array().unwrap();
        let a = f.iter().find(|x| x["p"] == "src/a.rs").unwrap();
        assert_eq!((a["st"].as_str(), a["by"].clone()), (Some("M"), json!(["n1"])));
        let n = f.iter().find(|x| x["p"] == "docs/x/notes.md").expect("new files one by one, not their folder");
        assert_eq!((n["st"].as_str(), n["by"].clone()), (Some("?"), json!([])));
        let _ = std::fs::remove_dir_all(&d);
    }
}
