//! Data model: the flow graph, the prompt library and the user config.
//! Everything here is plain JSON on disk so it can be read, diffed and edited by hand.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// How a node decides that its work is finished.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    /// The screen stopped changing for `quiet` seconds (works for any TUI).
    #[default]
    Idle,
    /// The process exited (right for plain commands like `cargo test`).
    Exit,
    /// Only an explicit `weft done` counts.
    Signal,
}

/// How several incoming edges combine.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    /// Wait until every incoming edge has fired.
    #[default]
    All,
    /// Fire on the first incoming edge.
    Any,
}

/// What a git node does. Each step does what the steps before it would have done: a push with an
/// agent straight in front of it asks that agent to commit first, a pull request pushes first.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GitOp {
    /// The older both-in-one node: commits every arriving result, then pushes (unless `push` is off).
    #[serde(rename = "")]
    #[default]
    Both,
    /// One agent's work is committed by that agent.
    Commit,
    /// Waits for every commit linked into it, then pushes.
    Push,
    /// Pushes if nothing before it did, then has an agent open the pull request.
    Pr,
}

impl GitOp {
    /// What the node is called on the canvas when nobody named it.
    pub fn label(self) -> &'static str {
        match self {
            GitOp::Both => "commit & push",
            GitOp::Commit => "commit",
            GitOp::Push => "push",
            GitOp::Pr => "open PR",
        }
    }
}

/// Which outcome of the source node fires an edge.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum On {
    #[default]
    Done,
    Ok,
    Fail,
}

impl On {
    pub fn matches(self, ok: bool) -> bool {
        match self {
            On::Done => true,
            On::Ok => ok,
            On::Fail => !ok,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Key into `Config.agents` (claude, codex, agy, sh, ...). Free text is fine.
    #[serde(default)]
    pub agent: String,
    /// Command override. Empty = the agent's configured command.
    #[serde(default)]
    pub cmd: String,
    /// Working directory. Empty = the project directory.
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    /// The card's size when the person resized it; 0 = the default size.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub w: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub h: f64,
    /// Prompt template sent when the node is triggered.
    #[serde(default)]
    pub prompt: String,
    /// Library ids of addenda appended to every prompt.
    #[serde(default)]
    pub addenda: Vec<String>,
    #[serde(default)]
    pub finish: Finish,
    /// Seconds of silence that count as "finished". 0 = config default.
    #[serde(default)]
    pub quiet_s: f64,
    #[serde(default)]
    pub join: Join,
    /// Run in a fresh git worktree (own branch) instead of the shared folder.
    #[serde(default)]
    pub worktree: bool,
    /// The name was derived from the prompt and follows it until the user renames the agent.
    #[serde(default)]
    pub auto_name: bool,
    /// Placed but not armed: collects inputs and waits for the user's go-ahead.
    #[serde(default)]
    pub draft: bool,
    /// git node: branch to commit to (empty = the current one).
    #[serde(default)]
    pub branch: String,
    /// git node: remote to push to (empty = origin).
    #[serde(default)]
    pub remote: String,
    /// git node: push once every incoming link has delivered.
    #[serde(default = "yes_push")]
    pub push: bool,
    /// git node: commit subject template (empty = "<agent>: <first line of its result>").
    #[serde(default)]
    pub message: String,
    /// screenshot / tests gate: what to capture or cover, in the user's words.
    #[serde(default)]
    pub subject: String,
    /// tests gate: ask the upstream agent to write tests before running the suite.
    #[serde(default = "yes_push")]
    pub write_tests: bool,
    /// tests gate: how many times a failing run is sent back to the agent automatically.
    #[serde(default = "two")]
    pub retries: u32,
    /// tests gate: ask for a human decision even when the suite is green.
    #[serde(default)]
    pub confirm: bool,
    /// Model and reasoning level for an agent; empty = the CLI's own default.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
    /// The agent this one took over from. Lineage for the canvas, not a link: nothing fires along it.
    #[serde(default)]
    pub takeover: String,
    /// Run without permission prompts. None = the config's default (on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yolo: Option<bool>,
    /// step node: the agent whose session answers this prompt.
    #[serde(default)]
    pub host: String,
    /// git node: what it does (see `GitOp`).
    #[serde(default)]
    pub git_op: GitOp,
    /// Checks attached to this agent: they watch it work (never block anything).
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Kept on the canvas: never goes to the archive by itself, and neither does its flow.
    #[serde(default)]
    pub keep: bool,
    /// The cards that failed and this agent was asked to fix (see engine/fix.rs). Not a link.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
}

/// What a watch collects.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// Screenshots: shown by the agent (`weft shot`), taken by a command, or pasted.
    #[default]
    Shot,
    /// Test runs of a command, red or green.
    Tests,
    /// The agent's own estimate of how long it will take, counted down.
    Eta,
    /// What it has touched so far, read from git.
    Changes,
    /// A notification when it is done or needs a person.
    Ping,
}

/// A watch on an agent: it collects evidence while the agent works, for the human to
/// approve or steer from. Dropped on a link the same thing is a gate instead.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Check {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: CheckKind,
    /// what to show / cover, in the user's words
    #[serde(default)]
    pub subject: String,
    /// run by weft (capture command with $WEFT_SHOT, or the test suite); empty = the agent does it
    #[serde(default)]
    pub cmd: String,
    /// ask again every N seconds while the agent is working; 0 = only on demand / when the agent decides
    #[serde(default)]
    pub every_s: u32,
}

/// png / jpg / webp / gif by magic bytes; anything else is not an image we serve.
pub fn image_ext(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else {
        None
    }
}

fn yes_push() -> bool {
    true
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// How many terminal rows a card shows: 12 on a card of the default height, more on a taller one.
pub fn preview_rows(card_h: f64) -> usize {
    12 + ((card_h - 232.0).max(0.0) / 16.0) as usize
}

fn two() -> u32 {
    2
}

/// What a node is. Agents and shell commands run in tmux; the rest are
/// lightweight steps the daemon performs itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Agent,
    /// A fixed piece of text other agents can be pointed at.
    Note,
    /// Holds the flow until a human approves.
    Gate,
    /// A barrier that asks the upstream agent for a screenshot and shows it.
    Shot,
    /// A barrier that has tests written and run before the flow may continue.
    Tests,
    /// `commit`: every arriving result is committed by the agent that made it. `push`: pushes
    /// when every input has been committed. Neither op set: both, as before.
    Git,
    /// A prompt queued on the same agent: the agent answers it in its own session and the answer
    /// is this node's result (the flow goes back to the agent that was asked).
    Step,
}

impl Kind {
    /// Gates are checkpoints: nothing downstream starts until they are solved.
    pub fn is_gate(self) -> bool {
        matches!(self, Kind::Gate | Kind::Shot | Kind::Tests)
    }
}

impl Node {
    /// Does a git node end up with the commits on the remote? (A push, a PR, and the older
    /// both-in-one node unless it was told to keep them local.)
    pub fn pushes(&self) -> bool {
        match self.git_op {
            GitOp::Commit => false,
            GitOp::Push | GitOp::Pr => true,
            GitOp::Both => self.push,
        }
    }

    pub fn kind(&self) -> Kind {
        match self.agent.as_str() {
            "note" => Kind::Note,
            "gate" => Kind::Gate,
            "shot" => Kind::Shot,
            "tests" => Kind::Tests,
            "git" => Kind::Git,
            "step" => Kind::Step,
            _ => Kind::Agent,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub on: On,
}

/// Two agents that coordinate while they work: each may message the other directly (`weft tell`),
/// and nothing fires along it. Talks that share an agent make one group, where everyone hears
/// everyone.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Talk {
    pub id: String,
    pub a: String,
    pub b: String,
    /// What they should mainly coordinate on, in the person's words ("" = wherever their work meets).
    #[serde(default)]
    pub topic: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Flow {
    /// the board's name, when it was given one (else its folder's)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub talks: Vec<Talk>,
    /// Archiving is held (the lock in the top bar): no countdown runs, nothing files itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold: bool,
}

/// How a library snippet is used.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SnippetKind {
    /// Copied into a node's prompt when inserted.
    Prompt,
    /// Referenced by id and appended on send, so edits propagate.
    Addendum,
}

/// A reusable text block: a standard prompt (copied into a node) or an
/// addendum (referenced by id and appended on send, so edits propagate).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snippet {
    pub id: String,
    pub kind: SnippetKind,
    pub title: String,
    pub text: String,
    /// New nodes start with this addendum switched on.
    #[serde(default)]
    pub default_on: bool,
}

/// A network the user saved: cards and their links, placed on the canvas in one go. The page builds
/// it with the ordinary node and link commands (the built-in networks live in the page), so the
/// engine only keeps the list.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Network {
    pub id: String,
    pub title: String,
    /// One line: what it is for.
    #[serde(default)]
    pub about: String,
    /// Cards as `node_add` takes them, each with a `key` the links name; x/y are relative.
    #[serde(default)]
    pub nodes: Vec<serde_json::Value>,
    /// In the order they are drawn: a queued prompt is asked of the agent its last link comes from.
    #[serde(default)]
    pub links: Vec<NetLink>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct NetLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub on: On,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentCfg {
    pub cmd: String,
    #[serde(default)]
    pub label: String,
    /// Screen substrings that mean "this needs a human" (trust / approval dialogs).
    #[serde(default)]
    pub attention: Vec<String>,
    /// How a chosen model reaches the command line (`{}` becomes the model), e.g. `--model {}`.
    /// Empty = the model is not selectable for this agent.
    #[serde(default)]
    pub model_arg: String,
    /// Fixed model choices, shown ahead of whatever `models_cmd` reports.
    #[serde(default)]
    pub models: Vec<String>,
    /// A command that lists the CLI's own models: `id<TAB>label` lines or a JSON catalog.
    #[serde(default)]
    pub models_cmd: String,
    /// How a reasoning level reaches the command line, e.g. `--effort {}`.
    #[serde(default)]
    pub effort_arg: String,
    #[serde(default)]
    pub efforts: Vec<String>,
    /// The flag that makes this agent act without asking (yolo). Empty = weft does not manage it.
    #[serde(default)]
    pub yolo_arg: String,
    /// Screen substrings that mean "idle, but still working on something it started in the
    /// background" (a shell it launched, a build). Such an agent is not finished yet.
    #[serde(default)]
    pub working: Vec<String>,
    /// Screen substrings in the agent's status bar that it shows only while a turn runs ("esc to
    /// interrupt"): a pause in its output is then not the end of the turn, and a message just sent
    /// has been taken.
    #[serde(default)]
    pub busy: Vec<String>,
    /// The reasoning level applies only to these models (substrings; "" = the CLI's default
    /// model). Empty = every model.
    #[serde(default)]
    pub effort_models: Vec<String>,
    /// A one-shot command that names a card from its prompt, run with `sh -c`: the request is in
    /// `$WEFT_TITLE_REQ`, the name is the last line it prints. The CLI's smallest model at its
    /// lowest reasoning. Empty = the card keeps the first words of its prompt.
    #[serde(default)]
    pub title_cmd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub agents: BTreeMap<String, AgentCfg>,
    pub quiet_s: f64,
    pub ready_s: f64,
    pub submit_ms: u64,
    pub cols: u16,
    pub rows: u16,
    pub max_runs: u32,
    /// An agent told to `weft done` gets this long to do so before silence counts as finished.
    pub signal_grace_s: f64,
    /// Screen lines containing any of these are terminal chrome, not content.
    pub chrome: Vec<String>,
    pub library: Vec<Snippet>,
    /// The networks the user saved (the built-in ones are part of the page).
    pub networks: Vec<Network>,
    /// What the user chose last (agent, model, reasoning, addenda). Owned by the UI.
    pub prefs: serde_json::Value,
    /// Agents run without permission prompts unless a card (or the launch pad) switches it off.
    pub yolo: bool,
    /// How long weft keeps waiting for background work an idle agent announced (seconds).
    pub background_s: f64,
    /// Every prompt to an agent asks it to end with a `STATUS:` line (done, issue, waiting, failed)
    /// that weft reads from the output: before a chain hands the result on, and so that a lone card
    /// that asks a question needs the person instead of looking done. (The name is from when only
    /// chains had it.)
    pub chain_status: bool,
    /// How long an agent that said it is waiting may be silent before weft wakes it to check (seconds).
    pub recheck_s: f64,
    /// A finished card that is part of no flow goes to the archive after this long (seconds of work on
    /// the canvas: the clock stops while nobody is at it; 0 = never).
    pub archive_s: f64,
    /// A flow of several cards goes to the archive this long after its last step finished (likewise; 0 = never).
    pub archive_flow_s: f64,
    /// An agent that finished and has sat idle this long ends its process (seconds, wall clock;
    /// 0 = never). Its card and result stay; the next prompt starts a new process.
    #[serde(default = "rest_default")]
    pub rest_s: f64,
    /// Cards are named by their agent's `title_cmd` (until you rename one yourself).
    pub titles: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agents: default_agents(),
            quiet_s: 2.5,
            ready_s: 1.2,
            submit_ms: 300,
            cols: 110,
            rows: 34,
            max_runs: 25,
            signal_grace_s: 12.0,
            chrome: default_chrome(),
            library: default_library(),
            networks: Vec::new(),
            prefs: serde_json::json!({}),
            yolo: true,
            background_s: 600.0,
            chain_status: true,
            recheck_s: 60.0,
            archive_s: 600.0,
            archive_flow_s: 900.0,
            rest_s: rest_default(),
            titles: true,
        }
    }
}

fn rest_default() -> f64 {
    1800.0
}

fn default_chrome() -> Vec<String> {
    [
        "bypass permissions",
        "shift+tab to cycle",
        "? for shortcuts",
        "esc to interrupt",
        "tmux detected",
        "ctrl+c to exit",
        "Cooked for",
        "Worked for",
        "Brewed for",
        "/effort",
        "paste again to expand",
        "focus-events",
        "scroll with PgUp",
        "to manage",
        "/model to change",
        "Ask Codex to do anything",
        "\u{2190} for agents",
        "Verbosity now defaults",
        "f2 to view",
        "Update installed",
        "/clear to save",
        "Compacted while idle",
        "ctrl+g to edit in",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// The agents weft knows how to start. Everything here is only a default: the user's
/// `config.json` is merged over it, so an upgrade keeps improving what they did not touch.
fn default_agents() -> BTreeMap<String, AgentCfg> {
    let levels = ["low", "medium", "high", "xhigh", "max"];
    let mut m = BTreeMap::new();
    m.insert(
        "claude".into(),
        AgentCfg {
            cmd: "claude".into(),
            label: "Claude Code".into(),
            // the second is the one-time "bypass permissions" acceptance
            attention: strings(&["Yes, I trust this folder", "Do you want to proceed?", "Yes, I accept"]),
            model_arg: "--model {}".into(),
            models: strings(&["opus", "sonnet", "haiku"]),
            effort_arg: "--effort {}".into(),
            efforts: strings(&levels),
            yolo_arg: "--dangerously-skip-permissions".into(),
            // "✻ Brewed for 3s · done 7:07 · 1 shell still running": it will wake up when it ends
            working: strings(&["shell still running", "shells still running"]),
            busy: strings(&["esc to interrupt"]),
            // no tools: the request must not turn into work
            title_cmd: r#"claude -p "$WEFT_TITLE_REQ" --model haiku --effort low --no-session-persistence --tools """#.into(),
            ..AgentCfg::default()
        },
    );
    m.insert(
        "codex".into(),
        AgentCfg {
            cmd: "codex".into(),
            label: "Codex".into(),
            attention: strings(&[
                "Do you trust the contents of this directory",
                "Would you like to run the following command",
                // blocking self-update menu: Enter would pick "Update now"
                "Update now",
                "Skip until next version",
            ]),
            model_arg: "-m {}".into(),
            models_cmd: "codex debug models".into(),
            effort_arg: "-c model_reasoning_effort={}".into(),
            efforts: strings(&levels),
            yolo_arg: "--dangerously-bypass-approvals-and-sandbox".into(),
            working: strings(&["background terminal"]),
            busy: strings(&["esc to interrupt"]),
            title_cmd: r#"codex exec -m gpt-6-luna -c model_reasoning_effort=low -s read-only --ephemeral --skip-git-repo-check "$WEFT_TITLE_REQ""#
                .into(),
            ..AgentCfg::default()
        },
    );
    // agy: --effort is low|medium|high and only fits its default model; the other models either
    // carry the level in their name (gemini-3.8-flash-high) or have none (claude, gpt-oss), and
    // agy drops the whole model choice when the two disagree
    m.insert(
        "agy".into(),
        AgentCfg {
            cmd: "agy".into(),
            label: "Antigravity".into(),
            attention: strings(&["Do you trust the contents of this project"]),
            model_arg: "--model {}".into(),
            models_cmd: "agy models".into(),
            effort_arg: "--effort {}".into(),
            efforts: strings(&["low", "medium", "high"]),
            effort_models: vec![String::new()],
            yolo_arg: "--dangerously-skip-permissions".into(),
            // the reasoning level is in the model's name (an --effort beside it drops the model)
            title_cmd: r#"agy -p "$WEFT_TITLE_REQ" --model gemini-3.8-flash-low"#.into(),
            ..AgentCfg::default()
        },
    );
    m.insert("sh".into(), AgentCfg { label: "Shell".into(), ..AgentCfg::default() });
    m
}

fn snip(id: &str, kind: SnippetKind, title: &str, text: &str, default_on: bool) -> Snippet {
    Snippet { id: id.into(), kind, title: title.into(), text: text.into(), default_on }
}

pub fn default_library() -> Vec<Snippet> {
    vec![
        snip("plan", SnippetKind::Prompt, "Plan", "Analyze the task and write a concrete, step-by-step implementation plan. Do not modify any files.\n\nTask:\n", false),
        snip("implement", SnippetKind::Prompt, "Implement", "Implement the following. Keep the change focused, make sure the project builds and the existing tests pass.\n\n", false),
        snip("review", SnippetKind::Prompt, "Review", "Review the work from the previous step: look for bugs, missed edge cases and regressions. List concrete findings with file and line. Do not modify files.\n\n{{inputs}}", false),
        snip("fix", SnippetKind::Prompt, "Fix findings", "Fix the issues listed below, then re-run the tests.\n\n{{inputs}}", false),
        snip("tests", SnippetKind::Prompt, "Write tests", "Write tests for the behaviour described below. Run them and make sure they pass.\n\n", false),
        snip("summarize", SnippetKind::Prompt, "Summarize", "Summarize the results below into a short report for the user: what was done, what is open.\n\n{{inputs}}", false),
        snip("report-done", SnippetKind::Addendum, "Report with weft commands", crate::words::REPORT_ADDENDUM, true),
        snip("concise", SnippetKind::Addendum, "Be concise", "Keep your answers short. No preamble, no recap of the task.", false),
        snip("no-commit", SnippetKind::Addendum, "No commits", "Do not commit or push. Leave all changes in the working tree.", false),
        snip("scope", SnippetKind::Addendum, "Stay in scope", "Only change what the task requires. Mention anything else you notice instead of fixing it.", false),
    ]
}

/// When this binary was built (see build.rs).
pub const BUILD: &str = env!("WEFT_BUILD");

// ---------------------------------------------------------------- paths & io

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("weft")
}

/// The user's own settings: only what they changed, merged over the built-in defaults.
pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/state")).join("weft")
}

/// FNV-1a: a small, stable hash, for noticing that a screen changed and for short ids.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Short, stable id for a project directory (FNV-1a of its canonical path).
pub fn project_id(dir: &Path) -> String {
    let h = fnv1a(dir.to_string_lossy().as_bytes());
    format!("{:08x}", (h ^ (h >> 32)) as u32)
}

pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(tmp, path)
}

pub fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn save_json<T: Serialize>(path: &Path, v: &T) {
    if let Ok(s) = serde_json::to_vec_pretty(v) {
        let _ = write_atomic(path, &s);
    }
}

/// Objects merge key by key (so a partial `agents.codex` entry keeps the other
/// built-in fields); arrays and scalars from the user replace the default.
fn merge(base: &mut serde_json::Value, user: &serde_json::Value) {
    use serde_json::Value;
    match (base, user) {
        (Value::Object(b), Value::Object(u)) => {
            for (k, v) in u {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, u) => *b = u.clone(),
    }
}

/// Built-in defaults overlaid with the user's `config.json`. The file only ever
/// holds what the user changed, so upgrades keep improving the defaults.
pub fn load_config() -> Config {
    let mut base = serde_json::to_value(Config::default()).unwrap_or_default();
    if let Some(user) = load_json::<serde_json::Value>(&config_path()) {
        merge(&mut base, &user);
    }
    serde_json::from_value(base).unwrap_or_default()
}

/// Persist one key of the user's config without freezing any other default.
pub fn save_config_key(path: &Path, key: &str, value: serde_json::Value) {
    let mut cur = load_json::<serde_json::Value>(path).unwrap_or_else(|| serde_json::json!({}));
    if let Some(o) = cur.as_object_mut() {
        o.insert(key.into(), value);
    }
    save_json(path, &cur);
}

/// Keep runtime files (prompt files, state, filed work) out of `git add .`; flow.json stays committable.
pub fn ensure_weft_gitignore(dir: &Path) {
    let p = dir.join(".weft").join(".gitignore");
    match fs::read_to_string(&p) {
        Err(_) => {
            let _ = write_atomic(&p, b"run/\nshots/\nstate.json\narchive.json\narchive/\n");
        }
        // a folder from before the archive (or its packs) existed
        Ok(have) => {
            let miss: Vec<&str> = ["archive.json", "archive/"].into_iter().filter(|w| !have.lines().any(|l| l.trim() == *w)).collect();
            if !miss.is_empty() {
                let sep = if have.is_empty() || have.ends_with('\n') { "" } else { "\n" };
                let _ = write_atomic(&p, format!("{have}{sep}{}\n", miss.join("\n")).as_bytes());
            }
        }
    }
}
