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
| `pad.cjs` | the first screen: folder, agent / model / reasoning, addenda, remembered choices, start; a reload while an agent works opens on the canvas instead |
| `folders.cjs` | other folders: the pad's headline picks a sibling repository (a few letters, enter), the agent starts there and wears its tag, a commit after it lands in that repository; ctrl+o / the folder line move selected agents, typed paths complete, a linked card inherits the folder, a missing folder fails loudly |
| `tree.cjs` | the git tree in the corner: beside the bar, live (an uncommitted file, a commit made outside weft), a merge in two lanes, a commit unfolded with its files, another branch / every branch picked without checking out (kept per folder), folded to a box, above the bar in a narrow window, fit keeps cards out from under it; the remote: not-pushed commits marked, a push while folded unfolds it (arrow up, ripples) and folds again, a commit pushed from another clone found by asking the remote (dashed, arrow down), a pull |
| `next.cjs` | then / hand over / branch from the ◇, the message bar's tab, the baton line |
| `watch.cjs` | checks: tray, watch on an agent, grid, steer, approve, zoom, paste, tests watch, gate on a link |
| `eta.cjs` | the countdown: asked once, large on the card, in the top bar, asked again when it runs out |
| `aim.cjs` | what the bar talks to: solid chip + tag on the card, a prompt for a busy agent becomes a queued-prompt card (the next one after it), letting go after 30 s untouched (countdown, held by hovering / text in the bar), ✕ |
| `fix.cjs` | what failed, fixed: the top bar's *N failed* offers a fix, a click starts an agent a level up told what the card was asked and said (selected, under it); the chip says *fixing*, the card who is on it, a second click shows the same agent; the card's menu; `weft retry` runs it again with what was fixed |
| `joins.cjs` | links into a finished card: two finished cards linked one after the other into an agent go out as one prompt, both links drawn as in; a fork and an undone delete start once, with both results; a card linked in while it works leaves the junction done (2/3 in, its link not lit), and when it is done the junction runs again with its result alone, nothing before it running again |
| `place.cjs` | where a card that starts working lands (pad, bar with nothing selected, hand over): never left of a card that runs or has run in its row, a draft still may; a prompt queued after it lines up right next to it; no card covers another |
| `bar.cjs` | the message bar's own controls: agent, model, reasoning, yolo, bound to the selected agent |
| `dial.cjs` | the dial (a ring of equal slices that opens where a link is let go, never while drawing; keys, Esc): queue a prompt (the same agent answers), hand off, git; two commits meeting in one push, an open PR, an agent after it that starts from everything carried |
| `archive.cjs` | the archive: a drop zone in the bottom left while cards move, filing by drop and by menu, ctrl+z, the list with what each card said, bring back where it was, forget; a card at work stays; keep; a lone card and a whole flow filing themselves (short times in the config) |
| `nets.cjs` | networks: the two in the top bar, a drag with the outline following, the box (empty task shakes), enter places and starts it (links, gate on "ok", push disarmed, ETA, the agent told the task, use counted); dropped on an agent the rest carries on from it (the fix asked of that agent); the library (pictures, drag out); three roles on three agents, place only; save a selection (the task back to `{{task}}`, link order kept, in config.json), delete; the one used most comes first, after a reload too |
| `talk.cjs` | talks: the round port on top of an agent (none on a note), a drop on a note makes none, drawing one onto an agent (teal line, target lit), the topic field takes the keyboard and Enter keeps it; the working agent is told once, with the topic (the briefing waits for it), the sim says what it works on with `weft tell`, a bubble over it; the agent not started yet hears it all with its task and answers; the line counts and unfolds what they said; no link; a third agent by "let them talk"; a moved card takes its line; stop a talk; a deleted agent takes its talks |
| `tidy.cjs` | tidy on a scattered canvas: columns left to right, a fork and its join level with the middle branch, a link that skips columns straight on a lane of its own, no link or talk through a card or across another (checked on the drawn paths), a tall card pushing only its own column, two agents that talk side by side, lone cards in a block of their own, groups further apart than cards in a flow and laid out wide for the wide window; ctrl+z, the same result again, kept by the daemon, a second tidy moves nothing |
| `marks.cjs` | marks from the browser: `weft extension` (files, host told to Brave), the extension in a full Chromium with native messaging; a dev page served from the board's folder: hover label with the component, a click marks the button (the page never sees it), the note, ctrl+enter: a started card on that board with the note first, page, element, where it is written, components, selector, text, box, html, console error, failed request, two screenshots; the probe for React 19 / Vue / Svelte / Astro; a dragged area goes to the same agent as the paragraph it holds. Needs a full Chromium (`CHROME`, else Playwright's) |
| `quit.cjs` | quitting: the question names who is mid-work, cancel changes nothing, keep leaves the agents in tmux, end leaves no session, window or daemon |
| `plea.cjs` | needs you: an agent's question unfolds under its card with an answer box (enter on the card focuses it), a dialog shows its options as buttons, the toast says which it is; a queued prompt rewired after another agent is asked of that one |
| `peek.cjs` | the live output on a card: a click opens the terminal large, at a readable size; the wheel there scrolls weft's history (jump to latest), or reaches a program that reads the mouse as wheel events; zoomed to 200% the cards are drawn sharp, not a stretched picture |
| `dev.cjs` | dev only (`src/dev.rs`): the page follows edits to its files, styles in place, scripts by a reload that waits while you type (a tag says so; none otherwise) and puts back what was open, errors named by file and line |

`TESTS="pad next" tests/ui/run.sh` runs a subset. The UI is copied into the sandbox when the run
starts (each test gets its own copy), so you can keep editing while it runs; `WEFT_WEB_DIR` names
another folder to copy. `WEFT_BIN` picks the binary (default `target/debug/weft`; inside an agent
that weft started it is already set, to the installed weft). Screenshots land in the sandbox folder.
