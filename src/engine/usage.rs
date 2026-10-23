//! How much of each agent's plan is used: the meter beside the agent in the bar. Each CLI tells it
//! without a model request (`usage_cmd`: claude and agy answer `-p /usage` themselves, codex's own
//! session logs carry its limits), and it is asked at most every 15 minutes while a page shows that
//! agent. The answer is kept in a file every board reads, so several boards do not ask again.

use super::*;
use std::collections::HashSet;

/// An answer younger than this is shown as it is: the CLI is not asked again.
const FRESH_MS: u64 = 15 * 60_000;
/// Asking again by hand (a click on the meter) waits at least this long after the last answer.
const AGAIN_MS: u64 = 10_000;
/// A CLI that has not told it by then is given up.
const TIMEOUT: Duration = Duration::from_secs(60);

/// One limit: how much of it is used (percent), when it starts over (ms) and which models it
/// counts (lower-case words, "" = all of them).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(super) struct Bar {
    pub name: String,
    pub used: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets: Option<u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
}

/// What an agent's CLI said last, and when (ms).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(super) struct Usage {
    pub at: u64,
    pub bars: Vec<Bar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Default)]
pub(super) struct Usages {
    got: HashMap<String, Usage>,
    busy: HashSet<String>,
    /// shared by every board (`None` in tests)
    file: Option<PathBuf>,
}

impl Usages {
    pub(super) fn new(file: Option<PathBuf>) -> Usages {
        Usages { file, ..Usages::default() }
    }

    pub(super) fn json(&self) -> Value {
        json!(self.got)
    }

    /// What another board found out meanwhile is taken over.
    fn read_file(&mut self) {
        let Some(f) = &self.file else { return };
        let Ok(m) = std::fs::read(f).map(|b| serde_json::from_slice::<HashMap<String, Usage>>(&b).unwrap_or_default()) else { return };
        for (a, u) in m {
            if self.got.get(&a).is_none_or(|g| g.at < u.at) {
                self.got.insert(a, u);
            }
        }
    }

    fn write_file(&self) {
        let Some(f) = &self.file else { return };
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, json!(self.got).to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, f);
        }
    }
}

impl Engine {
    /// The usage of an agent's plan: the last answer while it is fresh, else the CLI is asked (the
    /// answer reaches every page as a `usage` message). `again`: a click on the meter.
    pub(super) fn cmd_usage(&mut self, agent: String, again: bool) -> Value {
        let Some(cmd) = self.cfg.agents.get(&agent).map(|a| a.usage_cmd.trim().to_string()) else {
            return json!({"ok": false, "error": "unknown agent"});
        };
        if cmd.is_empty() {
            return json!({"ok": true, "usage": null});
        }
        self.usage.read_file();
        let have = self.usage.got.get(&agent).cloned();
        let age = have.as_ref().map_or(u64::MAX, |u| now_ms().saturating_sub(u.at));
        if age < if again { AGAIN_MS } else { FRESH_MS } {
            return json!({"ok": true, "usage": have});
        }
        if self.usage.busy.insert(agent.clone()) {
            let tx = self.tx.clone();
            let a = agent.clone();
            tokio::spawn(async move {
                let out = run_oneshot(&cmd, "WEFT_AGENT", &a, TIMEOUT).await;
                let _ = tx.send(Ev::Usage { agent: a, out });
            });
        }
        json!({"ok": true, "pending": true, "usage": have})
    }

    pub(super) fn on_usage(&mut self, agent: String, out: Option<String>) {
        self.usage.busy.remove(&agent);
        let now = now_ms();
        let u = match out.as_deref().map(|o| read(o, now, limits::local_offset())) {
            Some(bars) if !bars.is_empty() => Usage { at: now, bars, err: None },
            Some(_) => Usage { at: now, bars: Vec::new(), err: Some("its CLI did not say how much is used".into()) },
            None => Usage { at: now, bars: Vec::new(), err: Some("its CLI could not be asked".into()) },
        };
        self.usage.read_file();
        self.usage.got.insert(agent.clone(), u.clone());
        self.usage.write_file();
        self.bcast(&json!({"t": "usage", "agent": agent, "usage": u}));
    }
}

/// The limits in what a CLI printed: agy's JSON (`-p /usage --output-format json`), a codex session
/// log line (`rate_limits`), or Claude Code's lines ("Current session: 9% used · resets 7:40pm ...").
pub(super) fn read(out: &str, now_ms: u64, off_s: i64) -> Vec<Bar> {
    for l in out.lines().rev().map(str::trim).filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        if let Some(g) = v.pointer("/command/data/groups").and_then(Value::as_array) {
            return groups(g);
        }
        if let Some(r) = find(&v, "rate_limits") {
            return rate_limits(r, now_ms);
        }
    }
    out.lines().filter_map(|l| line(l, now_ms, off_s)).collect()
}

/// agy: groups of models that share a weekly and a 5-hour limit, each told as what is left.
fn groups(groups: &[Value]) -> Vec<Bar> {
    let mut bars = Vec::new();
    for g in groups {
        let scope = words_of(g["name"].as_str().unwrap_or_default());
        for b in g["buckets"].as_array().into_iter().flatten() {
            let Some(left) = b["remaining_fraction"].as_f64() else { continue };
            let name = match b["window"].as_str().unwrap_or_default() {
                "weekly" => "week".to_string(),
                w => w.to_string(),
            };
            let resets = b["reset_time"].as_str().and_then(rfc3339_ms);
            bars.push(Bar { name, used: ((1.0 - left) * 100.0).clamp(0.0, 100.0), resets, scope: scope.clone() });
        }
    }
    bars
}

/// codex: a primary and a secondary window, by their length; a window that started over since the
/// log was written is unused.
fn rate_limits(r: &Value, now_ms: u64) -> Vec<Bar> {
    let scope = r["limit_name"].as_str().map(words_of).unwrap_or_default();
    ["primary", "secondary"]
        .iter()
        .filter_map(|k| {
            let w = r.get(*k).filter(|w| w.is_object())?;
            let used = w["used_percent"].as_f64()?;
            let resets = w["resets_at"].as_u64().map(|s| s * 1000);
            let mins = w["window_minutes"].as_u64().unwrap_or(0);
            let name = match mins {
                10_080 => "week".to_string(),
                43_200 => "month".to_string(),
                m if m >= 1_440 && m % 1_440 == 0 => format!("{}d", m / 1_440),
                m if m >= 60 => format!("{}h", m / 60),
                m => format!("{m}m"),
            };
            let used = if resets.is_some_and(|r| r <= now_ms) { 0.0 } else { used };
            Some(Bar { name, used, resets, scope: scope.clone() })
        })
        .collect()
}

/// Claude Code: "Current session: 9% used · resets Oct 23, 7:40pm (Europe/Berlin)", "Current week
/// (all models): 2% used ...", "Current week (Sonnet only): ...", "Current week (Opus): ...".
fn line(l: &str, now_ms: u64, off_s: i64) -> Option<Bar> {
    let (title, rest) = l.trim().split_once(':')?;
    let pct = rest.trim_start().split_once("% used")?.0.trim();
    let used: f64 = pct.parse().ok()?;
    let t = title.to_lowercase();
    let name = if t.contains("session") {
        "5h"
    } else if t.contains("week") {
        "week"
    } else {
        return None;
    };
    let scope = match (t.find('('), t.rfind(')')) {
        (Some(a), Some(b)) if a < b && !t[a..b].contains("all models") => words_of(&t[a + 1..b]),
        _ => String::new(),
    };
    Some(Bar { name: name.into(), used, resets: limits::reset_at(rest, now_ms, off_s), scope })
}

/// The words that name models in a limit's name: "Claude and GPT models" -> "claude gpt".
fn words_of(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "models" | "model" | "and" | "only" | "limit" | "limits"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first value under `key` anywhere in `v`.
fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Object(m) => m.get(key).filter(|x| !x.is_null()).or_else(|| m.values().find_map(|x| find(x, key))),
        Value::Array(a) => a.iter().find_map(|x| find(x, key)),
        _ => None,
    }
}

/// "2026-10-23T22:52:35Z" / "...+02:00": ms since 1970.
fn rfc3339_ms(s: &str) -> Option<u64> {
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let tail = s.get(19..)?.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let off = match tail.as_bytes().first() {
        None | Some(b'Z' | b'z') => 0,
        Some(b'+' | b'-') => {
            let sign = if tail.starts_with('-') { -1 } else { 1 };
            sign * (tail.get(1..3)?.parse::<i64>().ok()? * 3_600 + tail.get(4..6).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0) * 60)
        }
        _ => return None,
    };
    let at = limits::days_from_civil(y, mo as u32, d as u32) * 86_400 + h * 3_600 + mi * 60 + se - off;
    (at > 0).then(|| at as u64 * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-23 12:00:00 in Berlin (UTC+2)
    const NOON: u64 = 1_792_749_600_000;
    const BERLIN: i64 = 7_200;

    #[test]
    fn claude_codes_usage_lines_are_read() {
        let out = "You are currently using your subscription to power your Claude Code usage\n\n\
                   Current session: 9% used · resets Oct 23, 7:40pm (Europe/Berlin)\n\
                   Current week (all models): 2% used · resets Oct 24, 9am (Europe/Berlin)\n\
                   Current week (Sonnet only): 41% used · resets Oct 24, 9am (Europe/Berlin)\n\n\
                   What's contributing to your limits usage?\n  67%  Turn off skills you don't use · 14k tokens per request\n";
        let b = read(out, NOON, BERLIN);
        assert_eq!(b.len(), 3, "{b:?}");
        assert_eq!((b[0].name.as_str(), b[0].used, b[0].scope.as_str()), ("5h", 9.0, ""));
        // 19:40 Berlin = 17:40 UTC
        assert_eq!(b[0].resets, Some(NOON + 7 * 3_600_000 + 40 * 60_000));
        assert_eq!((b[1].name.as_str(), b[1].used, b[1].scope.as_str()), ("week", 2.0, ""));
        assert_eq!((b[2].used, b[2].scope.as_str()), (41.0, "sonnet"));
    }

    #[test]
    fn agys_groups_are_read_as_what_is_used() {
        let out = r#"{"conversation_id":"","status":"SUCCESS","response":"x","command":{"name":"usage","data":{"groups":[
            {"name":"Gemini Models","buckets":[{"id":"gemini-weekly","window":"weekly","remaining_fraction":0.63,"reset_time":"2026-10-23T22:52:35Z"},
                                                {"id":"gemini-5h","window":"5h","remaining_fraction":0.92,"reset_time":"2026-10-23T15:02:58Z"}]},
            {"name":"Claude and GPT models","buckets":[{"id":"3p-weekly","window":"weekly","remaining_fraction":1,"reset_time":"2026-10-30T12:43:44Z"}]}]}}}"#
            .replace('\n', "");
        let b = read(&out, NOON, BERLIN);
        assert_eq!(b.len(), 3, "{b:?}");
        assert_eq!((b[0].name.as_str(), b[0].scope.as_str()), ("week", "gemini"));
        assert!((b[0].used - 37.0).abs() < 0.01);
        // 15:02:58 UTC
        assert_eq!(b[1].resets, Some(NOON + (5 * 3_600 + 2 * 60 + 58) * 1000));
        assert_eq!((b[2].used, b[2].scope.as_str()), (0.0, "claude gpt"));
    }

    #[test]
    fn a_codex_log_line_gives_its_windows_and_a_window_that_started_over_is_unused() {
        let l = |resets: u64| {
            format!(
                r#"{{"timestamp":"x","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{{"limit_id":"codex","limit_name":null,
                "primary":{{"used_percent":34.0,"window_minutes":300,"resets_at":{resets}}},"secondary":{{"used_percent":12.5,"window_minutes":10080,"resets_at":{}}},"credits":null}}}}}}"#,
                NOON / 1000 + 86_400
            )
            .replace('\n', "")
        };
        let b = read(&l(NOON / 1000 + 600), NOON, BERLIN);
        assert_eq!(b, vec![
            Bar { name: "5h".into(), used: 34.0, resets: Some(NOON + 600_000), scope: String::new() },
            Bar { name: "week".into(), used: 12.5, resets: Some(NOON + 86_400_000), scope: String::new() },
        ]);
        assert_eq!(read(&l(NOON / 1000 - 600), NOON, BERLIN)[0].used, 0.0);
    }

    #[test]
    fn nothing_known_is_no_bar() {
        assert!(read("error: not logged in\n", NOON, BERLIN).is_empty());
        assert!(read("{\"result\":\"hi\"}", NOON, BERLIN).is_empty());
    }
}
