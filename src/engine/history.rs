//! The commits of a branch, for the git tree in the corner of the page. Read when the page asks,
//! nothing kept: git answers in milliseconds, and the page asks again only when a branch moved
//! (`refs` in what `project_info` reports).

use super::*;

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

/// How far `rev` is ahead of and behind its upstream, if it has one.
async fn ahead_behind(dir: &Path, rev: &str) -> Option<(String, u64, u64)> {
    let up = format!("{rev}@{{upstream}}");
    let name = git_run(dir, &["rev-parse", "--abbrev-ref", &up]).await.ok().filter(|(ok, _)| *ok)?.1.trim().to_string();
    let (ok, out) = git_run(dir, &["rev-list", "--left-right", "--count", &format!("{rev}...{up}")]).await.ok()?;
    if !ok {
        return None;
    }
    let mut n = out.split_whitespace().filter_map(|x| x.parse::<u64>().ok());
    Some((name, n.next()?, n.next()?))
}

/// The tree of `branch` in `dir`: "" is what is checked out (and follows it), "*" every branch.
/// A name must be one of the branches there are, so nothing typed reaches git as an option.
async fn log(dir: &Path, branch: &str, n: usize) -> Value {
    if !dir.is_dir() {
        return json!({"ok": true, "repo": false, "missing": true});
    }
    match git_run(dir, &["rev-parse", "--is-inside-work-tree"]).await {
        Ok((true, _)) => {}
        _ => return json!({"ok": true, "repo": false}),
    }
    let (_, cur) = git_run(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).await.unwrap_or_default();
    let (_, at) = git_run(dir, &["rev-parse", "-q", "--verify", "HEAD"]).await.unwrap_or_default();
    let all = branches(dir).await;
    let gone = !branch.is_empty() && branch != "*" && !all.iter().any(|(b, _)| b == branch);
    let shown = if gone { "" } else { branch };
    let n = n.clamp(1, LOG_MAX);
    let mut args =
        vec!["log".to_string(), "--topo-order".into(), format!("-n{}", n + 1), format!("--format=%H{SEP}%P{SEP}%an{SEP}%at{SEP}%D{SEP}%s")];
    match shown {
        "" => args.push("HEAD".into()),
        "*" => args.extend(["HEAD".into(), "--branches".into(), "--remotes".into()]),
        b => {
            args.push(if all.iter().any(|(x, remote)| x == b && !remote) { format!("refs/heads/{b}") } else { format!("refs/remotes/{b}") })
        }
    }
    args.push("--".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    // (a repository without commits yet has no HEAD to log: an empty tree, not an error)
    let (ok, out) = git_run(dir, &args).await.unwrap_or_default();
    let mut commits: Vec<Value> = if ok { out.lines().filter_map(commit_line).collect() } else { Vec::new() };
    let more = commits.len() > n;
    commits.truncate(n);
    let rev = match shown {
        "" | "*" => "HEAD".to_string(),
        b => b.to_string(),
    };
    let track = if all.iter().any(|(b, remote)| *b == rev && *remote) { None } else { ahead_behind(dir, &rev).await };
    let list: Vec<Value> = all.iter().map(|(b, remote)| json!({"name": b, "remote": remote})).collect();
    json!({
        "ok": true, "repo": true, "head": cur.trim(), "at": at.trim(), "branch": shown, "gone": gone,
        "branches": list, "commits": commits, "more": more,
        "up": track.as_ref().map(|t| t.0.clone()), "ahead": track.as_ref().map(|t| t.1), "behind": track.as_ref().map(|t| t.2),
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
    /// the engine does not wait for it.
    pub(super) fn cmd_log(&self, dir: String, branch: String, n: usize, show: String, reply: Option<oneshot::Sender<Value>>) {
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
                    log(&d, branch.trim(), n).await
                } else {
                    commit(&d, show.trim()).await
                }
            };
            let v = tokio::time::timeout(GIT_WAIT, job).await.unwrap_or_else(|_| json!({"ok": false, "error": "git took too long"}));
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
        let v = log(&dir, "", 50).await;
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

        let v = log(&dir, "", 50).await;
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
        let v = log(&dir, "", 2).await;
        assert_eq!(v["commits"].as_array().unwrap().len(), 2);
        assert_eq!(v["more"], true);
        // another branch; a name that is not a branch is not handed to git
        let v = log(&dir, "side", 50).await;
        assert_eq!(v["commits"][0]["s"], "on the side");
        let v = log(&dir, "--output=/tmp/x", 50).await;
        assert_eq!(v["gone"], true);
        assert_eq!(v["commits"][0]["s"], "merge side");

        // one commit: its message and files; the merge shows what it brought in
        let side = log(&dir, "side", 1).await["commits"][0]["h"].as_str().unwrap().to_string();
        let one = commit(&dir, &side).await;
        assert_eq!(one["body"], "on the side");
        assert_eq!(one["files"][0]["path"], "a.txt");
        assert_eq!(one["files"][0]["add"], 2);
        let merge = log(&dir, "", 1).await["commits"][0]["h"].as_str().unwrap().to_string();
        assert_eq!(commit(&dir, &merge).await["files"][0]["path"], "a.txt");
        assert_eq!(commit(&dir, "--help").await["ok"], false);

        let _ = std::fs::remove_dir_all(&dir);
        // not a repository; not there at all
        let plain = std::env::temp_dir().join(format!("weft-history-plain-{}", now_ms()));
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(log(&plain, "", 5).await["repo"], false);
        let _ = std::fs::remove_dir_all(&plain);
        assert_eq!(log(&plain, "", 5).await["missing"], true);
    }
}
