# weft

A canvas for coding agents. **tmux is the only runtime**: an agent is a command in a tmux window,
a prompt is a paste, and "done" is read off the screen or reported with one shell command. Nothing
is specific to Claude, Codex or Antigravity, so anything that runs in a terminal works.

```
cd your-project
weft                # starts the daemon for this folder and opens the canvas
```

One ~2 MB Rust binary. No database, no Node, no build step for the UI. Dark, quick, keyboard
friendly.

## The first screen

You open weft and see where you are and what to do:

```
~/work/
shop-api                        main · 3 uncommitted · last: 4f2a1c Fix cart rounding

[ claude | agy | codex ]   model  opus ▾   reasoning  auto low med high xhigh max   ■ yolo
┌──────────────────────────────────────────────────────────────────────────────┐
│ Tell claude what to do in shop-api…                                          │
└──────────────────────────────────────────────────────────────────────────────┘
addenda   □ Be concise   □ No commits   □ Stay in scope   + add   prompts ▾
```

* The **folder** is the headline, with branch and uncommitted files. Starting in your home folder
  or `/` shows a warning. The same path sits above the prompt bar everywhere else.
* **Agent, model and reasoning** are one click each and remembered, for the agent you used last.
  Model lists come from the CLI itself (`codex debug models`, `agy models`).
* **Enter** creates the agent in the middle of the screen and starts it. `Tab` cycles agents,
  `Alt+↑/↓` the reasoning level, `/` opens your standard prompts, `Esc` shows the canvas.
* **yolo** is on by default: agents act without asking (`claude --dangerously-skip-permissions`,
  `codex --dangerously-bypass-approvals-and-sandbox`, `agy --dangerously-skip-permissions`).
  One click makes new agents *ask first*; the choice is remembered, and a card's options
  (*permissions*) override it for that agent. It applies when an agent starts.
* **Addenda** are standard sentences appended to the prompt. Click to switch on, right-click to
  edit or remove, `+ add` to write one right there.

`N` brings the pad back at any time.

## The canvas

Every card is one agent (one tmux window). What you can do, without a menu:

| Gesture | What happens |
| --- | --- |
| type in the bar at the bottom | a new agent, or a message to the selected one (queued if busy) |
| double-click the canvas | a draft agent there, prompt focused. Enter runs it |
| click a card's ◇ | **what next?** then · hand over · branch (below) |
| drag a ◇ onto an agent | link them: the result is handed on |
| drag a ◇ into empty space and let go | the line's end is where the next card goes; the **dial** opens there: queue a prompt the same agent answers · hand off to a fresh agent · git (`Alt` chooses any type) |
| click a link's chip | cycles *always* / *only if ok* / *only if it fails* |
| double-click a link | splice a step into it |
| drag a **check** from the tray onto a link / an agent | a gate / a watch (below) |
| `Alt`-drag a card | clone it |
| marquee or shift-click, then `G` | gather several results into one agent |
| right-click anything | what it can do |

Several links into one agent draw an **AND** ring (`Promise.all`): it starts when all have
arrived; switch it to `≥1` for the first (`Promise.any`). A junction that can no longer be
satisfied shows *blocked by …* instead of waiting forever. Notes are constants you can point any
agent at; they never trigger anything.

### The dial: what the link is for

Drag from a card's ◇ (its *out port*) and let go in empty space: where the line ends is where the
next card goes. Its outline stays there and a ring opens around the line's end: a small hole, a wide
body that fades out, cut into one equal slice per option (for an agent three thirds, the first to
the right, the rest clockwise). Point into a slice, at any distance, and click; or use the arrow
keys and `Enter`. `Esc`, a click in the hole or the wheel drops the link. Nothing pops up while you
are still drawing, so dragging onto another card to link it stays clean. Where there is only one
option (after a PR: the next agent), letting go adds it at once.

| Slice | What | Context |
| --- | --- | --- |
| **right** | **queue a prompt.** A *same agent* card appears; its prompt is asked of the very same agent in its own session, and the **answer comes back into that card** (and goes on from there). The agent's own result is left as it was. | everything it knows |
| **lower left** | **hand off.** A draft agent that will take over the work; on enter the old agent writes the handover note | a fresh session plus the note |
| **upper left** | **git**: commit, push or open a PR (below) | |

After a commit or push the ring has two halves: right is the *next agent*, which receives everything the git chain carried, left is push / open PR.
A plain click on the ◇ still opens the composer below; dropping on a card links to it.

### Next: then, hand over, or branch

A new LLM session is not free: it must re-read everything. So the choice is explicit, cheapest first.

| | What | Context | Use it for |
| --- | --- | --- | --- |
| **then** | queue a prompt on the **same** agent | keeps everything it knows | follow-ups, "also do X" |
| **hand over** | a **fresh** agent takes over the work | new session, no history; the old agent first writes a handover note, the files stay as they are | context getting heavy, a different agent or model, a clean start that keeps the work |
| **branch** | a **new** agent runs alongside and receives the result | only the result | parallel review, fan-out |

* Clicking the ◇ opens the composer with *then* preselected. `Tab` switches the mode. In the
  message bar with one agent selected, `Tab` switches between continuing it and handing over.
* **Queue**: every agent has one FIFO. Prompts go out one after another; links fire when the
  queue is drained, so a script of several steps hands over only its final result. A message you
  type on a settled agent is a conversation: it does not re-fire the links (hand it over
  explicitly). `Alt+Enter` steers a busy agent immediately. The queue panel reorders, removes
  and promotes.
* **Hand over** asks the old agent for a note (goal, state, open items, gotchas) because it knows
  best, then starts the new one with: your instruction, the original task, that note, and what
  git says (branch, uncommitted files, latest commits). The new agent works in the old one's
  worktree. A busy agent finishes its step first (the new card's ▶ means *don't wait*). A dotted
  baton line ties the two cards; nothing fires along it.
* What links hand on is capped (the end of the output, which is where the conclusion is, plus
  the path of the full text), so a long transcript never floods another agent.

### Checks: gates and watches

One chip, two placements. Drop a check from the tray (bottom left):

* **on a link: a gate.** A card on the link that holds the flow. Nothing downstream starts until
  it is solved.
  * *approve*: holds the arrived results until you decide.
  * *screenshot*: asks the upstream agent for a screenshot when it is done, shows it, and waits
    for you: confirm, request changes (the note goes back to the agent, which tries again) or
    reject. Paste or drop an image if no agent can take one.
  * *tests*: asks for tests to be written, runs the suite (your command or the agent's), sends
    failures back to the agent a few times, then asks you.
* **on an agent: a watch.** Nothing is blocked. While the agent works, evidence collects in a
  grid under its card: newest first, small, each with its own **steer field** and ✓.
  * *Enter* in a field steers the agent right now ("the header is cut off"); ✓ approves.
  * Click a picture to zoom; arrows walk the series; the steer field is there too.
  * Evidence arrives when the agent shows it (`weft shot <png>`; it is told once how), when you
    press *take one*, every N seconds while it works, or when you paste / drop an image.
    With a **capture command** weft takes the screenshot itself and never disturbs the agent.
  * A *tests* watch runs your test command the same way and shows red / green results.

Approve has no "watching" form, so dropped on an agent it becomes a gate after that agent.

Three more checks only watch, so they always go on an agent (dropped on a link they attach to the
agent the link comes from):

* **ETA.** The agent is asked how long the task will take (in seconds) when it starts, and to say
  the remaining time again at every major step. A large **countdown** sits at the top right of
  its card (amber when it is nearly up, red and counting up when it ran out) and the same number
  for the whole flow sits in the top bar. If the estimate runs out while it is still working it
  is asked once more. The note it sends with each estimate shows next to the digits.
* **changes.** What the agent has touched so far, read from git in its folder (its worktree if it
  has one) every 20 s while it works and once more when it finishes: files, `+added −removed`.
  Nobody is asked. The grid under the card has the list; click for all of it, type a note to ask
  about or steer a change, ✓ to mark it reviewed.
* **ping.** A desktop notification (and the tab title) when the agent is done, fails or needs you.

### Git: commit, push, open a PR

Three steps, one idea each. Each step does what the steps before it would have done, so an
agent can go straight to *open PR*; the explicit chain is just easier to read.

```
 agent ──▶ commit ──┐
 agent ──▶ commit ──┼──▶ push ──▶ open PR ──▶ agent that tests it live
 agent ──▶ commit ──┘
```

* **commit takes one agent's work.** It asks that agent (it has seen the files; nobody else has)
  to commit its own files on the target branch. Then git decides what happened: weft reads the
  commit and the working tree, merges the agent's worktree branch if it has one, and the card
  shows the commit. A second agent gets its own commit: drop its link on an occupied commit and
  that is what happens (the commit appears and joins the same push). One card per agent keeps
  failures, conflicts and branches attributable. Merge conflicts go back to the agent whose
  branch conflicts. An agent that failed is not committed; *commit now* takes what is there.
  Commands and agents that are gone are committed mechanically (secrets and `.weft` are never
  staged). Commits into one folder are serialised, so parallel agents never fight over its index.
* **push is the barrier.** It waits until every commit linked into it has resolved
  (`Promise.all`), then pushes the branch. It never forces. If the remote moved on, the agent
  that did the work is asked to bring the branch up to date (`pull --rebase`) and the push is
  tried once more. Linked straight from an agent it asks for the commit first.
* **open PR** pushes if nothing before it did, then asks an agent that did the work to write the
  title and description and open it (`gh pr create`, or whatever the host offers). That agent
  knows why; a template does not. Extra instructions go in its options (base branch, labels,
  reviewers). The card links to the pull request.
* Push and PR are outward-facing: they are created **disarmed** and wait until you arm them. A
  commit only asks an agent about its own work, so it starts at once.

**What travels.** A commit, a push and a PR each hand on one result: where the work is (branch,
HEAD, project folder), the commits, the files changed, the pull request link if there is one, and
for every agent that took part what it was asked, what it said and which commit came of it
(budgeted to stay inside the 8 000 byte limit of piped results). Whatever you link after the
chain, a tester, a reviewer, a deploy script, starts from that, not from a bare "done".

## Prompt library

*Standard prompts* are copied in when you insert them (`/`). *Addenda* are referenced, so editing
one changes every agent that uses it. Everything is editable under *library* or in place. The
built-in **Report with weft commands** addendum (off by default, see below) teaches an agent to
finish with `weft done`, `weft issue` or `weft fail`.

## The prompt bar

The bar at the bottom always carries the choices that make an agent, one click each: **agent**,
**model**, **reasoning** and **yolo**. With nothing selected they are for the next new agent (the
same remembered choices as the launch pad). With an agent selected they are *that agent's* own
settings and apply the next time it starts. Not every agent takes every choice: `agy` accepts
low / medium / high, and only with its default model (its other models carry the level in the
name, and agy drops the whole model choice when the two disagree), so the control is hidden where
it would not work.

## What counts as finished

| Setting | Meaning | Use for |
| --- | --- | --- |
| Screen goes quiet | the screen did not change for N s after it was given work | any agent TUI (default) |
| Process exits | exit code 0 = succeeded, otherwise failed | `cargo test`, scripts |
| Reports done | only an explicit `weft done [--fail]` counts | long, bursty agents |

A card that shows a trust, update or approval dialog goes to *needs you* and nothing is typed into
it (patterns per agent, configurable).

### Silence is not an answer

An agent that stops talking may be finished, or it may have said "I will get back to you when the
build is done", or it may be waiting for an answer. Nothing resumes it, and handing a promise on
would send nonsense to the next step. So **a step of a running chain is asked, in its prompt, to end
its final message with one line**, and weft reads that line from the output:

```
STATUS: DONE                         finished, and checked
STATUS: ISSUE <your question>        cannot go on without the person
STATUS: WAITING <what for>           waits for something; weft wakes it to check
STATUS: FAILED <why>                 cannot be done
```

| The last line says | weft does |
| --- | --- |
| `DONE` | its message (without the line) is the result and goes on |
| `FAILED` | the step fails with the reason: the "only if it fails" links fire |
| `ISSUE` | the card says *asks you* with the question, the chain waits; your answer in the bar continues the same run |
| `WAITING` | the card says what it waits for; after `recheck_s` (60 s) weft types a short *check on it now* into the session, repeatedly, until it is DONE, or an ISSUE after `background_s` |
| nothing | the agent is asked once for the line; if that does not help, silence is the end, so a flow never hangs |

It is plain text on the screen, so it needs no tool, no shell, no permission and no particular CLI:
claude, codex, agy or anything with a prompt, in any permission mode. It costs one instruction in
the prompt (about 70 tokens the first time, a one-line reminder after), no extra turn, and nothing
at all for an agent you are just chatting with, for the last step of a chain, or for weft's own
requests (commit, screenshot, handover: those weft checks itself, with git, a file or a test run).
`"chain_status": false` in `config.json` turns it off.

A step of a chain also gets one line first: *you are one step in a chain run by weft, what you write
last is handed on, messages starting with `[weft]` are from weft*. A lone agent is told nothing.

Agents that can run a shell may say the same words with commands, which carry exact text:
`weft done "..."`, `weft issue "..."`, `weft fail "..."`. Switch the **Report with weft commands**
addendum on for an agent to teach it; it then does not get the status line. An agent told to use
the commands that goes quiet without doing so is reminded once.

A prompt is typed only into a process that is **up**: it has drawn something and then gone quiet
for a moment. A shell still loading its rc files, or a TUI still painting its first screen, loses
or garbles what is typed into it, so a slow start just delays the first prompt (a process that
draws nothing at all is given eight seconds).

An agent that ended its turn but says it is still waiting for something it started in the
background (Claude Code's status line reads `1 shell still running`) is **not** finished: the
card stays busy ("waiting on background work") until that ends and the agent has answered, or
`background_s` (10 minutes) have passed, in case a server was left running on purpose.

## CLI

```
weft [dir]                    open the canvas
weft ls                       nodes, status, runs, queued
weft send [--queue|--now] <node> <text|->   a prompt into a node (starts it if needed)
weft result <node>            its latest result
weft read <node> [-n N]       terminal history of a node
weft run                      run the flow
weft stop [dir] [--keep]      quit: the daemon and every agent of the project end (--keep leaves the agents in tmux)
# inside an agent:
weft done|issue|fail [text]   report this node: finished / cannot go on without the person / cannot be done
weft shot <image> [--note ..] show a screenshot to the reviewer; returns at once
weft eta <90|2m|1m30s> [--note ..]   say how long it will take (what is left); returns at once
```

Everything goes through one small local HTTP API, so scripts can drive a flow too:
`weft send reviewer "$(git diff)"`.

## Files and config

| Path | What |
| --- | --- |
| `<project>/.weft/flow.json` | the graph; plain JSON, safe to commit and hand-edit |
| `<project>/.weft/state.json`, `run/`, `shots/` | run state, prompt files, images; git-ignored |
| `~/.config/weft/config.json` | **your overrides only**, merged onto the defaults |

```json
{
  "yolo": true,
  "agents": {
    "claude": { "cmd": "claude", "models": ["opus", "sonnet"] },
    "codex":  { "cmd": "codex -m gpt-5.4" },
    "aider":  { "cmd": "aider --yes", "label": "Aider", "attention": ["(Y)es/(N)o"],
                "model_arg": "--model {}", "models": ["sonnet", "gpt-4o"] }
  },
  "quiet_s": 3,
  "cols": 120, "rows": 36
}
```

Commands run through `$SHELL -ic`, so PATH and aliases from your rc files apply as when you type
them. The exception is an agent whose `yolo_arg` weft manages (claude, codex, agy): there weft
runs `command claude …` so a shell alias cannot keep the flag on after you switched it off, or
double it (Codex refuses a repeated flag). Put what you want in `cmd` instead. A card whose own
command line is set is never touched.

`working` lists screen text that means "idle, but still working in the background", `effort_models`
the models a reasoning level applies to (`[""]` = only the default model, as for agy).
`"yolo": false` makes *ask first* the default; `yolo_arg` is the flag weft adds (empty: weft
leaves the agent alone). Model and reasoning are plain flags appended to the command: `model_arg` /
`effort_arg` hold the template (`--model {}`, `--effort {}`; for Codex `-m {}` and
`-c model_reasoning_effort={}`), `models` / `efforts` the choices, `models_cmd` a command that
lists what the CLI offers (`id<TAB>label` lines or a JSON catalog). `attention` lists screen
substrings that mean "a human is needed".

## How it works

* `tmux -C` (control mode) streams every pane's output as events. The daemon feeds each pane
  into an in-process terminal emulator (`vt100`), so previews, idle detection and the live
  terminal never spawn `capture-pane` and nothing polls tmux.
* Prompts are sent as a bracketed paste (if the app enabled it) followed by Enter, keystrokes as
  literal bytes, so UTF-8 and multi-line text arrive intact.
* **A prompt is seen taken before anything is judged.** The Enter follows the paste after
  `submit_ms`; weft then looks for the message in the input box next to the cursor. Still there
  after a second and a half: Enter again (twice at most, then the card asks you). No trace of it
  anywhere: pasted again, once. Until it is taken the card is busy, never quiet or done.
* One actor owns all state; the browser gets changed rows only, throttled to 10 Hz per node.
* **Ask the agent.** Whatever an agent does better, weft asks it to do: commits, merge conflicts,
  handover notes, screenshots, tests, summaries are requests in its own session. weft itself only
  does the mechanical and real-time part: moving text, watching screens, git plumbing, running
  your commands.
* Agents keep running if the daemon or the browser goes away; `weft` re-adopts them.
* **Quitting** ends them: the power button (or `q`, or `/quit`) asks once how many agents are running and
  which are mid-work, then ends every agent and its terminal, saves the canvas and stops the daemon. A
  second choice keeps the agents running in tmux. Worktrees and files stay. `weft stop` does the same
  from a terminal, and also clears the agents a crashed daemon left behind.

Each project has its own daemon, listening on `127.0.0.1` only. Every request needs the
per-project token and a loopback `Host`; WebSockets also check `Origin`; the image route serves
only image files from the shots folder.

Measured (release build, a browser tab connected; `weft` itself, not counting tmux): a canvas of
60 cards at rest 0.0 % CPU / 14 MB with two wake-ups a second; 12 live agents sitting at their
prompt the same; six processes printing ~7 000 lines a second ≈ 2.5 % of one core / 35 MB
(history is capped at 1 000 lines per agent, and every line costs a full terminal row).

## Source map

```
src/main.rs        the CLI (`weft`, `weft stop`, `weft done` ...) and how the daemon starts
src/model.rs       the flow graph, node and config types, built-in defaults: plain JSON on disk
src/server.rs      loopback HTTP + WebSocket, token and Host checks
src/web.rs         puts the page together from web/ (embedded in the binary)
src/dev.rs         dev only: the page from disk, following edits (see "Working on weft's UI")
src/tmux.rs        the control-mode client: events in, validated keystrokes out
src/term.rs        screen helpers: HTML rows, previews, telling an answer from terminal furniture
src/words.rs       everything weft says to an agent, and how it reads the status line back
src/client.rs      the small HTTP client the CLI commands use
src/engine/        the single actor that owns all state
    mod.rs         the Engine, its events, the loop and its heartbeat
    state.rs       what is known per node while it runs (Rt), and what survives a restart
    graph.rs       nodes and links: add, edit, delete, names, ids
    flow.rs        results travelling along links: joins, triggers, queued prompts, prompt text
    run.rs         reading a screen, finishing a step, settling, the prompt queue
    agent.rs       the process side: spawn in tmux, type, stop
    checks.rs      watches: screenshots, tests, changes, the ETA countdown
    gates.rs       the gates on a link: approve, screenshot, tests
    git.rs         commit, push and open PR as steps; repo.rs is the git plumbing under them
    handover.rs    a fresh agent takes over another one's work
    commands.rs    the command set and its dispatcher (each command lives in its domain's module)
    ui.rs          what browsers are told, and the state file
    info.rs        the project folder and each CLI's model list
web/               index.html (the shell), css/*.css and js/*.js (joined in file order); dev.js (dev only)
tests/             e2e.py (real tmux, simulated agent), ui/ (browser), engine/tests.rs (unit)
```

A change starts in the module that owns the idea. Nothing outside `src/engine/` touches a node's
state, and nothing inside `repo.rs` knows what a node is.

## Limits to know about

* "Screen goes quiet" is a heuristic. An agent that thinks silently for longer than the quiet
  time looks finished; raise *Quiet for* or use *Reports done*. In a chain the status line catches
  most of what the heuristic misses, but it trusts the agent to write it (and to write it truthfully).
* `weft done`, `weft issue`, `weft fail` and `weft shot` need the agent to be allowed to reach
  `127.0.0.1` and run a shell command. Sandboxed modes that block that fall back to the quiet
  heuristic and the status line (plain text), and watches then need a capture command or
  pasted images.
* A steering message typed into a TUI while it works is handled the way that TUI handles it
  (most queue it for the next turn).
* Dialogs shown on first start (trust, update, login) are held with *needs you*, but only for
  patterns weft knows. Add yours under `attention`.
* Two agents editing one folder collide. Tick *own git worktree* on agents that run in parallel.
* Linux/macOS with tmux ≥ 3.2. tmux 3.6 crashes on `window-size manual` with a control client;
  weft never sets it.

## Install

`weft` on your PATH is a **copy** of the binary, not the one in the project folder: a console
never looks at `target/` or the working directory. Install (or update) it with

```
cargo build --release && install -m 0755 target/release/weft ~/.local/bin/weft
weft --version                 # weft 0.1.0 (build 2026-10-14 04:56:11 UTC)
```

The UI and fonts are inside the binary, so that one file is everything. Every build carries a
stamp. Typing `weft` in a folder whose daemon was started by an *older* build replaces that
daemon with the new one (its agents keep running in tmux and are adopted again; port and token
stay), and an open browser tab reloads itself to the matching page. Nothing else to restart.

## Build and test

```
cargo build --release          # target/release/weft
cargo test                     # unit tests
python3 tests/e2e.py           # end to end against real tmux, with a simulated agent
tests/ui/run.sh                # browser simulation (see tests/ui/README.md)
```

### Working on weft's UI

Run weft in its own source folder and the page is read from `web/` on disk instead of the
binary, and an open tab follows every edit there, yours or an agent's: a style sheet is
swapped in place, a changed script or `index.html` reloads the page and puts back what was
open (view, selection, flyout, terminal, launch pad). It waits while you type or drag. A tag
in the bottom right corner says it is on (amber while a reload is waiting; click it to reload
now). An error in the page's script shows in a bar with its file and line; the follower runs
apart from the page's script, so a script that no longer parses reloads by itself once fixed.
A file added under `css/` or `js/` is part of the next load (and must be listed in `web.rs`
before the next build; `cargo test` says so).

Changes to `src/` still need a build and `weft` once (the agents keep running).
`WEFT_WEB_DIR=<folder>` serves another copy of `web/` the same way; `WEFT_WEB_DIR=off` turns it
off. It is a development aid, kept apart so it can go: `src/dev.rs`, `web/dev.js`,
`tests/ui/dev.cjs` and the lines marked `dev:` (`grep -rn 'dev:' src tests`).

Fonts: Chakra Petch and JetBrains Mono, both SIL Open Font License, embedded.
