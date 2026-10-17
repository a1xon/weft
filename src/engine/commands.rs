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
    },
    EdgeSet {
        id: String,
        on: On,
    },
    EdgeDel {
        id: String,
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
    /// List the models an agent's CLI offers (cached; `force` asks again).
    Models {
        agent: String,
        #[serde(default)]
        force: bool,
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
            Cmd::Log { .. }
                | Cmd::BoardOpen { .. }
                | Cmd::Proj { .. }
                | Cmd::Dirs { .. }
                | Cmd::Models { .. }
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
            Cmd::EdgeAdd { from, to, on } => self.cmd_edge_add(from, to, on),
            Cmd::EdgeSet { id, on } => self.cmd_edge_set(id, on),
            Cmd::EdgeDel { id } => self.cmd_edge_del(id),
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
            Cmd::PrefSet { prefs } => self.cmd_pref_set(prefs),
            Cmd::Proj { dir } => self.cmd_proj(dir),
            Cmd::Dirs { q, like } => self.cmd_dirs(&q, &like),
            Cmd::Models { agent, force } => self.cmd_models(agent, force),
            Cmd::Log { dir, branch, n, show, remote } => {
                // (only a caller that waits for the answer gets one: see `Engine::handle`)
                self.cmd_log(dir, branch, n, show, remote, None);
                json!({"ok": false, "error": "the log answers by itself"})
            }
            Cmd::BoardOpen { dir, window } => {
                self.cmd_board_open(dir, window, None);
                json!({"ok": false, "error": "a board opens by itself"})
            }
            Cmd::BoardForget { dir } => self.cmd_board_forget(dir),
            Cmd::BoardName { name } => self.cmd_board_name(name),
            Cmd::LibSet { library } => self.cmd_lib_set(library),
            Cmd::NetSet { networks } => self.cmd_net_set(networks),
            Cmd::Start { id } => self.cmd_start(id),
            Cmd::Stop { id } => self.cmd_stop(id),
            Cmd::Restart { id } => self.cmd_restart(id),
            Cmd::Arm { id } => self.cmd_arm(id),
            Cmd::Approve { id, ok } => self.cmd_approve(id, ok),
            Cmd::GateChanges { id, text } => self.cmd_gate_changes(id, text),
            Cmd::GateRetry { id, fix } => self.cmd_gate_retry(id, fix),
            Cmd::GateShot { id, name } => self.cmd_gate_shot(id, name),
            Cmd::Fire { id } => self.cmd_fire(id),
            Cmd::Run {} => self.cmd_run(),
            Cmd::Reset {} => self.cmd_reset(),
            Cmd::ClearInputs { id } => self.cmd_clear_inputs(id),
            Cmd::Send { id, text, mode, submit } => self.cmd_send(id, text, mode, submit),
            Cmd::QueueAdd { id, text, front } => self.cmd_queue_add(id, text, front),
            Cmd::QueueDel { id, qid } => self.cmd_queue_del(id, qid),
            Cmd::QueueMove { id, qid, to } => self.cmd_queue_move(id, qid, to),
            Cmd::QueueNow { id, qid } => self.cmd_queue_now(id, qid),
            Cmd::QueueClear { id } => self.cmd_queue_clear(id),
            Cmd::Key { id, k } => self.cmd_key(id, k),
            Cmd::Text { id, text, paste } => self.cmd_text(id, text, paste),
            Cmd::Done { id, ok, text, issue } => self.cmd_done(id, ok, text, issue),
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
            Cmd::ArchiveHold { on } => self.cmd_archive_hold(on),
            Cmd::Mark { text, to, agent, cwd, draft } => self.cmd_mark(text, to, agent, cwd, draft),
        }
    }
}
