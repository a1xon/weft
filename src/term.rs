//! Screen emulation helpers. The daemon keeps a `vt100` model of every pane
//! (fed from tmux control-mode output), so previews, idle detection and the
//! focused terminal never have to spawn `capture-pane`.

use vt100::{Color, Parser, Screen};

fn esc(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
struct Style {
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

fn xterm256(n: u8) -> (u8, u8, u8) {
    match n {
        16..=231 => {
            let i = n - 16;
            let lv = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            (lv(i / 36), lv((i / 6) % 6), lv(i % 6))
        }
        232..=255 => {
            let g = 8 + 10 * (n - 232);
            (g, g, g)
        }
        _ => (0, 0, 0),
    }
}

fn put_color(cls: &mut String, sty: &mut String, c: Color, bg: bool) {
    let rgb = match c {
        Color::Default => return,
        Color::Idx(n) if n < 16 => {
            cls.push_str(if bg { " g" } else { " f" });
            cls.push_str(&n.to_string());
            return;
        }
        Color::Idx(n) => xterm256(n),
        Color::Rgb(r, g, b) => (r, g, b),
    };
    sty.push_str(&format!("{}:#{:02x}{:02x}{:02x};", if bg { "background" } else { "color" }, rgb.0, rgb.1, rgb.2));
}

fn open_tag(out: &mut String, st: &Style) -> bool {
    let mut cls = String::new();
    let mut sty = String::new();
    if st.inverse {
        // Swap; a "default" colour that lands in the other slot needs a class
        // because the terminal's default fg/bg are themeable CSS variables.
        match st.bg {
            Color::Default => cls.push_str(" xf"),
            c => put_color(&mut cls, &mut sty, c, false),
        }
        match st.fg {
            Color::Default => cls.push_str(" xg"),
            c => put_color(&mut cls, &mut sty, c, true),
        }
    } else {
        put_color(&mut cls, &mut sty, st.fg, false);
        put_color(&mut cls, &mut sty, st.bg, true);
    }
    if st.bold {
        cls.push_str(" B");
    }
    if st.dim {
        cls.push_str(" D");
    }
    if st.italic {
        cls.push_str(" I");
    }
    if st.underline {
        cls.push_str(" U");
    }
    if cls.is_empty() && sty.is_empty() {
        return false;
    }
    out.push_str("<i");
    if !cls.is_empty() {
        out.push_str(" class=\"");
        out.push_str(cls.trim_start());
        out.push('"');
    }
    if !sty.is_empty() {
        out.push_str(" style=\"");
        out.push_str(&sty);
        out.push('"');
    }
    out.push('>');
    true
}

/// One screen row as compact HTML. `cursor` draws a block cursor at that column.
pub fn row_html(screen: &Screen, row: u16, cols: u16, cursor: Option<u16>) -> String {
    let mut end = 0u16;
    for c in 0..cols {
        if let Some(cell) = screen.cell(row, c) {
            if cell.has_contents() || cell.bgcolor() != Color::Default || cell.inverse() {
                end = c + 1;
            }
        }
    }
    if let Some(cc) = cursor {
        end = end.max(cc + 1).min(cols);
    }
    let mut out = String::with_capacity(end as usize + 16);
    let mut cur: Option<Style> = None;
    let mut tag_open = false;
    let mut c = 0u16;
    while c < end {
        let cell = screen.cell(row, c);
        if cell.is_some_and(|x| x.is_wide_continuation()) {
            c += 1;
            continue;
        }
        let st = cell
            .map(|x| Style {
                fg: x.fgcolor(),
                bg: x.bgcolor(),
                bold: x.bold(),
                dim: x.dim(),
                italic: x.italic(),
                underline: x.underline(),
                inverse: x.inverse(),
            })
            .unwrap_or(Style {
                fg: Color::Default,
                bg: Color::Default,
                bold: false,
                dim: false,
                italic: false,
                underline: false,
                inverse: false,
            });
        let is_cursor = cursor == Some(c);
        if cur != Some(st) || is_cursor {
            if tag_open {
                out.push_str("</i>");
            }
            tag_open = open_tag(&mut out, &st);
            cur = Some(st);
        }
        let txt = cell.map(|x| x.contents()).unwrap_or("");
        if is_cursor {
            out.push_str("<b class=\"cur\">");
        }
        if txt.is_empty() {
            out.push(' ');
        } else {
            esc(txt, &mut out);
        }
        if is_cursor {
            out.push_str("</b>");
            // force a fresh run after the cursor so styles stay balanced
            if tag_open {
                out.push_str("</i>");
                tag_open = false;
            }
            cur = None;
        }
        c += 1;
    }
    if tag_open {
        out.push_str("</i>");
    }
    out
}

const BORDER: &str = "─━│┃┌┐└┘├┤┬┴┼╭╮╰╯═║╔╗╚╝▀▄█▌▐ \u{a0}";

fn is_chrome(line: &str) -> bool {
    if line.chars().all(|c| BORDER.contains(c)) {
        return true;
    }
    let t = line.trim();
    // an agent's logo: a run of block glyphs, then the product name or the folder ("▐▛███▛█   Claude Code v2")
    let logo = t.chars().take_while(|c| "▐▛▜▌▝▘▖▗▞▚█▀▄".contains(*c)).count();
    if logo >= 2 && t.chars().nth(logo).is_none_or(|c| c == ' ') && t.chars().skip(logo).take(2).all(|c| c == ' ') {
        return true;
    }
    // the turn summary: "✻ Brewed for 3s · done 7:07 · 1 shell still running"
    if matches!(t.chars().next(), Some('✻' | '✢' | '✳' | '✶' | '✽')) && t.contains(" for ") && t.chars().any(|c| c.is_ascii_digit())
    {
        return true;
    }
    // "-----", "=====": a rule, not content (a lone "-" is a list bullet and stays)
    let t = line.trim();
    t.chars().count() >= 5 && t.chars().all(|c| "-=_~*".contains(c))
}

/// True when a plain line is terminal furniture rather than content.
pub fn is_noise(line: &str, chrome: &[String]) -> bool {
    let t = line.trim();
    if t.is_empty() || is_chrome(t) {
        return true;
    }
    // an empty input box such as "❯" / ">" / "› Ask ..."
    if matches!(t.chars().next(), Some('❯' | '›' | '>')) && t.chars().count() <= 2 {
        return true;
    }
    chrome.iter().any(|c| !c.is_empty() && t.contains(c.as_str()))
}

/// The last `n` meaningful rows (blank, border-only and chrome rows skipped) as HTML.
pub fn preview(screen: &Screen, n: usize, chrome: &[String]) -> Vec<String> {
    let (rows, cols) = screen.size();
    let plain: Vec<String> = screen.rows(0, cols).collect();
    let mut keep: Vec<u16> = Vec::new();
    for r in (0..rows).rev() {
        if !is_noise(&plain[r as usize], chrome) {
            keep.push(r);
            if keep.len() == n {
                break;
            }
        }
    }
    keep.reverse();
    keep.into_iter().map(|r| row_html(screen, r, cols, None)).collect()
}

/// Every line, history first then the live screen, right-trimmed, trailing blanks dropped.
/// Temporarily scrolls the parser; the offset is restored before returning.
pub fn lines(p: &mut Parser) -> Vec<String> {
    let (rows, cols) = p.screen().size();
    let rows = rows as usize;
    p.screen_mut().set_scrollback(usize::MAX);
    let len = p.screen().scrollback();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < len {
        let k = len - start;
        p.screen_mut().set_scrollback(k);
        let take = rows.min(k);
        out.extend(p.screen().rows(0, cols).take(take));
        start += take;
    }
    p.screen_mut().set_scrollback(0);
    out.extend(p.screen().rows(0, cols));
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
}

/// The history and live screen as text, right-trimmed, blank runs collapsed, `skip` lines left
/// out, the last `max_lines` kept.
fn transcript_where(p: &mut Parser, max_lines: usize, skip: impl Fn(&str) -> bool) -> String {
    let mut tidy: Vec<String> = Vec::new();
    for l in lines(p) {
        let blank = l.trim().is_empty();
        if !blank && skip(&l) {
            continue;
        }
        if blank && tidy.last().is_none_or(|p| p.trim().is_empty()) {
            continue;
        }
        tidy.push(l.trim_end().to_string());
    }
    while tidy.last().is_some_and(|l| l.trim().is_empty()) {
        tidy.pop();
    }
    let from = tidy.len().saturating_sub(max_lines);
    tidy[from..].join("\n")
}

/// What an agent's screen history says with the furniture taken out: the fallback result when
/// the answer could not be told apart from the rest.
pub fn transcript_clean(p: &mut Parser, max_lines: usize, chrome: &[String]) -> String {
    transcript_where(p, max_lines, |l| is_noise(l, chrome))
}

/// Plain text of the whole history plus the live screen, last `max_lines` lines (only the
/// borders and rules are dropped: this is what `weft read` shows).
pub fn transcript(p: &mut Parser, max_lines: usize) -> String {
    transcript_where(p, max_lines, is_chrome)
}

fn norm(s: &str) -> String {
    let t = s.trim_start_matches(|c: char| "❯›>│●⏺•*- \u{a0}".contains(c));
    t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// How to find a message again in an input box: the tail of its last line, lowercased and
/// single-spaced (a terminal wraps lines, so only a short tail is looked for).
pub fn probe_of(text: &str) -> String {
    let last = text.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let n: Vec<char> = norm(last).chars().collect();
    n[n.len().saturating_sub(24)..].iter().collect::<String>().trim().to_string()
}

/// How many places on the whole screen show a message: its tail (see [`probe_of`]), or a folded
/// "[Pasted text ...]" placeholder. A message that was taken shows up once more than before.
pub fn traces(screen: &Screen, probe: &str) -> usize {
    let (_, cols) = screen.size();
    let t = norm(&screen.rows(0, cols).collect::<Vec<_>>().join(" "));
    let tail = if probe.chars().count() >= 6 { t.matches(probe).count() } else { 0 };
    tail + t.matches("[pasted").count()
}

/// Is a message still sitting in the program's input box? The cursor stands at the end of what is
/// typed, so the rows at and just above it are looked at for the tail of the message (see
/// [`probe_of`]) or for a "[Pasted text ...]" placeholder. After a submit the message is in the
/// transcript, further up, and the cursor is in an empty box.
pub fn input_holds(screen: &Screen, probe: &str) -> bool {
    let (rows, cols) = screen.size();
    let cur = screen.cursor_position().0.min(rows.saturating_sub(1));
    let from = cur.saturating_sub(2);
    let near: Vec<String> =
        screen.rows(0, cols).enumerate().filter(|(i, _)| (*i as u16) >= from && (*i as u16) <= cur).map(|(_, r)| r).collect();
    let t = norm(&near.join(" "));
    (probe.chars().count() >= 6 && t.contains(probe)) || t.contains("[pasted")
}

fn is_tool_call(t: &str) -> bool {
    const TOOLS: [&str; 12] = ["Read", "Bash", "Update", "Write", "Edit", "Search", "Grep", "Glob", "List", "Task", "Fetch", "WebFetch"];
    let t = t.trim_start_matches(|c: char| "●⏺•* \u{a0}".contains(c));
    TOOLS.iter().any(|n| t.starts_with(&format!("{n}(")))
}

/// What the agent said in response to `prompt`: the lines that appeared since
/// `before`, minus the echo of the prompt, tool-call lines and terminal chrome.
/// Falls back to an empty string when nothing is left (the caller then uses the tail).
pub fn extract_response(before: &[String], after: &[String], prompt: &str, chrome: &[String]) -> String {
    let mut k = 0;
    while k < before.len() && k < after.len() && before[k] == after[k] {
        k += 1;
    }
    let want: Vec<String> = prompt.lines().map(norm).filter(|l| l.chars().count() >= 8).collect();
    let mut out: Vec<String> = Vec::new();
    for l in &after[k..] {
        let t = l.trim();
        if t.is_empty() {
            if out.last().is_some_and(|p| !p.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        if is_noise(t, chrome) || is_tool_call(t) || t.contains("[Pasted text") {
            continue;
        }
        if matches!(t.chars().next(), Some('❯' | '›')) {
            continue; // the user's own message / input box
        }
        let n = norm(t);
        if n.is_empty() || (n.chars().count() >= 8 && want.iter().any(|w| w.contains(n.as_str()) || n.contains(w.as_str()))) {
            continue;
        }
        let t = t.trim_start_matches(|c: char| "●⏺•".contains(c)).trim_start();
        out.push(t.to_string());
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Wrap `text` for delivery to a pane: strip control characters that could
/// break out of a bracketed paste, then wrap if the app enabled the mode.
pub fn paste_payload(screen: &Screen, text: &str) -> Vec<u8> {
    let clean: String = text.chars().filter(|&c| c == '\n' || c == '\t' || !c.is_control()).collect();
    if screen.bracketed_paste() {
        let mut v = b"\x1b[200~".to_vec();
        v.extend_from_slice(clean.as_bytes());
        v.extend_from_slice(b"\x1b[201~");
        v
    } else {
        clean.replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn screen_of(text: &str) -> Parser {
        let mut p = Parser::new(14, 60, 100);
        p.process(text.replace('\n', "\r\n").as_bytes());
        p
    }

    #[test]
    fn a_message_in_the_input_box_is_told_from_one_that_was_sent() {
        let msg = "check which page could we use as a blog page.\nI only want mini blog pages and a slider on the homepage, reason about that.";
        let probe = probe_of(msg);
        assert_eq!(probe, "ider on the homepage, reason about that.".chars().rev().take(24).collect::<Vec<_>>().into_iter().rev().collect::<String>().trim());
        // typed, not submitted: the text is in the box and the cursor is at its end
        let waiting = screen_of(
            "\n                      ctrl+g to edit in VS Code\n\
             ----------------------------------------------\n\
             \u{276f} check which page could we use as a blog page.\n  I only want mini blog pages and a slider on the\n  homepage, reason about that.",
        );
        assert!(input_holds(waiting.screen(), &probe), "the tail is next to the cursor");
        // sent: the message is in the transcript, the answer and a spinner are under it, the box is empty
        let sent = screen_of(
            "\u{276f} check which page could we use as a blog page.\n  I only want mini blog pages and a slider on the\n  homepage, reason about that.\n\
             * Thinking...\n                                     xhigh\n\
             ----------------------------------------------\n\u{276f} ",
        );
        assert!(!input_holds(sent.screen(), &probe));
        // a long paste is folded into a placeholder
        let folded = screen_of("----------------------------\n\u{276f} [Pasted text #1 +12 lines]");
        assert!(input_holds(folded.screen(), &probe_of("something else entirely, which is long")));
        // an empty box with a hint is not holding anything
        let empty = screen_of("----------------------------\n\u{276f} Try \"edit <filepath> to...\"");
        assert!(!input_holds(empty.screen(), &probe));
        // a probe that is too short to mean anything finds nothing on its own
        assert!(!input_holds(empty.screen(), "ok"));
        // the whole screen: one more place after the message was sent
        assert_eq!(traces(waiting.screen(), &probe), 1);
        assert_eq!(traces(sent.screen(), &probe), 1);
        assert_eq!(traces(empty.screen(), &probe), 0);
        assert_eq!(traces(folded.screen(), &probe), 1);
    }

    #[test]
    fn extracts_only_the_answer() {
        let before = v(&["banner", "", "──────", "❯", "──────", "  bypass permissions on"]);
        let after = v(&[
            "banner",
            "",
            "❯ [Pasted text +4 lines]",
            "● Read(textkit/slug.py)",
            "● Updated textkit/slug.py and kept the API.",
            "  All five slug tests now pass.",
            "✻ Cooked for 4s",
            "──────",
            "❯",
            "──────",
            "  bypass permissions on",
        ]);
        let chrome = v(&["bypass permissions", "Cooked for"]);
        let got = extract_response(&before, &after, "Fix the slugify function in textkit/slug.py", &chrome);
        assert_eq!(
            got,
            "Updated textkit/slug.py and kept the API.
All five slug tests now pass."
        );
    }

    #[test]
    fn drops_wrapped_echo_of_the_prompt() {
        let before = v(&["❯"]);
        let after = v(&[
            "❯ Fix the slugify function in textkit/slug.py so that",
            "  all of tests/test_slug.py passes and keep the public",
            "  API.",
            "● Done.",
        ]);
        let prompt = "Fix the slugify function in textkit/slug.py so that all of tests/test_slug.py passes and keep the public API.";
        // "API." is short and kept out by the 8-char rule only if it is also quoted; "Done." stays
        let got = extract_response(&before, &after, prompt, &[]);
        assert!(got.ends_with("Done."), "{got:?}");
        assert!(!got.contains("slugify"), "{got:?}");
    }

    #[test]
    fn noise_detection() {
        let chrome = v(&["? for shortcuts"]);
        assert!(is_noise("──────────────", &chrome));
        assert!(is_noise("  ❯", &chrome));
        assert!(is_noise("  ? for shortcuts  Gemini", &chrome));
        assert!(!is_noise("Updated slug.py", &chrome));
        assert!(is_noise("----------------------------------------", &chrome));
        assert!(!is_noise("- a bullet", &chrome));
        // an agent's logo and its turn summary are furniture; a progress bar and a sentence are not
        assert!(is_noise("▐▛███▛█   Claude Code v2.1.299", &chrome));
        assert!(is_noise("▝▜██████▀  Haiku 5.5 with medium effort · Claude Max", &chrome));
        assert!(is_noise("✻ Crunched for 2m 26s · done 7:03", &chrome));
        assert!(is_noise("✻ Brewed for 3s · done 7:07 · 1 shell still running", &chrome));
        assert!(!is_noise("██████ 80% of the tests pass", &chrome));
        assert!(!is_noise("The tests ran for 3s and all pass.", &chrome));
    }
}
