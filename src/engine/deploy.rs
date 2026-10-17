//! Deploy: the step after a push that puts what was pushed to use (a server, an install, a release).
//! It is a command card (`Node.deploy`; its command in `cmd`, done when it exits 0), so it has what a
//! command card has: a live terminal, its output as its result, "only if it fails" links, the fixer.
//! It runs on exactly what the git steps before it pushed: a checkout of that commit that weft keeps
//! for the repository (`DeployAt::Pushed`: builds there stay incremental, and another agent's
//! unfinished files never ship), or in the folder itself (`DeployAt::Folder`). It starts once the push
//! before it is in and it has something to run: a command, or an agent it was handed to.

use super::*;

/// What a deploy step runs on: the repository, and the revision there.
pub(super) struct DeployPlan {
    pub(super) repo: PathBuf,
    pub(super) rev: String,
    pub(super) at: DeployAt,
}

/// The command that puts a folder's work to use, guessed from what is in it. Empty: none found (the
/// person types it into the card).
pub(super) fn guess_deploy(dir: &Path) -> String {
    let has = |f: &str| dir.join(f).exists();
    for s in ["deploy.sh", "manual-deploy.sh", "scripts/deploy.sh", "bin/deploy"] {
        if has(s) {
            return format!("./{s}");
        }
    }
    if has("package.json") && super::info::package_scripts(dir).get("deploy").is_some() {
        return format!("{} run deploy", super::info::package_manager(dir));
    }
    if let Ok(m) = std::fs::read_to_string(dir.join("Makefile")) {
        for t in ["deploy", "install"] {
            if m.lines().any(|l| l.starts_with(&format!("{t}:"))) {
                return format!("make {t}");
            }
        }
    }
    if has("fly.toml") {
        return "fly deploy".into();
    }
    if has("wrangler.toml") || has("wrangler.jsonc") || has("wrangler.json") {
        return "npx wrangler deploy".into();
    }
    // a program of its own: installed where cargo installs
    let bin = std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|t| t.contains("[[bin]]")) || has("src/main.rs");
    if bin {
        return "cargo install --path .".into();
    }
    String::new()
}

impl Engine {
    /// Where a deploy step runs and on what: the branch the git step before it carried, as the remote
    /// has it when that step pushes (so a fix pushed since is what goes out), else as it is here.
    pub(super) fn deploy_plan(&self, node: &Node) -> Option<DeployPlan> {
        let at = node.deploy?;
        let repo = self.node_dir(node);
        let git = self.flow.edges.iter().filter(|e| e.to == node.id).find_map(|e| self.node(&e.from).filter(|n| n.kind() == Kind::Git));
        let rev = match git {
            Some(g) => {
                let facts = self.rt.get(&g.id).map(|r| r.git.facts.clone()).unwrap_or_default();
                let branch = if g.branch.trim().is_empty() { facts.branch.trim().to_string() } else { g.branch.trim().to_string() };
                let remote = if g.remote.trim().is_empty() { "origin" } else { g.remote.trim() };
                match (branch.as_str(), g.pushes()) {
                    ("" | "HEAD", _) => "HEAD".to_string(),
                    (b, true) => format!("refs/remotes/{remote}/{b}"),
                    (b, false) => format!("refs/heads/{b}"),
                }
            }
            None => "HEAD".to_string(),
        };
        Some(DeployPlan { repo, rev, at })
    }
}

/// The folder a deploy runs in, and the commit it is at. For `Pushed`, a checkout of `rev` that
/// weft keeps per repository under its state folder (`key` names it): made the first time, moved to the
/// commit and cleaned of what git tracks every time after (ignored build output stays).
pub(super) async fn deploy_folder(plan: &DeployPlan, key: &str) -> Result<(PathBuf, String), String> {
    let sha = match git_run(&plan.repo, &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", plan.rev)]).await? {
        (true, out) => out.trim().to_string(),
        _ => return Err(format!("{} is not in {}", plan.rev, plan.repo.display())),
    };
    let short: String = sha.chars().take(9).collect();
    if plan.at == DeployAt::Folder {
        return Ok((plan.repo.clone(), short));
    }
    let wt = state_dir().join("deploy").join(key);
    let ours = git_run(&wt, &["rev-parse", "--is-inside-work-tree"]).await.is_ok_and(|(ok, _)| ok) && wt.join(".git").is_file();
    if ours {
        let (ok, out) = git_run(&wt, &["checkout", "-q", "--detach", "-f", &sha]).await?;
        if !ok {
            return Err(format!("could not check out {short} for the deploy: {}", out.trim()));
        }
        let _ = git_run(&wt, &["clean", "-fdq"]).await;
    } else {
        let _ = std::fs::remove_dir_all(&wt);
        if let Some(p) = wt.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        let _ = git_run(&plan.repo, &["worktree", "prune"]).await;
        let (ok, out) = git_run(&plan.repo, &["worktree", "add", "-q", "--detach", &wt.to_string_lossy(), &sha]).await?;
        if !ok {
            return Err(format!("could not make a checkout for the deploy: {}", out.trim()));
        }
    }
    Ok((wt, short))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folders_deploy_command_is_guessed_from_what_is_in_it() {
        let dir = std::env::temp_dir().join(format!("weft-deploy-guess-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        assert_eq!(guess_deploy(&dir), "");
        std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        assert_eq!(guess_deploy(&dir), "cargo install --path .");
        std::fs::write(dir.join("Makefile"), "build:\n\tcc x.c\ninstall: build\n\tcp x /usr/local/bin\n").unwrap();
        assert_eq!(guess_deploy(&dir), "make install");
        std::fs::write(dir.join("package.json"), r#"{"scripts": {"deploy": "wrangler deploy"}}"#).unwrap();
        std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(guess_deploy(&dir), "pnpm run deploy");
        std::fs::write(dir.join("manual-deploy.sh"), "#!/bin/sh\n").unwrap();
        assert_eq!(guess_deploy(&dir), "./manual-deploy.sh");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn a_deploy_runs_on_the_pushed_commit_never_on_unfinished_work() {
        let root = std::env::temp_dir().join(format!("weft-deploy-{}-{}", std::process::id(), now_ms()));
        let dir = root.join("w");
        std::fs::create_dir_all(&dir).unwrap();
        sh(&root, &["init", "-q", "--bare", "r.git"]);
        sh(&dir, &["init", "-q", "-b", "main"]);
        sh(&dir, &["config", "user.name", "t"]);
        sh(&dir, &["config", "user.email", "t@t"]);
        sh(&dir, &["remote", "add", "origin", &root.join("r.git").to_string_lossy()]);
        std::fs::write(dir.join("app.txt"), "v1").unwrap();
        sh(&dir, &["add", "app.txt"]);
        sh(&dir, &["commit", "-q", "-m", "v1"]);
        sh(&dir, &["push", "-q", "-u", "origin", "main"]);
        // another agent is half-way through v2 here
        std::fs::write(dir.join("app.txt"), "v2, half done").unwrap();
        let key = format!("test-{}-{}", std::process::id(), now_ms());
        let plan = DeployPlan { repo: dir.clone(), rev: "refs/remotes/origin/main".into(), at: DeployAt::Pushed };
        let (wt, sha) = deploy_folder(&plan, &key).await.unwrap();
        assert_eq!(std::fs::read_to_string(wt.join("app.txt")).unwrap(), "v1");
        assert!(sh(&dir, &["rev-parse", "HEAD"]).starts_with(&sha));
        // a build output it left (ignored) stays; what git tracks follows the next push
        std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(dir.join("app.txt"), "v2").unwrap();
        sh(&dir, &["add", "app.txt", ".gitignore"]);
        sh(&dir, &["commit", "-q", "-m", "v2"]);
        sh(&dir, &["push", "-q"]);
        std::fs::write(wt.join("app.txt"), "changed by the last deploy").unwrap();
        std::fs::create_dir_all(wt.join("target")).unwrap();
        std::fs::write(wt.join("target/cache"), "x").unwrap();
        let (again, _) = deploy_folder(&plan, &key).await.unwrap();
        assert_eq!(again, wt);
        assert_eq!(std::fs::read_to_string(wt.join("app.txt")).unwrap(), "v2");
        assert!(wt.join("target/cache").exists(), "ignored build output stays");
        // in the folder itself: as it is
        let here = DeployPlan { repo: dir.clone(), rev: "HEAD".into(), at: DeployAt::Folder };
        assert_eq!(deploy_folder(&here, &key).await.unwrap().0, dir);
        let missing = DeployPlan { repo: dir.clone(), rev: "refs/remotes/origin/nope".into(), at: DeployAt::Pushed };
        assert!(deploy_folder(&missing, &key).await.is_err());
        let _ = std::fs::remove_dir_all(&wt);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod flow_tests {
    use super::super::tests::*;
    use super::*;

    #[test]
    fn a_deploy_after_a_push_collects_its_result_and_starts_with_it() {
        let mut e = engine();
        add_kind(&mut e, "p", "push", "git", "");
        {
            let n = e.flow.nodes.iter_mut().find(|n| n.id == "p").unwrap();
            n.git_op = GitOp::Push;
            let r = e.rt.get_mut("p").unwrap();
            r.finished = true;
            r.ok = Some(true);
            r.status = Status::Done;
            r.output = Some("Pushed `main` to `origin`.".into());
        }
        let r = e.cmd_node_add(
            json!({"agent": "sh", "name": "deploy", "cmd": "./deploy.sh", "finish": "exit", "deploy": "pushed", "draft": true}),
            Some("p".into()),
            false,
        );
        let d = r["id"].as_str().unwrap().to_string();
        assert!(e.flow.edges.iter().any(|x| x.from == "p" && x.to == d), "linked after the push: {r}");
        // nothing to arm: a deploy that has its command goes as soon as the push's result is in it
        assert!(!e.node(&d).unwrap().draft);
        assert!(!e.rt[&d].queue.is_empty() || e.rt[&d].status != Status::New, "it runs: {:?}", e.rt[&d].status);
        let plan = e.deploy_plan(e.node(&d).unwrap()).unwrap();
        assert_eq!(plan.at, DeployAt::Pushed);
        assert_eq!(plan.rev, "HEAD", "the push carried no branch here");
        // one with no command fails at once, saying so, instead of opening a bare shell
        let r = e.cmd_node_add(
            json!({"agent": "sh", "name": "deploy", "cmd": "", "finish": "exit", "deploy": "pushed", "draft": true}),
            Some("p".into()),
            false,
        );
        let d = r["id"].as_str().unwrap().to_string();
        e.cmd_arm(d.clone());
        assert!(e.rt[&d].finished && e.rt[&d].ok == Some(false), "{:?}", e.rt[&d].status);
        assert!(e.rt[&d].output.as_deref().is_some_and(|o| o.contains("no command")));
    }

    #[test]
    fn a_deploy_handed_to_an_agent_starts_when_the_push_is_in() {
        // (rubbermetrics n72: made with no command, then given to claude with a prompt; the push went
        // through and the deploy sat there as a draft)
        let mut e = engine();
        add_kind(&mut e, "p", "push", "git", "");
        e.flow.nodes.iter_mut().find(|n| n.id == "p").unwrap().git_op = GitOp::Push;
        let r = e.cmd_node_add(
            json!({"agent": "sh", "name": "deploy", "cmd": "", "finish": "exit", "deploy": "pushed", "draft": true}),
            Some("p".into()),
            false,
        );
        let d = r["id"].as_str().unwrap().to_string();
        let r = e.cmd_node_add(
            json!({"agent": "sh", "name": "deploy 2", "cmd": "", "finish": "exit", "deploy": "pushed", "draft": true}),
            Some("p".into()),
            false,
        );
        let bare = r["id"].as_str().unwrap().to_string();
        e.cmd_node_set(d.clone(), json!({"agent": "claude", "finish": "idle", "prompt": "run ./manual-deploy.sh and watch it"}));
        assert!(e.node(&d).unwrap().draft && !e.node(&d).unwrap().deploy_unset(), "it has something to run now");
        e.propagate("p", true, "Pushed `main` to `origin`.");
        assert!(!e.node(&d).unwrap().draft, "the push coming in started it");
        assert!(!e.rt[&d].queue.is_empty() || e.rt[&d].status != Status::New, "{:?}", e.rt[&d].status);
        assert!(!e.rt[&d].finished, "an agent deploy is not failed for having no command: {:?}", e.rt[&d].output);
        // a shell deploy with no command still waits for it, holding the push's result
        assert!(e.node(&bare).unwrap().draft && e.rt[&bare].queue.is_empty());
        assert_eq!(e.rt[&bare].inputs.len(), 1);
    }
}
