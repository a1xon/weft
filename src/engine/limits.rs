//! An agent that ran into its usage limit sleeps until the limit resets, then carries on. Its card
//! says until when, the cards after it wait, and the CLI's limit message is never taken for its
//! result (before, the silence after it settled the card as done and the links handed the message
//! on to agents that often hit the same limit).

use super::*;

/// What the CLI said about the limit, and when weft wakes the agent.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Limit {
    /// when weft types the "carry on" (ms since the epoch)
    pub(super) until: u64,
    /// the lines the CLI showed
    pub(super) said: String,
    /// the reset time was read from them (else weft only tries again after a while)
    pub(super) known: bool,
}

/// How often one step sleeps off a limit before the person is asked.
const MAX_SLEEPS: u32 = 30;
/// Margin after the time the CLI names: it rounds to the minute, and the clocks may differ a little.
const AFTER_RESET_MS: u64 = 90_000;
/// No sleep is longer than this (a weekly limit is the longest there is).
const LONGEST_MS: u64 = 8 * 86_400_000;

impl Engine {
    /// The step's output ends with the CLI saying it ran into its usage limit: those lines. A step
    /// that gave its status line finished the normal way, whatever it talked about.
    pub(super) fn limit_hit(&mut self, id: &str) -> Option<String> {
        let agent = self.node(id).filter(|n| n.kind() == Kind::Agent)?.agent.clone();
        let pats = self.cfg.agents.get(&agent).map(|a| a.limits.clone()).unwrap_or_default();
        if pats.is_empty() {
            return None;
        }
        let chrome = &self.cfg.chrome;
        let rt = self.rt.get_mut(id)?;
        let said = match rt.cur.as_ref() {
            Some(c) => {
                let said = run::new_since(&mut rt.parser, &c.before, &c.typed(), chrome)?;
                if c.rule && words::read_status(&said, c.asked).is_some() {
                    return None;
                }
                said
            }
            None => rt.parser.screen().contents(),
        };
        limit_line(&said, &pats)
    }

    /// Sleep through the limit: nothing is judged and nothing is typed into the agent until it resets.
    pub(super) fn limit_sleep(&mut self, id: &str, said: String) {
        let (now, off) = (now_ms(), local_offset());
        let at = reset_at(&said, now, off);
        let retry = (self.cfg.limit_retry_s.max(60.0) * 1000.0) as u64;
        let until = at.map_or(now + retry, |t| t.max(now) + AFTER_RESET_MS).min(now + LONGEST_MS);
        let name = self.node(id).map(|n| n.name.clone()).unwrap_or_default();
        let Some(rt) = self.rt.get_mut(id) else { return };
        rt.limit_sleeps += 1;
        if rt.limit_sleeps > MAX_SLEEPS {
            rt.limit_sleeps = 0;
            self.raise_issue(id, format!("It is still at its usage limit after {MAX_SLEEPS} tries. The CLI says: {said}"));
            return;
        }
        rt.limit = Some(Limit { until, said, known: at.is_some() });
        rt.ui = true;
        self.state_dirty = true;
        let when = if at.is_some() { "resumes" } else { "tries again" };
        self.toast("warn", &format!("{name} hit its usage limit \u{b7} {when} {}", local_clock(until, now, off)));
    }

    /// Agents whose limit has reset are told to carry on.
    pub(super) fn tick_limits(&mut self) {
        let now = now_ms();
        let due: Vec<String> =
            self.rt.iter().filter(|(_, r)| r.limit.as_ref().is_some_and(|l| l.until <= now)).map(|(i, _)| i.clone()).collect();
        for id in due {
            self.limit_wake(&id);
        }
    }

    /// The limit reset (or the person says try now): the agent carries on with its step.
    pub(super) fn limit_wake(&mut self, id: &str) {
        let exe = self.boot.exe.clone();
        let Some(rt) = self.rt.get_mut(id) else { return };
        if rt.limit.take().is_none() {
            return;
        }
        rt.ui = true;
        self.state_dirty = true;
        if !rt.alive {
            return;
        }
        let (signal, rule) = rt.cur.as_ref().map_or((false, false), |c| (c.expect_signal, c.rule));
        self.raw_type(id, &words::limit_resume(&exe, signal, rule), true);
    }

    pub(super) fn cmd_limit_wake(&mut self, id: String) -> Value {
        if self.rt.get(&id).is_none_or(|r| r.limit.is_none()) {
            return json!({"ok": false, "error": "it is not waiting for a usage limit"});
        }
        self.limit_wake(&id);
        json!({"ok": true})
    }

    /// A fresh agent takes over from one that sleeps at its limit (it cannot write a note): the
    /// sleeper ends, so it never wakes up in the same files, and its links go to the new agent,
    /// which carries the flow on in its place.
    pub(super) fn limit_hand_over(&mut self, to: &str, from: &str) {
        let mut moved = 0;
        for e in self.flow.edges.iter_mut().filter(|e| e.from == from && e.to != to) {
            e.from = to.to_string();
            moved += 1;
        }
        if let Some(rt) = self.rt.get_mut(from) {
            rt.limit = None;
            rt.unsent.clear();
        }
        self.cmd_stop(from.to_string());
        if moved > 0 {
            self.flow_changed();
        }
    }
}

/// The lines in which the CLI says it hit its usage limit, among the last lines of what it showed,
/// with the next two (the reset time can wrap onto them).
pub(super) fn limit_line(text: &str, pats: &[String]) -> Option<String> {
    let low = |s: &str| s.to_lowercase().replace('\u{2019}', "'");
    let pats: Vec<String> = pats.iter().filter(|p| !p.is_empty()).map(|p| low(p)).collect();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = &lines[lines.len().saturating_sub(8)..];
    let at = tail.iter().position(|l| {
        let l = low(l);
        // Claude Code's fast mode has a limit of its own: it then goes on at the normal speed
        !l.contains("fast limit") && pats.iter().any(|p| l.contains(p.as_str()))
    })?;
    Some(tail[at..].iter().take(3).copied().collect::<Vec<_>>().join(" "))
}

/// When the limit resets, read from the CLI's own words: "resets 3pm (Europe/Berlin)", "resets Oct 20,
/// 9am", "try again at Oct 18th, 2026 3:04 PM", "try again in 2 hours 13 minutes", "resets in 4h 13m",
/// or an epoch after a bar ("usage limit reached|1760000000"). Times are local (`off_s` east of UTC).
pub(super) fn reset_at(text: &str, now_ms: u64, off_s: i64) -> Option<u64> {
    let t = text.to_lowercase().replace('\u{2019}', "'");
    if let Some(i) = t.find('|') {
        let d: String = t[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if d.len() == 10 {
            return d.parse::<u64>().ok().map(|s| s * 1000);
        }
    }
    for key in ["resets", "reset at", "reset after", "reset in", "try again", "available again", "until"] {
        let mut from = 0;
        while let Some(p) = t[from..].find(key) {
            let rest = &t[from + p + key.len()..];
            if let Some(ms) = when(rest, now_ms, off_s) {
                return Some(ms);
            }
            from += p + key.len();
        }
    }
    None
}

/// What follows "resets" / "try again": a span ("in 2h 13m") or a moment ("at 3:04 pm", "oct 20, 9am").
fn when(rest: &str, now_ms: u64, off_s: i64) -> Option<u64> {
    let words: Vec<String> = rest
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|w| w.trim_matches(|c: char| matches!(c, '.' | ';' | ':' | '(' | ')' | '\u{b7}' | '!')).to_string())
        .filter(|w| !w.is_empty())
        .take(10)
        .collect();
    let mut i = 0;
    while words.get(i).is_some_and(|w| matches!(w.as_str(), "at" | "on" | "to")) {
        i += 1;
    }
    if words.get(i).is_some_and(|w| matches!(w.as_str(), "in" | "after")) {
        return span(&words[i + 1..]).map(|s| now_ms + s * 1000);
    }
    moment(&words[i..], now_ms, off_s)
}

/// "2 hours 13 minutes", "4h 13m", "1d 2h", "90 seconds": seconds.
fn span(words: &[String]) -> Option<u64> {
    let unit = |u: &str| -> Option<u64> {
        Some(match u {
            "d" | "day" | "days" => 86_400,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            _ => return None,
        })
    };
    let (mut total, mut i) = (0u64, 0);
    while i < words.len() {
        let w = words[i].as_str();
        if w == "and" {
            i += 1;
            continue;
        }
        let digits: String = w.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        let Ok(n) = digits.parse::<f64>() else { break };
        let attached = &w[digits.len()..];
        let (u, step) = if attached.is_empty() { (words.get(i + 1).map_or("", |s| s.as_str()), 2) } else { (attached, 1) };
        // "4h13m" in one word
        if let Some(p) = attached.find(|c: char| c.is_ascii_digit()) {
            let (a, b) = attached.split_at(p);
            total += (n * unit(a)? as f64) as u64;
            total += span(&[b.to_string()]).unwrap_or(0);
            i += 1;
            continue;
        }
        let Some(mul) = unit(u) else { break };
        total += (n * mul as f64) as u64;
        i += step;
    }
    (total > 0).then_some(total)
}

/// "3pm", "3:04 pm", "15:00", "oct 20 9am", "oct 18th 2026 3:04 pm", "mon 9am", "tomorrow at 3pm".
fn moment(words: &[String], now_ms: u64, off_s: i64) -> Option<u64> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    let local = now_ms as i64 / 1000 + off_s;
    let today = local.div_euclid(86_400);
    let (mut day, mut dated, mut i) = (today, false, 0);
    let (mut month, mut mday, mut year) = (None, None, None);
    while let Some(w) = words.get(i) {
        let w3 = w.get(..3).unwrap_or(w);
        if w == "tomorrow" {
            day = today + 1;
            dated = true;
        } else if w == "today" || w == "at" {
        } else if let Some(m) = MONTHS.iter().position(|m| *m == w3).filter(|_| w.chars().all(|c| c.is_ascii_alphabetic())) {
            month = Some(m as u32 + 1);
        } else if let Some(d) = DAYS.iter().position(|d| *d == w3).filter(|_| w.chars().all(|c| c.is_ascii_alphabetic())) {
            day = today + (d as i64 - (today + 4).rem_euclid(7) + 7) % 7;
            dated = true;
        } else if month.is_some() && mday.is_none() && w.trim_end_matches(['s', 't', 'n', 'd', 'r', 'h']).parse::<u32>().is_ok_and(|d| (1..=31).contains(&d)) {
            mday = w.trim_end_matches(['s', 't', 'n', 'd', 'r', 'h']).parse().ok();
        } else if w.len() == 4 && w.parse::<i64>().is_ok_and(|y| (2000..2200).contains(&y)) {
            year = w.parse().ok();
        } else {
            break;
        }
        i += 1;
    }
    let (h, m) = clock_of(words.get(i)?, words.get(i + 1).map(|s| s.as_str()))?;
    if let (Some(mo), Some(d)) = (month, mday) {
        let (y_now, _, _) = civil(today);
        let y = year.unwrap_or(y_now);
        day = days_from_civil(y, mo, d);
        // "Jan 3" said in December is next year's
        if year.is_none() && day < today - 1 {
            day = days_from_civil(y + 1, mo, d);
        }
        dated = true;
    }
    let mut at = day * 86_400 + h as i64 * 3_600 + m as i64 * 60 - off_s;
    // a time of day that has passed is tomorrow's; one a few minutes ago is now (the CLI rounds it,
    // and the screen is read a moment after it said it)
    if !dated && at * 1000 <= now_ms as i64 - 600_000 {
        at += 86_400;
    }
    (at > 0).then(|| at as u64 * 1000)
}

/// "3pm" / "3:04pm" / "3:04" + "pm" / "15:00": hours and minutes. A bare number is no time.
fn clock_of(w: &str, next: Option<&str>) -> Option<(u32, u32)> {
    let (mut body, mut half) = (w.to_string(), None);
    for (suffix, pm) in [("a.m", false), ("p.m", true), ("am", false), ("pm", true)] {
        if let Some(b) = w.strip_suffix(suffix) {
            body = b.to_string();
            half = Some(pm);
            break;
        }
    }
    if half.is_none() {
        half = match next {
            Some("am" | "a.m") => Some(false),
            Some("pm" | "p.m") => Some(true),
            _ => None,
        };
    }
    let (hs, ms) = body.split_once(':').unwrap_or((&body, ""));
    if half.is_none() && ms.is_empty() {
        return None;
    }
    let h: u32 = hs.parse().ok()?;
    let m: u32 = if ms.is_empty() { 0 } else { ms.parse().ok()? };
    let h = match half {
        Some(_) if h == 0 || h > 12 => return None,
        Some(pm) => h % 12 + if pm { 12 } else { 0 },
        None => h,
    };
    (h < 24 && m < 60).then_some((h, m))
}

/// Days since 1970-01-01 of a date (proleptic Gregorian).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Seconds east of UTC here and now (`date +%z`: GNU and BSD both have it); 0 when it cannot be told.
fn local_offset() -> i64 {
    let out = std::process::Command::new("date").arg("+%z").output().ok().filter(|o| o.status.success());
    let z = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let (sign, d) = match z.split_at_checked(1) {
        Some(("-", d)) => (-1, d),
        Some(("+", d)) => (1, d),
        _ => return 0,
    };
    match (d.get(..2).and_then(|h| h.parse::<i64>().ok()), d.get(2..4).and_then(|m| m.parse::<i64>().ok())) {
        (Some(h), Some(m)) => sign * (h * 3_600 + m * 60),
        _ => 0,
    }
}

/// "15:02", or "Oct 20 09:00" when it is more than a day away: for the toast.
fn local_clock(ms: u64, now_ms: u64, off_s: i64) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let s = ms as i64 / 1000 + off_s;
    let (h, m) = (s.rem_euclid(86_400) / 3_600, s.rem_euclid(3_600) / 60);
    if ms.saturating_sub(now_ms) < 20 * 3_600_000 {
        format!("{h:02}:{m:02}")
    } else {
        let (_, mo, d) = civil(s.div_euclid(86_400));
        format!("{} {d} {h:02}:{m:02}", MONTHS[mo as usize - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    /// 2026-10-17 12:00:00 in Berlin (UTC+2)
    const NOON: u64 = 1_792_231_200_000;
    const BERLIN: i64 = 7_200;

    fn at(text: &str) -> Option<String> {
        reset_at(text, NOON, BERLIN).map(|ms| local_clock(ms, 0, BERLIN))
    }

    #[test]
    fn reset_times_are_read_the_way_each_cli_writes_them() {
        assert_eq!((days_from_civil(2026, 10, 17) * 86_400_000 + 10 * 3_600_000) as u64, NOON, "the test clock itself");
        // Claude Code
        assert_eq!(at("You've hit your session limit \u{b7} resets 3pm (Europe/Berlin)").as_deref(), Some("Oct 17 15:00"));
        assert_eq!(at("You've hit your limit \u{b7} resets 3:30pm").as_deref(), Some("Oct 17 15:30"));
        assert_eq!(at("You\u{2019}ve hit your weekly limit \u{b7} resets Oct 20, 9am (Europe/Berlin)").as_deref(), Some("Oct 20 09:00"));
        // a time that has passed today is tomorrow's, one a few minutes ago is now
        assert_eq!(at("resets 9am").as_deref(), Some("Oct 18 09:00"));
        assert_eq!(at("resets 11:55am").as_deref(), Some("Oct 17 11:55"));
        // Codex
        assert_eq!(at("You've hit your usage limit. Upgrade to Plus (https://chatgpt.com/explore/plus), or try again at 3:04 PM.").as_deref(), Some("Oct 17 15:04"));
        assert_eq!(at("You've hit your usage limit. Try again at Oct 18th, 2026 3:04 PM.").as_deref(), Some("Oct 18 15:04"));
        assert_eq!(at("You've hit your usage limit. Try again in 2 hours 13 minutes.").as_deref(), Some("Oct 17 14:13"));
        assert_eq!(at("quota resets in 4h 13m").as_deref(), Some("Oct 17 16:13"));
        assert_eq!(at("resets in 1d2h").as_deref(), Some("Oct 18 14:00"));
        // 24 hours, a weekday, an epoch
        assert_eq!(at("resets at 18:45").as_deref(), Some("Oct 17 18:45"));
        assert_eq!(at("limit resets Mon 9am").as_deref(), Some("Oct 19 09:00"));
        assert_eq!(at("Claude AI usage limit reached|1792256400").as_deref(), Some("Oct 17 19:00"));
        // no time in it: weft tries again after a while
        assert_eq!(at("You've hit your usage limit. Visit https://chatgpt.com/settings/usage to purchase more credits"), None);
        assert_eq!(at("resets 3 (soon)"), None);
    }

    #[test]
    fn only_the_last_lines_and_never_the_fast_mode_limit_count() {
        let pats: Vec<String> = ["you've hit your", "usage limit reached"].iter().map(|s| s.to_string()).collect();
        let said = "\u{25cf} Working on it.\n\n  \u{23bf} You've hit your session limit \u{b7} resets 3pm\n    (Europe/Berlin)\n\n\u{276f} \n";
        assert_eq!(limit_line(said, &pats).as_deref(), Some("\u{23bf} You've hit your session limit \u{b7} resets 3pm (Europe/Berlin) \u{276f}"));
        assert_eq!(limit_line("You've hit your fast limit \u{b7} resets in 8m", &pats), None);
        // a mention far up in what it said is not the CLI stopping
        let long = format!("The CLI says 'usage limit reached' when it stops.\n{}", "more\n".repeat(12));
        assert_eq!(limit_line(&long, &pats), None);
    }

    /// An agent in a step of a chain whose CLI just said it hit its limit.
    fn limited(e: &mut Engine, said: &str) {
        alive_agent(e, "a", "build");
        e.flow.nodes.iter_mut().find(|n| n.id == "a").unwrap().agent = "claude".into();
        add(e, "b", "next", "{{inputs}}");
        link(e, "e1", "a", "b", On::Done);
        let r = e.rt.get_mut("a").unwrap();
        r.armed = true;
        r.status = Status::Busy;
        r.cause = Cause::Flow;
        r.cur = Some(Step { rule: true, ..Step::new("go".into(), vec![]) });
        r.parser.process(format!("\x1b[2J\x1b[H\u{25cf} Editing src/app.rs\r\n{said}\r\n").as_bytes());
        r.dirty = true;
    }

    #[test]
    fn a_limit_is_slept_off_and_never_handed_on_as_a_result() {
        let mut e = engine();
        limited(&mut e, "  \u{23bf} You've hit your session limit \u{b7} resets 3pm (Europe/Berlin)");
        let t0 = Instant::now();
        e.eval("a", t0);
        e.eval("a", t0 + Duration::from_secs(30));
        let r = &e.rt["a"];
        assert!(!r.finished, "the limit message is no result");
        assert!(r.limit.as_ref().is_some_and(|l| l.known && l.said.contains("resets 3pm")));
        assert!(!r.cur.as_ref().unwrap().asked, "nobody types the missing-status question into a limited CLI");
        assert!(e.rt["b"].inputs.is_empty() && e.rt["b"].queue.is_empty(), "nothing went down the link");
        // while it sleeps a queued prompt waits too: the CLI would drop it
        e.rt.get_mut("a").unwrap().queue.push_back(QItem { id: 9, text: "also".into(), shown: "also".into(), src: Src::You });
        e.eval("a", t0 + Duration::from_secs(600));
        assert_eq!(e.rt["a"].queue.len(), 1);
        assert!(!e.rt["a"].finished);
        // the reset time is due: it is woken (no tmux here, so only the state shows it) and judged again
        e.rt.get_mut("a").unwrap().limit.as_mut().unwrap().until = 1;
        e.tick_limits();
        assert!(e.rt["a"].limit.is_none());
        // a restart keeps the sleep
        e.rt.get_mut("a").unwrap().limit = Some(Limit { until: u64::MAX, said: "x".into(), known: false });
        let mut back = Rt::new(80, 24);
        back.restore(e.rt["a"].to_saved());
        assert_eq!(back.limit.map(|l| l.until), Some(u64::MAX));
    }

    #[test]
    fn a_step_that_reported_is_done_whatever_it_said_about_limits() {
        let mut e = engine();
        limited(&mut e, "I made weft read \"You've hit your limit \u{b7} resets 3pm\".\r\nSTATUS: DONE");
        let t0 = Instant::now();
        e.eval("a", t0);
        e.eval("a", t0 + Duration::from_secs(30));
        assert!(e.rt["a"].finished && e.rt["a"].limit.is_none());
        assert_eq!(e.rt["b"].inputs.len() + e.rt["b"].queue.len(), 1, "the result went on");
    }

    #[tokio::test]
    async fn handing_over_from_a_sleeper_ends_it_and_gives_its_links_to_the_new_agent() {
        let mut e = engine();
        limited(&mut e, "You've hit your session limit \u{b7} resets 3pm");
        let t0 = Instant::now();
        e.eval("a", t0);
        e.eval("a", t0 + Duration::from_secs(30));
        assert!(e.rt["a"].limit.is_some());
        let r = e.cmd_takeover("a".into(), json!({"agent": "codex"}), String::new());
        let new = r["id"].as_str().unwrap().to_string();
        assert!(e.rt["a"].limit.is_none() && e.rt["a"].status == Status::Stopped, "the sleeper never wakes in the same files");
        assert!(e.flow.edges.iter().any(|x| x.from == new && x.to == "b"), "the new agent carries the flow on");
        assert!(!e.flow.edges.iter().any(|x| x.from == "a"));
    }
}
