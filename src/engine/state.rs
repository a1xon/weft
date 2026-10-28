//! What the engine remembers about every node while it runs, and the part of it that survives a restart.

use super::*;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    New,
    Draft,
    Waiting,
    Blocked,
    Starting,
    Ready,
    Busy,
    Idle,
    Attention,
    Done,
    Failed,
    Exited,
    Stopped,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) from: String,
    pub(super) name: String,
    pub(super) ok: bool,
    pub(super) text: String,
    #[serde(default)]
    pub(super) kind: InputKind,
}

/// What an input is.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InputKind {
    /// a result that arrived along a link
    #[default]
    #[serde(rename = "")]
    Result,
    /// the note of the agent being taken over
    Handover,
    /// what was fixed since a card failed, when it runs again (`weft retry`)
    Fixed,
    /// a card's share of the list a "for each" link handed on (each.rs)
    Part,
}

/// Why a run started: the flow keeps going after a flow-caused run; a run the
/// user starts by chatting waits for an explicit hand-over.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub(super) enum Cause {
    #[default]
    Flow,
    User,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct QItem {
    pub(super) id: u64,
    pub(super) text: String,
    /// What the user should see: the prompt without boilerplate addenda.
    pub(super) shown: String,
    pub(super) src: Src,
}

/// Where a queued prompt came from. On disk and on the wire it is `"you"`, `"flow"` or
/// `"gate:<node>"`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(super) enum Src {
    /// typed by a person
    You,
    /// queued by the flow when a link fired
    Flow,
    /// a request from another node (a gate, a git step, a queued prompt, a takeover): the
    /// agent's answer goes back to it instead of down the links
    Reply(String),
}

impl From<String> for Src {
    fn from(s: String) -> Src {
        match s.as_str() {
            "flow" => Src::Flow,
            _ => s.strip_prefix("gate:").map_or(Src::You, |n| Src::Reply(n.to_string())),
        }
    }
}

impl From<Src> for String {
    fn from(s: Src) -> String {
        match s {
            Src::You => "you".into(),
            Src::Flow => "flow".into(),
            Src::Reply(n) => format!("gate:{n}"),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Step {
    pub(super) prompt: String,
    pub(super) before: Vec<String>,
    /// the agent was asked to report with `weft done|issue|fail`
    pub(super) expect_signal: bool,
    /// the step was asked to end with a `STATUS:` line
    pub(super) rule: bool,
    /// no flow takes its answer (a lone card, the last of a chain): a missing status line is not
    /// asked for, silence is the end
    pub(super) lone: bool,
    /// weft has already asked this step once more (a reminder, or the missing status line)
    pub(super) asked: bool,
    /// its output ends with its last word (DONE, FAILED, ISSUE): nothing it left running holds it
    pub(super) closed: bool,
    /// the person's own task, not one of weft's requests: a report that still asks them something
    /// waits for their answer (see `Engine::finish_task`)
    pub(super) task: bool,
    /// the report it gave while it asked the person something: the step's result if they let it go
    pub(super) report: Option<String>,
    /// it handed its result in itself (`weft done|fail`): what it says is not read off the screen
    pub(super) given: bool,
    /// the questions the person answered on the way (quoted in their answer): asked again, they hold nothing
    pub(super) answered: Vec<String>,
    /// the report they answered: what is read while the agent works on their answers
    pub(super) earlier: Option<String>,
    /// what a model read in its report or ISSUE (reads.rs): the questions left to the person, with
    /// answers to pick. None = weft's rules read it.
    pub(super) asks: Option<Vec<asks::Ask>>,
    /// the model's reading of its report or ISSUE that is on its way (its ticket; 0 = none). A step
    /// whose report is being read is still at work: it is done or asks once the reading is in.
    pub(super) reading: u64,
    /// what weft typed into the session after the prompt: part of the echo, not of the answer
    pub(super) extra: String,
    /// The conversation as the screen showed it while this step ran, rebuilt row by row: a program
    /// on the alternate screen shows only its last rows, so the head of a long answer is on no
    /// screen at the end. Rows `base..` are the answer.
    pub(super) acc: Vec<String>,
    pub(super) base: usize,
}

/// Rows of an answer that are kept: far more than anything an agent writes, so that a program that
/// redraws its whole history in a loop cannot grow the state without bound.
const MAX_ACC: usize = 4000;

impl Step {
    pub(super) fn new(prompt: String, before: Vec<String>) -> Step {
        Step { prompt, before, ..Step::default() }
    }

    /// The screen changed: lay its rows over the answer so far (see `term::merge_frame`).
    pub(super) fn see(&mut self, screen_rows: &[String], chrome: &[String]) {
        let typed = self.typed();
        if self.acc.is_empty() {
            self.acc = term::response_rows(&self.before, &typed, chrome);
            self.base = self.acc.len();
        }
        let frame = term::response_rows(screen_rows, &typed, chrome);
        term::merge_frame(&mut self.acc, &frame);
        if self.acc.len() > MAX_ACC {
            let cut = self.acc.len() - MAX_ACC;
            self.acc.drain(..cut);
            self.base = self.base.saturating_sub(cut);
        }
    }

    /// The answer as the frames built it, when it holds more than the last screen alone does and
    /// ends the way the last screen ends (`tail`: what that screen shows of it). A frame that was
    /// put in the wrong place must not win over what is on the screen.
    pub(super) fn rebuilt(&self, tail: Option<&str>) -> Option<String> {
        let rows = self.acc.get(self.base..).filter(|r| r.iter().any(|l| !l.is_empty()))?;
        let text = term::dedent(rows.join("\n").trim_matches('\n'));
        let Some(tail) = tail else { return Some(text) };
        let end = |s: &str| s.lines().map(str::trim).filter(|l| !l.is_empty()).rev().take(3).map(String::from).collect::<Vec<_>>();
        (text.len() > tail.len() && end(&text) == end(tail)).then_some(text)
    }

    /// The screen history was rebuilt (a daemon restart): what the agent says from here on is read
    /// after the echo of what weft typed, or, with no echo left on it, after what is there now.
    pub(super) fn rebase(&mut self, mut lines: Vec<String>) {
        if let Some(i) = term::echo_end(&lines, &term::probe_of(&self.typed())) {
            lines.truncate(i + 1);
        }
        self.before = lines;
    }

    /// Is the echo of what weft typed on these lines? (Then `rebase` puts the step's start there.)
    pub(super) fn echoed_in(&self, lines: &[String]) -> bool {
        term::echo_end(lines, &term::probe_of(&self.typed())).is_some()
    }

    /// Everything weft typed for this step.
    pub(super) fn typed(&self) -> String {
        if self.extra.is_empty() {
            self.prompt.clone()
        } else {
            format!("{}\n{}", self.prompt, self.extra)
        }
    }

    /// What `report` asks the person that they have not answered yet (offers included: they hold
    /// nothing, but are shown with the rest).
    pub(super) fn asks_in(&self, report: &str) -> Vec<asks::Ask> {
        asks::asks(report).into_iter().filter(|a| !self.answered.contains(&asked_key(&a.text))).collect()
    }
}

/// A question as it is compared with the ones answered: case and spacing aside.
pub(super) fn asked_key(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// A message weft pasted into a session, until it has been seen taken. The Enter that submits it
/// follows the paste a moment later and is checked: a program that was still starting loses the
/// text, one that reads the Enter as a newline leaves it sitting in its input box, and a screen
/// that does not show the message anywhere is not an agent that has finished.
pub(super) struct Typing {
    pub(super) text: String,
    /// the tail of its last line, to find it on the screen (see `term::probe_of`)
    pub(super) probe: String,
    /// how often the message (or a placeholder for it) was on the screen before it was pasted
    pub(super) traces_before: usize,
    /// when the paste, or the last Enter, went out
    pub(super) at: Instant,
    pub(super) entered: bool,
    /// Enters repeated, pastes repeated
    pub(super) enters: u8,
    pub(super) pastes: u8,
    /// a task prompt (checked, repeated) or a note to a working agent (sent once)
    pub(super) checked: bool,
    /// the agent was in the middle of a turn when it went out (see `Rt::turn`)
    pub(super) turn: bool,
}

/// What `Typing::next` asks the engine to do.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Typed {
    Wait,
    Enter,
    Paste,
    Taken,
    GaveUp,
}

/// Enters weft repeats for a message that stays in the input box, and pastes it repeats.
const ENTER_RETRIES: u8 = 2;
const PASTE_RETRIES: u8 = 1;
/// How long after an Enter the program has to show it took the message.
const TAKEN_WITHIN: Duration = Duration::from_millis(1500);
/// A screen that shows no trace of the message and has been still this long is waiting for it.
const STILL_FOR: Duration = Duration::from_millis(1000);
/// ... and the message is pasted again only this long after the first try.
const LOST_AFTER: Duration = Duration::from_millis(3000);

impl Typing {
    pub(super) fn new(text: &str, checked: bool, screen: &vt100::Screen, now: Instant) -> Typing {
        let probe = term::probe_of(text);
        let traces_before = term::traces(screen, &probe);
        Typing { text: text.to_string(), probe, traces_before, at: now, entered: false, enters: 0, pastes: 0, checked, turn: false }
    }

    /// The agent started a turn after the Enter (`turn`: it shows it is in one now): it has the
    /// message, wherever its echo went (folded, or scrolled away by what it is doing).
    pub(super) fn started_turn(&self, turn: bool) -> bool {
        self.entered && turn && !self.turn
    }

    /// What to do now, given the screen and when it last changed.
    pub(super) fn next(&self, screen: &vt100::Screen, last_change: Instant, now: Instant, submit: Duration) -> Typed {
        let waited = now.saturating_duration_since(self.at);
        if !self.entered {
            return if waited >= submit { Typed::Enter } else { Typed::Wait };
        }
        if !self.checked {
            return Typed::Taken;
        }
        if waited < TAKEN_WITHIN {
            return Typed::Wait;
        }
        if term::input_holds(screen, &self.probe) {
            // it is still in the box: the Enter was not taken as a submit
            return if self.enters < ENTER_RETRIES { Typed::Enter } else { Typed::GaveUp };
        }
        if term::traces(screen, &self.probe) > self.traces_before {
            // it left the box and is in the transcript (or folded into a placeholder): the program has it
            return Typed::Taken;
        }
        // no trace of it anywhere: the paste went to a program that was not reading yet. A screen
        // that is still repainting (starting up) is given time; one that is still is waiting for it
        if now.saturating_duration_since(last_change) < STILL_FOR || waited < LOST_AFTER {
            Typed::Wait
        } else if self.pastes < PASTE_RETRIES {
            Typed::Paste
        } else {
            Typed::GaveUp
        }
    }
}

/// An agent said it is waiting for something; weft checks on it now and then.
pub(super) struct Waiting {
    pub(super) what: String,
    /// how many times it has been woken
    pub(super) checks: u32,
    /// between the status line and the next check: silence is expected, for a longer while
    pub(super) holding: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct StepRec {
    pub(super) prompt: String,
    pub(super) text: String,
    pub(super) ok: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum How {
    Signal,
    Quiet,
    Exit,
}

/// Everything the engine knows about one node while it runs.
pub(super) struct Rt {
    // ---- the process and its screen
    pub(super) pane: Option<String>,
    pub(super) win: Option<String>,
    pub(super) parser: vt100::Parser,
    pub(super) alive: bool,
    pub(super) spawning: bool,
    /// the start that is in flight (what `Ev::Spawned` has to carry to be taken)
    pub(super) spawn: Option<u64>,
    /// the screen changed since the engine last looked at it
    pub(super) dirty: bool,
    /// the process has drawn something since it started (a shell still loading its rc files has not)
    pub(super) painted: bool,
    pub(super) hash: u64,
    pub(super) last_change: Instant,
    /// a dialog on the screen needs a human (trust, update, login)
    pub(super) attn: bool,
    /// the agent reported an ISSUE (`weft issue`): it cannot go on without the person, and says what it needs
    pub(super) issue: Option<String>,
    /// the agent said it waits for something (`STATUS: WAITING`)
    pub(super) waiting: Option<Waiting>,
    /// its CLI ran into the usage limit: it sleeps until then (see limits.rs), and how often this step did
    pub(super) limit: Option<Limit>,
    pub(super) limit_sleeps: u32,
    /// how often this step's turn broke off on an API error (the waits grow with it; a restart
    /// starts them short again)
    pub(super) error_tries: u32,
    /// its steps as snapshots of its folder, before and after (see rewind.rs); the last number given;
    /// and what the next prompt tells it about a rewind the person did
    pub(super) cps: Vec<Cp>,
    pub(super) cp_seq: u32,
    pub(super) rewound: Option<String>,
    /// a message pasted that has not been seen taken yet
    pub(super) typing: Option<Typing>,
    /// what the person typed into the terminal from the page since their last Enter
    pub(super) keyed: String,
    /// this session was told what weft is (the first line of its first message)
    pub(super) briefed: bool,
    /// this session was given the status line rule in full (later prompts get a one-line reminder)
    pub(super) ruled: bool,
    /// the node's own git worktree, if it runs in one
    pub(super) worktree: Option<String>,
    /// the CLI's id for the card's conversation, so a later process takes it up again (resume.rs)
    pub(super) session: Option<String>,
    /// the process was started on that conversation again and has not taken a prompt yet
    pub(super) resuming: bool,
    /// a new process of a card that worked before, on a new conversation: its first prompt says what it did
    pub(super) blank: bool,
    /// a twin's first process: its first prompt says that it is one
    pub(super) twin_due: bool,
    /// a resting agent was asked to end (ms since the epoch): its window closes once it has
    pub(super) retiring: Option<u64>,
    /// a "still working in the background" marker is on screen; since when; and whether it holds
    /// the agent back from finishing right now
    pub(super) work: bool,
    pub(super) work_since: Option<Instant>,
    pub(super) bg: bool,
    /// the agent shows it is in the middle of a turn ("esc to interrupt")
    pub(super) turn: bool,
    /// when the text on the screen last changed (a hold on a busy agent does not count)
    pub(super) screen_changed: Instant,
    /// tmux was asked what the pane really shows while the screen sat unchanged under a turn marker
    pub(super) verified: Option<Instant>,

    // ---- the run
    pub(super) status: Status,
    pub(super) since: u64,
    /// Screen-quiet detection is active (we or the user gave it something to do).
    pub(super) armed: bool,
    pub(super) finished: bool,
    pub(super) ok: Option<bool>,
    pub(super) output: Option<String>,
    /// the output was read off the screen: the agent went quiet without handing a result in, so
    /// what it said last is at the end (a report reads from its start)
    pub(super) out_read: bool,
    pub(super) queue: VecDeque<QItem>,
    pub(super) cur: Option<Step>,
    pub(super) steps: Vec<StepRec>,
    pub(super) asked: String,
    pub(super) inputs: Vec<(String, Input)>,
    /// what its last trigger took, while that is not taken up yet: the prompt waiting in the queue
    /// (its id) or a gate's open round. What arrives meanwhile joins it.
    pub(super) unstarted: Option<(u64, Vec<(String, Input)>)>,
    pub(super) runs: u32,
    pub(super) cause: Cause,
    /// links that have yet to get its result: it had none for them yet, or more to do first (`propagate`)
    pub(super) unsent: Vec<String>,
    /// this run answers a gate's (or a step's) request: its result goes there, not down the links
    pub(super) reply_to: Option<String>,
    /// when the current run began (ms since the epoch)
    pub(super) run_ms: u64,
    /// when it came back from the archive (ms since the epoch): its archive clock starts again then
    pub(super) revived: u64,
    /// when someone wrote a prompt into it (ms since the epoch): its archive clock stood then
    pub(super) writes: Vec<(u64, u64)>,

    // ---- what the browsers were last told
    pub(super) ui: bool,
    pub(super) scr: bool,
    pub(super) last_msg: String,
    /// who the browsers were last told is in this node's way
    pub(super) blocked_by: Option<String>,
    /// when the browsers were last told it goes to the archive by itself
    pub(super) filing: Option<u64>,
    /// a finished flow that a draft nobody armed keeps on the canvas: that card, and why (loose.rs)
    pub(super) held: Option<(String, String)>,

    // ---- the node's role
    pub(super) gate: GateRound,
    pub(super) hand: Takeover,
    /// step node: the agent that is answering its prompt
    pub(super) step_wait: Option<String>,
    pub(super) checks: Checks,
    pub(super) git: GitRound,
    /// the agents it talks with: what it was told, what waits for it, what it said
    pub(super) talk: TalkRt,
}

/// A gate's round: the results it holds and where the round stands.
#[derive(Default)]
pub(super) struct GateRound {
    /// the merged results being held while the round is open
    pub(super) held: Option<String>,
    pub(super) phase: GatePhase,
    pub(super) stage: GateStage,
    pub(super) round: u32,
    pub(super) targets: Vec<String>,
    pub(super) wait: Vec<String>,
    pub(super) shots: Vec<String>,
    pub(super) out: String,
    pub(super) failed: bool,
    /// bumped when a round restarts: a capture or test run that finishes late is dropped
    pub(super) gen: u64,
    /// when the gate last opened a round (ms, only ever goes up): its screenshots are named after it
    pub(super) opened: u64,
    /// a CI check: its watch of the runs is going on, the commit it looks at, and what the runs show
    pub(super) ci: bool,
    pub(super) ci_sha: String,
    pub(super) ci_runs: Vec<CiRun>,
}

impl GateRound {
    /// Back to idle; the generation moves on so nothing in flight can land in the next round.
    pub(super) fn reset(&mut self) {
        let gen = self.gen + 1;
        *self = GateRound { gen, ..GateRound::default() };
    }

    /// A restart cut what the round was doing short: it is the person's turn.
    pub(super) fn cut_short(&mut self) {
        self.phase = GatePhase::Review;
        self.wait.clear();
        self.out = format!("{}\n(interrupted by a restart: retake or run it again)", self.out).trim().to_string();
    }
}

/// This agent is taking over from another one; `phase` says what it is waiting for.
#[derive(Default)]
pub(super) struct Takeover {
    pub(super) from: Option<String>,
    pub(super) phase: HandPhase,
}

/// What its watches collected, and the ETA they keep.
#[derive(Default)]
pub(super) struct Checks {
    /// the agent's estimate of how long it will take (a watch of kind "eta")
    pub(super) eta: Option<Eta>,
    /// when weft last asked for an estimate it did not get
    pub(super) eta_asked: u64,
    /// evidence its watches collected, newest last
    pub(super) feed: Vec<FeedItem>,
    pub(super) feed_seq: u64,
    /// when each watch last asked, and which ones have a command running
    pub(super) at: HashMap<String, u64>,
    pub(super) busy: std::collections::HashSet<String>,
    /// standing orders (ETA, screenshots) already given in this session
    pub(super) told: std::collections::HashSet<String>,
}

/// A commit, push or PR step's round: what it still has to do, what it has seen, what it carries.
#[derive(Default)]
pub(super) struct GitRound {
    pub(super) jobs: VecDeque<GitJob>,
    /// ids of the links that delivered this round
    pub(super) seen: Vec<String>,
    pub(super) log: Vec<String>,
    /// a job (or an agent's answer) is outstanding
    pub(super) busy: bool,
    /// the closing part of the round (push, PR, or settling) has started
    pub(super) finishing: bool,
    /// "commit now" / "push now": go with what is there
    pub(super) force: bool,
    /// bumped on reset: a job that finishes late is dropped
    pub(super) gen: u64,
    /// the agent a request is waiting on, and what it was asked
    pub(super) wait: Option<String>,
    pub(super) stage: GitStage,
    /// who just committed (and in which worktree), kept so a merge conflict can be handed back
    pub(super) last: Option<GitLast>,
    pub(super) conflicts: u32,
    /// what this round passes on to the steps after it: who did the work, and what the repo looks like now
    pub(super) carry: Vec<Carry>,
    pub(super) facts: Facts,
    /// the repositories still to push (or to open a pull request in) this round, and the one at hand
    pub(super) repos: VecDeque<PathBuf>,
    pub(super) at: Option<PathBuf>,
    /// where each repository was pushed to this round ("<folder> <hash>"), and where it was in the
    /// round this push last handed on: a round that ends there brings the cards after it nothing new
    pub(super) heads: Vec<String>,
    pub(super) handed: Vec<String>,
    /// a push nobody armed: what git says it has left to do, and when it was last looked at (loose.rs)
    pub(super) left: Option<Left>,
    pub(super) looked: u64,
}

impl GitRound {
    pub(super) fn reset(&mut self) {
        let gen = self.gen + 1;
        *self = GitRound { gen, ..GitRound::default() };
    }
}

/// The commit request that went out last.
#[derive(Clone)]
pub(super) struct GitLast {
    pub(super) name: String,
    pub(super) text: String,
    pub(super) wt: Option<PathBuf>,
    /// the agent node that was asked
    pub(super) agent: String,
}

/// What the agent last said about how long it will take.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Eta {
    /// when it should be done (ms since the epoch)
    pub(super) at: u64,
    /// when that was said, and the estimate then, in seconds
    pub(super) set: u64,
    pub(super) total: u64,
    pub(super) note: String,
    /// when weft last asked for a new one because this one ran out
    #[serde(default)]
    pub(super) asked: u64,
}

/// One piece of evidence an agent's watch collected: a screenshot or a test run.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct FeedItem {
    pub(super) id: u64,
    pub(super) check: String,
    pub(super) at: u64,
    /// image file in the shots folder; empty for text evidence
    #[serde(default)]
    pub(super) img: String,
    #[serde(default)]
    pub(super) note: String,
    #[serde(default)]
    pub(super) ok: Option<bool>,
    #[serde(default)]
    pub(super) state: FeedState,
    #[serde(default)]
    pub(super) reply: String,
}

/// What happened to a piece of evidence.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FeedState {
    #[default]
    New,
    /// approved with a click
    Ok,
    /// the agent was told what to change
    Steered,
}

/// Where a takeover stands: waiting for the old agent to finish its step, or for its note.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum HandPhase {
    #[default]
    None,
    Settle,
    Asking,
}

/// Where a gate is in its round: idle, waiting on an agent, running a command, or waiting for you.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GatePhase {
    #[default]
    Idle,
    Asking,
    Running,
    Review,
}

/// What the agent was asked for, so its answer is read the right way.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GateStage {
    #[default]
    None,
    Shot,
    Write,
    Fix,
    RunByAgent,
}

/// One agent's part in a commit or push round, kept so the report that goes on can say who did what.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Carry {
    pub(super) name: String,
    /// "claude", "claude \u{b7} queued prompt", "sh"
    pub(super) agent: String,
    /// the agent node that did the work (a queued prompt or a gate hands on its agent's work)
    #[serde(default)]
    pub(super) node: String,
    /// what it was asked, and what it answered
    pub(super) task: String,
    pub(super) result: String,
    /// the commit that came of it, as the commit step logged it
    pub(super) commit: String,
}

/// What a commit or push step knows about the repository once it is done.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Facts {
    pub(super) branch: String,
    /// "a1b2c3d subject"
    pub(super) head: String,
    /// "a1b2c3d subject", newest first
    pub(super) commits: Vec<String>,
    pub(super) files: Vec<String>,
    /// the pull request, once one is open
    #[serde(default)]
    pub(super) pr: String,
    #[serde(default)]
    pub(super) pr_title: String,
    /// every pull request, when the work is in several repositories (one each)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) prs: Vec<String>,
}

/// Which job of a git step has finished.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GitJobKind {
    /// a commit (or merge): the round goes on with the next arrival or settles
    Commit,
    /// the push: the round goes on to the pull request or settles
    Push,
    /// a look at what a push nobody armed has left to do (loose.rs): changes nothing
    Look,
}

/// What a commit step is waiting for an agent to answer.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(super) enum GitStage {
    /// "commit your work"
    #[default]
    Commit,
    /// "the push was rejected: bring the branch up to date"
    Rebase,
    /// "open the pull request"
    Pr,
}

impl Facts {
    /// Fold what a later job found into what is known: the newest HEAD wins, commits and files add up.
    pub(super) fn absorb(&mut self, f: Facts) {
        if !f.branch.is_empty() {
            self.branch = f.branch;
        }
        if !f.head.is_empty() {
            self.head = f.head;
        }
        if !f.pr.is_empty() {
            self.pr = f.pr;
            self.pr_title = f.pr_title;
        }
        for p in f.prs {
            if !self.prs.contains(&p) {
                self.prs.push(p);
            }
        }
        let new: Vec<String> = f.commits.into_iter().filter(|c| !self.commits.contains(c)).collect();
        self.commits.splice(0..0, new);
        self.files.extend(f.files);
        self.files.sort();
        self.files.dedup();
    }
}

#[derive(Clone)]
pub(super) enum GitJob {
    /// Ask the agent that did the work to commit it (it knows what it changed and why).
    /// `wt` is its own worktree, if it has one: its branch is merged afterwards.
    Ask { from: String, name: String, text: String, wt: Option<PathBuf>, resolve: bool },
    /// Nobody to ask (a command, or an agent that is gone): commit mechanically.
    Commit { agent: String, name: String, text: String, wt: Option<PathBuf> },
}

impl Rt {
    pub(super) fn new(cols: u16, rows: u16) -> Rt {
        Rt {
            pane: None,
            win: None,
            parser: vt100::Parser::new(rows, cols, SCROLLBACK),
            alive: false,
            spawning: false,
            spawn: None,
            dirty: false,
            painted: false,
            hash: 0,
            last_change: Instant::now(),
            attn: false,
            issue: None,
            waiting: None,
            limit: None,
            limit_sleeps: 0,
            error_tries: 0,
            cps: Vec::new(),
            cp_seq: 0,
            rewound: None,
            typing: None,
            keyed: String::new(),
            briefed: false,
            ruled: false,
            worktree: None,
            session: None,
            resuming: false,
            blank: false,
            twin_due: false,
            retiring: None,
            work: false,
            work_since: None,
            bg: false,
            turn: false,
            screen_changed: Instant::now(),
            verified: None,
            status: Status::New,
            since: now_ms(),
            armed: false,
            finished: false,
            ok: None,
            output: None,
            out_read: false,
            queue: VecDeque::new(),
            cur: None,
            steps: Vec::new(),
            asked: String::new(),
            inputs: Vec::new(),
            unstarted: None,
            runs: 0,
            cause: Cause::Flow,
            unsent: Vec::new(),
            reply_to: None,
            run_ms: 0,
            revived: 0,
            writes: Vec::new(),
            ui: true,
            scr: true,
            last_msg: String::new(),
            blocked_by: None,
            filing: None,
            held: None,
            gate: GateRound::default(),
            hand: Takeover::default(),
            step_wait: None,
            checks: Checks::default(),
            git: GitRound::default(),
            talk: TalkRt::default(),
        }
    }

    /// What the card shows changes, and since when. A finished card has no countdown, whatever time was left.
    pub(super) fn set_status(&mut self, s: Status) {
        if self.status == s {
            return;
        }
        if matches!(s, Status::Done | Status::Failed | Status::Stopped | Status::Exited) {
            self.checks.eta = None;
        }
        self.status = s;
        self.since = now_ms();
        self.ui = true;
    }

    /// What survives a restart.
    pub(super) fn to_saved(&self) -> Saved {
        Saved {
            feed: self.checks.feed.clone(),
            feed_seq: self.checks.feed_seq,
            eta: self.checks.eta.clone(),
            hand_from: if self.hand.phase != HandPhase::None { self.hand.from.clone() } else { None },
            armed: self.armed,
            issue: self.issue.clone(),
            limit: self.limit.clone(),
            limit_sleeps: self.limit_sleeps,
            cps: self.cps.clone(),
            cp_seq: self.cp_seq,
            rewound: self.rewound.clone(),
            briefed: self.briefed,
            ruled: self.ruled,
            finished: self.finished,
            ok: self.ok,
            output: self.output.clone(),
            out_read: self.out_read,
            runs: self.runs,
            inputs: self.inputs.clone(),
            worktree: self.worktree.clone(),
            session: self.session.clone(),
            blank: self.blank,
            twin_due: self.twin_due,
            queue: self.queue.iter().cloned().collect(),
            steps: self.steps.clone(),
            asked: self.asked.clone(),
            cause: self.cause,
            unsent: self.unsent.clone(),
            gate: self.gate.held.clone(),
            gate_phase: self.gate.phase,
            gate_round: self.gate.round,
            gate_shots: self.gate.shots.clone(),
            gate_out: self.gate.out.clone(),
            gate_targets: self.gate.targets.clone(),
            gate_stage: self.gate.stage,
            gate_opened: self.gate.opened,
            git_seen: self.git.seen.clone(),
            git_log: self.git.log.clone(),
            git_carry: self.git.carry.clone(),
            git_facts: self.git.facts.clone(),
            git_handed: self.git.handed.clone(),
            since: self.since,
            revived: self.revived,
            cur: self.cur.clone(),
            reply_to: self.reply_to.clone(),
            step_wait: self.step_wait.clone(),
            talk_told: self.talk.told.clone(),
            talk_inbox: self.talk.inbox.clone(),
            talk_said: self.talk.said.clone(),
        }
    }

    /// Take back what `to_saved` wrote. A gate that was asking or running when the daemon stopped
    /// goes back to review, so a person decides. The step an agent was answering goes on if the
    /// agent comes back (`Engine::adopt`): its answer still goes where it was asked for.
    pub(super) fn restore(&mut self, s: Saved) {
        self.checks.feed = s.feed;
        self.checks.feed_seq = s.feed_seq;
        self.checks.eta = s.eta;
        self.hand.from = s.hand_from;
        self.armed = s.armed;
        self.issue = s.issue;
        self.limit = s.limit;
        self.limit_sleeps = s.limit_sleeps;
        // (a snapshot that was being taken when the daemon stopped never arrives)
        self.cps = s.cps.into_iter().filter(|c| !c.before.is_empty()).collect();
        self.cp_seq = s.cp_seq;
        self.rewound = s.rewound;
        self.briefed = s.briefed;
        self.ruled = s.ruled;
        self.finished = s.finished;
        self.ok = s.ok;
        self.output = s.output;
        self.out_read = s.out_read;
        self.runs = s.runs;
        self.inputs = s.inputs;
        self.worktree = s.worktree;
        self.session = s.session;
        self.blank = s.blank;
        self.twin_due = s.twin_due;
        self.queue = s.queue.into();
        self.steps = s.steps;
        self.asked = s.asked;
        self.cause = s.cause;
        self.unsent = s.unsent;
        self.gate.held = s.gate;
        self.gate.phase = s.gate_phase;
        self.gate.round = s.gate_round;
        self.gate.shots = s.gate_shots;
        self.gate.out = s.gate_out;
        self.gate.targets = s.gate_targets;
        self.gate.stage = s.gate_stage;
        self.gate.opened = s.gate_opened;
        // a capture or test run went with the daemon; what it asked of an agent may still be answered
        // (`resume_gates` looks)
        if self.gate.phase == GatePhase::Running {
            self.gate.cut_short();
        }
        self.git.seen = s.git_seen;
        self.git.log = s.git_log;
        self.git.carry = s.git_carry;
        self.git.facts = s.git_facts;
        self.git.handed = s.git_handed;
        self.cur = s.cur;
        self.reply_to = s.reply_to;
        self.step_wait = s.step_wait;
        self.revived = s.revived;
        self.talk.told = s.talk_told;
        self.talk.inbox = s.talk_inbox;
        self.talk.said = s.talk_said;
        if self.finished {
            self.status = if self.ok == Some(false) { Status::Failed } else { Status::Done };
            // "done 2h ago" stays true across a restart (and so does its archive clock)
            if s.since > 0 {
                self.since = s.since;
            }
        }
        if self.gate.phase == GatePhase::Asking {
            self.status = Status::Busy;
        } else if self.gate.held.is_some() || (self.issue.is_some() && !self.finished) {
            self.status = Status::Attention;
        } else if !self.finished && !self.queue.is_empty() {
            // no process came back with the state and weft starts none by itself: what is queued
            // waits for a person, and the card says so instead of "starting"
            self.status = Status::Stopped;
        }
    }

    /// The screen changed: look for a dialog that needs a person and for the marker of work the
    /// agent started in the background. Only a change in the text counts (a repaint of the same
    /// screen is not activity). True if the text changed.
    pub(super) fn read_screen(&mut self, agent: Option<&AgentCfg>, now: Instant) -> bool {
        self.dirty = false;
        let text = self.parser.screen().contents();
        let h = fnv(&text);
        if h == self.hash {
            return false;
        }
        self.hash = h;
        self.last_change = now;
        self.screen_changed = now;
        self.verified = None;
        let (attention, working, busy) = agent.map_or((&[][..], &[][..], &[][..]), |a| (&a.attention[..], &a.working[..], &a.busy[..]));
        self.attn = term::dialog_on(&text, attention);
        // the status line lives just above the input box: look only at the bottom of the screen,
        // so a sentence that merely mentions it, scrolled up, does not hold anything back
        let bottom: String = text.lines().rev().filter(|l| !l.trim().is_empty()).take(10).collect::<Vec<_>>().join("\n");
        self.work = working.iter().any(|p| !p.is_empty() && bottom.contains(p.as_str()));
        // the turn marker sits in the status bar, the last lines: not in what the agent wrote above them
        let bar: String = bottom.lines().take(4).collect::<Vec<_>>().join("\n");
        self.turn = busy.iter().any(|p| !p.is_empty() && bar.contains(p.as_str()));
        self.ui = true;
        self.scr = true;
        true
    }

    /// The screen changed under a step whose result a flow takes: lay it over the answer so far.
    pub(super) fn accumulate(&mut self, chrome: &[String]) {
        if !self.cur.as_ref().is_some_and(|c| c.rule || c.expect_signal) {
            return;
        }
        let (_, cols) = self.parser.screen().size();
        let rows: Vec<String> = self.parser.screen().rows(0, cols).map(|r| r.trim_end().to_string()).collect();
        if let Some(c) = self.cur.as_mut() {
            c.see(&rows, chrome);
        }
    }

    /// Has a turn marker sat on an unchanged screen long enough to ask tmux whether it is real?
    pub(super) fn needs_check(&self, now: Instant) -> bool {
        self.turn && self.alive && self.verified.is_none() && now.duration_since(self.screen_changed) >= VERIFY_AFTER
    }

    /// An agent in the middle of a turn is not finished, however long its output pauses. One that
    /// ended its turn but announced work in the background ("1 shell still running", it will answer
    /// when that ends) is not finished either: keep it busy, for a while. A step that already gave its
    /// last word is not held: what it left running (a server, a loop that never ends) is not the step.
    pub(super) fn hold_for_background(&mut self, idle_finish: bool, hold: Duration, now: Instant) {
        if self.turn && now.duration_since(self.screen_changed) >= hold {
            // a turn redraws its clock every second: a marker on a screen that has not moved for this
            // long is a leftover of a repaint that did not clear it, not an agent that is thinking
            self.turn = false;
        }
        if self.turn && self.armed && !self.finished && idle_finish && !self.attn {
            // in the middle of a turn: a pause in its output (a long think, a loaded machine) is not its end
            self.last_change = now;
        }
        let closed = self.cur.as_ref().is_some_and(|c| c.closed);
        let holding = self.work && self.armed && !self.finished && idle_finish && !self.attn && !closed;
        if holding {
            let since = *self.work_since.get_or_insert(now);
            if now.duration_since(since) < hold {
                self.last_change = now;
            }
        } else {
            self.work_since = None;
        }
        let bg = holding && self.work_since.is_some_and(|s| now.duration_since(s) < hold);
        if bg != self.bg {
            self.bg = bg;
            self.ui = true;
        }
    }

    /// Can a prompt be typed now? A TUI that is still starting (a shell loading its rc files, an
    /// agent drawing its first screen) loses or garbles what is typed into it, so it must have drawn
    /// something and then gone quiet. One that never draws anything gets the benefit of the doubt
    /// after a while; a plain command reads its stdin whenever.
    pub(super) fn can_take_input(&self, finish: Finish, quiet: Duration, ready_d: Duration) -> bool {
        quiet >= ready_d && (finish == Finish::Exit || self.painted || quiet >= START_PATIENCE)
    }

    /// The status the screen and the node's state imply after `quiet` without a change.
    pub(super) fn implied_status(&self, finish: Finish, quiet: Duration, quiet_d: Duration, ready_d: Duration) -> Status {
        if self.finished {
            // a question or a dialog after the result: once it is answered the result stands again
            if self.attn || self.issue.is_some() {
                Status::Attention
            } else if self.turn {
                // it took something up after its result (its background shell ended, a person typed into it)
                Status::Busy
            } else if matches!(self.status, Status::Attention | Status::Busy) {
                if self.ok == Some(false) { Status::Failed } else { Status::Done }
            } else {
                self.status
            }
        } else if self.attn || self.issue.is_some() {
            Status::Attention
        } else if self.cur.as_ref().is_some_and(|c| c.reading != 0) {
            // its report is being read for questions (reads.rs): not done before that is in
            Status::Busy
        } else if !self.armed {
            if self.can_take_input(finish, quiet, ready_d) {
                Status::Ready
            } else {
                Status::Starting
            }
        } else if finish == Finish::Exit {
            Status::Busy
        } else if quiet >= quiet_d && self.typing.is_none() {
            // (a prompt not yet seen taken is not a quiet agent: see `Typing`)
            Status::Idle
        } else {
            Status::Busy
        }
    }

    /// A new run begins (not an answer to the agent's own question, which continues its run).
    pub(super) fn begin_run(&mut self, cause: Cause) {
        self.runs += 1;
        self.steps.clear();
        self.cause = cause;
        self.run_ms = now_ms();
        self.checks.eta_asked = 0;
        self.checks.eta = None;
    }

    /// A git step that finished once starts over: the old result and what it carried are dropped.
    pub(super) fn begin_git_round(&mut self) {
        self.finished = false;
        self.output = None;
        self.ok = None;
        self.git.log.clear();
        self.git.carry.clear();
        self.git.facts = Facts::default();
        self.git.finishing = false;
    }

    /// A node that was waiting for this agent's answer is gone (or gave up): its requests leave the
    /// queue and the agent is no longer answering them.
    pub(super) fn forget_requests_from(&mut self, node: &str) {
        self.queue.retain(|q| !matches!(&q.src, Src::Reply(n) if n == node));
        if self.reply_to.as_deref() == Some(node) {
            self.reply_to = None;
        }
    }

    /// Back to a clean slate for a new run of the whole flow. What you queued by hand is your
    /// content and stays; what the flow queued is progress and goes.
    pub(super) fn reset(&mut self) {
        self.inputs.clear();
        self.unstarted = None;
        self.queue.retain(|q| q.src != Src::Flow);
        self.runs = 0;
        self.finished = false;
        self.armed = false;
        self.cur = None;
        self.issue = None;
        self.waiting = None;
        self.limit = None;
        self.limit_sleeps = 0;
        self.error_tries = 0;
        self.typing = None;
        self.ok = None;
        self.output = None;
        self.steps.clear();
        self.asked.clear();
        self.unsent.clear();
        self.cause = Cause::Flow;
        self.reply_to = None;
        self.gate.reset();
        self.git.reset();
        self.ui = true;
    }
}

/// How long a process that has drawn nothing is waited for before it counts as started.
const START_PATIENCE: Duration = Duration::from_secs(8);

/// Lines of history kept per agent: enough to scroll back through a long answer and to read what
/// a step produced; every line costs a full terminal row of cells, so it is also the memory bound.
pub(super) const SCROLLBACK: usize = 1000;

/// A turn marker on a screen that has not changed for this long is checked against tmux's own copy.
pub(super) const VERIFY_AFTER: Duration = Duration::from_secs(20);

#[derive(Serialize, Deserialize, Default)]
pub(super) struct Saved {
    #[serde(default)]
    pub(super) feed: Vec<FeedItem>,
    #[serde(default)]
    pub(super) feed_seq: u64,
    #[serde(default)]
    pub(super) eta: Option<Eta>,
    /// a takeover that was waiting for its note: the old agent's id
    #[serde(default)]
    pub(super) hand_from: Option<String>,
    #[serde(default)]
    pub(super) armed: bool,
    #[serde(default)]
    pub(super) issue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) limit: Option<Limit>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub(super) limit_sleeps: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) cps: Vec<Cp>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub(super) cp_seq: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) rewound: Option<String>,
    #[serde(default)]
    pub(super) briefed: bool,
    #[serde(default)]
    pub(super) ruled: bool,
    #[serde(default)]
    pub(super) finished: bool,
    #[serde(default)]
    pub(super) ok: Option<bool>,
    #[serde(default)]
    pub(super) output: Option<String>,
    #[serde(default)]
    pub(super) out_read: bool,
    #[serde(default)]
    pub(super) runs: u32,
    #[serde(default)]
    pub(super) inputs: Vec<(String, Input)>,
    #[serde(default)]
    pub(super) worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) session: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) blank: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) twin_due: bool,
    #[serde(default)]
    pub(super) queue: Vec<QItem>,
    #[serde(default)]
    pub(super) steps: Vec<StepRec>,
    #[serde(default)]
    pub(super) asked: String,
    #[serde(default)]
    pub(super) cause: Cause,
    #[serde(default)]
    pub(super) unsent: Vec<String>,
    #[serde(default)]
    pub(super) gate: Option<String>,
    #[serde(default)]
    pub(super) gate_phase: GatePhase,
    #[serde(default)]
    pub(super) gate_round: u32,
    #[serde(default)]
    pub(super) gate_shots: Vec<String>,
    #[serde(default)]
    pub(super) gate_out: String,
    #[serde(default)]
    pub(super) gate_targets: Vec<String>,
    /// what an asking gate asked for, and when its round opened (its screenshots' names)
    #[serde(default)]
    pub(super) gate_stage: GateStage,
    #[serde(default)]
    pub(super) gate_opened: u64,
    #[serde(default)]
    pub(super) git_seen: Vec<String>,
    #[serde(default)]
    pub(super) git_log: Vec<String>,
    #[serde(default)]
    pub(super) git_carry: Vec<Carry>,
    #[serde(default)]
    pub(super) git_facts: Facts,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) git_handed: Vec<String>,
    /// when its status last changed (ms since the epoch; kept for a finished node: "done 2h ago")
    #[serde(default)]
    pub(super) since: u64,
    #[serde(default)]
    pub(super) revived: u64,
    /// the step the agent was answering, and the node its answer goes to (a step, a gate)
    #[serde(default)]
    pub(super) cur: Option<Step>,
    #[serde(default)]
    pub(super) reply_to: Option<String>,
    /// a step: the agent that answers it
    #[serde(default)]
    pub(super) step_wait: Option<String>,
    /// talks: what the session was told about the others, what waits for it, what it said
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(super) talk_told: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) talk_inbox: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) talk_said: Vec<TalkMsg>,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

pub(super) struct Watch {
    pub(super) node: String,
    pub(super) scroll: usize,
    pub(super) sent: Vec<u64>,
}

pub(super) struct Client {
    pub(super) tx: mpsc::Sender<String>,
    pub(super) watch: Option<Watch>,
}
