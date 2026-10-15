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
| `next.cjs` | then / hand over / branch from the ◇, the message bar's tab, the baton line |
| `watch.cjs` | checks: tray, watch on an agent, grid, steer, approve, zoom, paste, tests watch, gate on a link |
| `eta.cjs` | the countdown: asked once, large on the card, in the top bar, asked again when it runs out |
| `bar.cjs` | the message bar's own controls: agent, model, reasoning, yolo, bound to the selected agent |
| `dial.cjs` | the dial (a ring of equal slices that opens where a link is let go, never while drawing; keys, Esc): queue a prompt (the same agent answers), hand off, git; two commits meeting in one push, an open PR, an agent after it that starts from everything carried |
| `archive.cjs` | the archive: a drop zone in the bottom left while cards move, filing by drop and by menu, ctrl+z, the list with what each card said, bring back where it was, forget; a card at work stays; keep; a lone card and a whole flow filing themselves (short times in the config) |
| `quit.cjs` | quitting: the question names who is mid-work, cancel changes nothing, keep leaves the agents in tmux, end leaves no session, window or daemon |
| `plea.cjs` | needs you: an agent's question unfolds under its card with an answer box (enter on the card focuses it), a dialog shows its options as buttons, the toast says which it is; a queued prompt rewired after another agent is asked of that one |
| `peek.cjs` | the live output on a card: a click opens the terminal large, at a readable size |
| `dev.cjs` | dev only (`src/dev.rs`): the page follows edits to its files, styles in place, scripts by a reload that waits while you type and puts back what was open, errors named by file and line |

`TESTS="pad next" tests/ui/run.sh` runs a subset. The UI is copied into the sandbox when the run
starts (each test gets its own copy), so you can keep editing while it runs; `WEFT_WEB_DIR` names
another folder to copy. `WEFT_BIN` picks the binary (default `target/debug/weft`; inside an agent
that weft started it is already set, to the installed weft). Screenshots land in the sandbox folder.
