//! The commits of a branch, for the git tree in the corner of the page. Read when the page asks,
//! nothing kept: git answers in milliseconds, and the page asks again only when a branch moved
//! (`refs` in what `project_info` reports). What happened to the branch lately is read from git's
//! own record of it (the reflog), so a commit or a push shows up whoever made it: an agent, a weft
//! step, the person in a terminal. The remote is asked now and then where its branches are
//! (`look_remote`); that is the one thing remembered, so many pages do not ask it many times.

use super::*;
use std::sync::{Mutex, OnceLock};

/// the most commits one request returns (the page asks for more as you scroll)
const LOG_MAX: usize = 600;
/// the most branches offered to pick from, the ones committed to last first
const BRANCHES_MAX: usize = 80;
/// the most files listed for one commit
const FILES_MAX: usize = 300;
/// git that takes longer than this (a huge repository, a network filesystem) gives up
const GIT_WAIT: Duration = Duration::from_secs(6);
/// fields of a line `git log` prints for the tree, apart
const SEP: char = '\u{1f}';
/// the most commits marked as not pushed yet (or not pulled yet) in one answer
const MARK_MAX: usize = 2000;
/// the latest things that happened to the branch the page is told about
const EVENTS_MAX: usize = 8;
/// asking the remote where its branches are may take this long, fetching what moved there this long
const LS_WAIT: Duration = Duration::from_secs(10);
const FETCH_WAIT: Duration = Duration::from_secs(25);
/// the remote is asked at most this often (seconds), however many pages and boards want to know
/// (a click on the remote in the corner asks again after this); after a look that failed (offline,
/// no access) not again before `RETRY_S`
const AGAIN_S: u64 = 8;
const RETRY_S: u64 = 300;

/// The branches there are, local ones first: `(name, remote)`. A remote's `HEAD` is not a branch.
async fn branches(dir: &Path) -> Vec<(String, bool)> {
    let fmt = "--format=%(refname)";
    let (ok, out) = git_run(dir, &["for-each-ref", "--sort=-committerdate", fmt, "refs/heads", "refs/remotes"]).await.unwrap_or_default();
    if !ok {
        return Vec::new();
    }
    let mut list: Vec<(String, bool)> = out
        .lines()
        .filter(|r| !r.ends_with("/HEAD"))
        .filter_map(|r| {
            r.strip_prefix("refs/heads/")
                .map(|n| (n.to_string(), false))
                .or_else(|| r.strip_prefix("refs/remotes/").map(|n| (n.to_string(), true)))
        })
        .collect();
    list.sort_by_key(|(_, remote)| *remote);
    list.truncate(BRANCHES_MAX);
    list
}

/// The remote branch `rev` follows, in full (`refs/remotes/origin/main`), if it follows one.
async fn upstream(dir: &Path, rev: &str) -> Option<String> {
    let (ok, out) = git_run(dir, &["rev-parse", "--symbolic-full-name", &format!("{rev}@{{upstream}}")]).await.ok()?;
    let r = out.trim();
    (ok && r.starts_with("refs/remotes/")).then(|| r.to_string())
}

/// How far `rev` is ahead of and behind `up`.
async fn ahead_behind(dir: &Path, rev: &str, up: &str) -> Option<(u64, u64)> {
    let (ok, out) = git_run(dir, &["rev-list", "--left-right", "--count", &format!("{rev}...{up}"), "--"]).await.ok()?;
    if !ok {
        return None;
    }
    let mut n = out.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
    Some((n.next()?, n.next()?))
}

/// The remote that `up` belongs to, else `origin`, else the only one there is. None: no remote.
async fn remote_of(dir: &Path, up: Option<&str>) -> Option<String> {
    let (_, out) = git_run(dir, &["remote"]).await.unwrap_or_default();
    let names: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty() && !l.contains(' ')).collect();
    if let Some(short) = up.and_then(|u| u.strip_prefix("refs/remotes/")) {
        // (a remote's name may itself have a slash in it: the longest that fits)
        if let Some(r) = names.iter().filter(|r| short.starts_with(&format!("{r}/"))).max_by_key(|r| r.len()) {
            return Some(r.to_string());
        }
    }
    names.iter().find(|r| **r == "origin").or(if names.len() == 1 { names.first() } else { None }).map(|r| r.to_string())
}

/// Hashes of the commits `args` lists (`rev-list`), at most `MARK_MAX`.
async fn hashes(dir: &Path, args: &[&str]) -> std::collections::HashSet<String> {
    let mut a = vec!["rev-list".to_string(), format!("-n{MARK_MAX}")];
    a.extend(args.iter().map(|s| s.to_string()));
    a.push("--".into());
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    match git_run(dir, &a).await {
        Ok((true, out)) => out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
        _ => Default::default(),
    }
}

/// The tree of `branch` in `dir`: "" is what is checked out (and follows it), "*" every branch.
/// A name must be one of the branches there are, so nothing typed reaches git as an option.
/// With `remote` the remote is asked first where its branches are (see `look_remote`).
async fn log(dir: &Path, branch: &str, n: usize, remote: bool) -> Value {
    if !dir.is_dir() {
        return json!({"ok": true, "repo": false, "missing": true});
    }
    match git_run(dir, &["rev-parse", "--is-inside-work-tree"]).await {
        Ok((true, _)) => {}
        _ => return json!({"ok": true, "repo": false}),
    }
    let (_, cur) = git_run(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).await.unwrap_or_default();
    let all = branches(dir).await;
    let gone = !branch.is_empty() && branch != "*" && !all.iter().any(|(b, _)| b == branch);
    let shown = if gone { "" } else { branch };
    let picked_remote = all.iter().any(|(b, remote)| b == shown && *remote);
    // the ref the tree is of, and the remote branch it follows
    let rev = match shown {
        "" | "*" => "HEAD".to_string(),
        b if picked_remote => format!("refs/remotes/{b}"),
        b => format!("refs/heads/{b}"),
    };
    // (`@{upstream}` wants a branch's short name)
    let up = match shown {
        _ if picked_remote => None,
        "" | "*" => upstream(dir, "HEAD").await,
        b => upstream(dir, b).await,
    };
    let rname = remote_of(dir, up.as_deref()).await;
    let look = match &rname {
        Some(r) => Some(look_remote(dir, r, remote).await),
        None => None,
    };
    // (after the look: a fetch may have moved the remote branch)
    let (_, at) = git_run(dir, &["rev-parse", "-q", "--verify", "HEAD"]).await.unwrap_or_default();
    let track = match &up {
        Some(u) => ahead_behind(dir, &rev, u).await,
        None => None,
    };
    let behind = track.map(|t| t.1).unwrap_or(0);
    let n = n.clamp(1, LOG_MAX);
    let mut args =
        vec!["log".to_string(), "--topo-order".into(), format!("-n{}", n + 1), format!("--format=%H{SEP}%P{SEP}%an{SEP}%at{SEP}%D{SEP}%s")];
    match shown {
        "*" => args.extend(["HEAD".into(), "--branches".into(), "--remotes".into()]),
        _ => args.push(rev.clone()),
    }
    // what the remote has that is not here yet stands beside it, in a lane of its own
    if let (Some(u), true, false) = (&up, behind > 0, shown == "*") {
        args.push(u.clone());
    }
    args.push("--".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    // (a repository without commits yet has no HEAD to log: an empty tree, not an error)
    let (ok, out) = git_run(dir, &args).await.unwrap_or_default();
    let mut commits: Vec<Value> = if ok { out.lines().filter_map(commit_line).collect() } else { Vec::new() };
    let more = commits.len() > n;
    commits.truncate(n);
    // not pushed yet: what the remote branch it follows lacks, else what no remote branch has (only
    // where there is a remote to push to); not here yet: what the remote branch has on top
    let mine: Vec<&str> = if shown == "*" { vec!["--branches", "HEAD"] } else { vec![rev.as_str()] };
    let out_of = match (&up, &rname) {
        (Some(u), _) if shown != "*" => hashes(dir, &[rev.as_str(), format!("^{u}").as_str()]).await,
        (_, Some(_)) => hashes(dir, &[mine, vec!["--not", "--remotes"]].concat()).await,
        _ => Default::default(),
    };
    let into = match &up {
        Some(u) if behind > 0 => hashes(dir, &[u.as_str(), format!("^{rev}").as_str()]).await,
        _ => Default::default(),
    };
    for c in commits.iter_mut() {
        let h = c["h"].as_str().unwrap_or("").to_string();
        if out_of.contains(&h) {
            c["o"] = json!(true);
        } else if into.contains(&h) {
            c["i"] = json!(true);
        }
    }
    let ev = match shown {
        "" | "*" => events(dir, Some("HEAD"), up.as_deref()).await,
        _ if picked_remote => events(dir, None, Some(&rev)).await,
        _ => events(dir, Some(&rev), up.as_deref()).await,
    };
    let list: Vec<Value> = all.iter().map(|(b, remote)| json!({"name": b, "remote": remote})).collect();
    let short = up.as_deref().map(|u| u.trim_start_matches("refs/remotes/").to_string());
    let remote = match (&rname, &look) {
        (Some(name), Some(l)) => json!({
            "name": name, "at": l.at, "ok": l.ok, "err": l.err, "fetched": l.fetched,
            // (the remote branch it follows was deleted there: a pull request merged, say)
            "gone": short.as_ref().is_some_and(|s| l.gone.contains(s)),
        }),
        _ => Value::Null,
    };
    json!({
        "ok": true, "repo": true, "head": cur.trim(), "at": at.trim(), "branch": shown, "gone": gone,
        "branches": list, "commits": commits, "more": more,
        "up": short, "ahead": track.map(|t| t.0), "behind": track.map(|t| t.1),
        "events": ev, "remote": remote,
    })
}

/// `hash SEP parents SEP author SEP time SEP refs SEP subject` as the page reads it.
fn commit_line(l: &str) -> Option<Value> {
    let mut f = l.splitn(6, SEP);
    let (h, p, a, t, r, s) = (f.next()?, f.next()?, f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
    if h.len() < 7 || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let refs: Vec<&str> = r.split(", ").map(str::trim).filter(|x| !x.is_empty()).collect();
    Some(json!({"h": h, "p": p.split_whitespace().collect::<Vec<_>>(), "a": a, "t": t.parse::<u64>().unwrap_or(0), "r": refs, "s": s}))
}

/// One line of a reflog: the commit the ref pointed at after it, when (unix seconds), what git
/// wrote about it ("commit: …", "update by push") and that commit's subject.
struct Entry {
    h: String,
    t: u64,
    why: String,
    s: String,
}

async fn reflog(dir: &Path, rref: &str) -> Vec<Entry> {
    let fmt = format!("--format=%H{SEP}%gd{SEP}%gs{SEP}%s");
    let (ok, out) = git_run(dir, &["reflog", "show", "--date=unix", &fmt, "-n24", rref, "--"]).await.unwrap_or_default();
    if !ok {
        return Vec::new();
    }
    out.lines()
        .filter_map(|l| {
            let mut f = l.splitn(4, SEP);
            let (h, gd, why, s) = (f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
            // ("HEAD@{1792068565}": the moment, with --date=unix)
            let t = gd.rsplit_once("@{")?.1.trim_end_matches('}').parse().ok()?;
            Some(Entry { h: h.to_string(), t, why: why.to_string(), s: s.to_string() })
        })
        .collect()
}

/// A step in the middle of a rebase or a pull with rebase (the commits picked one by one): only its
/// end says what happened.
fn mid_step(why: &str) -> bool {
    let verb = why.split(": ").next().unwrap_or("");
    (verb.starts_with("rebase") || verb.starts_with("pull")) && verb.contains('(') && !verb.contains("(finish)")
}

/// What a reflog line says happened, as the page names it, and a word about it (the branch merged
/// in, where a switch went). None: a step in between, or nothing the tree tells about.
fn what(why: &str, remote: bool) -> Option<(&'static str, String)> {
    let (verb, rest) = why.split_once(": ").unwrap_or((why, ""));
    if remote {
        return if why == "update by push" {
            Some(("push", String::new()))
        } else if verb.starts_with("fetch") || verb.starts_with("pull") {
            // (someone rewrote the remote branch: it did not move forward)
            Some((if rest.contains("forced") { "forced" } else { "fetch" }, String::new()))
        } else {
            None
        };
    }
    if mid_step(why) {
        return None;
    }
    let k = if verb.starts_with("commit (amend)") {
        "amend"
    } else if verb.starts_with("commit (merge)") {
        "merge"
    } else if verb.starts_with("commit") || verb.starts_with("cherry-pick") || verb.starts_with("revert") {
        "commit"
    } else if let Some(name) = verb.strip_prefix("merge ") {
        return Some(("merge", name.to_string()));
    } else if verb.starts_with("pull") {
        return Some(("pull", if rest.starts_with("Fast-forward") { "ff".to_string() } else { String::new() }));
    } else if verb.starts_with("rebase") {
        "rebase"
    } else if verb.starts_with("reset") {
        "reset"
    } else if verb.starts_with("checkout") {
        let (from, to) = rest.strip_prefix("moving from ")?.split_once(" to ")?;
        return (from != to).then(|| ("switch", format!("{from} → {to}")));
    } else {
        return None;
    };
    Some((k, String::new()))
}

/// What happened lately, newest first: commits, merges, pulls, rebases, resets and switches where
/// the tree is (`local`: HEAD or a branch), pushes and fetches on the remote branch it follows.
/// Each says the commit it left the branch at; a push, a fetch or a fast-forward also how many
/// commits it moved.
async fn events(dir: &Path, local: Option<&str>, remote: Option<&str>) -> Vec<Value> {
    // (moment, order of things in the same second, before, the event)
    let mut lists: [Vec<(u64, u8, Option<String>, Value)>; 2] = Default::default();
    for (rref, far) in [(local, false), (remote, true)] {
        let Some(rref) = rref else { continue };
        let ev = &mut lists[far as usize];
        let list = reflog(dir, rref).await;
        let name = rref.trim_start_matches("refs/remotes/").trim_start_matches("refs/heads/");
        for (i, e) in list.iter().enumerate() {
            let Some((k, x)) = what(&e.why, far) else { continue };
            // where the ref was before (the line under it that was not a step in between)
            let before = list[i + 1..].iter().find(|o| !mid_step(&o.why)).map(|o| o.h.clone());
            // (between the two records, in the same second: a push comes after the commit it
            // carries, a fetch before the pull that takes it in)
            let order = match k {
                "fetch" | "forced" => 0,
                "push" => 2,
                _ => 1,
            };
            let mut v = json!({"k": k, "t": e.t, "h": e.h, "s": e.s});
            if !x.is_empty() {
                v["x"] = json!(x);
            }
            if far {
                v["ref"] = json!(name);
            }
            ev.push((e.t, order, before, v));
        }
    }
    // newest first; each record keeps its own order (a reflog has whole seconds only)
    let [mut here, mut there] = lists.map(VecDeque::from);
    let mut ev = Vec::new();
    while ev.len() < EVENTS_MAX {
        let from_here = match (here.front(), there.front()) {
            (Some(a), Some(b)) => (a.0, a.1) >= (b.0, b.1),
            (a, _) => a.is_some(),
        };
        match if from_here { here.pop_front() } else { there.pop_front() } {
            Some(e) => ev.push(e),
            None => break,
        }
    }
    let mut out = Vec::with_capacity(ev.len());
    for (_, _, before, mut v) in ev {
        let counted = matches!(v["k"].as_str(), Some("push" | "fetch")) || v["x"] == "ff";
        if let (true, Some(b), Some(h)) = (counted, before, v["h"].as_str().map(str::to_string)) {
            if b != h {
                if let Ok((true, c)) = git_run(dir, &["rev-list", "--count", &format!("{b}..{h}"), "--"]).await {
                    v["n"] = json!(c.trim().parse::<u64>().unwrap_or(0));
                }
            }
        }
        out.push(v);
    }
    out
}

/// What is known about a remote: when it was last asked (unix seconds), whether that worked and
/// what went wrong, when what moved there was last fetched, the remote branches here whose
/// branch the remote no longer has.
#[derive(Clone, Default)]
struct Look {
    at: u64,
    ok: bool,
    err: String,
    fetched: u64,
    gone: Vec<String>,
    busy: bool,
}

fn looks() -> &'static Mutex<HashMap<String, Look>> {
    static LOOKS: OnceLock<Mutex<HashMap<String, Look>>> = OnceLock::new();
    LOOKS.get_or_init(Default::default)
}

fn now_s() -> u64 {
    now_ms() / 1000
}

/// The remote `name` of the repository `dir` is in: asked now (`ask`, unless it was asked moments
/// ago, or failed a few minutes ago), else what the last look found. One look per repository and
/// remote at a time, shared by its worktrees and by every page that wants to know.
async fn look_remote(dir: &Path, name: &str, ask: bool) -> Look {
    let (_, common) = git_run(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await.unwrap_or_default();
    let key = format!("{}\n{name}", common.trim());
    {
        let mut m = looks().lock().unwrap_or_else(|e| e.into_inner());
        let l = m.entry(key.clone()).or_default();
        let wait = if l.ok { AGAIN_S } else { RETRY_S };
        if !ask || l.busy || (l.at > 0 && now_s().saturating_sub(l.at) < wait) {
            return l.clone();
        }
        l.busy = true;
    }
    let res = ask_remote(dir, name).await;
    let mut m = looks().lock().unwrap_or_else(|e| e.into_inner());
    let l = m.entry(key).or_default();
    l.busy = false;
    l.at = now_s();
    match res {
        Ok((fetched, gone)) => {
            l.ok = true;
            l.err.clear();
            if fetched {
                l.fetched = l.at;
            }
            l.gone = gone;
        }
        Err(e) => {
            l.ok = false;
            l.err = e;
        }
    }
    l.clone()
}

/// Where the remote's branches are (one short request, nothing is written here) against the
/// remote branches here; a fetch only when one of them moved there. A fetch moves remote branches
/// and nothing else: no branch of this repository, no file. `(fetched, remote branches it no
/// longer has)`.
async fn ask_remote(dir: &Path, name: &str) -> Result<(bool, Vec<String>), String> {
    let out = git_net(dir, &["ls-remote", name, "refs/heads/*"], LS_WAIT).await?;
    let there: HashMap<&str, &str> =
        out.lines().filter_map(|l| l.split_once('\t')).filter_map(|(h, r)| Some((r.strip_prefix("refs/heads/")?, h))).collect();
    let pre = format!("refs/remotes/{name}/");
    let (_, here) = git_run(dir, &["for-each-ref", "--format=%(objectname) %(refname)", &pre]).await.unwrap_or_default();
    let (mut moved, mut gone, mut known) = (false, Vec::new(), 0);
    for (h, b) in here.lines().filter_map(|l| l.split_once(' ')).filter_map(|(h, r)| Some((h, r.strip_prefix(&pre)?))) {
        if b == "HEAD" {
            continue;
        }
        known += 1;
        match there.get(b) {
            Some(x) if *x == h => {}
            Some(_) => moved = true,
            None => gone.push(format!("{name}/{b}")),
        }
    }
    // (nothing fetched from it yet: what it has is fetched once)
    if known == 0 && !there.is_empty() {
        moved = true;
    }
    if moved {
        git_net(dir, &["fetch", "--quiet", "--no-write-fetch-head", name], FETCH_WAIT).await?;
    }
    Ok((moved, gone))
}

/// git that talks to a remote. It never waits for a password or a passphrase: a look that would
/// need one fails instead (the person's own pushes and fetches are not affected). Stopped after `wait`.
async fn git_net(dir: &Path, args: &[&str], wait: Duration) -> Result<String, String> {
    let mut c = tokio::process::Command::new("git");
    c.arg("-C")
        .arg(dir)
        .args(["-c", "core.askPass="])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    // ssh asks nothing either (on top of the person's own ssh command, if they set one)
    if std::env::var_os("GIT_SSH").is_none() {
        let ssh = match std::env::var("GIT_SSH_COMMAND") {
            Ok(s) if !s.trim().is_empty() => s,
            _ => match git_run(dir, &["config", "core.sshCommand"]).await {
                Ok((true, s)) if !s.trim().is_empty() => s.trim().to_string(),
                _ => "ssh".to_string(),
            },
        };
        c.env("GIT_SSH_COMMAND", format!("{ssh} -o BatchMode=yes"));
    }
    match tokio::time::timeout(wait, c.output()).await {
        Err(_) => Err(format!("no answer in {} s", wait.as_secs())),
        Ok(Err(e)) => Err(format!("git: {e}")),
        Ok(Ok(o)) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(Ok(o)) => {
            let err = String::from_utf8_lossy(&o.stderr);
            Err(head_chars(err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("failed"), 160))
        }
    }
}

/// One commit in full: who, when, the whole message and the files it changed (a merge: against
/// its first parent, what it brought in).
async fn commit(dir: &Path, hash: &str) -> Value {
    if hash.len() < 4 || hash.len() > 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return json!({"ok": false, "error": "not a commit"});
    }
    let (ok, head) =
        git_run(dir, &["show", "-s", &format!("--format=%H{SEP}%an{SEP}%ae{SEP}%at{SEP}%B"), hash, "--"]).await.unwrap_or_default();
    if !ok {
        return json!({"ok": false, "error": head.trim()});
    }
    let mut f = head.splitn(5, SEP);
    let (h, a, e, t, body) =
        (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
    let (_, stat) = git_run(dir, &["show", "--format=", "--numstat", "--diff-merges=first-parent", hash, "--"]).await.unwrap_or_default();
    let mut files = Vec::new();
    let mut count = 0;
    for l in stat.lines() {
        let mut c = l.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (c.next(), c.next(), c.next()) else { continue };
        count += 1;
        if files.len() < FILES_MAX {
            // (a binary file has "-" for both)
            files.push(json!({"path": path, "add": add.parse::<u64>().ok(), "del": del.parse::<u64>().ok()}));
        }
    }
    json!({"ok": true, "h": h, "a": a, "e": e, "t": t.parse::<u64>().unwrap_or(0), "body": body.trim_end(), "files": files, "count": count})
}

impl Engine {
    /// The tree of a folder's branch, or one commit of it (`show`). Answers by itself once git has:
    /// the engine does not wait for it. With `remote` the remote is asked first (a network round
    /// trip, a fetch when something moved there), so that answer takes longer.
    pub(super) fn cmd_log(&self, dir: String, branch: String, n: usize, show: String, remote: bool, reply: Option<oneshot::Sender<Value>>) {
        let Some(reply) = reply else { return };
        // (a path the page sends is absolute; anything else is taken from the project folder)
        let d = self.boot.dir.join(dir.trim());
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            let _ = reply.send(json!({"ok": false, "error": "not running"}));
            return;
        };
        handle.spawn(async move {
            let job = async {
                if show.trim().is_empty() {
                    log(&d, branch.trim(), n, remote).await
                } else {
                    commit(&d, show.trim()).await
                }
            };
            let wait = if remote { GIT_WAIT + LS_WAIT + FETCH_WAIT } else { GIT_WAIT };
            let v = tokio::time::timeout(wait, job).await.unwrap_or_else(|_| json!({"ok": false, "error": "git took too long"}));
            let _ = reply.send(v);
        });
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
    async fn a_branch_its_merges_and_one_commit() {
        let dir = std::env::temp_dir().join(format!("weft-history-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        sh(&dir, &["init", "-q", "-b", "main"]);
        // no commits yet: an empty tree
        let v = log(&dir, "", 50, false).await;
        assert_eq!(v["repo"], true);
        assert_eq!(v["head"], "main");
        assert_eq!(v["commits"].as_array().unwrap().len(), 0);
        sh(&dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
        sh(&dir, &["switch", "-q", "-c", "side"]);
        std::fs::write(dir.join("a.txt"), "a\nb\n").unwrap();
        sh(&dir, &["add", "a.txt"]);
        sh(&dir, &["commit", "-q", "-m", "on the side"]);
        sh(&dir, &["switch", "-q", "main"]);
        sh(&dir, &["commit", "-q", "--allow-empty", "-m", "on main"]);
        sh(&dir, &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);

        let v = log(&dir, "", 50, false).await;
        let c = v["commits"].as_array().unwrap();
        assert_eq!(c.len(), 4, "{v}");
        assert_eq!(c[0]["s"], "merge side");
        assert_eq!(c[0]["p"].as_array().unwrap().len(), 2);
        assert!(c[0]["r"].as_array().unwrap().iter().any(|r| r == "HEAD -> main"), "{v}");
        assert_eq!(v["branch"], "");
        assert_eq!(v["more"], false);
        let names: Vec<&str> = v["branches"].as_array().unwrap().iter().filter_map(|b| b["name"].as_str()).collect();
        assert!(names.contains(&"main") && names.contains(&"side"));
        // only so many, and it says there are more
        let v = log(&dir, "", 2, false).await;
        assert_eq!(v["commits"].as_array().unwrap().len(), 2);
        assert_eq!(v["more"], true);
        // another branch; a name that is not a branch is not handed to git
        let v = log(&dir, "side", 50, false).await;
        assert_eq!(v["commits"][0]["s"], "on the side");
        let v = log(&dir, "--output=/tmp/x", 50, false).await;
        assert_eq!(v["gone"], true);
        assert_eq!(v["commits"][0]["s"], "merge side");

        // one commit: its message and files; the merge shows what it brought in
        let side = log(&dir, "side", 1, false).await["commits"][0]["h"].as_str().unwrap().to_string();
        let one = commit(&dir, &side).await;
        assert_eq!(one["body"], "on the side");
        assert_eq!(one["files"][0]["path"], "a.txt");
        assert_eq!(one["files"][0]["add"], 2);
        let merge = log(&dir, "", 1, false).await["commits"][0]["h"].as_str().unwrap().to_string();
        assert_eq!(commit(&dir, &merge).await["files"][0]["path"], "a.txt");
        assert_eq!(commit(&dir, "--help").await["ok"], false);

        let _ = std::fs::remove_dir_all(&dir);
        // not a repository; not there at all
        let plain = std::env::temp_dir().join(format!("weft-history-plain-{}", now_ms()));
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(log(&plain, "", 5, false).await["repo"], false);
        let _ = std::fs::remove_dir_all(&plain);
        assert_eq!(log(&plain, "", 5, false).await["missing"], true);
    }

    #[tokio::test]
    async fn what_happened_and_what_the_remote_has() {
        let root = std::env::temp_dir().join(format!("weft-history-remote-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let (origin, a, b) = (root.join("origin.git"), root.join("a"), root.join("b"));
        let p = |d: &PathBuf| d.to_str().unwrap().to_string();
        sh(&root, &["init", "-q", "--bare", "-b", "main", &p(&origin)]);
        sh(&root, &["clone", "-q", &p(&origin), &p(&a)]);
        sh(&a, &["commit", "-q", "--allow-empty", "-m", "one"]);

        // a branch that was never pushed: all of it is not on the remote
        let v = log(&a, "", 50, false).await;
        assert_eq!(v["remote"]["name"], "origin", "{v}");
        assert_eq!(v["commits"][0]["o"], true);
        assert_eq!(v["events"][0]["k"], "commit");
        assert_eq!(v["events"][0]["s"], "one");
        sh(&a, &["push", "-q", "-u", "origin", "main"]);
        // (a reflog keeps whole seconds: two pushes in one second cannot be told apart from their commits)
        tokio::time::sleep(Duration::from_millis(1100)).await;
        sh(&a, &["commit", "-q", "--allow-empty", "-m", "two"]);
        let v = log(&a, "", 50, false).await;
        assert_eq!(v["up"], "origin/main");
        assert_eq!(v["ahead"], 1);
        assert_eq!(v["commits"][0]["o"], true, "two is not pushed");
        assert!(v["commits"][1].get("o").is_none(), "one is");

        // pushed: the push says how many commits it carried, after the commit it carried
        sh(&a, &["push", "-q"]);
        let v = log(&a, "", 50, false).await;
        let ev = v["events"].as_array().unwrap();
        assert_eq!((ev[0]["k"].as_str(), ev[0]["n"].as_u64(), ev[0]["ref"].as_str()), (Some("push"), Some(1), Some("origin/main")), "{v}");
        assert_eq!((ev[1]["k"].as_str(), ev[1]["s"].as_str()), (Some("commit"), Some("two")));
        assert!(v["commits"][0].get("o").is_none());
        assert_eq!(v["ahead"], 0);

        // someone else pushes: only asking the remote finds out; then it is fetched and shown as not here yet
        tokio::time::sleep(Duration::from_millis(1100)).await;
        sh(&root, &["clone", "-q", &p(&origin), &p(&b)]);
        sh(&b, &["commit", "-q", "--allow-empty", "-m", "from b"]);
        sh(&b, &["push", "-q"]);
        let v = log(&a, "", 50, false).await;
        assert_eq!((v["behind"].as_u64(), v["remote"]["at"].as_u64()), (Some(0), Some(0)), "not asked yet: {v}");
        let v = log(&a, "", 50, true).await;
        assert_eq!(v["remote"]["ok"], true, "{v}");
        assert!(v["remote"]["at"].as_u64().unwrap() > 0 && v["remote"]["fetched"].as_u64().unwrap() > 0);
        assert_eq!(v["behind"], 1);
        assert_eq!((v["commits"][0]["s"].as_str(), v["commits"][0]["i"].as_bool()), (Some("from b"), Some(true)), "{v}");
        assert!(v["commits"][1].get("i").is_none());
        assert_eq!((v["events"][0]["k"].as_str(), v["events"][0]["n"].as_u64()), (Some("fetch"), Some(1)));

        // pulled: a fast-forward of one commit
        sh(&a, &["pull", "-q", "--ff-only"]);
        let v = log(&a, "", 50, false).await;
        assert_eq!((v["events"][0]["k"].as_str(), v["events"][0]["n"].as_u64()), (Some("pull"), Some(1)), "{v}");
        assert_eq!(v["behind"], 0);

        // a switch; a branch deleted on the remote (a pull request merged there) is gone
        sh(&a, &["switch", "-q", "-c", "side"]);
        sh(&a, &["push", "-q", "-u", "origin", "side"]);
        sh(&b, &["push", "-q", "origin", "--delete", "side"]);
        looks().lock().unwrap().clear();
        let v = log(&a, "", 50, true).await;
        assert_eq!(v["remote"]["gone"], true, "{v}");
        let ev = v["events"].as_array().unwrap();
        assert!(ev.iter().any(|e| e["k"] == "switch" && e["x"] == "main → side"), "{v}");
        // (the remote was looked at moments ago: the next page does not ask it again)
        let at = v["remote"]["at"].as_u64().unwrap();
        sh(&b, &["commit", "-q", "--allow-empty", "-m", "b again"]);
        sh(&b, &["push", "-q"]);
        let v = log(&a, "main", 50, true).await;
        assert_eq!((v["remote"]["at"].as_u64(), v["behind"].as_u64()), (Some(at), Some(0)));

        // a remote that cannot be reached: the look says so and nothing waits for a password
        sh(&a, &["remote", "set-url", "origin", &p(&root.join("nowhere.git"))]);
        looks().lock().unwrap().clear();
        let v = log(&a, "", 50, true).await;
        assert_eq!(v["remote"]["ok"], false, "{v}");
        assert!(!v["remote"]["err"].as_str().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reflog_lines_say_what_happened() {
        assert_eq!(what("commit (initial): one", false), Some(("commit", String::new())));
        assert_eq!(what("commit (amend): two", false), Some(("amend", String::new())));
        assert_eq!(what("merge side: Merge made by the 'ort' strategy.", false), Some(("merge", "side".into())));
        assert_eq!(what("pull -q --rebase: Fast-forward", false), Some(("pull", "ff".into())));
        assert_eq!(what("pull -q --rebase (start): checkout 7a2e", false), None);
        assert_eq!(what("pull --rebase (finish): returning to refs/heads/main", false), Some(("pull", String::new())));
        assert_eq!(what("rebase (pick): x", false), None);
        assert_eq!(what("rebase (finish): returning to refs/heads/main", false), Some(("rebase", String::new())));
        assert_eq!(what("reset: moving to HEAD~1", false), Some(("reset", String::new())));
        assert_eq!(what("checkout: moving from main to main", false), None);
        assert_eq!(what("Branch: renamed refs/heads/main to refs/heads/master", false), None);
        assert_eq!(what("update by push", true), Some(("push", String::new())));
        assert_eq!(what("fetch -q: fast-forward", true), Some(("fetch", String::new())));
        assert_eq!(what("pull -q --rebase: forced-update", true), Some(("forced", String::new())));
        assert_eq!(what("clone: from /x", true), None);
    }
}
