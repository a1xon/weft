//! Development aid, not part of weft proper. While weft's own UI is being worked on, the page is
//! read from `web/` on disk on every load and an open tab follows the edits as they land: style
//! sheets are swapped in place, a changed script or shell reloads the page and puts back what was
//! open (`web/dev.js`). Nothing to rebuild, nothing to restart.
//!
//! On when the folder weft serves is weft's own source (`web/index.html` and `src/web.rs`), or
//! when `WEFT_WEB_DIR` names a folder of the same shape; `WEFT_WEB_DIR=off` turns it off.
//!
//! To shed it: delete this file, `web/dev.js` and `tests/ui/dev.cjs`, then the lines marked
//! `dev:` (`grep -rn 'dev:' src tests`) and the "Working on weft's UI" part of the README.

use crate::server::{host_ok, App};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    ffi::OsString,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

/// What runs in the page, for a folder that does not have it.
const FOLLOWER: &str = include_str!("../web/dev.js");
/// How long a tab's question is held open when nothing changes.
const HOLD: Duration = Duration::from_secs(25);
/// How often the files are looked at while a tab waits.
const LOOK: Duration = Duration::from_millis(200);
/// An editor or an agent writes several files in a row: answer once they have been still this long.
const SETTLE: Duration = Duration::from_millis(250);

static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Decided once, when the daemon starts: the folder the page comes from, if any.
pub fn init(project: &Path) -> Option<&'static Path> {
    DIR.get_or_init(|| pick(project, std::env::var_os("WEFT_WEB_DIR"))).as_deref()
}

fn dir() -> Option<&'static Path> {
    DIR.get().and_then(|d| d.as_deref())
}

fn pick(project: &Path, env: Option<OsString>) -> Option<PathBuf> {
    let dir = match env {
        Some(v) if v.is_empty() || v == "off" || v == "0" => return None,
        Some(v) => std::path::absolute(PathBuf::from(v)).ok()?,
        None if project.join("src/web.rs").is_file() => project.join("web"),
        None => return None,
    };
    dir.join("index.html").is_file().then_some(dir)
}

/// The page, read from disk now; `None` when weft serves its embedded one.
pub fn page(build: &str) -> Option<String> {
    dir().map(|d| Snap::read(d).page(build).0)
}

pub fn routes() -> Router<App> {
    Router::new().route("/dev/wait", get(wait)).route("/dev/css", get(css))
}

/// A tab asks "anything newer than what I have?" and gets an answer once there is (or after a while).
async fn wait(State(app): State<App>, h: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    if !host_ok(&h, app.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(dir) = dir() else { return StatusCode::NOT_FOUND.into_response() };
    let t0 = Instant::now();
    let mut sig = signature(dir);
    loop {
        let now = Snap::read(dir).stamps();
        if q.get("css") != Some(&now.css) || q.get("page") != Some(&now.page) || t0.elapsed() > HOLD {
            return Json(json!({"css": now.css, "page": now.page})).into_response();
        }
        loop {
            tokio::time::sleep(LOOK).await;
            let s = signature(dir);
            if s != sig || t0.elapsed() > HOLD {
                sig = s;
                break;
            }
        }
        let still = Instant::now();
        loop {
            tokio::time::sleep(SETTLE).await;
            let s = signature(dir);
            if s == sig || still.elapsed() > Duration::from_secs(3) {
                break;
            }
            sig = s;
        }
    }
}

/// The style sheets as they are on disk, swapped into the open page.
async fn css(State(app): State<App>, h: HeaderMap) -> Response {
    if !host_ok(&h, app.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(dir) = dir() else { return StatusCode::NOT_FOUND.into_response() };
    let css = Snap::read(dir).css();
    let stamp = stamp(&css);
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], [("x-weft-css", stamp)], css)
        .into_response()
}

/// The files of the page as they are on disk right now.
struct Snap {
    shell: String,
    css: Vec<(String, String)>,
    js: Vec<(String, String)>,
    follower: String,
}

struct Stamps {
    css: String,
    page: String,
}

/// `css/*.css` or `js/*.js` in file-name order, the order `web.rs` keeps (its test makes sure):
/// a file added while working is part of the next load.
fn listing(dir: &Path, sub: &str, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.join(sub))
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.retain(|n| n.ends_with(ext) && !n.starts_with('.'));
    v.sort();
    v.into_iter().map(|n| format!("{sub}/{n}")).collect()
}

fn all_files(dir: &Path) -> Vec<String> {
    let mut v = vec!["index.html".to_string(), "dev.js".to_string()];
    v.extend(listing(dir, "css", ".css"));
    v.extend(listing(dir, "js", ".js"));
    v
}

/// Cheap: names, sizes and times. Looked at a few times a second while a tab waits.
fn signature(dir: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    for f in all_files(dir) {
        f.hash(&mut h);
        if let Ok(m) = std::fs::metadata(dir.join(&f)) {
            m.len().hash(&mut h);
            m.modified().ok().hash(&mut h);
        }
    }
    h.finish()
}

fn stamp<T: Hash + ?Sized>(t: &T) -> String {
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    format!("{:016x}", h.finish())
}

impl Snap {
    fn read(dir: &Path) -> Snap {
        let get = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
        let parts = |sub, ext| {
            listing(dir, sub, ext)
                .into_iter()
                .map(|f| {
                    let t = get(&f);
                    (f, t)
                })
                .collect()
        };
        Snap {
            shell: get("index.html"),
            css: parts("css", ".css"),
            js: parts("js", ".js"),
            follower: std::fs::read_to_string(dir.join("dev.js")).unwrap_or_else(|_| FOLLOWER.to_string()),
        }
    }

    fn css(&self) -> String {
        self.css.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// A style sheet changes only `css`; anything else changes `page`.
    fn stamps(&self) -> Stamps {
        Stamps { css: stamp(&self.css()), page: stamp(&(&self.shell, &self.js, &self.follower)) }
    }

    /// The page with the follower in its head, and the line each script starts on in it, so an
    /// error can be named by file and line.
    fn page(&self, build: &str) -> (String, Vec<(usize, String)>) {
        let css = self.css();
        let js = self.js.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n");
        // its own script: it still runs when the page's script does not even parse
        let shell = self.shell.replacen("</head>", &format!("<script>\n{}\n</script>\n</head>", self.follower), 1);
        let mut line = shell.find(crate::web::JS_SLOT).map_or(0, |at| shell[..at].replace(crate::web::CSS_SLOT, &css).matches('\n').count() + 1);
        let mut lines = vec![];
        for (f, t) in &self.js {
            lines.push((line, f.clone()));
            line += t.matches('\n').count() + 1;
        }
        let st = self.stamps();
        // on one line, so the lines counted above stay true
        let cfg = json!({"css": st.css, "page": st.page, "lines": lines});
        (crate::web::fill(&shell, &css, &js, build).replacen("__DEVCFG__", &cfg.to_string(), 1), lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// A copy of `web/` to change, one per test (they run in parallel).
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("weft-dev-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for f in all_files(&repo().join("web")) {
            std::fs::create_dir_all(d.join(&f).parent().unwrap()).unwrap();
            std::fs::copy(repo().join("web").join(&f), d.join(&f)).unwrap();
        }
        d
    }

    #[test]
    fn it_is_on_for_weft_itself_and_where_asked() {
        let r = repo();
        assert_eq!(pick(&r, None), Some(r.join("web")), "weft's own source folder");
        assert_eq!(pick(&r.join("src"), None), None, "any other folder");
        assert_eq!(pick(&r, Some("off".into())), None, "turned off");
        assert_eq!(pick(&r, Some("".into())), None, "turned off");
        assert_eq!(pick(&r.join("src"), Some(r.join("web").into())), Some(r.join("web")), "asked for");
        assert_eq!(pick(&r, Some(r.join("src").into())), None, "a folder without a page");
    }

    #[test]
    fn the_page_from_disk_is_the_embedded_page_plus_the_follower() {
        let mut s = Snap::read(&repo().join("web"));
        s.follower = String::new();
        let (p, _) = s.page("b");
        assert_eq!(p.replacen("<script>\n\n</script>\n", "", 1), crate::web::page("b"));
        assert!(Snap::read(&repo().join("web")).page("b").0.contains("\"lines\":[["), "the follower gets its settings");
    }

    #[test]
    fn an_error_can_be_traced_to_its_file_and_line() {
        let s = Snap::read(&repo().join("web"));
        let (p, lines) = s.page("b");
        let page: Vec<&str> = p.split('\n').collect();
        assert_eq!(lines.len(), s.js.len());
        for ((at, f), (_, text)) in lines.iter().zip(&s.js) {
            assert_eq!(page[at - 1], text.split('\n').next().unwrap(), "{f} starts on line {at}");
            let last = text.trim_end_matches('\n').split('\n').count();
            assert_eq!(page[at - 1 + last - 1], text.trim_end_matches('\n').split('\n').last().unwrap(), "{f} ends where it should");
        }
    }

    #[test]
    fn a_style_sheet_is_told_apart_from_a_script_and_new_files_count() {
        let d = scratch("stamps");
        let before = Snap::read(&d).stamps();
        let sig = signature(&d);
        let f = d.join("css/00-base.css");
        std::fs::write(&f, std::fs::read_to_string(&f).unwrap() + "\nbody{color:red}").unwrap();
        let css = Snap::read(&d).stamps();
        assert!(css.css != before.css && css.page == before.page, "a style sheet changes only the style stamp");
        assert_ne!(signature(&d), sig);
        std::fs::write(d.join("js/50-new.js"), "const NEW_PART = 1;\n").unwrap();
        let js = Snap::read(&d).stamps();
        assert!(js.page != css.page && js.css == css.css, "a new script changes only the page stamp");
        let (p, lines) = Snap::read(&d).page("b");
        assert!(p.contains("const NEW_PART = 1;") && lines.iter().any(|(_, f)| f == "js/50-new.js"), "and is part of the page");
        let _ = std::fs::remove_dir_all(&d);
    }
}
