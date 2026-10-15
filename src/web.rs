//! The page the daemon serves: a shell, style sheets and scripts kept in `web/` as small files
//! and put together once. There is no build step. The files are embedded in the binary.
//! (While weft's own UI is being worked on, `src/dev.rs` reads them from disk instead.)

use std::sync::OnceLock;

/// A file of the page: where it is under `web/`, and what was embedded at build time.
macro_rules! part {
    ($path:literal) => {
        ($path, include_str!(concat!("../web/", $path)))
    };
}

const SHELL: (&str, &str) = part!("index.html");

/// In the order they are joined: later files may rely on earlier ones.
const CSS: &[(&str, &str)] = &[
    part!("css/00-base.css"),
    part!("css/10-canvas.css"),
    part!("css/20-cards.css"),
    part!("css/30-chrome.css"),
    part!("css/40-dock.css"),
    part!("css/45-archive.css"),
    part!("css/50-flyout.css"),
    part!("css/55-terminal.css"),
    part!("css/60-library.css"),
    part!("css/70-watches.css"),
    part!("css/75-next.css"),
    part!("css/80-pad.css"),
    part!("css/85-evidence.css"),
    part!("css/88-tree.css"),
    part!("css/90-dial.css"),
    part!("css/92-eta.css"),
    part!("css/95-bar.css"),
    part!("css/96-quit.css"),
];

/// All of it shares one script scope, so the order is the order of the declarations.
const JS: &[(&str, &str)] = &[
    part!("js/00-base.js"),
    part!("js/10-state-net.js"),
    part!("js/20-view.js"),
    part!("js/25-tidy.js"),
    part!("js/30-cards.js"),
    part!("js/40-watches.js"),
    part!("js/45-archive.js"),
    part!("js/50-links.js"),
    part!("js/60-dial.js"),
    part!("js/70-menus.js"),
    part!("js/75-eta.js"),
    part!("js/80-pad.js"),
    part!("js/85-next.js"),
    part!("js/88-tree.js"),
    part!("js/90-omnibar.js"),
    part!("js/92-quit.js"),
    part!("js/95-flyout.js"),
    part!("js/96-terminal.js"),
    part!("js/97-library.js"),
    part!("js/98-keys.js"),
    part!("js/99-boot.js"),
];

/// The fonts, served from the binary.
pub const FONTS: [(&str, &[u8]); 4] = [
    ("chakra-500.woff2", include_bytes!("../web/chakra-500.woff2")),
    ("chakra-600.woff2", include_bytes!("../web/chakra-600.woff2")),
    ("chakra-700.woff2", include_bytes!("../web/chakra-700.woff2")),
    ("jbmono.woff2", include_bytes!("../web/jbmono.woff2")),
];

/// Where the style sheets and the scripts go in the shell.
pub const CSS_SLOT: &str = "/*__CSS__*/";
pub const JS_SLOT: &str = "/*__JS__*/";

/// The page for this build.
pub fn page(build: &str) -> String {
    static EMBEDDED: OnceLock<String> = OnceLock::new();
    let join = |parts: &[(&str, &str)]| parts.iter().map(|(_, text)| *text).collect::<Vec<_>>().join("\n");
    EMBEDDED.get_or_init(|| fill(SHELL.1, &join(CSS), &join(JS), "__BUILD__")).replace("__BUILD__", build)
}

/// The shell with its style sheets and scripts in place.
pub fn fill(shell: &str, css: &str, js: &str, build: &str) -> String {
    // the page knows which build served it, so it can reload when the daemon is replaced
    shell.replace(CSS_SLOT, css).replace(JS_SLOT, js).replace("__BUILD__", build)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &str, ext: &str) -> Vec<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web").join(dir);
        let mut v: Vec<String> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| format!("{dir}/{}", e.file_name().to_string_lossy()))
            .filter(|n| n.ends_with(ext))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn every_file_under_web_is_part_of_the_page() {
        let listed = |parts: &[(&str, &str)]| parts.iter().map(|(p, _)| p.to_string()).collect::<Vec<_>>();
        assert_eq!(files("css", ".css"), listed(CSS), "a style sheet is missing from (or not in order in) web.rs");
        assert_eq!(files("js", ".js"), listed(JS), "a script is missing from (or not in order in) web.rs");
    }

    #[test]
    fn the_page_is_whole_and_knows_its_build() {
        let p = page("test-build");
        assert!(!p.contains("/*__CSS__*/") && !p.contains("/*__JS__*/") && !p.contains("__BUILD__"));
        assert!(p.contains("name=\"weft-build\" content=\"test-build\""));
        assert!(p.contains("function applyFlow") && p.contains("--cy"));
    }
}
