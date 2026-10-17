//! tmux as the only runtime: one control-mode client for events and fast
//! keystrokes, plus one-off `tmux` invocations for anything that needs a reply.

use crate::engine::Ev;
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

#[derive(Debug)]
pub enum TEv {
    Output { pane: String, data: Vec<u8> },
    Dead { pane: String, status: i32 },
    WinClosed { win: String },
    Layout { win: String, cols: u16, rows: u16 },
    Gone,
}

#[derive(Clone)]
pub struct Tmux {
    pub sock: String,
    pub session: String,
    ctl: mpsc::UnboundedSender<Vec<u8>>,
}

/// Run a one-off tmux command on our private socket. Returns (success, stdout).
pub async fn run(sock: &str, args: &[String]) -> (bool, String) {
    let out = Command::new("tmux")
        .arg("-L")
        .arg(sock)
        .arg("-f")
        .arg("/dev/null")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    match out {
        Ok(o) => (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned()),
        Err(_) => (false, String::new()),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Make sure the session (and a placeholder window that keeps it alive) exists
/// and carries our server options. Idempotent.
pub async fn ensure_session(sock: &str, session: &str, cols: u16, rows: u16) -> Result<(), String> {
    let (ok, _) = run(sock, &s(&["has-session", "-t", session])).await;
    if !ok {
        let (ok, _) = run(
            sock,
            &[
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                session.into(),
                "-x".into(),
                cols.to_string(),
                "-y".into(),
                rows.to_string(),
                "-n".into(),
                "weft".into(),
                "sh".into(),
                "-c".into(),
                "while :; do sleep 86400; done".into(),
            ],
        )
        .await;
        if !ok {
            return Err("could not start tmux (is it installed?)".into());
        }
    }
    for opt in [["remain-on-exit", "on"], ["allow-rename", "off"], ["automatic-rename", "off"], ["history-limit", "10000"], ["mouse", "on"]]
    {
        run(sock, &s(&["set-option", "-g", opt[0], opt[1]])).await;
    }
    Ok(())
}

/// Connect the control client. Events flow into the engine channel.
pub async fn connect(sock: &str, session: &str, cols: u16, rows: u16, ev: mpsc::UnboundedSender<Ev>) -> Result<Tmux, String> {
    ensure_session(sock, session, cols, rows).await?;
    let mut child = Command::new("tmux")
        .args(["-L", sock, "-f", "/dev/null", "-C", "attach", "-t", session])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("tmux: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (ctl_tx, mut ctl_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    tokio::spawn(async move {
        while let Some(chunk) = ctl_rx.recv().await {
            if stdin.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let mut rd = BufReader::new(stdout);
        let mut line = Vec::with_capacity(4096);
        loop {
            line.clear();
            match rd.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    while matches!(line.last(), Some(b'\n' | b'\r')) {
                        line.pop();
                    }
                    if let Some(e) = parse_line(&line) {
                        if ev.send(Ev::Tmux(e)).is_err() {
                            return;
                        }
                    }
                }
            }
        }
        let _ = child.wait().await;
        let _ = ev.send(Ev::Tmux(TEv::Gone));
    });

    let t = Tmux { sock: sock.to_string(), session: session.to_string(), ctl: ctl_tx };
    t.ctl(format!("refresh-client -C {cols}x{rows}"));
    t.ctl("refresh-client -B 'dead:%*:#{pane_id}_#{pane_dead}_#{pane_dead_status}'");
    Ok(t)
}

impl Tmux {
    /// Fire-and-forget command on the control stream. Callers must only pass
    /// validated tokens (pane ids, key names, hex) - never raw user text.
    pub fn ctl(&self, line: impl AsRef<str>) {
        let mut b = line.as_ref().as_bytes().to_vec();
        b.push(b'\n');
        let _ = self.ctl.send(b);
    }

    /// Send literal bytes to a pane (byte-exact, UTF-8 safe).
    pub fn send_bytes(&self, pane: &str, bytes: &[u8]) {
        if !valid_pane(pane) {
            return;
        }
        use std::fmt::Write;
        for chunk in bytes.chunks(240) {
            let mut l = format!("send-keys -t {pane} -H");
            for b in chunk {
                let _ = write!(l, " {b:02x}");
            }
            self.ctl(l);
        }
    }

    /// Send a named key (tmux key syntax: Enter, Up, C-c, M-x, F5 ...).
    pub fn send_key(&self, pane: &str, key: &str) {
        if valid_pane(pane) && valid_key(key) {
            self.ctl(format!("send-keys -t {pane} {key}"));
        }
    }

    pub fn kill_window(&self, win: &str) {
        if valid_id(win, '@') {
            self.ctl(format!("kill-window -t {win}"));
        }
    }
}

pub fn valid_id(s: &str, sigil: char) -> bool {
    let mut c = s.chars();
    c.next() == Some(sigil) && s.len() > 1 && s.len() < 12 && c.all(|d| d.is_ascii_digit())
}

pub fn valid_pane(s: &str) -> bool {
    valid_id(s, '%')
}

/// tmux key names we accept from the browser.
pub fn valid_key(k: &str) -> bool {
    let mut rest = k;
    while rest.len() > 2 && matches!(rest.get(..2), Some("C-" | "M-" | "S-")) {
        rest = &rest[2..];
    }
    if rest.chars().count() == 1 {
        return rest.chars().all(|c| c.is_ascii_alphanumeric());
    }
    matches!(
        rest,
        "Enter"
            | "Escape"
            | "Tab"
            | "BTab"
            | "BSpace"
            | "Space"
            | "Up"
            | "Down"
            | "Left"
            | "Right"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "DC"
            | "IC"
            | "F1"
            | "F2"
            | "F3"
            | "F4"
            | "F5"
            | "F6"
            | "F7"
            | "F8"
            | "F9"
            | "F10"
            | "F11"
            | "F12"
    )
}

fn parse_line(line: &[u8]) -> Option<TEv> {
    if let Some(rest) = line.strip_prefix(b"%output ") {
        let sp = rest.iter().position(|&b| b == b' ')?;
        let pane = std::str::from_utf8(&rest[..sp]).ok()?.to_string();
        return Some(TEv::Output { pane, data: unescape(&rest[sp + 1..]) });
    }
    let text = std::str::from_utf8(line).ok()?;
    if let Some(rest) = text.strip_prefix("%subscription-changed dead ") {
        // "$0 @1 1 %1 : %1_1_130"
        let val = rest.rsplit(" : ").next()?;
        let mut it = val.split('_');
        let pane = it.next()?.to_string();
        let dead = it.next()? == "1";
        let status = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        return dead.then_some(TEv::Dead { pane, status });
    }
    if let Some(rest) = text.strip_prefix("%window-close ") {
        return Some(TEv::WinClosed { win: rest.trim().to_string() });
    }
    if let Some(rest) = text.strip_prefix("%layout-change ") {
        // "@1 a87e,100x30,0,0,1 a87e,100x30,0,0,1 *"
        let mut it = rest.split(' ');
        let win = it.next()?.to_string();
        let layout = it.next()?;
        let dims = layout.split(',').nth(1)?;
        let (c, r) = dims.split_once('x')?;
        return Some(TEv::Layout { win, cols: c.parse().ok()?, rows: r.parse().ok()? });
    }
    // (`%exit` is not an event: the end of tmux's output that always follows it is, see `connect`)
    None
}

/// tmux escapes bytes < 0x20 and '\' in %output as three-digit octal.
fn unescape(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'\\' && i + 3 < src.len() && src[i + 1..i + 4].iter().all(|d| (b'0'..=b'7').contains(d)) {
            let v = (src[i + 1] - b'0') as u16 * 64 + (src[i + 2] - b'0') as u16 * 8 + (src[i + 3] - b'0') as u16;
            out.push(v as u8);
            i += 4;
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_octal_and_keeps_utf8() {
        assert_eq!(unescape(b"hi\\015\\012"), b"hi\r\n");
        assert_eq!(unescape(b"a\\134b"), b"a\\b");
        assert_eq!(unescape("ä😀".as_bytes()), "ä😀".as_bytes());
        // a lone backslash that is not an escape stays as is
        assert_eq!(unescape(b"x\\y"), b"x\\y");
    }

    #[test]
    fn parses_control_mode_lines() {
        match parse_line(b"%output %3 hello\\015\\012") {
            Some(TEv::Output { pane, data }) => {
                assert_eq!(pane, "%3");
                assert_eq!(data, b"hello\r\n");
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(
            parse_line(b"%subscription-changed dead $0 @1 1 %1 : %1_1_130"),
            Some(TEv::Dead { ref pane, status: 130 }) if pane == "%1"
        ));
        assert!(parse_line(b"%subscription-changed dead $0 @1 1 %1 : %1_0_").is_none());
        assert!(matches!(
            parse_line(b"%layout-change @1 a87e,100x30,0,0,1 a87e,100x30,0,0,1 *"),
            Some(TEv::Layout { cols: 100, rows: 30, .. })
        ));
    }

    #[test]
    fn only_safe_tokens_reach_the_control_stream() {
        for k in ["Enter", "C-c", "M-x", "Up", "F5", "a", "C-M-a"] {
            assert!(valid_key(k), "{k}");
        }
        for k in ["", "Enter;kill-server", "C-", "a b", "$(x)", "Enter\n", "F13", "%1", "\u{20ac}x", "C-\u{20ac}"] {
            assert!(!valid_key(k), "{k:?}");
        }
        assert!(valid_pane("%12") && !valid_pane("%1;x") && !valid_pane("1") && !valid_pane("%"));
    }
}
