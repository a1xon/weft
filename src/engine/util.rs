//! Small pure helpers shared by the engine's modules.

use super::*;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub(super) fn fnv(s: &str) -> u64 {
    fnv1a(s.as_bytes())
}

pub(super) fn secs(s: f64) -> Duration {
    Duration::from_millis((s.max(0.0) * 1000.0) as u64)
}

/// Single-pass `{{key}}` expansion: substituted text is never re-scanned, so
/// agent output can't smuggle in template variables.
pub(super) fn expand(t: &str, f: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find("}}") {
            Some(j) => {
                match f(after[..j].trim()) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[i..i + 2 + j + 2]),
                }
                rest = &after[j + 2..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub(super) fn tail_bytes(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut i = s.len() - n;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    &s[i..]
}

pub(super) fn head_chars(s: &str, n: usize) -> String {
    let mut it = s.chars();
    let h: String = it.by_ref().take(n).collect();
    if it.next().is_some() {
        format!("{h}\u{2026}")
    } else {
        h
    }
}

/// A title for a shell command: its first words, exactly as typed.
pub(super) fn title_cmd(cmd: &str, fallback: &str) -> String {
    let t = cmd.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        fallback.to_string()
    } else {
        head_chars(&t, 30).trim_end_matches('\u{2026}').trim_end().to_string()
    }
}

/// A short human title from a prompt: first words, no dangling stop-words.
pub(super) fn title_from(text: &str, fallback: &str) -> String {
    const STOP: [&str; 14] = ["in", "of", "to", "for", "the", "a", "an", "and", "with", "on", "at", "by", "from", "so"];
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let first = first.trim_start_matches(|c: char| "#>*-$ ".contains(c));
    let mut words: Vec<&str> = first.split_whitespace().take(4).collect();
    // whole words only: drop dangling stop-words, then words until it is short enough
    loop {
        while words.len() > 1 && words.last().is_some_and(|w| STOP.contains(&w.to_lowercase().as_str())) {
            words.pop();
        }
        if words.len() > 1 && words.join(" ").chars().count() > 30 {
            words.pop();
        } else {
            break;
        }
    }
    let mut t = words.join(" ");
    t = t.trim_end_matches(|c: char| ".,:;!?".contains(c)).to_string();
    if t.chars().count() > 30 {
        t = head_chars(&t, 30).trim_end_matches('\u{2026}').trim_end().to_string();
    }
    let mut c = t.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => fallback.to_string(),
    }
}

/// What one agent's output may take up inside another agent's prompt.
pub(super) const PIPE_MAX: usize = 8_000;

/// Run a shell command and capture its output; a timeout counts as failure.
pub(super) async fn run_shell(dir: &Path, cmd: &str, env: &[(&str, &str)], timeout_s: u64) -> (bool, String) {
    let mut c = tokio::process::Command::new("sh");
    c.arg("-c").arg(cmd).current_dir(dir).stdin(std::process::Stdio::null()).kill_on_drop(true);
    for (k, v) in env {
        c.env(k, v);
    }
    match tokio::time::timeout(Duration::from_secs(timeout_s), c.output()).await {
        Ok(Ok(o)) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s)
        }
        Ok(Err(e)) => (false, format!("could not run it: {e}")),
        Err(_) => (false, format!("timed out after {timeout_s}s")),
    }
}

/// A test suite: it may take a while.
pub(super) async fn run_tests(dir: &Path, cmd: &str) -> (bool, String) {
    run_shell(dir, cmd, &[], 900).await
}

/// A screenshot command writes its image to `$WEFT_SHOT`: it worked only if the image is there.
pub(super) async fn run_capture(dir: &Path, cmd: &str, shot: &Path) -> (bool, String) {
    let (ok, out) = run_shell(dir, cmd, &[("WEFT_SHOT", shot.to_string_lossy().as_ref())], 120).await;
    (ok && shot.is_file(), out)
}
