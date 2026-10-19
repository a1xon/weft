//! What an answer leaves to the person: the questions and decisions in it.
//!
//! An agent can report that it is done and still leave something to the person ("Should I push?",
//! a list under "Decisions left for you", "...: your call"). weft holds such a report instead of
//! handing it on (`Engine::finish_task` in engine/run.rs), and the page shows each question with
//! quick answers. Only the agent's own prose counts: never code, tables, quotes, headings or words
//! in quotation marks. A question it answers right away ("Is it fast? Yes, 3 ms") asks nothing, and
//! neither does one in the middle of the text that does not speak to the person. An offer ("let me
//! know if you want a PR") is shown, but nothing waits for it. "Decisions:" on its own lists what
//! the agent decided, not what it asks.
//!
//! These rules are the fallback. A report is read by a model first (`read`, engine/reads.rs): the
//! rules took a recap of the person's own answers under "Your decisions:" for decisions left open
//! (rubbermetrics n142), and missed questions that end in a recommendation ("...? (yes)").

use serde::{Deserialize, Serialize};

/// One thing the person is asked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ask {
    /// the question (or the decision) as the person reads it
    pub text: String,
    pub kind: Kind,
    /// answered with yes or no
    pub yn: bool,
    /// what it offers to choose from: "a or b" in it, "(a) .. (b) ..", or a list right under it
    pub opts: Vec<Opt>,
    /// the lines of the text it stands on (counted from 0)
    pub from: usize,
    pub to: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// asked with a question mark
    Question,
    /// stated, and left to the person ("Found but not changed, your call: ..."): answered with "do it"
    Decision,
    /// an offer to do more: shown, nothing waits for it
    Offer,
    /// what an agent said it needs (an ISSUE) when no question can be told apart in it
    Need,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Opt {
    pub label: String,
    pub answer: String,
}

/// The most a text is taken to ask: beyond that it is a list of things, not questions to answer.
const MAX: usize = 12;

/// Does anything asked wait for the person (offers do not)?
pub fn waits(asks: &[Ask]) -> bool {
    asks.iter().any(|a| a.kind != Kind::Offer)
}

/// What an agent's ISSUE asks: its questions, or all of it as one thing it needs.
pub fn in_issue(text: &str) -> Vec<Ask> {
    let found = asks(text);
    if waits(&found) || text.trim().is_empty() {
        return found;
    }
    let last = text.lines().count().saturating_sub(1);
    vec![Ask { text: cut(&clean(text), 600), kind: Kind::Need, yn: false, opts: vec![], from: 0, to: last }]
}

/// The questions and decisions `text` leaves to the person, in the order they stand.
pub fn asks(text: &str) -> Vec<Ask> {
    let lines: Vec<&str> = text.lines().collect();
    let parts = parts(&lines);
    let tail = lines.len().saturating_sub((lines.len() * 3 / 10).max(15));
    let mut out: Vec<Ask> = Vec::new();
    // the section a heading or a "...:" label opened, and whether it is one of things for the person
    let mut head_asks = false;
    let mut label: Option<(usize, bool, bool)> = None; // (indent, an item, asks)
    let mut taken = vec![false; parts.len()];
    for (b, part) in parts.iter().enumerate() {
        let k = match part {
            Part::Head(h) => {
                head_asks = heads_asks(h);
                label = None;
                continue;
            }
            Part::Text(k) => k,
        };
        if taken[b] || quotes_the_task(&k.text) {
            continue;
        }
        let plain = clean(&k.text);
        // "Decisions left for you:" opens a list of them
        if plain.ends_with(':') && plain.chars().count() < 90 {
            label = Some((k.ind, k.item(), heads_asks(plain.trim_end_matches(':'))));
            continue;
        }
        if label.is_some_and(|(ind, item, _)| if item { k.ind <= ind } else { !k.item() }) {
            label = None;
        }
        // the nearest says: a plain label inside a list of decisions ("Next steps:") opens a list of its own
        let section = label.map_or(head_asks, |l| l.2);
        let next = match parts.get(b + 1) {
            Some(Part::Text(n)) if n.from <= k.to + 2 => Some(n),
            _ => None,
        };
        let mut found: Vec<Ask> = Vec::new();
        for s in sentences(&k.text) {
            let words = s.text.split_whitespace().count();
            let after = if s.after.is_empty() && s.text.len() + 1 >= k.text.trim().len() {
                next.map_or("", |n| n.text.as_str())
            } else {
                s.after.as_str()
            };
            let lower = s.masked.to_lowercase();
            if offers(&lower) {
                found.push(ask(&s.text, Kind::Offer));
            } else if s.asks && words >= 2 && (section || (!answered(after) && !rhetorical(&lower))) {
                let to_you = has(&lower, YOU);
                // in the middle of the text only a question to the person counts; at the end any
                if section || to_you || (k.from >= tail && s.after.is_empty()) {
                    found.push(ask(&s.text, Kind::Question));
                }
            }
        }
        let decides = decides(&mask(&k.text).to_lowercase());
        if !found.iter().any(|a| a.kind == Kind::Question) && (decides || (section && found.is_empty())) {
            // a decision stated without a question mark: the whole item is what is to be decided
            found.retain(|a| a.kind != Kind::Offer);
            found.push(ask(&unlabel(&plain), Kind::Decision));
        }
        if found.is_empty() {
            continue;
        }
        // the list right under a closing question is what it offers to choose from
        let last = found.len() - 1;
        if found[last].kind == Kind::Question && found[last].opts.is_empty() && ends_with_question(&k.text) {
            if let Some((opts, used)) = list_opts(&parts, b) {
                found[last].opts = opts;
                found[last].yn = false;
                for j in used {
                    taken[j] = true;
                }
            }
        }
        for mut a in found {
            a.from = k.from;
            a.to = k.to;
            out.push(a);
        }
    }
    out.truncate(MAX);
    out
}

/// A question, a decision or an offer, with the answers it suggests.
fn ask(text: &str, kind: Kind) -> Ask {
    let text = unlabel(&clean(text));
    // a decision goes on after what it asks: its first sentence holds the choice
    let first = sentences(&text).into_iter().next().map_or_else(|| text.clone(), |s| s.text);
    let opts = if kind == Kind::Offer { None } else { inline_opts(&first).or_else(|| or_opts(&first, kind == Kind::Question)) }.unwrap_or_default();
    let yn = opts.is_empty() && yes_or_no(&text, kind);
    Ask { text: cut(&text, 300), kind, yn, opts, from: 0, to: 0 }
}

/// Is it answered with yes or no (an offer always is; "which ...?" and "this or that?" are not)?
fn yes_or_no(text: &str, kind: Kind) -> bool {
    let lower = text.to_lowercase();
    let word = lower.split(|c: char| !c.is_alphanumeric()).find(|w| !w.is_empty()).unwrap_or("");
    match kind {
        Kind::Offer => true,
        Kind::Question => !WH.contains(&word) && !has(&lower.replace("or not", ""), &["or"]),
        _ => false,
    }
}

// ---------------------------------------------------------------- what a model read

/// What a model found `text` leaves to the person, from its answer to `words::asks_request`
/// (`{"asks": [{"text", "quote", "answers"}]}`), each on the lines of `text` its quote stands on
/// (past the end when the quote is not found: then it is listed, but marked nowhere). `None` when
/// the answer is not that JSON.
pub fn read(out: &str, text: &str) -> Option<Vec<Ask>> {
    #[derive(Deserialize)]
    struct Said {
        asks: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        #[serde(default)]
        text: String,
        #[serde(default)]
        quote: String,
        #[serde(default)]
        answers: Vec<String>,
    }
    // (a model may still put it in a code block, or say something around it)
    let said: Said = serde_json::from_str(out.get(out.find('{')?..=out.rfind('}')?)?).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for it in said.asks {
        let q = clean(&it.text);
        if q.is_empty() {
            continue;
        }
        let kind = if q.ends_with('?') { Kind::Question } else { Kind::Decision };
        let opts: Vec<Opt> =
            it.answers.iter().map(|a| clean(a)).filter(|a| !a.is_empty()).take(4).map(|a| Opt { label: cut(&a, 32), answer: cut(&a, 160) }).collect();
        let yn = opts.is_empty() && yes_or_no(&q, kind);
        let (from, to) = stands_on(&lines, &it.quote).unwrap_or((lines.len(), lines.len()));
        found.push(Ask { text: cut(&q, 300), kind, yn, opts, from, to });
    }
    // (a long list of questions is still what it asks: only a runaway answer is cut)
    found.truncate(MAX * 2);
    Some(found)
}

/// The lines a quote of the text stands on, emphasis, list markers and spacing aside: the line it
/// is in, or the lines its start and its end are in.
fn stands_on(lines: &[&str], quote: &str) -> Option<(usize, usize)> {
    let norm = |s: &str| clean(&s.replace('`', "")).to_lowercase();
    let q = norm(quote);
    let q = list_lead(&q).map_or(q.as_str(), |(_, rest)| rest).trim();
    if q.chars().count() < 4 {
        return None;
    }
    let ls: Vec<String> = lines.iter().map(|l| norm(l)).collect();
    if let Some(i) = ls.iter().position(|l| l.contains(q)) {
        return Some((i, i));
    }
    // over several lines: where its first words are, and its last ones after them
    let words: Vec<&str> = q.split(' ').collect();
    let head = words[..words.len().min(6)].join(" ");
    let tail = words[words.len().saturating_sub(6)..].join(" ");
    let from = ls.iter().position(|l| l.contains(&head))?;
    let to = (from..ls.len().min(from + 12)).find(|&j| ls[j].contains(&tail)).unwrap_or(from);
    Some((from, to))
}

// ---------------------------------------------------------------- the text in parts

/// A paragraph, or a list item with the lines that continue it.
struct Block {
    from: usize,
    to: usize,
    ind: usize,
    /// its list marker ("-", "1.", "a)"); empty for a paragraph
    lead: String,
    text: String,
}

impl Block {
    fn item(&self) -> bool {
        !self.lead.is_empty()
    }
}

enum Part {
    Head(String),
    Text(Block),
}

const BOX: [char; 14] = ['┌', '│', '├', '└', '┃', '┏', '┣', '┗', '╭', '╰', '║', '╔', '╠', '╚'];

/// The headings and blocks of prose of `lines`; code, tables, box drawings and quotes are left out.
fn parts(lines: &[&str]) -> Vec<Part> {
    let mut out = Vec::new();
    let mut cur: Option<Block> = None;
    let mut i = 0;
    while i < lines.len() {
        let (raw, at) = (lines[i], i);
        let t = raw.trim();
        let ind = raw.chars().take_while(|c| c.is_whitespace()).count();
        i += 1;
        if let Some(f) = ["```", "~~~"].into_iter().find(|f| t.starts_with(f)) {
            out.extend(cur.take().map(Part::Text));
            while i < lines.len() && !lines[i].trim().starts_with(f) {
                i += 1;
            }
            i += 1;
            continue;
        }
        if t.is_empty() || t.starts_with(['|', '>']) || t.starts_with(BOX) {
            out.extend(cur.take().map(Part::Text));
            continue;
        }
        if let Some((lead, rest)) = list_lead(t) {
            out.extend(cur.take().map(Part::Text));
            cur = Some(Block { from: at, to: at, ind, lead: lead.into(), text: rest.into() });
            continue;
        }
        if let Some(h) = heading(t) {
            out.extend(cur.take().map(Part::Text));
            out.push(Part::Head(h));
            continue;
        }
        if let Some(c) = cur.as_mut().filter(|c| if c.item() { ind > c.ind } else { ind >= c.ind }) {
            c.to = at;
            c.text.push(' ');
            c.text.push_str(t);
            continue;
        }
        out.extend(cur.take().map(Part::Text));
        cur = Some(Block { from: at, to: at, ind, lead: String::new(), text: t.into() });
    }
    out.extend(cur.take().map(Part::Text));
    out
}

/// `## Heading`, a line bold all through, or one in capitals ("YOUR DECISIONS", "STILL OPEN").
/// A bold question is a question, not a heading.
fn heading(t: &str) -> Option<String> {
    if let Some(r) = t.strip_prefix('#') {
        let r = r.trim_start_matches('#');
        return r.starts_with(' ').then(|| clean(r));
    }
    if let Some(inner) = t.strip_prefix("**").and_then(|r| r.strip_suffix("**:").or_else(|| r.strip_suffix("**"))) {
        let inner = inner.trim();
        return (!inner.contains("**") && !inner.ends_with('?') && inner.chars().count() < 80).then(|| clean(inner));
    }
    if t.chars().count() >= 80 || t.ends_with(['.', '!']) || t.contains(": ") {
        return None;
    }
    let (mut letters, mut words, mut all) = (0, 0, true);
    for w in t.split_whitespace() {
        let l: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
        if l.is_empty() {
            continue;
        }
        if !l.iter().all(|c| c.is_uppercase()) {
            all = false;
            break;
        }
        letters += l.len();
        words += 1;
    }
    (letters >= 4 && (words >= 2 || all)).then(|| clean(t))
}

/// A list marker at the start of a line ("- ", "* ", "• ", "1. ", "2) ", "a) "): the marker and the rest.
fn list_lead(t: &str) -> Option<(&str, &str)> {
    let marker = if t.starts_with(['-', '*', '+', '•']) {
        t.chars().next().map_or(0, char::len_utf8)
    } else {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        let n = if (1..=3).contains(&digits) {
            digits
        } else if t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            1
        } else {
            return None;
        };
        if !t[n..].starts_with(['.', ')']) {
            return None;
        }
        n + 1
    };
    let rest = &t[marker..];
    rest.starts_with([' ', '\t']).then(|| (&t[..marker], rest.trim_start()))
}

/// The list right under block `b`: two to six options (an option's own details, indented deeper,
/// belong to it). `None` when the items ask things themselves.
fn list_opts(parts: &[Part], b: usize) -> Option<(Vec<Opt>, Vec<usize>)> {
    let Part::Text(k) = &parts[b] else { return None };
    let (mut opts, mut used, mut prev_to, mut lvl) = (Vec::new(), Vec::new(), k.to, None);
    for (j, part) in parts.iter().enumerate().skip(b + 1) {
        let Part::Text(o) = part else { break };
        if !o.item() || o.from > prev_to + 2 || if k.item() { o.ind <= k.ind } else { o.ind < k.ind } || opts.len() == 6 {
            break;
        }
        let lvl = *lvl.get_or_insert(o.ind);
        if o.ind < lvl {
            break;
        }
        prev_to = o.to;
        used.push(j);
        if o.ind > lvl {
            continue; // a detail of the option above
        }
        if sentences(&o.text).iter().any(|s| s.asks) {
            return None;
        }
        let text = clean(&o.text);
        let numbered = o.lead.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
        let named = o.text.strip_prefix("**").and_then(|r| r.split_once("**")).map(|(n, _)| n.to_string());
        let label = named.or_else(|| text.split_once(": ").map(|(n, _)| n.to_string()).filter(|n| n.chars().count() <= 28));
        let label = match (numbered, label) {
            (true, Some(l)) => format!("{} {}", o.lead, l),
            (true, None) => format!("{} {}", o.lead, text),
            (false, l) => l.unwrap_or_else(|| text.clone()),
        };
        let answer = if numbered { format!("{} {text}", o.lead) } else { text };
        opts.push(Opt { label: cut(&label, 32), answer: cut(&answer, 160) });
    }
    (opts.len() >= 2).then_some((opts, used))
}

// ---------------------------------------------------------------- sentences

struct Sentence {
    text: String,
    /// the same with code, links and quoted words blanked out
    masked: String,
    /// ends with a question mark
    asks: bool,
    /// what follows it in its block
    after: String,
}

/// The sentences of a block. A question mark inside `code`, a link or quotation marks ends nothing.
fn sentences(text: &str) -> Vec<Sentence> {
    let masked = mask(text);
    let (t, m): (Vec<char>, Vec<char>) = (text.chars().collect(), masked.chars().collect());
    let closing = |c: char| "\"')]*_\u{201d}\u{2019}".contains(c);
    let mut out = Vec::new();
    let mut from = 0;
    let mut i = 0;
    while i < m.len() {
        let c = m[i];
        i += 1;
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let mut e = i;
        while e < m.len() && closing(m[e]) {
            e += 1;
        }
        if e < m.len() && !m[e].is_whitespace() {
            continue; // "v1.2", "?x"
        }
        if c == '.' && i >= 2 && m[i - 2].is_alphabetic() && (i < 3 || !m[i - 3].is_alphanumeric()) {
            continue; // "K.O.", "e.g.": a single letter
        }
        let s: String = t[from..e].iter().collect();
        if !s.trim().is_empty() {
            out.push(Sentence {
                text: s.trim().to_string(),
                masked: m[from..e].iter().collect::<String>().trim().to_string(),
                asks: c == '?',
                after: String::new(),
            });
        }
        from = e;
        i = e;
    }
    let rest: String = t[from..].iter().collect();
    if !rest.trim().is_empty() {
        out.push(Sentence { text: rest.trim().into(), masked: m[from..].iter().collect::<String>().trim().into(), asks: false, after: String::new() });
    }
    for k in 0..out.len() {
        out[k].after = out[k + 1..].iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    }
    out
}

/// `text` with what it only mentions blanked out (same length in chars): `code`, links, and words
/// in quotation marks.
fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut prev = ' ';
    while let Some(c) = chars.next() {
        let close = match c {
            '`' => Some('`'),
            '"' if !prev.is_alphanumeric() => Some('"'),
            '\u{201c}' => Some('\u{201d}'),
            _ => None,
        };
        if let Some(close) = close {
            let inner: String = chars.clone().take_while(|&x| x != close).collect();
            if inner.chars().count() < 300 && chars.clone().nth(inner.chars().count()) == Some(close) {
                out.push(c);
                out.extend(std::iter::repeat_n('x', inner.chars().count()));
                for _ in 0..inner.chars().count() {
                    chars.next();
                }
                out.push(chars.next().unwrap_or(close));
                prev = close;
                continue;
            }
        }
        if c == 'h' && (chars.clone().take(7).collect::<String>().starts_with("ttps://") || chars.clone().take(6).collect::<String>().starts_with("ttp://")) {
            out.push('x');
            while chars.peek().is_some_and(|x| !x.is_whitespace()) {
                chars.next();
                out.push('x');
            }
            prev = 'x';
            continue;
        }
        out.push(c);
        prev = c;
    }
    out
}

// ---------------------------------------------------------------- what the words say

/// Question words: such a question is not answered with yes or no.
const WH: [&str; 9] = ["which", "what", "how", "where", "when", "who", "whom", "whose", "why"];

/// A question with one of these speaks to the person.
const YOU: &[&str] = &[
    "you", "your", "should i", "shall i", "can i", "may i", "want me", "should we", "shall we", "go ahead", "proceed", "prefer", "ok to",
    "okay to", "is that ok", "is that okay", "is that fine", "fine with",
];

/// Words in a heading or a "...:" label that make what follows things for the person to decide.
const HEAD: &[&str] = &[
    "decide", "deciding", "decisions for", "decision for", "decisions left", "decision left", "decisions needed", "decision needed",
    "decisions to make", "decision to make", "decisions to take", "decision to take", "pending decisions", "open decisions",
    "decisions pending", "decisions open", "left for you", "left to you", "over to you", "for you to decide", "for you to answer",
    "for you to choose", "for you to pick", "for you to confirm", "from you", "open question", "open questions", "questions for",
    "question for", "questions to you", "questions left", "remaining questions", "outstanding questions", "pending questions",
    "unanswered questions", "my questions", "one question", "two questions", "three questions", "a few questions", "your decision",
    "your decisions", "your choice", "your input", "your answer", "your answers",
];

/// Words in prose that leave something to the person (unless the sentence says no to them).
const DECIDE: &[&str] = &[
    "your call", "up to you", "worth deciding", "for you to decide", "needs your", "need your", "on your word", "on your go", "on your ok", "on your okay", "on your signal",
    "your go-ahead", "your go ahead", "your approval", "your confirmation", "waiting for your", "waiting on you", "waiting for you",
    "it needs you", "this needs you", "that needs you", "need you to", "please decide", "please confirm", "please choose", "please answer", "needs a decision",
    "need a decision", "decide whether", "decide if", "decide which",
];

/// Words that turn a sentence's phrase around ("nothing here needs you").
const NOT: &[&str] = &["no", "not", "nothing", "none", "never", "without", "n't", "nobody", "neither"];

/// Does a heading (or label) open a list of things for the person? "Questions" alone does too.
fn heads_asks(h: &str) -> bool {
    let h = h.to_lowercase();
    let h = h.trim().trim_end_matches(':');
    h == "question" || h == "questions" || h.starts_with("questions ") || said(h, HEAD) || said(h, DECIDE)
}

/// Does prose (lowercased, quotes masked) leave something to the person to decide?
fn decides(lower: &str) -> bool {
    said(lower, DECIDE)
}

/// One of `phrases` in a clause of `lower` that does not say no to it first ("not changed, your
/// call" leaves it to the person; "nothing needs your decision" does not).
fn said(lower: &str, phrases: &[&str]) -> bool {
    let lower = lower.replace(['\u{2019}', '\u{2018}'], "'");
    lower.split(['.', '!', '?', ';', ',', ':', '(', ')', '\u{2014}', '\u{2013}']).any(|s| {
        phrases.iter().filter_map(|p| find_word(s, p)).any(|at| !has(&s[..at].replace("n't", " n't"), NOT))
    })
}

/// "let me know if you want a PR", "if you like, I can ...": more on offer, not a question.
fn offers(lower: &str) -> bool {
    let l = lower.trim_start_matches(|c: char| !c.is_alphanumeric()).replace('\u{2019}', "'");
    let l = l.strip_prefix("just ").unwrap_or(&l);
    ["let me know if", "let me know whether", "tell me if", "tell me whether", "if you want", "if you'd like", "if you would like", "if you like"]
        .iter()
        .any(|p| l.starts_with(p))
        || (["i can ", "i could ", "happy to ", "i'm happy to ", "i am happy to "].iter().any(|p| l.starts_with(p)) && l.contains("if you"))
}

/// The answer follows the question ("Is it fast? Yes, 3 ms.").
fn answered(after: &str) -> bool {
    let a = after.trim_start_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let first = a.split(|c: char| !c.is_alphanumeric() && c != '\'').next().unwrap_or("");
    ["yes", "no", "because", "since", "it", "it's", "this", "that", "mostly", "partly", "nope", "yep", "correct"].contains(&first)
        || a.starts_with("the answer")
        || a.starts_with("answer:")
        || a.starts_with("short answer")
        || a.starts_with("not really")
}

/// "Why did it loop?" asks itself, to explain.
fn rhetorical(lower: &str) -> bool {
    let l = lower.trim_start_matches(|c: char| !c.is_alphanumeric());
    l.starts_with("why ") || l.starts_with("how come")
}

/// A block that quotes what was asked ("was asked: ...?").
fn quotes_the_task(text: &str) -> bool {
    let l = text.trim_start().to_lowercase();
    l.starts_with("was asked:") || l.starts_with("asked:") || l.starts_with("you asked")
}

fn ends_with_question(text: &str) -> bool {
    sentences(text).last().is_some_and(|s| s.asks)
}

/// Is `word` (a word or words) in `hay` as a whole word?
fn has(hay: &str, words: &[&str]) -> bool {
    words.iter().any(|w| find_word(hay, w).is_some())
}

fn find_word(hay: &str, w: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = hay[from..].find(w) {
        let at = from + p;
        let end = at + w.len();
        let open = hay[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let shut = hay[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if open && shut {
            return Some(at);
        }
        from = at + hay[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

// ---------------------------------------------------------------- the answers it suggests

/// "(a) this, (b) that" in the question itself.
fn inline_opts(q: &str) -> Option<Vec<Opt>> {
    let mut keys = Vec::new();
    let chars: Vec<(usize, char)> = q.char_indices().collect();
    for w in chars.windows(3) {
        if w[0].1 == '(' && w[1].1.is_ascii_alphanumeric() && w[2].1 == ')' && (w[0].0 == 0 || q[..w[0].0].ends_with(' ')) {
            keys.push((w[0].0, w[1].1, w[2].0 + 1));
        }
    }
    if keys.len() < 2 || keys.len() > 6 {
        return None;
    }
    let mut opts = Vec::new();
    for (k, &(_, key, start)) in keys.iter().enumerate() {
        let end = keys.get(k + 1).map_or(q.len(), |n| n.0);
        let t = q[start..end].trim().trim_end_matches(['?', '"', '\'', ')', '*', '.']);
        let t = t.trim_end_matches([',', ';', ' ']);
        let t = t.strip_suffix(" or").unwrap_or(t).trim_end_matches([',', ';', ' ']);
        if t.is_empty() {
            return None;
        }
        opts.push(Opt { label: cut(&format!("({key}) {t}"), 32), answer: cut(&format!("({key}) {t}"), 140) });
    }
    Some(opts)
}

/// "Should I do this, or that?": the two (to four) short things it weighs. After a colon, the
/// choice of a question ("The old reels: now or after the re-analysis?") or of a statement whose
/// head asks ("Which level: the hand-rated one or the crawl one").
fn or_opts(q: &str, question: bool) -> Option<Vec<Opt>> {
    let s = q.trim().trim_end_matches(['?', '.', '"', '\'', ')', '*']).trim();
    let lower = s.to_lowercase();
    if find_word(&lower, "or").is_none() || find_word(&lower, "or not").is_some() {
        return None;
    }
    // "Which level: the hand-rated one or the crawl one" (what comes before the colon asks)
    let asks_before = |head: &str| {
        let h = head.to_lowercase();
        let w = h.split(|c: char| !c.is_alphanumeric()).find(|w| !w.is_empty()).unwrap_or("");
        WH.contains(&w) || w == "whether" || has(&h, &["choose", "pick", "between", "options", "should i", "should we", "do you"])
    };
    let s = match s.rfind(": ") {
        Some(colon) if question || asks_before(&s[..colon]) => &s[colon + 2..],
        // "Checkout: should we keep it or drop it"
        Some(colon) => return or_opts(&s[colon + 2..], question),
        None => {
            let mut l = lower.as_str();
            let mut skip = 0;
            for p in ["so ", "and ", "also ", "then "] {
                if let Some(r) = l.strip_prefix(p) {
                    skip += p.len();
                    l = r;
                }
            }
            const LEADS: [&str; 26] = [
                "should i rather ", "should we rather ", "should i ", "should we ", "shall i ", "shall we ", "would you rather ",
                "would you prefer that i ", "would you prefer me to ", "would you prefer to ", "would you prefer ", "do you prefer that i ",
                "do you prefer me to ", "do you prefer to ", "do you prefer ", "would you like me to ", "would you like ", "do you want me to ",
                "do you want to ", "do you want ", "want me to ", "can i ", "could i ", "do i ", "would i ", "will i ",
            ];
            let lead = LEADS.iter().find(|p| l.starts_with(*p))?;
            s.get(skip + lead.len()..)?
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    for chunk in s.split(", ") {
        let chunk = chunk.strip_prefix("or ").unwrap_or(chunk);
        let mut rest = chunk;
        while let Some(at) = find_or(rest) {
            parts.push(rest[..at].trim());
            rest = &rest[at + 2..];
        }
        parts.push(rest.trim());
    }
    parts.retain(|p| !p.is_empty());
    if parts.len() < 2 || parts.len() > 4 || parts.iter().any(|p| p.split_whitespace().count() > 8) {
        return None;
    }
    Some(parts.into_iter().map(|p| Opt { label: cut(p, 32), answer: p.to_string() }).collect())
}

/// Where the word "or" stands in `s`, whatever its case.
fn find_or(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(1)).find(|&i| {
        b[i..i + 2].eq_ignore_ascii_case(b"or")
            && (i == 0 || !s[..i].chars().next_back().is_some_and(char::is_alphanumeric))
            && !s[i + 2..].chars().next().is_some_and(char::is_alphanumeric)
    })
}

// ---------------------------------------------------------------- text

/// Markdown emphasis off, white space as one space.
fn clean(s: &str) -> String {
    s.replace("**", "").replace("__", "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "Question 2: Should we ...?" is the question; a status word in front of it is not part of it.
fn unlabel(s: &str) -> String {
    let s = s.strip_prefix("STATUS: ").map_or(s, |r| r.split_once(' ').map_or(r, |(_, q)| q));
    match s.split_once(": ") {
        Some((head, rest))
            if head.split_whitespace().count() <= 3 && !head.contains(['.', '?', '!']) && rest.chars().count() >= 8 && heads_asks(head) =>
        {
            rest.to_string()
        }
        _ => s.to_string(),
    }
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!("{}…", s.chars().take(n - 1).collect::<String>().trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(t: &str) -> Vec<String> {
        asks(t).into_iter().filter(|a| a.kind != Kind::Offer).map(|a| a.text).collect()
    }

    #[test]
    fn a_report_that_lists_decisions_for_the_person_asks_each() {
        // rubbermetrics n37, reported as done
        let t = "Website commit fcea8566: 106 files. LOCAL ONLY, NOT PUSHED.\n\nLEFT, in order\n1. The harvest disk's dirty flag.\n2. \
                 The corpus re-analysis finishes.\n\nYOUR DECISIONS\n- Push fcea8566?\n- The two live reels: Harimoto's is only out of \
                 date; Möregårdh's has one arrow that is Lin Yun-Ju's stroke.";
        let a = asks(t);
        assert_eq!(a.len(), 2, "{a:?}");
        assert_eq!((a[0].text.as_str(), a[0].kind, a[0].yn), ("Push fcea8566?", Kind::Question, true));
        assert_eq!(a[1].kind, Kind::Decision);
        assert!(a[1].text.starts_with("The two live reels"));
        assert_eq!((a[0].from, a[1].from), (7, 8), "the lines they stand on");
        // n79: "Decisions left for you:" and a paragraph that waits "on your word"
        let t = "Changed files: docs/pips.md.\n\nDecisions left for you:\n1. Which level is the scale for long pips, OX and anti: the \
                 hand-rated one or the crawl one. About 60 rows would then be re-rated.\n2. Should K.O. and Aggressor move to medium \
                 pips? That changes their Racket Builder class.\n3. One OX rule, so the OX row and the \"0\" option cannot disagree.\n\n\
                 Ready on your word, as one migration: Super Do Knuckle OX -> Medium Pips.";
        let a = asks(t);
        assert_eq!(a.len(), 4, "{a:?}");
        assert_eq!(a[0].opts.iter().map(|o| o.answer.as_str()).collect::<Vec<_>>(), ["the hand-rated one", "the crawl one"]);
        assert_eq!(a[1].text, "Should K.O. and Aggressor move to medium pips?");
        assert!(a[1].yn);
        assert_eq!(a[2].kind, Kind::Decision);
        assert!(a[3].text.starts_with("Ready on your word"));
    }

    #[test]
    fn your_call_in_a_list_is_a_decision_left_open() {
        // rubbermetrics n41 and n9
        let t = "Found but not changed, your call:\n- Damian Floro is Slovak per four sources; he is stored as Romania.\n- Lee Sang Su's \
                 blade has no catalog row, so it stays empty.\n\nWhen it shows on the site:\n- Pro pages are cached for up to a day.";
        assert_eq!(texts(t).len(), 2, "{:?}", asks(t));
        let t = "STILL OPEN\n1. Commit and deploy the engine (v59).\n2. Sanwei Dynamo: its 6 mm and 90 g appear on no page. Clearing \
                 the numbers would unpublish the page. Your call.\n3. GEWO Balsa Carbon 575: no total thickness is published.";
        let a = asks(t);
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(a[0].text.starts_with("Sanwei Dynamo") && a[0].kind == Kind::Decision);
        // n6: "worth deciding" inside a list of what was decided
        let t = "Decisions and what is left:\n- Home keeps uid god-mode.\n- Release markers are on by default only on Edge.\n- Not \
                 changed, worth deciding: off-site backups are not set up (RCLONE_DEST).";
        assert_eq!(texts(t), ["Not changed, worth deciding: off-site backups are not set up (RCLONE_DEST)."]);
    }

    #[test]
    fn what_the_agent_decided_asks_nothing() {
        // n98, n136: "Decisions:" lists the agent's own choices
        let t = "Checks: cargo test passes.\n\nDecisions:\n- An answer typed into the terminal is not passed on to linked agents.\n- \
                 Plain Enter still only saves it. I left that alone.\n\nWhat is left: nothing is committed.";
        assert!(asks(t).is_empty(), "{:?}", asks(t));
        let t = "Still open:\n- n72 on the live board: click arm on n72 once the build is installed. I did not start the deploy myself.";
        assert!(asks(t).is_empty(), "an action left to the person is not a question: {:?}", asks(t));
        // a question it answers itself, one quoted, one in code, a heading that is a question
        let t = "## Is the sheet flawed?\n\nNo: the sheet measures dwell time.\n\nWhy did it loop? The queue kept a typed line.\n\nThe \
                 chip puts \"explain the options first: which do you recommend?\" in the bar. Run `test -f x || echo missing?` once.\n\n\
                 Is it fast? Yes, 3 ms.";
        assert!(asks(t).is_empty(), "{:?}", asks(t));
        let t = "Nothing here needs your decision. No action is needed from you.";
        assert!(asks(t).is_empty(), "{:?}", asks(t));
    }

    #[test]
    fn a_closing_question_and_questions_to_the_person_ask() {
        // n43, n75, n66
        let t = "What I'd build next\n\n1. Check gates first.\n2. Then the networks.\n\nWant me to start with 1 and 2? They're page-only.";
        let a = asks(t);
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].text.as_str(), a[0].yn), ("Want me to start with 1 and 2?", true));
        let a = asks("The tree shows the focused repo.\n\nDo you want it this way, or two full trees always open?");
        assert_eq!(a[0].opts.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["it this way", "two full trees always open"]);
        assert!(!a[0].yn);
        let a = asks("Edge and Safari behave the same. I got around it only in my scratch test and haven't fixed it — want me to?");
        assert_eq!(a.len(), 1);
        // in the middle of a long text, a question that does not speak to the person is the agent thinking aloud
        let mut t = String::from("What changed? The slug function keeps digits now.\n");
        t.push_str(&"- a checked case\n".repeat(40));
        assert!(asks(&t).is_empty(), "{:?}", asks(&t));
    }

    #[test]
    fn the_list_under_a_question_is_its_options_and_offers_hold_nothing() {
        let t = "I fixed the slug function.\n\nWhich way should the old slugs go?\n1. Keep them as they are\n2. Redirect them to the new \
                 ones\n\nShould I also update the README? Want me to commit this now, or wait until the CI is green?\n\nLet me know if \
                 you want a PR.";
        let a = asks(t);
        assert_eq!(a.len(), 4, "{a:?}");
        assert_eq!(a[0].opts.iter().map(|o| o.answer.as_str()).collect::<Vec<_>>(), ["1. Keep them as they are", "2. Redirect them to the new ones"]);
        assert_eq!(a[0].opts[0].label, "1. Keep them as they are");
        assert!(a[1].yn && a[1].opts.is_empty());
        assert_eq!(a[2].opts.iter().map(|o| o.answer.as_str()).collect::<Vec<_>>(), ["commit this now", "wait until the CI is green"]);
        assert_eq!((a[1].from, a[2].from), (6, 6), "two questions on one line");
        assert_eq!((a[3].kind, a[3].yn), (Kind::Offer, true));
        assert!(waits(&a));
        assert!(!waits(&asks("Done: the tests pass.\n\nLet me know if you want a PR.")), "an offer alone waits for nothing");
        let a = asks("Which one should I take: (a) the cache, (b) the index, or (c) both?");
        assert_eq!(a[0].opts.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["(a) the cache", "(b) the index", "(c) both"]);
    }

    #[test]
    fn an_issue_with_no_question_in_it_is_one_thing_needed() {
        let a = in_issue("I need the staging database password to run the migration");
        assert_eq!((a.len(), a[0].kind), (1, Kind::Need));
        let a = in_issue("The API changed. Should I target v2 or v3?");
        assert_eq!(a[0].opts.len(), 2, "{a:?}");
    }

    /// rubbermetrics n142's last report, after the person answered its question: the recap of their
    /// answers under "Your decisions" is no decision left to them.
    const RECAP: &str = "Deployed, and the Tactics LP merge is applied.\n\nWhole run\n- Added 120 new products.\n- Your decisions \
                         (crawler commit 922d891):\n  - 88 duplicate tasks dismissed.\n  - The 17 inverted rubbers stay hidden: no source \
                         names a hardness scale.\n- No brand was added.\n\nStill pending in the queue\n- 30 MegaSpin tasks.";

    #[test]
    fn the_rules_take_a_recap_of_the_persons_answers_for_decisions_and_a_model_does_not() {
        // why reports are read by a model first
        assert_eq!(texts(RECAP), ["88 duplicate tasks dismissed.", "The 17 inverted rubbers stay hidden: no source names a hardness scale."]);
        assert_eq!(read("{\"asks\": []}", RECAP), Some(vec![]));
    }

    #[test]
    fn what_a_model_read_stands_on_the_lines_it_quotes_with_its_answers() {
        let t = "Committed 916f5f4, not pushed.\n\nQuestions:\n1. **Should Hallmark Tactics LP Special be merged into Tactics LP?**\n   \
                 - merge it\n   - keep it separate\n2. What hardness scale should the 17 hidden rubbers get? Gambler states \"MID 37\".";
        // (models put it in a code block, or say something around it)
        let out = "Here it is:\n```json\n{\"asks\": [\
                   {\"text\": \"Merge Tactics LP Special into Tactics LP?\", \"quote\": \"Should Hallmark Tactics LP Special be merged into Tactics LP?\", \
                    \"answers\": [\"yes, merge it\", \"no, keep it separate\"]},\
                   {\"text\": \"Which hardness scale should the 17 hidden rubbers get?\", \"quote\": \"2. What hardness scale should the 17 hidden rubbers get?\", \"answers\": []},\
                   {\"text\": \"Is the deploy fine?\", \"quote\": \"nowhere in the report\", \"answers\": []},\
                   {\"text\": \"Apply the migration on your word\", \"quote\": \"\", \"answers\": [\"apply it\", \" \"]}\
                   ]}\n```";
        let a = read(out, t).expect("its JSON");
        assert_eq!(a.len(), 4);
        assert_eq!((a[0].kind, a[0].yn, a[0].from, a[0].to), (Kind::Question, false, 3, 3), "{:?}", a[0]);
        assert_eq!(a[0].opts.iter().map(|o| o.answer.as_str()).collect::<Vec<_>>(), ["yes, merge it", "no, keep it separate"]);
        assert_eq!((a[1].from, a[1].yn), (6, false), "a list marker or emphasis does not hide the line; \"which\" is no yes/no");
        assert_eq!((a[2].from, a[2].yn), (7, true), "a quote that is nowhere: listed, marked on no line");
        assert_eq!((a[3].kind, a[3].opts.len()), (Kind::Decision, 1), "no question mark: a decision; empty answers are dropped");
        assert_eq!(read("I could not read it.", t), None);
        assert_eq!(read("{\"questions\": []}", t), None);
    }
}
