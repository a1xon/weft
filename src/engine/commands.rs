//! Everything a client can ask of the engine: the command set and what each command does.

use super::*;

pub(super) fn yes() -> bool {
    true
}

fn log_n() -> usize {
    100
}

#[derive(Deserialize, Debug)]
pub struct Mv {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// How a message to an agent is delivered.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SendMode {
    /// Type it now unless the agent is actively working, then queue it.
    #[default]
    Auto,
    /// Always append to the queue.
    Queue,
    /// Always type it now (steer the agent mid-task).
    Now,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Cmd {
    NodeAdd {
        node: Value,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        run: bool,
    },
    NodeSet {
        id: String,
        patch: Value,
    },
    NodeMove {
        moves: Vec<Mv>,
    },
    NodeDel {
        id: String,
    },
    EdgeAdd {
        from: String,
        to: String,
        #[serde(default)]
        on: On,
        /// "for each" (see each.rs)
        #[serde(default)]
        each: bool,
    },
    EdgeSet {
        id: String,
        on: On,
    },
    EdgeDel {
        id: String,
    },
    /// "for each" on a link, or back to a plain one (see each.rs)
    EdgeEach {
        id: String,
        each: bool,
    },
    Handover {
        from: String,
        to: String,
    },
    /// Let two agents talk: they coordinate directly while they work (see talk.rs).
    TalkAdd {
        a: String,
        b: String,
        #[serde(default)]
        topic: String,
        /// the person is about to write its topic: the agents wait for it
        #[serde(default)]
        typing: bool,
    },
    /// What a talk should mainly be about (no topic: the person started writing one).
    TalkSet {
        id: String,
        #[serde(default)]
        topic: Option<String>,
        #[serde(default)]
        typing: bool,
    },
    TalkDel {
        id: String,
    },
    /// An agent says something to the agents it talks with (`weft tell`): ids or names, or "all".
    Tell {
        from: String,
        #[serde(default)]
        to: String,
        text: String,
    },
    LibSet {
        library: Vec<Snippet>,
    },
    /// The networks the user saved (all of them: the list replaces the one there is).
    NetSet {
        networks: Vec<Network>,
    },
    /// The user's last choices (agent, model, reasoning, addenda), kept across projects.
    PrefSet {
        prefs: Value,
    },
    /// Look at the project folder again (branch, uncommitted files), and at `dir` from now on.
    Proj {
        #[serde(default)]
        dir: String,
    },
    /// Folders to work in: the project's neighbours (`like`: those whose name has it), or the
    /// folders that complete a typed path `q`.
    Dirs {
        #[serde(default)]
        q: String,
        #[serde(default)]
        like: String,
    },
    /// Open a board (its daemon started when it sleeps; a folder that is none yet becomes one):
    /// answers with its address. `window`: in a window of its own too. Answered in `Engine::handle`.
    BoardOpen {
        dir: String,
        #[serde(default)]
        window: bool,
    },
    /// Leave a board that sleeps off the board list (its cards stay in its folder).
    BoardForget {
        dir: String,
    },
    /// Give this board a name ("": its folder's).
    BoardName {
        name: String,
    },
    /// Move cards to the board in `dir` (see moves.rs). Answered in `Engine::handle`, once that board
    /// has them or could not take them.
    CardMove {
        ids: Vec<String>,
        dir: String,
    },
    /// Cards another board's daemon hands over: `pack` as the archive keeps them, the windows of its
    /// agents at work, and the folder and name of the board they come from.
    CardTake {
        pack: Value,
        #[serde(default)]
        wins: HashMap<String, Win>,
        from: String,
        #[serde(default)]
        board: String,
    },
    /// The commits of a folder's branch for the git tree in the corner ("": what is checked out,
    /// "*": every branch), or one commit in full (`show`). Answered in `Engine::handle`, off the engine.
    /// `remote`: ask the remote first where its branches are (fetching what moved there).
    Log {
        #[serde(default)]
        dir: String,
        #[serde(default)]
        branch: String,
        #[serde(default = "log_n")]
        n: usize,
        #[serde(default)]
        show: String,
        #[serde(default)]
        remote: bool,
    },
    /// The agent says how long it will take (seconds from now); an update replaces the last one.
    Eta {
        node: String,
        seconds: u64,
        #[serde(default)]
        note: String,
    },
    /// Attach a watch to an agent.
    CheckAdd {
        node: String,
        kind: CheckKind,
        #[serde(default)]
        subject: String,
        #[serde(default)]
        cmd: String,
        #[serde(default)]
        every_s: u32,
    },
    CheckSet {
        node: String,
        check: String,
        patch: Value,
    },
    CheckDel {
        node: String,
        check: String,
    },
    /// Collect one now: run its command, or ask the agent.
    CheckNow {
        node: String,
        check: String,
    },
    /// An image file the agent (or the user) hands to a watch: copied into the shots folder.
    FeedFile {
        node: String,
        path: String,
        #[serde(default)]
        note: String,
    },
    /// An image already saved in the shots folder (an upload).
    FeedAdd {
        node: String,
        name: String,
        #[serde(default)]
        note: String,
    },
    /// Approve one item (no text) or steer the agent with it (text).
    FeedAct {
        node: String,
        item: u64,
        #[serde(default)]
        text: String,
    },
    FeedDel {
        node: String,
        item: u64,
    },
    /// A fresh agent takes over `from`'s work: `from` writes a handover note and the files stay put.
    Takeover {
        from: String,
        #[serde(default)]
        node: Value,
        #[serde(default)]
        text: String,
    },
    /// A twin of `from`: a new card on a copy of its conversation, asked `text` (see resume.rs).
    Twin {
        from: String,
        text: String,
        #[serde(default)]
        x: f64,
        #[serde(default)]
        y: f64,
    },
    /// List the models an agent's CLI offers (cached; `force` asks again).
    Models {
        agent: String,
        #[serde(default)]
        force: bool,
    },
    /// How much of an agent's plan is used (asked of its CLI at most every 15 minutes; `again`: a
    /// click on the meter, at most every 10 seconds).
    Usage {
        agent: String,
        #[serde(default)]
        again: bool,
    },
    Start {
        id: String,
    },
    Stop {
        id: String,
    },
    Restart {
        id: String,
    },
    /// An agent asleep at its usage limit tries now instead of at the reset time.
    LimitWake {
        id: String,
    },
    /// What step `n` of an agent changed, as a diff (answered by itself: see `Engine::handle`).
    CpDiff {
        id: String,
        n: u32,
    },
    /// Undo steps `from..=to` of an agent (`to` missing: to its last); `force` also puts back files
    /// that changed after them.
    Rewind {
        id: String,
        from: u32,
        #[serde(default)]
        to: Option<u32>,
        #[serde(default)]
        force: bool,
    },
    Fire {
        id: String,
    },
    /// Arm a draft node: it starts reacting to its inputs.
    Arm {
        id: String,
    },
    /// Decide a gate: confirm (ok) or reject.
    Approve {
        id: String,
        #[serde(default = "yes")]
        ok: bool,
    },
    /// Ask for changes: the text goes back to the upstream agent, which then tries again.
    GateChanges {
        id: String,
        text: String,
    },
    /// A fresh, stronger agent finds out why cards failed (`ids`; none: all of them), fixes it and
    /// runs them again. An agent already at it is shown instead (see fix.rs).
    Fix {
        #[serde(default)]
        ids: Vec<String>,
    },
    /// Run a finished card again with the input it had (`weft retry`); `text`: what was fixed since.
    Retry {
        id: String,
        #[serde(default)]
        text: String,
        /// the agent that fixed it
        #[serde(default)]
        by: String,
    },
    /// Retake the screenshot / run the suite again; `fix` sends the failure to the agent instead.
    GateRetry {
        id: String,
        #[serde(default)]
        fix: bool,
    },
    /// An image the user attached (pasted or dropped) is already saved in the shots folder.
    GateShot {
        id: String,
        name: String,
    },
    Run {},
    Reset {},
    ClearInputs {
        id: String,
    },
    Send {
        id: String,
        text: String,
        #[serde(default)]
        mode: SendMode,
        #[serde(default = "yes")]
        submit: bool,
    },
    QueueAdd {
        id: String,
        text: String,
        #[serde(default)]
        front: bool,
    },
    QueueDel {
        id: String,
        qid: u64,
    },
    QueueMove {
        id: String,
        qid: u64,
        to: usize,
    },
    QueueNow {
        id: String,
        qid: u64,
    },
    QueueClear {
        id: String,
    },
    Key {
        id: String,
        k: String,
    },
    Text {
        id: String,
        text: String,
        #[serde(default)]
        paste: bool,
    },
    Done {
        id: String,
        #[serde(default = "yes")]
        ok: bool,
        #[serde(default)]
        text: String,
        /// ISSUE: the agent cannot go on without the person
        #[serde(default)]
        issue: bool,
    },
    /// The person lets a report that asked them something go as it is: it is the result (run.rs).
    Skip {
        id: String,
    },
    Watch {
        id: Option<String>,
    },
    Scroll {
        id: String,
        offset: usize,
    },
    /// the wheel over the focused terminal: `n` notches, up when negative, over `row`/`col`
    Wheel {
        id: String,
        n: i32,
        #[serde(default)]
        row: u16,
        #[serde(default)]
        col: u16,
    },
    Preview {
        id: String,
    },
    Read {
        id: String,
        #[serde(default)]
        lines: usize,
    },
    Result {
        id: String,
    },
    Ls {},
    Quit {
        /// also end every agent (default: they keep running in tmux)
        #[serde(default)]
        end: bool,
        /// a daemon of another build takes over (`boards::ensure`): the pages come back to it
        #[serde(default)]
        replace: bool,
    },
    /// File cards away (they were dropped on the archive). What is still at work stays on the canvas.
    Archive {
        ids: Vec<String>,
    },
    /// Bring filed work back onto the canvas, moved by (dx, dy).
    Revive {
        group: String,
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
    },
    /// What filed work asked and said, in full.
    ArchiveGet {
        group: String,
    },
    /// Forget filed work for good.
    ArchiveDel {
        group: String,
    },
    /// Someone is at the canvas (a browser says so while they use it): the archive clock runs.
    Here {},
    /// Someone writes a prompt into these cards (a follow-up, a queued prompt; none: they stopped): their
    /// flows do not go to the archive meanwhile, and their countdowns stand.
    Writing {
        #[serde(default)]
        ids: Vec<String>,
    },
    /// Hold archiving (the lock in the top bar): every countdown stands until it is let go.
    ArchiveHold {
        on: bool,
    },
    /// Something the person marked on a web page (the browser extension, see marks.rs): a new
    /// agent starts on `text` (in `cwd`, `agent` or the person's last one; `draft`: only placed),
    /// or the agent `to` is asked.
    Mark {
        text: String,
        #[serde(default)]
        to: String,
        #[serde(default)]
        agent: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        draft: bool,
    },
}

impl Cmd {
    /// Sent because someone did something, not by a browser asking for what it shows by itself.
    pub(super) fn is_work(&self) -> bool {
        !matches!(
            self,
            Cmd::Proj { .. }
                | Cmd::Dirs { .. }
                | Cmd::Models { .. }
                | Cmd::Usage { .. }
                | Cmd::Preview { .. }
                | Cmd::Read { .. }
                | Cmd::Result { .. }
                | Cmd::Ls {}
                | Cmd::Eta { .. }
                | Cmd::Tell { .. }
                | Cmd::Done { .. }
        )
    }
}

impl Engine {
    pub(super) fn cmd(&mut self, client: Option<u64>, cmd: Cmd) -> Value {
        match cmd {
            Cmd::NodeAdd { node, from, run } => self.cmd_node_add(node, from, run),
            Cmd::NodeSet { id, patch } => self.cmd_node_set(id, patch),
            Cmd::NodeMove { moves } => self.cmd_node_move(moves),
            Cmd::NodeDel { id } => self.cmd_node_del(id),
            Cmd::EdgeAdd { from, to, on, each } => self.cmd_edge_add(from, to, on, each),
            Cmd::EdgeSet { id, on } => self.cmd_edge_set(id, on),
            Cmd::EdgeDel { id } => self.cmd_edge_del(id),
            Cmd::EdgeEach { id, each } => self.cmd_edge_each(id, each),
            Cmd::Handover { from, to } => self.cmd_handover(from, to),
            Cmd::TalkAdd { a, b, topic, typing } => self.cmd_talk_add(a, b, topic, typing),
            Cmd::TalkSet { id, topic, typing } => self.cmd_talk_set(id, topic, typing),
            Cmd::TalkDel { id } => self.cmd_talk_del(id),
            Cmd::Tell { from, to, text } => self.cmd_tell(from, to, text),
            Cmd::Eta { node, seconds, note } => self.cmd_eta(node, seconds, note),
            Cmd::CheckAdd { node, kind, subject, cmd, every_s } => self.cmd_check_add(node, kind, subject, cmd, every_s),
            Cmd::CheckSet { node, check, patch } => self.cmd_check_set(node, check, patch),
            Cmd::CheckDel { node, check } => self.cmd_check_del(node, check),
            Cmd::CheckNow { node, check } => self.cmd_check_now(node, check),
            Cmd::FeedFile { node, path, note } => self.cmd_feed_file(node, path, note),
            Cmd::FeedAdd { node, name, note } => self.cmd_feed_add(node, name, note),
            Cmd::FeedAct { node, item, text } => self.cmd_feed_act(node, item, text),
            Cmd::FeedDel { node, item } => self.cmd_feed_del(node, item),
            Cmd::Takeover { from, node, text } => self.cmd_takeover(from, node, text),
            Cmd::Twin { from, text, x, y } => self.cmd_twin(from, text, x, y),
            Cmd::PrefSet { prefs } => self.cmd_pref_set(prefs),
            Cmd::Proj { dir } => self.cmd_proj(dir),
            Cmd::Dirs { q, like } => self.cmd_dirs(&q, &like),
            Cmd::Models { agent, force } => self.cmd_models(agent, force),
            Cmd::Usage { agent, again } => self.cmd_usage(agent, again),
            // (these answer when their job is done: `Engine::handle` gives them the caller's reply)
            Cmd::Log { .. } | Cmd::CpDiff { .. } | Cmd::BoardOpen { .. } | Cmd::CardMove { .. } => {
                json!({"ok": false, "error": "it answers by itself"})
            }
            Cmd::CardTake { pack, wins, from, board } => self.cmd_card_take(pack, wins, from, board),
            Cmd::BoardForget { dir } => self.cmd_board_forget(dir),
            Cmd::BoardName { name } => self.cmd_board_name(name),
            Cmd::LibSet { library } => self.cmd_lib_set(library),
            Cmd::NetSet { networks } => self.cmd_net_set(networks),
            Cmd::Start { id } => self.cmd_start(id),
            Cmd::Stop { id } => self.cmd_stop(id),
            Cmd::LimitWake { id } => self.cmd_limit_wake(id),
            Cmd::Rewind { id, from, to, force } => self.cmd_rewind(id, from, to, force),
            Cmd::Restart { id } => self.cmd_restart(id),
            Cmd::Arm { id } => self.cmd_arm(id),
            Cmd::Approve { id, ok } => self.cmd_approve(id, ok),
            Cmd::GateChanges { id, text } => self.cmd_gate_changes(id, text),
            Cmd::GateRetry { id, fix } => self.cmd_gate_retry(id, fix),
            Cmd::Fix { ids } => self.cmd_fix(ids),
            Cmd::Retry { id, text, by } => self.cmd_retry(id, text, by),
            Cmd::GateShot { id, name } => self.cmd_gate_shot(id, name),
            Cmd::Fire { id } => self.cmd_fire(id),
            Cmd::Run {} => self.cmd_run(),
            Cmd::Reset {} => self.cmd_reset(),
            Cmd::ClearInputs { id } => self.cmd_clear_inputs(id),
            Cmd::Send { id, text, mode, submit } => self.cmd_send(id, text, mode, submit),
            Cmd::QueueAdd { id, text, front } => self.cmd_queue_add(id, text, front),
            Cmd::QueueDel { id, qid } => self.cmd_queue_del(id, qid),
            Cmd::QueueMove { id, qid, to } => self.cmd_queue_move(id, qid, to),
            Cmd::QueueNow { id, qid } => self.cmd_queue_move(id, qid, 0),
            Cmd::QueueClear { id } => self.cmd_queue_clear(id),
            Cmd::Key { id, k } => self.cmd_key(id, k),
            Cmd::Text { id, text, paste } => self.cmd_text(id, text, paste),
            Cmd::Done { id, ok, text, issue } => self.cmd_done(id, ok, text, issue),
            Cmd::Skip { id } => self.cmd_skip(id),
            Cmd::Watch { id } => self.cmd_watch(client, id),
            Cmd::Scroll { id, offset } => self.cmd_scroll(client, id, offset),
            Cmd::Wheel { id, n, row, col } => self.cmd_wheel(client, id, n, row, col),
            Cmd::Preview { id } => self.cmd_preview(id),
            Cmd::Read { id, lines } => self.cmd_read(id, lines),
            Cmd::Result { id } => self.cmd_result(id),
            Cmd::Ls {} => self.cmd_ls(),
            Cmd::Quit { end, replace } => {
                if replace {
                    self.cmd_make_way()
                } else {
                    self.cmd_quit(end)
                }
            }
            Cmd::Archive { ids } => self.cmd_archive(ids),
            Cmd::Revive { group, dx, dy } => self.cmd_revive(group, dx, dy),
            Cmd::ArchiveGet { group } => self.cmd_archive_get(group),
            Cmd::ArchiveDel { group } => self.cmd_archive_del(group),
            Cmd::Here {} => json!({"ok": true}),
            Cmd::Writing { ids } => self.cmd_writing(client, ids),
            Cmd::ArchiveHold { on } => self.cmd_archive_hold(on),
            Cmd::Mark { text, to, agent, cwd, draft } => self.cmd_mark(text, to, agent, cwd, draft),
        }
    }
}
