//! weft from the desktop. `weft app` opens a project's canvas in a window of its own (a
//! Chromium-family browser in app mode, with a profile of its own), and `weft desktop` puts weft
//! in the app grid so it can be pinned to the dock. The dock icon's menu lists the projects
//! opened last.

use crate::model::{data_home, home, state_dir};
use std::{
    io::IsTerminal,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// How the canvas is shown once its daemon runs.
#[derive(Clone, Copy, PartialEq)]
pub enum Show {
    /// a tab of the default browser
    Tab,
    /// a window of its own
    Window,
    /// only print the address
    Nothing,
}

/// Browsers with an app mode, and the prefix of the id their app windows get. On Wayland that id
/// is `<prefix>-127.0.0.1__-Default` whatever the port (`--class` is ignored there), so every
/// project's window groups under the one dock icon whose `StartupWMClass` names it.
const BROWSERS: &[(&str, &str)] = &[
    ("brave-browser", "brave"),
    ("google-chrome-stable", "chrome"),
    ("google-chrome", "chrome"),
    ("microsoft-edge", "msedge"),
    ("chromium-browser", "chromium"),
    ("chromium", "chromium"),
];

/// The same on macOS, where browsers are apps rather than commands on PATH.
const MAC_BROWSERS: &[(&str, &str)] = &[
    ("/Applications/Brave Browser.app/Contents/MacOS/Brave Browser", "brave"),
    ("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "chrome"),
    ("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "msedge"),
    ("/Applications/Chromium.app/Contents/MacOS/Chromium", "chromium"),
];

fn on_path(cmd: &str) -> Option<PathBuf> {
    if cmd.contains('/') {
        return Some(PathBuf::from(cmd)).filter(|p| p.is_file());
    }
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(cmd)).find(|p| p.is_file())
}

/// The browser for weft's window and its window-id prefix: `$WEFT_BROWSER`, else the default
/// browser if it has an app mode, else the first one installed.
fn browser() -> Option<(PathBuf, &'static str)> {
    let prefix = |name: &str| BROWSERS.iter().find(|(b, _)| *b == name).map(|(_, p)| *p);
    if let Ok(b) = std::env::var("WEFT_BROWSER") {
        let name = Path::new(&b).file_name()?.to_string_lossy().into_owned();
        return on_path(&b).map(|p| (p, prefix(&name).unwrap_or("chromium")));
    }
    let default = Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_end_matches(".desktop").to_string())
        .unwrap_or_default();
    let first = BROWSERS.iter().filter(|(b, _)| *b == default);
    first.chain(BROWSERS).chain(MAC_BROWSERS).find_map(|(b, p)| on_path(b).map(|path| (path, *p)))
}

/// Shows the canvas at `url`; a window falls back to a tab when no browser has an app mode.
pub fn show(url: &str, how: Show) {
    if how == Show::Nothing {
        return;
    }
    if how == Show::Window {
        if let Some((b, _)) = browser() {
            let started = Command::new(b)
                .arg(format!("--app={url}"))
                .arg(format!("--user-data-dir={}", state_dir().join("window").display()))
                .args(["--no-first-run", "--no-default-browser-check"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn();
            if started.is_ok() {
                return;
            }
        }
    }
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(opener).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

// ------------------------------------------------------------- the folder

/// The folder `weft app` opens: the one named, else the current one when started in a project
/// from a terminal, else the project opened last. `--pick`, or no project yet, asks for one;
/// `None` = the question was cancelled.
pub fn app_dir(args: &[String]) -> Result<Option<PathBuf>, String> {
    if let Some(d) = args.iter().find(|a| !a.starts_with('-')) {
        return Path::new(d).canonicalize().map(Some).map_err(|e| format!("{d}: {e}"));
    }
    if !args.iter().any(|a| a == "--pick") {
        // the dock starts apps in the home folder
        if let Some(cwd) = std::env::current_dir().ok().filter(|c| *c != home() && c.parent().is_some()) {
            return Ok(Some(cwd));
        }
        if let Some(last) = recent().into_iter().next() {
            return Ok(Some(last));
        }
    }
    let start = recent().first().and_then(|d| d.parent()).map(Path::to_path_buf).unwrap_or_else(home);
    let mut ask = if cfg!(target_os = "macos") {
        let mut c = Command::new("osascript");
        c.arg("-e").arg(format!(
            "POSIX path of (choose folder with prompt \"weft: which project folder?\" default location POSIX file {})",
            applescript_str(&start.to_string_lossy())
        ));
        c
    } else {
        let mut c = Command::new("zenity");
        c.args(["--file-selection", "--directory", "--title=weft: which project folder?"]).arg(format!("--filename={}/", start.display()));
        c
    };
    let out = ask
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "which folder? nothing here to ask with (zenity): weft app <folder>".to_string())?;
    let picked = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || picked.is_empty() {
        return Ok(None);
    }
    Path::new(&picked).canonicalize().map(Some).map_err(|e| format!("{picked}: {e}"))
}

fn recent_file() -> PathBuf {
    state_dir().join("recent")
}

/// The projects opened last, newest first (folders that are gone are left out).
pub fn recent() -> Vec<PathBuf> {
    let listed: Vec<PathBuf> =
        std::fs::read_to_string(recent_file()).unwrap_or_default().lines().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    if !listed.is_empty() {
        return listed;
    }
    // nothing on the list (yet): the records the daemons left, newest first
    let mut seen: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(state_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let at = e.metadata().ok()?.modified().ok()?;
            let info: crate::client::Info = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            Some((at, PathBuf::from(info.dir)))
        })
        .filter(|(_, d)| d.is_dir())
        .collect();
    seen.sort_by(|a, b| b.0.cmp(&a.0));
    seen.into_iter().map(|(_, d)| d).collect()
}

/// `dir` was opened: it heads the list, and the dock icon's menu follows if this weft put it there.
pub fn remember(dir: &Path) {
    let mut list = recent();
    list.retain(|d| d != dir);
    list.insert(0, dir.to_path_buf());
    list.truncate(8);
    keep_recent(&list);
}

/// `dir` leaves the list (a board left off the board list).
pub fn forget(dir: &Path) {
    let mut list = recent();
    if list.iter().any(|d| d == dir) {
        list.retain(|d| d != dir);
        keep_recent(&list);
    }
}

fn keep_recent(list: &[PathBuf]) {
    let text: String = list.iter().map(|d| format!("{}\n", d.display())).collect();
    let _ = std::fs::create_dir_all(state_dir());
    let _ = std::fs::write(recent_file(), text);
    let mine = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok());
    let entry = std::fs::read_to_string(entry_path()).unwrap_or_default();
    if mine.is_some_and(|e| entry.contains(&format!("Exec={} app\n", exec_arg(&e.to_string_lossy())))) {
        let _ = install();
    }
}

/// An error of a start from the desktop, where nobody reads stderr, also goes to a notification.
pub fn tell(e: String) -> String {
    if !std::io::stderr().is_terminal() {
        let mut c = if cfg!(target_os = "macos") {
            let mut c = Command::new("osascript");
            c.arg("-e").arg(format!("display notification {} with title \"weft\"", applescript_str(&e)));
            c
        } else {
            let mut c = Command::new("notify-send");
            c.args(["--app-name=weft", "weft", &e]);
            c
        };
        let _ = c.stdin(Stdio::null()).status();
    }
    e
}

/// A string literal for AppleScript.
fn applescript_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

// ------------------------------------------------------- the desktop entry


fn entry_path() -> PathBuf {
    data_home().join("applications/weft.desktop")
}

fn icon_path() -> PathBuf {
    data_home().join("icons/hicolor/scalable/apps/weft.svg")
}

/// weft's icon: one weft thread, as a shuttle lays it, back and forth over and under three warp
/// threads, ending in a card's out port (◇). Where a warp lies on top, the weft is cut by a strip of
/// background and the warp drawn again. The favicon in `web/index.html` is the 16 px cut of it.
const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<defs>
<linearGradient id="bg" gradientUnits="userSpaceOnUse" x1="0" y1="8" x2="0" y2="116"><stop offset="0" stop-color="#18222f"/><stop offset=".55" stop-color="#0a0e15"/><stop offset="1" stop-color="#05070b"/></linearGradient>
<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".08"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient>
<linearGradient id="warp" gradientUnits="userSpaceOnUse" x1="0" y1="24" x2="0" y2="100"><stop offset="0" stop-color="#6f84a3"/><stop offset="1" stop-color="#2f4058"/></linearGradient>
<linearGradient id="weft" gradientUnits="userSpaceOnUse" x1="30" y1="30" x2="104" y2="94"><stop offset="0" stop-color="#d4f1ff"/><stop offset=".4" stop-color="#5cc8ff"/><stop offset="1" stop-color="#2f95c9"/></linearGradient>
<filter id="glow" x="-.5" y="-.5" width="2" height="2"><feGaussianBlur stdDeviation="5"/></filter>
<filter id="soft" x="-.2" y="-.2" width="1.4" height="1.4"><feGaussianBlur stdDeviation="2.5"/></filter>
</defs>
<rect x="10" y="13" width="108" height="108" rx="26" fill="#000" opacity=".5" filter="url(#soft)"/>
<rect x="10" y="8" width="108" height="108" rx="26" fill="url(#bg)"/>
<rect x="10" y="8" width="108" height="108" rx="26" fill="url(#sheen)"/>
<rect x="10.75" y="8.75" width="106.5" height="106.5" rx="25.25" fill="none" stroke="#5a7394" stroke-opacity=".45" stroke-width="1.5"/>
<rect x="37.5" y="24" width="9" height="76" rx="4.5" fill="url(#warp)"/>
<rect x="59.5" y="24" width="9" height="76" rx="4.5" fill="url(#warp)"/>
<rect x="81.5" y="24" width="9" height="76" rx="4.5" fill="url(#warp)"/>
<path d="M22 39H97A11.5 11.5 0 0 1 97 62H31A11.5 11.5 0 0 0 31 85H97" fill="none" stroke="#5cc8ff" stroke-width="9.5" stroke-linecap="round" stroke-linejoin="round" opacity=".8" filter="url(#glow)"/>
<path d="M22 39H97A11.5 11.5 0 0 1 97 62H31A11.5 11.5 0 0 0 31 85H97" fill="none" stroke="url(#bg)" stroke-width="15.9" stroke-linecap="round" stroke-linejoin="round"/>
<path d="M22 39H97A11.5 11.5 0 0 1 97 62H31A11.5 11.5 0 0 0 31 85H97" fill="none" stroke="url(#weft)" stroke-width="9.5" stroke-linecap="round" stroke-linejoin="round"/>
<rect x="56.3" y="33.75" width="15.4" height="10.5" fill="url(#bg)"/>
<rect x="59.5" y="30.55" width="9" height="16.9" fill="url(#warp)"/>
<rect x="34.3" y="56.75" width="15.4" height="10.5" fill="url(#bg)"/>
<rect x="37.5" y="53.55" width="9" height="16.9" fill="url(#warp)"/>
<rect x="78.3" y="56.75" width="15.4" height="10.5" fill="url(#bg)"/>
<rect x="81.5" y="53.55" width="9" height="16.9" fill="url(#warp)"/>
<rect x="56.3" y="79.75" width="15.4" height="10.5" fill="url(#bg)"/>
<rect x="59.5" y="76.55" width="9" height="16.9" fill="url(#warp)"/>
<path d="M103 76L112 85L103 94L94 85Z" fill="url(#bg)" stroke="#5cc8ff" stroke-width="5.2" stroke-linejoin="round"/>
</svg>
"##;

/// One argument of an `Exec` line, quoted the way the desktop entry spec wants it.
fn exec_arg(s: &str) -> String {
    let s = s.replace('%', "%%");
    if !s.chars().any(|c| " \t\n\"'\\><~|&;$*?#()`".contains(c)) {
        return s;
    }
    let mut q = String::from("\"");
    for c in s.chars() {
        if "\"`$\\".contains(c) {
            q.push('\\');
        }
        q.push(c);
    }
    q.push('"');
    // the value itself is a desktop-entry string, where a backslash is written twice
    q.replace('\\', "\\\\")
}

fn entry_text(exe: &str, icon: &str, class: Option<&str>, recent: &[PathBuf]) -> String {
    let exe = exec_arg(exe);
    let mut s = format!(
        "[Desktop Entry]\nType=Application\nName=weft\nGenericName=Agent canvas\nComment=A canvas for coding agents\n\
         Exec={exe} app\nIcon={icon}\nTerminal=false\nCategories=Development;\nStartupNotify=true\n"
    );
    if let Some(c) = class {
        s += &format!("StartupWMClass={c}\n");
    }
    let shown: Vec<&PathBuf> = recent.iter().take(5).collect();
    let ids: Vec<String> = (0..shown.len()).map(|i| format!("p{i}")).chain(["pick".to_string()]).collect();
    s += &format!("Actions={};\n", ids.join(";"));
    for (i, d) in shown.iter().enumerate() {
        let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| d.display().to_string());
        s += &format!("\n[Desktop Action p{i}]\nName={name}\nExec={exe} app {}\n", exec_arg(&d.to_string_lossy()));
    }
    s += &format!("\n[Desktop Action pick]\nName=Other folder…\nExec={exe} app --pick\n");
    s
}

/// Writes weft's desktop entry and icon, or brings them up to date. Returns the entry's path.
pub fn install() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().and_then(|e| e.canonicalize()).map_err(|e| e.to_string())?;
    let class = browser().map(|(_, p)| format!("{p}-127.0.0.1__-Default"));
    let (entry, icon) = (entry_path(), icon_path());
    for f in [&entry, &icon] {
        std::fs::create_dir_all(f.parent().unwrap_or(Path::new("/"))).map_err(|e| e.to_string())?;
    }
    if std::fs::read_to_string(&icon).ok().as_deref() != Some(ICON) {
        std::fs::write(&icon, ICON).map_err(|e| format!("{}: {e}", icon.display()))?;
    }
    let text = entry_text(&exe.to_string_lossy(), &icon.to_string_lossy(), class.as_deref(), &recent());
    if std::fs::read_to_string(&entry).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&entry, text).map_err(|e| format!("{}: {e}", entry.display()))?;
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_lines_quote_what_the_desktop_spec_reserves() {
        assert_eq!(exec_arg("/home/p/.local/bin/weft"), "/home/p/.local/bin/weft");
        assert_eq!(exec_arg("/home/p/my projects/x"), "\"/home/p/my projects/x\"");
        assert_eq!(exec_arg("/a/$b"), "\"/a/\\\\$b\"", "a quoted $ is escaped, and that backslash written twice");
        assert_eq!(exec_arg("/a/100%"), "/a/100%%");
    }

    #[test]
    fn applescript_strings_escape_quotes_and_backslashes() {
        assert_eq!(applescript_str(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
    }

    #[test]
    fn the_dock_menu_lists_recent_projects_then_other_folder() {
        let recent: Vec<PathBuf> = (0..7).map(|i| PathBuf::from(format!("/w/proj{i}"))).collect();
        let t = entry_text("/bin/weft", "/i/weft.svg", Some("brave-127.0.0.1__-Default"), &recent);
        assert!(t.contains("\nExec=/bin/weft app\n"));
        assert!(t.contains("\nStartupWMClass=brave-127.0.0.1__-Default\n"));
        assert!(t.contains("\nActions=p0;p1;p2;p3;p4;pick;\n"), "five projects at most");
        assert!(t.contains("[Desktop Action p0]\nName=proj0\nExec=/bin/weft app /w/proj0\n"));
        assert!(!t.contains("proj5"));
        assert!(t.ends_with("[Desktop Action pick]\nName=Other folder…\nExec=/bin/weft app --pick\n"));
        assert!(!entry_text("/bin/weft", "/i/weft.svg", None, &[]).contains("StartupWMClass"), "no app-mode browser: a tab, no class");
    }
}
