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

#[derive(PartialEq, Clone, Copy, Default)]
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
            .unwrap_or_default();
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

/// Where a message was echoed in `lines`: the line its tail (see [`probe_of`]) ends on, the last
/// such place. A terminal wraps long lines, so each line is read with the two above it.
pub fn echo_end(lines: &[String], probe: &str) -> Option<usize> {
    if probe.chars().count() < 6 {
        return None;
    }
    let has = |i: usize| norm(&lines[i.saturating_sub(2)..=i].join(" ")).contains(probe);
    let mut end = (0..lines.len()).rev().find(|&i| has(i))?;
    // the same tail is still in reach a line or two further down: step back to where it ends
    while end > 0 && has(end - 1) {
        end -= 1;
    }
    Some(end)
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

/// Is one of an agent's dialogs (`patterns`: its question or one of its options) up on the screen?
/// A dialog takes over the bottom of the screen and offers choices: the phrase must be near the
/// bottom with a choice at or under it ("❯ 1. Yes", "› 2. No", "> Yes, I trust this folder"), or be
/// the last line, where a one-line question waits ("Add file? (Y)es/(N)o").
/// The same words in what an agent shows or writes (its own source, a test, a diff) are no dialog,
/// and neither is Claude Code's input box under them ("❯ " right below a rule).
pub fn dialog_on(text: &str, patterns: &[String]) -> bool {
    const BOTTOM: usize = 20;
    let rows: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let rows = &rows[rows.len().saturating_sub(BOTTOM)..];
    let choice = |i: usize| {
        let t = rows[i].trim_start_matches(|c: char| "│┃ \u{a0}".contains(c));
        let marked = t.strip_prefix(['❯', '›', '>']).map(|r| r.trim_start());
        let numbered = |s: &str| {
            let d = s.chars().take_while(char::is_ascii_digit).count();
            d > 0 && s[d..].starts_with(['.', ')']) && s[d + 1..].starts_with(' ')
        };
        let under_rule = i > 0 && rows[i - 1].chars().all(|c| BORDER.contains(c));
        numbered(t) || marked.is_some_and(|r| numbered(r) || (!r.is_empty() && !under_rule))
    };
    // every row with a phrase counts: an option may be one ("Yes, I trust this folder" under the
    // highlighted "❯ No, exit"), and then the dialog's footer, its last line, is what tells it
    rows.iter()
        .enumerate()
        .filter(|(_, l)| patterns.iter().any(|p| !p.is_empty() && l.contains(p.as_str())))
        .any(|(at, _)| at + 1 == rows.len() || (at..rows.len()).any(choice))
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
    dedent(&response_rows(&after[k..], prompt, chrome).join("\n"))
}

/// The line a working agent animates: "· Actualizing… (1s · ↓ 25 tokens · thinking with low effort)".
fn is_spinner(t: &str) -> bool {
    let mut c = t.chars();
    let (Some(g), Some(' ')) = (c.next(), c.next()) else { return false };
    let rest = c.as_str();
    "·✢✳✶✻✽*".contains(g) && rest.split_once('…').is_some_and(|(w, _)| !w.is_empty() && w.chars().all(char::is_alphabetic))
}

/// A row of border glyphs with a table's corners or joints in it (`├────┼────┤`): part of an answer.
/// The rules around an input box are plain `─` (or round-cornered), and stay furniture.
fn is_table_rule(t: &str) -> bool {
    t.chars().any(|c| "┌┐└┘├┤┬┴┼".contains(c)) && t.chars().all(|c| BORDER.contains(c))
}

/// `text` without the indent all its lines share (an agent indents its whole answer under its
/// message marker); deeper indents stay, blank lines too.
pub fn dedent(text: &str) -> String {
    let indent = |l: &str| l.len() - l.trim_start_matches(' ').len();
    let cut = text.lines().filter(|l| !l.trim().is_empty()).map(indent).min().unwrap_or(0);
    text.lines().map(|l| l.get(cut.min(indent(l))..).unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// The rows of `rows` that are the agent's words: no blank runs, echo of `prompt`, tool calls,
/// input box or terminal chrome. Applying it twice changes nothing.
pub fn response_rows(rows: &[String], prompt: &str, chrome: &[String]) -> Vec<String> {
    let want: Vec<String> = prompt.lines().map(norm).filter(|l| l.chars().count() >= 8).collect();
    let mut out: Vec<String> = Vec::new();
    for l in rows {
        let t = l.trim();
        if t.is_empty() {
            if out.last().is_some_and(|p| !p.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        if (is_noise(t, chrome) && !is_table_rule(t)) || is_tool_call(t) || is_spinner(t) || t.contains("[Pasted text") {
            continue;
        }
        if matches!(t.chars().next(), Some('❯' | '›')) {
            continue; // the user's own message / input box
        }
        let n = norm(t);
        if n.is_empty() || (n.chars().count() >= 8 && want.iter().any(|w| w.contains(n.as_str()) || n.contains(w.as_str()))) {
            continue;
        }
        // the row as the agent laid it out: its message marker becomes a space, so what stands under
        // it (a table, a nested list, code) keeps its columns ([`dedent`] takes the shared indent off)
        let l = l.trim_end();
        out.push(match l.chars().next() {
            Some(c) if "●⏺•".contains(c) => format!(" {}", &l[c.len_utf8()..]),
            _ => l.to_string(),
        });
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// A program on the alternate screen (Claude Code) keeps its own history and shows the last rows of
/// it: what scrolled out of the window is gone from the terminal. `acc` is that history, rebuilt:
/// each `frame` (its content rows, see [`response_rows`]) is laid over the end of `acc` where the
/// two share rows, and what is new is appended. A row that changed in place (a line still being
/// written, a timer) takes the frame's version. A frame that cannot be placed is left out, never
/// guessed at: a duplicate in a result is worse than a gap that the next frame closes.
pub fn merge_frame(acc: &mut Vec<String>, frame: &[String]) {
    let first = frame.iter().position(|r| !r.is_empty());
    let last = frame.iter().rposition(|r| !r.is_empty());
    let (Some(first), Some(last)) = (first, last) else { return };
    let frame = &frame[first..=last];
    if acc.is_empty() {
        acc.extend(frame.iter().cloned());
        return;
    }
    // the frame's first `m` rows are the last `m` rows of `acc`: the biggest `m` that agrees on
    // enough content rows wins (bigger first: a longer overlap is the stronger evidence)
    let tail = acc[acc.len() - 1].as_str();
    let overlap = (1..=frame.len().min(acc.len())).rev().find(|&m| {
        let base = acc.len() - m;
        let content = frame[..m].iter().filter(|r| !r.is_empty()).count();
        let same = (0..m).filter(|&i| !frame[i].is_empty() && acc[base + i] == frame[i]).count();
        // a line that is still being written counts: its earlier version is a prefix of it
        let grows = !tail.is_empty() && frame[m - 1] != tail && frame[m - 1].starts_with(tail);
        let need = if content <= 2 { content } else { content - content / 4 };
        content > 0 && same + usize::from(grows) >= need
    });
    let Some(m) = overlap else {
        // Its first rows were collected further up and it reaches at least as far down as what was
        // collected after them: the program redrew those rows in place (Claude Code redraws a table
        // for every row it streams). The frame is the newer version of all of them.
        if let Some(p) = redrawn_from(acc, frame) {
            acc.truncate(p);
            acc.extend(frame.iter().cloned());
            return;
        }
        // Nothing in common with the rows collected: the window moved on by more than its height
        // (or the old rows were never on it). With no row in common nothing can be duplicated, so
        // the frame goes on after a gap; one that shares some rows but cannot be placed is left out.
        let known: std::collections::HashSet<&str> = acc.iter().rev().take(frame.len() * 3).map(String::as_str).collect();
        if frame.iter().all(|r| r.is_empty() || !known.contains(r.as_str())) {
            acc.push(String::new());
            acc.extend(frame.iter().cloned());
        }
        return;
    };
    let base = acc.len() - m;
    for i in 0..m {
        if !frame[i].is_empty() {
            acc[base + i] = frame[i].clone();
        }
    }
    acc.extend(frame[m..].iter().cloned());
}

/// Where `frame` starts in `acc`, when its first three rows with words in them (and what lies between)
/// are rows of `acc` and the frame is long enough to stand for everything collected from there on.
fn redrawn_from(acc: &[String], frame: &[String]) -> Option<usize> {
    let head = frame.iter().enumerate().filter(|(_, r)| r.chars().any(char::is_alphanumeric)).nth(2).map(|(i, _)| i + 1)?;
    let from = acc.len().saturating_sub(frame.len());
    (from..=acc.len().checked_sub(head)?).rev().find(|&p| acc[p..p + head] == frame[..head])
}

/// Put a `capture-pane` snapshot (`-e -p -J`) of a `rows`x`cols` pane on a fresh screen, row for
/// row. A line feed after the last row would scroll the whole screen up by one, and a program that
/// repaints by absolute position (Claude Code does) would never put that right. `modes` are the
/// DEC private modes the program had set (1049 alternate screen, 1000-1006 mouse): a capture does
/// not carry them.
pub fn seed_screen(rows: u16, cols: u16, scrollback: usize, snap: &str, cursor: Option<(u16, u16)>, modes: &[u16]) -> Parser {
    let mut p = Parser::new(rows, cols, scrollback);
    for m in modes {
        p.process(format!("\x1b[?{m}h").as_bytes());
    }
    for (i, l) in snap.trim_end_matches('\n').split('\n').enumerate() {
        if i > 0 {
            p.process(b"\r\n");
        }
        p.process(l.as_bytes());
    }
    if let Some((y, x)) = cursor {
        p.process(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    }
    p
}

/// The wheel as the program asked to get it: `n` mouse wheel events (up when `n` < 0) at `row`,
/// `col` (0-based), or None when it did not ask for the mouse and the wheel scrolls weft's own
/// history. Claude Code draws on the alternate screen, where no history is kept: only it can
/// scroll its transcript back.
pub fn wheel_bytes(screen: &Screen, n: i32, row: u16, col: u16) -> Option<Vec<u8>> {
    use vt100::{MouseProtocolEncoding, MouseProtocolMode};
    if screen.mouse_protocol_mode() == MouseProtocolMode::None || n == 0 {
        return None;
    }
    let b: u16 = if n < 0 { 64 } else { 65 };
    let one = match screen.mouse_protocol_encoding() {
        MouseProtocolEncoding::Sgr => format!("\x1b[<{b};{};{}M", col + 1, row + 1).into_bytes(),
        // X10 style: one byte each, offset by 32, so nothing past column 222 can be told apart
        _ => vec![0x1b, b'[', b'M', (32 + b) as u8, (33 + col.min(222)) as u8, (33 + row.min(222)) as u8],
    };
    Some(one.repeat(n.unsigned_abs().min(50) as usize))
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
    fn finds_where_a_wrapped_message_was_echoed() {
        let typed = "recheck this is working. increase the font size by 20%.\n\n[weft] End with the STATUS line: <WORD> is DONE, ISSUE (add your question), WAITING (add what for) or FAILED (add why).";
        let lines = v(&[
            "older answer",
            "\u{276f} recheck this is working. increase the font size by 20%.",
            "  [weft] End with the STATUS line: <WORD> is DONE, ISSUE (add your question), WAITING (add what for) or",
            "  FAILED (add why).",
            "\u{25cf} Rechecked.",
            "  STATUS: DONE",
        ]);
        assert_eq!(echo_end(&lines, &probe_of(typed)), Some(3));
        assert_eq!(echo_end(&lines, &probe_of("something that was never typed here")), None);
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

    #[test]
    fn a_dialog_is_told_from_its_words_in_what_an_agent_shows() {
        let claude = v(&["Yes, I trust this folder", "Do you want to proceed?", "Yes, I accept"]);
        let codex = v(&["Would you like to run the following command", "Update now"]);
        let agy = v(&["Do you trust the contents of this project"]);
        // the real dialogs, as Claude Code, Codex and agy draw them
        let trust = "Quick safety check: do you trust this folder?\n\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel";
        assert!(dialog_on(trust, &claude));
        // Claude Code 2.1.302 numbers nothing and highlights "No, exit": weft took it for no dialog,
        // typed the task, and its Enter picked "No, exit"
        let claude_now = crate::model::Config::default().agents["claude"].attention.clone();
        let trust_now = "─────────\n Accessing workspace:\n\n /home/piet/CAD/cad-project\n\n Quick safety check: Is this a project you \
                         created or one you trust? (Like your own code, a well-known open\n source project, or work from your team). \
                         If not, take a moment to review what's in this folder first.\n\n Claude Code'll be able to read, edit, and \
                         execute files here.\n\n Security guide\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · \
                         Esc to cancel\n\n\n";
        assert!(dialog_on(trust_now, &claude_now));
        let perm = "● Bash(rm -rf build)\n\n Bash command\n   rm -rf build\n Do you want to proceed?\n ❯ 1. Yes\n   2. No, and tell Claude what to do differently (esc)";
        assert!(dialog_on(perm, &claude));
        let update = "✨ Update available! 0.155 -> 0.162\n\n› 1. Update now (runs `npm install -g @openai/codex`)\n  2. Skip\n  3. Skip until next version\n\n  Press enter to continue";
        assert!(dialog_on(update, &codex));
        let agy_trust = "Do you trust the contents of this project?\nAntigravity CLI requires permission to read, edit, and execute files here.\n> Yes, I trust this folder\n  No, exit\n  ↑/↓ Navigate · enter Confirm";
        assert!(dialog_on(agy_trust, &agy));
        assert!(dialog_on("Add src/a.py to the chat? (Y)es/(N)o [Yes]: ", &v(&["(Y)es/(N)o"])));
        // weft's own source on an agent's screen, then its idle input box: no dialog
        let source = "● Update(src/model.rs)\n  477 -  attention: strings(&[\"Yes, I trust this folder\", \"Do you want to proceed?\"]),\n\n● Done. The patterns are unchanged.\n\n✻ Baked for 4m 15s · done 19:07\n\n──────────────\n❯ install it\n──────────────\n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert!(!dialog_on(source, &claude));
        // a numbered list under the words in an agent's message, long scrolled up: no dialog either
        let old = format!("It asks \"Do you want to proceed?\" and offers:\n1. Yes\n2. No\n{}❯ \n", "more text\n".repeat(25));
        assert!(!dialog_on(&old, &claude));
    }

    #[test]
    fn claude_asking_with_its_own_question_tool_is_a_dialog() {
        // Claude Code 2.1.302's AskUserQuestion, as it drew it: typing weft's reminder into it picked
        // "1. Red" for the person, and the agent went on as if they had answered
        let claude = crate::model::Config::default().agents["claude"].attention.clone();
        let one = "❯ Use your AskUserQuestion tool to ask me.\n────────\n ☐ Color\n\nWhich color do you prefer?\n\n❯ 1. Red\n     Choose \
                   red.\n  2. Blue\n     Choose blue.\n  3. Type something.\n────────\n  4. Chat about this\n\nEnter to select · ↑/↓ to \
                   navigate · Esc to cancel\n\n\n";
        assert!(dialog_on(one, &claude));
        let tabs = "←  ☐ Size  ☐ Fruits  ✔ Submit  →\n\nWhich size do you want?\n\n❯ 1. Small\n  2. Large\n  3. Type something.\n\
                    ────────\n  4. Chat about this\n\nEnter to select · Tab/Arrow keys to navigate · Esc to cancel";
        assert!(dialog_on(tabs, &claude));
        let review = "←  ☒ Size  ☐ Fruits  ✔ Submit  →\n\nReview your answers\n\n ● Which size do you want?\n   → Large\n\nReady to \
                      submit your answers?\n\n❯ 1. Submit answers\n  2. Cancel";
        assert!(dialog_on(review, &claude));
        // once answered, the transcript keeps no footer: the agent's idle input box is no dialog
        let after = "● User answered Claude's questions:\n  ⎿  · Which color do you prefer? → Red\n\n● You prefer red.\n\n✻ Brewed \
                     for 3s · done 23:14\n\n────────\n❯ \n────────\n  ⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert!(!dialog_on(after, &claude));
    }

    fn rows_of(p: &Parser) -> Vec<String> {
        let (_, cols) = p.screen().size();
        p.screen().rows(0, cols).map(|r| r.trim_end().to_string()).collect()
    }

    #[test]
    fn the_wheel_goes_to_a_program_that_reads_the_mouse() {
        let mut p = Parser::new(10, 40, 100);
        assert_eq!(wheel_bytes(p.screen(), -1, 3, 5), None, "no mouse: the wheel scrolls weft's history");
        p.process(b"\x1b[?1049h\x1b[?1003h\x1b[?1006h");
        assert_eq!(wheel_bytes(p.screen(), -2, 3, 5).unwrap(), b"\x1b[<64;6;4M\x1b[<64;6;4M");
        assert_eq!(wheel_bytes(p.screen(), 1, 0, 0).unwrap(), b"\x1b[<65;1;1M");
        p.process(b"\x1b[?1006l");
        assert_eq!(wheel_bytes(p.screen(), -1, 0, 0).unwrap(), [0x1b, b'[', b'M', 96, 33, 33]);
        // a restart seeds the screen from tmux: the mouse modes come with it
        let q = seed_screen(10, 40, 100, "x\n", None, &[1049, 1003, 1006]);
        assert_eq!(wheel_bytes(q.screen(), 1, 0, 0).unwrap(), b"\x1b[<65;1;1M");
    }

    #[test]
    fn a_seeded_screen_has_every_row_where_the_pane_has_it() {
        // a full-height snapshot: the line feed after its last row must not scroll anything
        let snap: String = (0..8).map(|i| format!("row {i}\n")).collect();
        let p = seed_screen(8, 20, 100, &snap, Some((7, 3)), &[1049]);
        let got = rows_of(&p);
        for i in 0..8 {
            assert_eq!(got[i], format!("row {i}"), "row {i} stays on row {i}");
        }
        assert_eq!(p.screen().cursor_position(), (7, 3));
        assert!(p.screen().alternate_screen());
        // a snapshot with history above the visible rows: the last rows are the screen
        let snap: String = (0..12).map(|i| format!("line {i}\n")).collect();
        let p = seed_screen(8, 20, 100, &snap, None, &[]);
        assert_eq!(rows_of(&p)[0], "line 4");
        assert_eq!(rows_of(&p)[7], "line 11");
    }

    fn s(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn frames_of_a_window_that_scrolls_rebuild_the_whole_answer() {
        let all: Vec<String> = (1..=12).map(|i| format!("answer line number {i}")).collect();
        let mut acc = Vec::new();
        // a window of 5 rows follows the end: 1-5, 2-6 (one row), 4-8 (two rows), ...
        for (a, b) in [(0, 5), (1, 6), (3, 8), (4, 9), (7, 12)] {
            merge_frame(&mut acc, &all[a..b]);
        }
        assert_eq!(acc, all, "the head is kept although it left the window");
        // the same frame again changes nothing
        merge_frame(&mut acc, &all[7..12]);
        assert_eq!(acc, all);
    }

    #[test]
    fn a_line_still_being_written_and_a_changed_row_are_taken_in_place() {
        let mut acc = s(&["intro", "second line", "third line is ge"]);
        merge_frame(&mut acc, &s(&["intro", "second line", "third line is getting longer", "fourth"]));
        assert_eq!(acc, s(&["intro", "second line", "third line is getting longer", "fourth"]));
        // a timer row in the middle changed and the window moved by one
        let mut acc = s(&["a one", "b two", "Running (3s)", "d four", "e five"]);
        merge_frame(&mut acc, &s(&["b two", "Running (4s)", "d four", "e five", "f six"]));
        assert_eq!(acc, s(&["a one", "b two", "Running (4s)", "d four", "e five", "f six"]));
    }

    #[test]
    fn a_frame_that_cannot_be_placed_is_left_out_unless_it_shares_nothing() {
        let mut acc = s(&["one", "two", "three", "four", "five"]);
        // it shares rows with the end, but not in an order that fits: left out, nothing guessed
        merge_frame(&mut acc, &s(&["two", "four", "other", "rows"]));
        assert_eq!(acc, s(&["one", "two", "three", "four", "five"]));
        merge_frame(&mut acc, &[]);
        merge_frame(&mut acc, &s(&["", ""]));
        assert_eq!(acc.len(), 5);
        // it shares nothing: the window jumped, nothing can be doubled, it goes on after a gap
        merge_frame(&mut acc, &s(&["something else", "entirely different"]));
        assert_eq!(acc, s(&["one", "two", "three", "four", "five", "", "something else", "entirely different"]));
        // and the next frame of that window is placed on it again
        merge_frame(&mut acc, &s(&["entirely different", "and more"]));
        assert_eq!(acc.len(), 9);
        assert_eq!(acc[8], "and more");
    }

    #[test]
    fn a_table_redrawn_in_place_is_kept_once() {
        // Claude Code redraws a table for every row it streams, wider as the cells grow
        let text = ["Here are the planets.", "Three of them, by size.", "Sorted by moons:", ""];
        let v1 = ["┌──────┬───┐", "│ Earth │ 1 │", "└──────┴───┘"];
        let v2 = ["┌─────────┬────┐", "│ Earth   │ 1  │", "├─────────┼────┤", "│ Jupiter │ 95 │", "└─────────┴────┘"];
        let mut acc = s(&["intro line", "more intro"]);
        acc.extend(s(&text));
        acc.extend(s(&v1));
        let mut frame = s(&text);
        frame.extend(s(&v2));
        merge_frame(&mut acc, &frame);
        let mut want = s(&["intro line", "more intro"]);
        want.extend(s(&text));
        want.extend(s(&v2));
        assert_eq!(acc, want, "the newer table replaces the older one");
        // a frame from further up (the transcript scrolled back) does not cut what came after it
        merge_frame(&mut acc, &s(&["intro line", "more intro", "Here are the planets."]));
        assert_eq!(acc, want);
    }

    #[test]
    fn the_spinner_is_not_part_of_the_answer() {
        let rows = s(&["· Actualizing… (1s · ↓ 25 tokens · thinking with low effort)", "✶ Pondering…", "● The answer.", "· a dot list item… with words"]);
        assert_eq!(response_rows(&rows, "a question", &[]), s(&["  The answer.", "· a dot list item… with words"]));
    }

    #[test]
    fn an_answer_keeps_its_tables_and_indents() {
        // Claude Code 2.1.300 drawing a markdown table, a nested list and code (a real screen, cut narrower)
        let after = s(&[
            "❯ Reply with a table of planets",
            "",
            "● ┌─────────┬───────┐",
            "  │ Planet  │ Moons │",
            "  ├─────────┼───────┤",
            "  │ Earth   │ 1     │",
            "  └─────────┴───────┘",
            "",
            "  - Planets",
            "    - Inner, rocky",
            "",
            "  moons = {\"Earth\": 1}",
            "",
            "✻ Crunched for 6s · done 17:13",
            "──────────────────────",
            "❯",
            "──────────────────────",
            "  ⏵⏵ bypass permissions on (shift+tab to cycle)",
        ]);
        let chrome = s(&["bypass permissions"]);
        let want = "┌─────────┬───────┐\n│ Planet  │ Moons │\n├─────────┼───────┤\n│ Earth   │ 1     │\n└─────────┴───────┘\n\n- Planets\n  - Inner, rocky\n\nmoons = {\"Earth\": 1}";
        assert_eq!(extract_response(&[], &after, "Reply with a table of planets", &chrome), want);
        let rows = response_rows(&after, "Reply with a table of planets", &chrome);
        assert_eq!(response_rows(&rows, "Reply with a table of planets", &chrome), rows, "applying it again changes nothing");
        assert_eq!(dedent("  a\n\n    b\n  c"), "a\n\n  b\nc");
    }

    #[test]
    fn response_rows_is_what_extract_response_keeps() {
        let after = s(&["❯ do the thing", "", "● Read(a.rs)", "The thing is done.", "", "", "esc to interrupt", "It took two steps."]);
        let chrome = s(&["esc to interrupt"]);
        let rows = response_rows(&after, "do the thing", &chrome);
        assert_eq!(rows, s(&["The thing is done.", "", "It took two steps."]));
        assert_eq!(extract_response(&[], &after, "do the thing", &chrome), rows.join("\n"));
        assert_eq!(response_rows(&rows, "do the thing", &chrome), rows, "applying it again changes nothing");
    }
}
