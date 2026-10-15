# Browser simulation

Plays the canvas the way a person does (type, click, drag, right-click) against a real daemon,
real tmux and a *simulated* agent (`examples/sim-agent.py`) that edits the `examples/textkit`
project, answers weft's requests (commit, screenshot, tests, handover note) and shows
screenshots with `weft shot`. Needs Playwright with a Chromium; point at it with:

    PLAYWRIGHT=/path/to/node_modules/playwright CHROMIUM=/path/to/chrome-headless-shell \
      tests/ui/run.sh

`run.sh` builds a throw-away sandbox (copy of the example, git repo, bare remote, private tmux
socket and config) and runs, one fresh daemon each:

| Script | What it plays |
| --- | --- |
| `story.cjs` | the guided story: the launch pad, two agents in parallel, tests, push, counting gestures |
| `features.cjs` | every gesture and node kind: drafts, links, splice, notes, approval gate, flyout, menus, gather, queue, library |
| `pad.cjs` | the first screen: folder, agent / model / reasoning, addenda, remembered choices, start |
| `tree.cjs` | the git tree in the corner: beside the bar, live (an uncommitted file, a commit made outside weft), a merge in two lanes, a commit unfolded with its files, another branch / every branch picked without checking out (kept per folder), folded to a box, above the bar in a narrow window, fit keeps cards out from under it |
| `next.cjs` | then / hand over / branch from the ◇, the message bar's tab, the baton line |
| `watch.cjs` | checks: tray, watch on an agent, grid, steer, approve, zoom, paste, tests watch, gate on a link |
| `eta.cjs` | the countdown: asked once, large on the card, in the top bar, asked again when it runs out |
| `bar.cjs` | the message bar's own controls: agent, model, reasoning, yolo, bound to the selected agent |
| `dial.cjs` | the dial (a ring of equal slices that opens where a link is let go, never while drawing; keys, Esc): queue a prompt (the same agent answers), hand off, git; two commits meeting in one push, an open PR, an agent after it that starts from everything carried |
| `archive.cjs` | the archive: a drop zone in the bottom left while cards move, filing by drop and by menu, ctrl+z, the list with what each card said, bring back where it was, forget; a card at work stays; keep; a lone card and a whole flow filing themselves (short times in the config) |
| `tidy.cjs` | tidy on a scattered canvas: columns left to right, a fork and its join level with the middle branch, a link that skips columns straight on a lane of its own, no link through a card or across another (checked on the drawn paths), flows stacked, lone cards in a row under them; ctrl+z, the same result again, kept by the daemon, a second tidy moves nothing |
| `quit.cjs` | quitting: the question names who is mid-work, cancel changes nothing, keep leaves the agents in tmux, end leaves no session, window or daemon |
| `plea.cjs` | needs you: an agent's question unfolds under its card with an answer box (enter on the card focuses it), a dialog shows its options as buttons, the toast says which it is; a queued prompt rewired after another agent is asked of that one |
| `peek.cjs` | the live output on a card: a click opens the terminal large, at a readable size |
| `dev.cjs` | dev only (`src/dev.rs`): the page follows edits to its files, styles in place, scripts by a reload that waits while you type (a tag says so; none otherwise) and puts back what was open, errors named by file and line |

`TESTS="pad next" tests/ui/run.sh` runs a subset. The UI is copied into the sandbox when the run
starts (each test gets its own copy), so you can keep editing while it runs; `WEFT_WEB_DIR` names
another folder to copy. `WEFT_BIN` picks the binary (default `target/debug/weft`; inside an agent
that weft started it is already set, to the installed weft). Screenshots land in the sandbox folder.
