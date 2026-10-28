//! Git plumbing: small async calls that say what git said. Nothing in here knows about nodes,
//! queues or the UI, so each function can be read (and tested by hand) on its own.

use super::*;

/// `git -C dir <args>`: whether it succeeded, and everything it printed (stdout, then stderr).
/// A call given up on (a probe's timeout) ends its git too.
pub(super) async fn git_run(dir: &Path, args: &[&str]) -> Result<(bool, String), String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("git: {e}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), s))
}

/// What git printed when it succeeded, trimmed.
pub(super) async fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    match git_run(dir, args).await {
        Ok((true, out)) => Some(out.trim().to_string()),
        _ => None,
    }
}

/// Switch to `branch`, creating it from where HEAD is when it does not exist yet.
async fn switch_branch(dir: &Path, branch: &str) -> Result<(), String> {
    let (exists, _) = git_run(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).await?;
    let args: Vec<&str> = if exists { vec!["checkout", "-q", branch] } else { vec!["checkout", "-q", "-b", branch] };
    let (ok, out) = git_run(dir, &args).await?;
    if ok {
        Ok(())
    } else {
        Err(format!("could not switch to {branch}: {}", out.trim()))
    }
}

/// `git add -A`, minus weft's own folder and anything that looks like a credential:
/// weft commits and pushes on its own, so it must never sweep up secrets by accident.
/// weft's folder is written `[.]weft/**`, not `.weft`: when `.gitignore` lists `.weft/`, git (2.53)
/// takes an exclude that names it literally for an ignored path the add was asked for, and exits 1.
fn add_args() -> Vec<&'static str> {
    vec![
        "add",
        "-A",
        "--",
        ".",
        ":(exclude,glob)[.]weft/**",
        ":(exclude,glob)**/.env",
        ":(exclude,glob)**/.env.*",
        ":(exclude,glob)**/*.pem",
        ":(exclude,glob)**/*.key",
        ":(exclude,glob)**/id_rsa*",
        ":(exclude,glob)**/id_ed25519*",
        ":(exclude,glob)**/*.p12",
        ":(exclude,glob)**/.npmrc",
        ":(exclude,glob)**/.netrc",
    ]
}

/// Stage what changed and commit it. `place` ("" or " in the worktree") says where, for the error.
/// Returns whether there was anything to commit.
async fn stage_and_commit(dir: &Path, place: &str, subject: &str, body: &str) -> Result<bool, String> {
    let (ok, out) = git_run(dir, &add_args()).await?;
    if !ok {
        return Err(format!("git add failed{place}: {}", out.trim()));
    }
    let (clean, _) = git_run(dir, &["diff", "--cached", "--quiet"]).await?;
    if clean {
        return Ok(false);
    }
    let mut args: Vec<&str> = vec!["commit", "-q", "-m", subject];
    if !body.is_empty() {
        args.extend(["-m", body]);
    }
    let (ok, out) = git_run(dir, &args).await?;
    if ok {
        Ok(true)
    } else {
        Err(format!("git commit failed{place}: {}", out.trim()))
    }
}

/// How a merge of an agent's branch ended.
enum Merged {
    /// nothing new came in
    Nothing,
    /// a merge commit, abbreviated
    At(String),
    /// it conflicts (the merge was aborted): git's first line about it
    Conflict(String),
}

async fn merge_no_ff(dir: &Path, branch: &str, subject: &str) -> Result<Merged, String> {
    let before = git_out(dir, &["rev-parse", "HEAD"]).await.unwrap_or_default();
    let (ok, out) = git_run(dir, &["merge", "--no-ff", "-q", "-m", subject, branch]).await?;
    if !ok {
        let _ = git_run(dir, &["merge", "--abort"]).await;
        return Ok(Merged::Conflict(out.trim().lines().next().unwrap_or("").to_string()));
    }
    if git_out(dir, &["rev-parse", "HEAD"]).await.unwrap_or_default() == before {
        return Ok(Merged::Nothing);
    }
    Ok(Merged::At(git_out(dir, &["rev-parse", "--short", "HEAD"]).await.unwrap_or_default()))
}

/// The name of the branch checked out in `dir`.
pub(super) async fn branch_of(dir: &Path) -> String {
    git_out(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await.unwrap_or_default()
}

/// Commit whatever changed in the working tree (except weft's own folder). An agent that worked in
/// its own worktree has its work committed there and merged here with a merge commit.
pub(super) async fn commit_work(
    dir: &Path,
    branch: &str,
    switch_first: bool,
    subject: &str,
    body: &str,
    worktree: Option<&Path>,
) -> Result<String, String> {
    if switch_first && !branch.is_empty() {
        switch_branch(dir, branch).await?;
    }
    if let Some(wt) = worktree {
        stage_and_commit(wt, " in the worktree", subject, body).await?;
        let b = branch_of(wt).await;
        return match merge_no_ff(dir, &b, subject).await? {
            Merged::Conflict(line) => Err(format!("merging {b} conflicts: {line}")),
            Merged::Nothing => Ok(format!("nothing new to merge from {b}")),
            Merged::At(h) => Ok(format!("{h} {subject} (merged {b})")),
        };
    }
    if !stage_and_commit(dir, "", subject, body).await? {
        return Ok(format!("nothing new to commit ({})", head_chars(subject, 48)));
    }
    let h = git_out(dir, &["rev-parse", "--short", "HEAD"]).await.unwrap_or_default();
    Ok(format!("{h} {subject}"))
}

/// An agent committed on its own: report what git itself shows, and merge its worktree branch
/// if it has one. A conflict comes back as `CONFLICT:<branch>: <line>` so the agent can be asked.
pub(super) async fn after_reply(
    dir: &Path,
    branch: &str,
    worktree: Option<&Path>,
    name: &str,
    note: &str,
    subject: &str,
) -> Result<String, String> {
    let at = worktree.unwrap_or(dir);
    let (_, head) = git_run(at, &["log", "-1", "--format=%h %s"]).await?;
    let (_, st) = git_run(at, &["status", "--porcelain", "--", ".", ":(exclude).weft"]).await?;
    let dirty = st.lines().filter(|l| !l.trim().is_empty()).count();
    let tail = if dirty > 0 { format!("  [{dirty} file(s) still uncommitted]") } else { String::new() };
    let Some(wt) = worktree else {
        let said = if note.is_empty() { String::new() } else { format!(": {note}") };
        return Ok(format!("{}  ({name}{said}){tail}", head.trim()));
    };
    if !branch.is_empty() {
        switch_branch(dir, branch).await?;
    }
    let b = branch_of(wt).await;
    match merge_no_ff(dir, &b, subject).await? {
        Merged::Conflict(line) => Err(format!("CONFLICT:{b}: {line}")),
        Merged::Nothing => Ok(format!("nothing new to merge from {b} ({name}){tail}")),
        Merged::At(h) => Ok(format!("{h} {subject} (merged {b}){tail}")),
    }
}

/// Push the branch. Besides the log line it returns what the repository looks like now, with
/// exactly the commits this push added to the remote (or the commits of the round when there is
/// no remote branch yet to compare with). A push the remote refused because it has commits this
/// branch lacks is reported as `REJECTED:...` so the caller can have the branch brought up to date.
pub(super) async fn push(dir: &Path, remote: &str, branch: &str, hashes: &[String]) -> (Result<String, String>, Option<Facts>) {
    let remote = if remote.is_empty() { "origin" } else { remote };
    match git_run(dir, &["remote", "get-url", remote]).await {
        Err(e) => return (Err(e), None),
        Ok((false, _)) => {
            let f = facts(dir, hashes, None).await;
            return (Ok(format!("no remote \"{remote}\": the commits stay local")), Some(f));
        }
        Ok(_) => {}
    }
    let branch = if branch.is_empty() {
        match branch_of(dir).await.as_str() {
            "" | "HEAD" => return (Err("not on a branch: set one on the node".into()), None),
            b => b.to_string(),
        }
    } else {
        branch.to_string()
    };
    // what the remote has now, to tell what this push adds
    let before =
        git_out(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{remote}/{branch}")]).await.filter(|o| !o.is_empty());
    match git_run(dir, &["push", "-u", remote, &branch]).await {
        Err(e) => (Err(e), None),
        Ok((true, _)) => (Ok(format!("pushed {branch} to {remote}")), Some(facts(dir, hashes, before.as_deref()).await)),
        Ok((false, out)) => {
            let rejected = out.contains("[rejected]") || out.contains("non-fast-forward") || out.contains("fetch first");
            let tag = if rejected { "REJECTED:" } else { "" };
            (Err(format!("{tag}push failed: {}", why_not_pushed(&out))), None)
        }
    }
}

/// The line of a failed push that says why: the refused branch's (" ! [remote rejected] main -> main
/// (incorrect old value provided)"), else git's last error. The last line alone is "error: failed to
/// push some refs to ..." (all n219 said) or a hint.
fn why_not_pushed(out: &str) -> String {
    let lines: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let line = match lines.iter().find_map(|l| l.strip_prefix("! ")) {
        Some(refused) => refused,
        None => lines.iter().rev().find(|l| l.starts_with("fatal:") || l.starts_with("error:")).or(lines.last()).copied().unwrap_or(""),
    };
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a push of `branch` still has to send, read from the refs only (writes nothing, asks nobody):
/// each of `hashes` on a branch of the remote already, or how many are not. With no hashes (the
/// commits before it had nothing new) the branch is compared with the remote's branch of that name.
pub(super) async fn left_to_push(dir: &Path, remote: &str, branch: &str, hashes: &[String]) -> Result<Left, String> {
    let remote = if remote.is_empty() { "origin" } else { remote };
    if !git_run(dir, &["remote", "get-url", remote]).await?.0 {
        return Err(format!("no remote \"{remote}\""));
    }
    let branch = if branch.is_empty() { branch_of(dir).await } else { branch.to_string() };
    if hashes.is_empty() {
        let range = format!("refs/remotes/{remote}/{branch}..refs/heads/{branch}");
        let Some(n) = git_out(dir, &["rev-list", "--count", &range]).await else {
            return Err(format!("{branch} is not on {remote}"));
        };
        return Ok(if n == "0" { Left::Pushed(format!("{remote}/{branch}")) } else { Left::Push(n.parse().unwrap_or(1)) });
    }
    let (mut missing, mut on) = (0, String::new());
    let theirs = format!("refs/remotes/{remote}/");
    for h in hashes {
        let refs = git_out(dir, &["for-each-ref", "--contains", h, "--format=%(refname:short)", &theirs]).await.unwrap_or_default();
        // (origin/HEAD only points at another of them)
        let mut has = refs.lines().map(str::trim).filter(|r| r.contains('/') && !r.ends_with("/HEAD"));
        let mine = format!("{remote}/{branch}");
        match has.clone().find(|r| *r == mine).or_else(|| has.next()) {
            Some(r) if on.is_empty() || r == mine => on = r.to_string(),
            Some(_) => {}
            None => missing += 1,
        }
    }
    Ok(if missing == 0 { Left::Pushed(on) } else { Left::Push(missing) })
}

/// The repository as a step after this one will want to know it: branch, HEAD, the commits and the
/// files they touched. With `since` it is everything from there to HEAD, else the given commits.
pub(super) async fn facts(dir: &Path, hashes: &[String], since: Option<&str>) -> Facts {
    let lines = |s: String| -> Vec<String> { s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect() };
    let mut f = Facts {
        branch: branch_of(dir).await,
        head: git_out(dir, &["log", "-1", "--format=%h %s"]).await.unwrap_or_default(),
        ..Facts::default()
    };
    let mut files: Vec<String> = Vec::new();
    match since.map(str::trim).filter(|s| !s.is_empty()) {
        Some(old) => {
            let range = format!("{old}..HEAD");
            f.commits = lines(git_out(dir, &["log", "-n", "30", "--format=%h %s", &range]).await.unwrap_or_default());
            files = lines(git_out(dir, &["diff", "--name-only", &range]).await.unwrap_or_default());
        }
        None => {
            for h in hashes {
                for c in lines(git_out(dir, &["log", "-1", "--format=%h %s", h]).await.unwrap_or_default()) {
                    if !f.commits.contains(&c) {
                        f.commits.push(c);
                    }
                }
                // a merge shows what came in over its first parent
                for x in lines(git_out(dir, &["show", "--name-only", "--format=", "-m", "--first-parent", h]).await.unwrap_or_default()) {
                    if !files.contains(&x) {
                        files.push(x);
                    }
                }
            }
        }
    }
    files.retain(|x| !x.starts_with(".weft/"));
    files.sort();
    files.dedup();
    f.files = files;
    f
}

/// The repository after a commit job that logged `res`.
pub(super) async fn facts_of(dir: &Path, res: &Result<String, String>) -> Option<Facts> {
    let h = lead_hash(res.as_ref().ok()?)?;
    Some(facts(dir, &[h], None).await)
}

/// The abbreviated commit a log line starts with, if it starts with one.
pub(super) fn lead_hash(line: &str) -> Option<String> {
    let t = line.split_whitespace().next()?;
    (t.len() >= 7 && t.len() <= 40 && t.chars().all(|c| c.is_ascii_hexdigit())).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }

    #[tokio::test]
    async fn weft_commits_the_work_but_not_its_folder_or_a_secret_ignored_or_not() {
        let dir = std::env::temp_dir().join(format!("weft-repo-{}", now_ms()));
        std::fs::create_dir_all(dir.join(".weft/archive")).unwrap();
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["config", "user.name", "t"]);
        sh(&dir, &["config", "user.email", "t@t"]);
        let committed = |dir: PathBuf| async move {
            let (_, files) = git_run(&dir, &["show", "--name-only", "--format=", "HEAD"]).await.unwrap();
            files.lines().map(str::to_string).collect::<Vec<_>>()
        };

        // .weft not ignored
        std::fs::write(dir.join("a.rs"), "a").unwrap();
        std::fs::write(dir.join(".weft/flow.json"), "{}").unwrap();
        std::fs::write(dir.join(".env"), "TOKEN=x").unwrap();
        let said = commit_work(&dir, "", false, "first", "", None).await.unwrap();
        assert!(said.ends_with(" first"), "{said}");
        assert_eq!(committed(dir.clone()).await, ["a.rs"]);

        // .gitignore lists .weft/ (as this repository's does): git add must not fail on it
        std::fs::write(dir.join(".gitignore"), ".weft/\n").unwrap();
        std::fs::write(dir.join(".weft/archive/1.json.gz"), "x").unwrap();
        std::fs::write(dir.join("b.rs"), "b").unwrap();
        let said = commit_work(&dir, "", false, "second", "", None).await.unwrap();
        assert!(said.ends_with(" second"), "{said}");
        assert_eq!(committed(dir.clone()).await, [".gitignore", "b.rs"]);
        let (_, left) = git_run(&dir, &["status", "--porcelain", "--untracked-files=all"]).await.unwrap();
        assert_eq!(left.trim(), "?? .env");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_push_says_why_not_only_that_it_failed() {
        let raced = "To https://github.com/a/b.git\n ! [remote rejected] master -> master (incorrect old value provided)\n\
                     error: failed to push some refs to 'https://github.com/a/b.git'\n";
        assert_eq!(why_not_pushed(raced), "[remote rejected] master -> master (incorrect old value provided)");
        let behind = "To /r.git\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to '/r.git'\n\
                      hint: Updates were rejected because the remote contains work that you do not\nhint: have locally.\n";
        assert_eq!(why_not_pushed(behind), "[rejected] main -> main (fetch first)");
        let auth = "remote: Invalid username or password.\nfatal: Authentication failed for 'https://github.com/a/b.git/'\n";
        assert_eq!(why_not_pushed(auth), "fatal: Authentication failed for 'https://github.com/a/b.git/'");
        assert_eq!(why_not_pushed("something else\n\n"), "something else");
    }
}
