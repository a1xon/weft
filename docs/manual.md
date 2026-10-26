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

[ claude | agy | codex ]   reasoning  auto low med high xhigh max   ■ yolo
model  default | opus | sonnet | haiku | other…
┌──────────────────────────────────────────────────────────────────────────────┐
│ Tell claude what to do in shop-api…                                          │
└──────────────────────────────────────────────────────────────────────────────┘
addenda   □ Be concise   □ No commits   □ Stay in scope   + add   prompts ▾
```

* The **folder** is the headline, with branch and uncommitted files. Starting in your home folder
  or `/` shows a warning. The same path sits above the prompt bar everywhere else. A click on it
  (or `Ctrl+O`) picks **another folder** for new agents: a sibling repository, say (see
  [Other folders](#other-folders)). Above it, your other [boards](#boards) and what they are doing.
* **Agent, model and reasoning** are one click each and remembered, for the agent you used last.
  The model is a row of choices like the reasoning level, not a menu; its list comes from the CLI
  itself (`codex debug models`, `agy models`), and *other…* takes a name that is not listed.
* **Ctrl+Enter** creates the agent in the middle of the screen and starts it; a plain `Enter` is a
  new line, so a prompt can be several paragraphs. `Tab` cycles agents, `Alt+↑/↓` the reasoning
  level, `/` opens your standard prompts, `Esc` shows the canvas.
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
| type in the bar at the bottom | a new agent, or a message to the selected one (a busy one gets a queued-prompt card after its work) |
| `↑` / `↓` in the bar or on the pad | the prompts you sent on this board before, newest first, as in a shell (`↑` on the first line, `↓` on the last; past the newest, what you were typing is back). `Ctrl+Z` brings back a prompt you just sent or cleared with `Esc`. What is typed and not sent is kept in the browser per board, so a reload (a new weft build, a board switch) puts it back |
| double-click the canvas | a draft agent there, prompt focused. Enter runs it |
| click a card's ◇ | **what next?** then · twin · hand over · branch (below) |
| drag a ◇ onto an agent | link them: the result is handed on |
| drag the ○ on top of an agent onto another agent | they **talk**: they coordinate directly while they work, and only they hear it (below) |
| drag a ◇ into empty space and let go | the line's end is where the next card goes; the **dial** opens there: queue a prompt the same agent answers · hand off to a fresh agent · git (`Alt` chooses any type) |
| click a link's chip | cycles *always* / *only if ok* / *only if it fails* |
| double-click a link | splice a step into it |
| drag a link | the end nearer to where you grabbed it comes loose: drop it on another card and the link (with its condition) goes there, a done card hands its result over at once; in empty space it opens the dial, as drawing a new link does. `Alt` keeps the old link too (a copy), `Ctrl+Z` puts it back |
| drag a **network** from the top bar onto the canvas / an agent | several cards and links in one go, told what to do / carrying on from that agent (below) |
| drag a **check** from the tray onto a link / an agent | a gate / a watch (below) |
| `Alt`-drag a card | clone it |
| drag a card's bottom-right corner | resize it: a taller card shows more rows of its terminal; double-click the corner for the default size |
| the wheel in an open terminal | scroll back: an agent that reads the mouse (Claude Code) scrolls its own transcript, anything else weft's history; *jump to latest* goes back down |
| click a card's answer, or `R` | the whole **result**, full size and laid out as the agent's terminal showed it: indents and tables kept (a markdown table from `weft done` is drawn as a table), long lines wrapped under their own indent; `+`/`-` text size, `Esc` back. `weft result <node>` prints the same text in your terminal. The prompt bar stays over it, aimed at that card: start typing (or `Enter`) to answer, `Esc` in the bar goes back to reading and keeps the text. A report that still asks you something ([below](#done-but-still-asking)) has a violet line beside each question and quick answers (`yes`/`no` or the options it gave, *do it*, *you decide*, *skip*, *answer…* to write your own) that put `> question` + answer into the bar; nothing is sent before `ctrl+enter`, *skip all* lets it go on as it is, and the reader then shows the next answer when it is in |
| click a step in a card's *steps* bar | what that step changed, as a diff; **undo this step** or **rewind to before it** puts its files back (below) |
| marquee or shift-click, then `G` | gather several results into one agent |
| right-click anything | what it can do |
| click **N failed** in the top bar | a fresh agent finds out why, fixes the cause and runs the failed cards again (below) |
| drag finished cards onto the **archive** (bottom left) | they are filed: off the canvas, their terminals end, their results kept (below) |
| look at the **git tree** (bottom right) | the commits of the branch where you are, live; its name picks another branch, a click on a commit shows its files (below) |
| **tidy** (top bar) or `T` | lines the cards up: each flow left to right in columns, a card level with what it is linked to (a chain is one straight row, a fork fans out around its start), no link crossing another or running through a card; a tall card pushes only its own column. Agents that talk stand side by side, lone cards gather in a block of their own (one per folder while agents work in several), and these groups are packed to suit the window: the picture shown largest, or one nearly as large that keeps closer to where things stood, more room between groups than inside one. `Ctrl+Z` puts it back |

Several links into one agent draw an **AND** ring (`Promise.all`): it starts when all have
arrived; switch it to `≥1` for the first (`Promise.any`). A junction that can no longer be
satisfied shows *blocked by …* instead of waiting forever. Notes are constants you can point any
agent at; they never trigger anything.

A link is lit while what it carries is in at its card, marches amber while the card before it works
on something that will come down it, and is dotted while it is still to come; the ring of a junction
counts the same way. An agent answering a prompt queued for another card works for that card: its
other links stay lit, and what already ran with its result stays *done*.

A link that delivered stays delivered, like a promise that resolved before. Link a new card into a
finished junction and it waits for that one alone, then runs again with only the new result (its
session still has the others, and the note rides along) and hands its answer down the same chain.
While a card before it works on something new, it waits for that too, so a round started further
up still meets in one run; it shows *waiting* again meanwhile, not *done*. Whatever sets a finished
card to work again (the flow, a message from you in the bar or in its terminal, a retry), what it
answers is its new result and goes down its links: the cards after it run again with only that.
No card starts, or is *done*, while anything before it is unfinished: a card at work again, a task
added before it that has not run yet, a junction that holds only some of a new round (*waiting ·
3/4 in*, a push whose commits are not all in), or a card that was green but still has prompts
queued on it (*same agent* cards) to answer. Then what follows it (another agent, a commit, a
deploy) waits until it has answered them all and gets its result then, so a commit after it takes
that work too; a queued prompt joins its agent when it is asked, so what the agent answers to it is
the agent's result (one that stays a card hands its answer on from there). That goes all the way
down the chain, and a card linked after one of them gets its next result, not the one it had. A
message typed into a card that waits for its links (a deploy before its push is in) does not start
it: it goes in after the card's own task once they are in. An agent whose process has ended starts
a new session, which is given every result again. A gate holds only what is new; a push is
described below. Only a card on a loop is no longer started by the flow once it has run `max_runs`
(25) times (*Loop guard*: run it by hand, or reset); one on no loop runs as often as the cards
before it give it something new.

### For each: one agent per job

When one agent's answer is a list of jobs that do not depend on each other (the failing tests
grouped by cause, the pages of a site, the packages still on an old API), switch the link after it
to **for each** (the link's bar or its menu; it is drawn dashed, with a *for each* badge). The agent
before it is asked to end its answer with the jobs as a numbered list. When the card after the link
starts, it takes the first job and weft makes a copy of it for each other one (same agent, model,
prompt and checks, named *fixer 2*, *fixer 3* …), all at work at the same time, each given its job
and the whole answer for what the jobs have in common. You do not need to know how many there
will be when you draw the flow: the list decides.

The copies link on where the card does, so what comes after them waits for all of them (the AND
ring) and runs once with every result. A commit, a check on the link or a prompt queued on the agent
belongs to one agent's work, so each copy gets its own and they meet at the next card that is none
of these: *plan* → *fixer* → *commit* → *push* becomes one commit per job and one push. At most
`each_max` cards (6) take a list; a longer one is shared out in runs (20 jobs: 4, 4, 3, 3, 3, 3). An
answer without a list goes to the card whole. The next list is taken by the same copies (and more
if it is longer). Jobs that touch the same files are better off with the card in its own worktree
(options). Switched back to a plain link, the copies stay with what they did; nothing comes down
to them any more.

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
| **right** | **queue a prompt.** A *same agent* card appears; its prompt is asked of the very same agent in its own session. When the agent takes it up, the card **joins the agent**: it is gone, the prompt is the agent's next turn, its links are the agent's (what was linked into it now comes into the agent, what was linked after it follows the agent), and the answer is the agent's result, so what is linked after the agent (its commit too) runs on it. Until then what is linked after the agent waits for it. Several queued on one agent are asked **one at a time, top to bottom** as the cards sit on the canvas (left to right on one line): each waits until the one above it has been answered, so they join the agent in that order, and what is linked after them takes the agent's answer to the last one. A click on its tag (*↻ same agent*) sets the **reasoning** it is asked at (as does the bar while the card is selected, or alt+↑/↓ in its box): the agent thinks at that level from then on, and one whose process runs at another level is started again on the same conversation first. A queued prompt that waits for work done after its own agent (a review of it, an approval after it, as in the networks' fixes and builds) stays a card, since joined it would loop that work back into the agent: its answer comes back into the card and goes on from there. Enter on its prompt asks it; while the agent is still at work, leaving the box any other way (drawing its commit next, a click elsewhere) queues it as well, so a prompt written for a busy agent never sits as a draft. Rewire the card after another agent (or after a gate on one) and it is asked of that agent, and so are the cards behind it. An agent is not sent back what it said itself in that session, only what comes from elsewhere. | everything it knows |
| **lower left** | **hand off.** A draft agent that will take over the work; on enter the old agent writes the handover note | a fresh session plus the note |
| **upper left** | **git**: commit, push or open a PR (below) | |

After a commit the ring has two halves: right is the *next agent*, which receives everything the git chain carried, left is push. After a push it has three: the next agent, *deploy* and open PR.
A plain click on the ◇ still opens the composer below; dropping on a card links to it.

### Next: then, twin, hand over, or branch

A new LLM session is not free: it must re-read everything. So the choice is explicit, cheapest first.

| | What | Context | Use it for |
| --- | --- | --- | --- |
| **then** | queue a prompt on the **same** agent | keeps everything it knows | follow-ups, "also do X" |
| **twin** | a **second** agent on a **copy** of its conversation | everything it knows, from then on its own | trying it another way, a side question that should not clutter the main thread |
| **hand over** | a **fresh** agent takes over the work | new session, no history; the old agent first writes a handover note, the files stay as they are | context getting heavy, a different agent or model, a clean start that keeps the work |
| **branch** | a **new** agent runs alongside and receives the result | only the result | parallel review, fan-out |

* Clicking the ◇ opens the composer with *then* preselected. `Tab` switches the mode. In the
  message bar with one agent selected, `Tab` switches between continuing it and handing over.
* **Twin** starts a new card on a copy of the agent's conversation (`claude --resume <id>
  --fork-session`, `codex fork <id>`): same agent, model and reasoning, same folder, and it is told
  it is a copy, so it does not undo, commit or report the other one's work. The first one goes on by
  itself. A dashed thread ties the two cards; nothing fires along it. It is offered where the CLI
  copies conversations and weft knows this one's id: claude always, codex once it has rested (it
  names its session when it ends), agy not at all. *twin* is in the card's menu too.
* **Queue**: every agent has one FIFO. Prompts go out one after another; links fire when the
  queue is drained, so a script of several steps hands over only its final result. A message you
  type on a settled agent is a conversation: it does not re-fire the links (hand it over
  explicitly). `Ctrl+Enter` sends; to a busy agent (from the bar or *then*) the prompt becomes a
  *same agent* card linked after the last one still waiting on it, so what comes next is on the
  canvas and can be edited, moved or deleted until it runs. `Alt+Enter` steers a busy agent
  immediately. The queue panel shows what an agent has queued inside (prompts from the flow) and
  reorders, removes and promotes.
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
  * *check*: a command must pass (build, lint, tests, a benchmark budget: anything that exits 0
    when it is right). Red goes back to the agent a few times, then you decide. It runs the
    folder's check command: weft guesses it from the build files (`Cargo.toml`, `package.json`
    scripts, `go.mod`, pytest or unittest, Gradle, Maven, mix, a `make test` target), and whatever
    you type in a network's box is remembered for that folder. A check is a tests gate that only
    runs (its options say *just run*), so a gate after it still asks the agent behind both.
  * A gate shows what came in. When the work before it goes on (the agent is set to work again
    from the bar, its terminal or a note on one of its screenshots, a prompt is queued on it, a
    card is added before it), its round is taken back: the screenshot or decision it waited for
    leaves it, a request still queued on the agent is withdrawn, and the card *waits for* that
    agent. Once everything before it is finished, it starts over with the newest results (a new
    screenshot, under a new name). A CI check is not taken back: a new push gets new runs anyway.
  * *CI*: after a push or a pull request, the CI runs of the pushed commit must pass (GitHub
    Actions, read with `gh run list --commit`). The card shows one pill per run, live. Red sends
    the end of each failed run's log (`gh run view --log-failed`) to the agent that did the work.
    That agent fixes it, commits and pushes, and the runs of the new commit are waited for: a few
    times, then you decide. Green passes on what CI said. It never pushes by itself: a commit that
    is not on the remote yet, or a repository `gh` cannot see, stops it for you. When no run
    starts within 3 minutes (no workflows, or none for this change) it passes and says so.
* **on an agent: a watch.** Nothing is blocked. While the agent works, evidence collects in a
  grid under its card: newest first, small, each with its own **steer field** and ✓.
  * *Enter* in a field steers the agent right now ("the header is cut off"); ✓ approves. A
    finished agent takes it as a new message (like one from the bar): what it does then is its
    new result, and the cards after it wait for that.
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

### Talks: agents that coordinate

Agents that work at the same time can get in each other's way: the same files, the GPU, a port,
the dev server. Drag the round port (○) on top of one agent onto another agent: a teal line
arches between their tops, and from then on the two coordinate directly. A third agent dragged
onto either of them joins the conversation (everyone a talk reaches hears everyone; *let them
talk* in the menu of a selection links several at once). Nothing fires along a talk: their
results, links and flows stay as they are.

* **What they are told.** Each agent is told once who the others are (id, name, what they were
  asked, their folder if it is another one), what to coordinate on, and how to say something
  to them. A working agent hears it at once; one that has not started gets it with its task.
  They then tell each other in a sentence or two what they work on, warn before they touch
  something of the other's, and answer only when it matters (never just to acknowledge).
* **What they coordinate on** is the field on the line: *GPU use*, *the files under src/api*,
  *who restarts the dev server*. It is optional (empty: wherever their work meets). While you
  write it the agents wait for it, so they are told once, with the topic. Changing it later
  tells them the new one.
* **How they talk:** `weft tell <id[,id]|all> "<message>"` from the agent's shell. weft types
  it into the others, marked `[weft] n3 "api" says: …`, nobody else. An agent that cannot take
  it right now (not started, a dialog, a question waiting for you, a commit weft asked for)
  gets it as soon as it can. One agent can say ten things in ten minutes, so two agents never
  chat forever.
* **On the canvas:** what is said flies along the line and shows over the one who said it for a
  moment; the bubble on the line counts it and unfolds all of it. Right-click the line or ✕ on
  the bubble stops the talk; deleting or filing an agent takes its talks along. Agents that talk
  go to the archive together, once all of them are finished.

### Git: commit, push, open a PR

Three steps, one idea each. Each step does what the steps before it would have done, so an
agent can go straight to *open PR*; the explicit chain is just easier to read.

```
 agent ──▶ commit ──┐
 agent ──▶ commit ──┼──▶ push ──▶ open PR ──▶ agent that tests it live
 agent ──▶ commit ──┘       └──▶ deploy ──▶ agent that tests it live
```

* **commit takes one agent's work.** It asks that agent (it has seen the files; nobody else has)
  to commit its own files on the target branch. Then git decides what happened: weft reads the
  commit and the working tree, merges the agent's worktree branch if it has one, and the card
  shows the commit. A second agent gets its own commit: drop its link on an occupied commit and
  that is what happens (the commit appears and joins the same push). One card per agent keeps
  failures, conflicts and branches attributable. Merge conflicts go back to the agent whose
  branch conflicts. An agent that failed is not committed; *commit now* takes what is there.
  An agent that rests is woken on its own conversation to do it. Commands, and agents whose
  conversation cannot be taken up again, are committed mechanically (secrets and `.weft` are never
  staged). Commits into one folder are serialised, so parallel agents never fight over its index.
* **push is the barrier.** It waits until every commit linked into it has resolved
  (`Promise.all`), then pushes the branch. It never forces. If the remote moved on, the agent
  that did the work is asked to bring the branch up to date (`pull --rebase`) and the push is
  tried once more. Linked straight from an agent it asks for the commit first. Commits from
  agents in different repositories ([other folders](#other-folders)) are pushed in each of them.
  A commit linked in after a push gets a push of its own: the commits pushed before count as in,
  and only the new one is reported on. After a push that failed (one commit failed), the commit
  run again brings the others back with it.
* **open PR** pushes if nothing before it did, then asks an agent that did the work to write the
  title and description and open it (`gh pr create`, or whatever the host offers). That agent
  knows why; a template does not. Extra instructions go in its options (base branch, labels,
  reviewers). The card links to the pull request.
* **deploy** puts what was pushed to use: a command card after the push (or PR; the dial offers it
  after a push, and the menu of a push or PR has *deploy what it pushes*). It runs the folder's deploy command,
  guessed from the folder (`deploy.sh`, `manual-deploy.sh`, a `deploy` script in `package.json`,
  `make deploy` or `make install`, `fly deploy`, `wrangler deploy`, `cargo install --path .`) or typed
  into its options once and kept for the folder (with none known yet the card waits for you to type it,
  and goes when you have). It runs, as soon as the push is in, on a checkout of exactly the pushed commit
  that weft keeps per repository (under `~/.local/state/weft/deploy/`; ignored build output stays,
  so builds are incremental): unfinished files of other agents in the folder never ship. *the folder
  as it is* in its options runs it in the folder instead, for a script that needs files git does not
  track. `$WEFT_FOLDER` is the folder, `$WEFT_SHA` the commit. Its output is its result (the first
  line says `[weft] deploying <sha>`), so a tester linked after it starts from that. A red deploy is
  a failed card: *N failed* has a fixer fix it, commit and push the fix, and run it again, which
  then deploys the new commit. Switched to an agent (*runs on* in its options), the agent deploys,
  in the same checkout: the command becomes what it is asked, and can be reworded on the card.
  A new deploy is made like the folder's last one: the same command or agent, prompt, model and
  options, the same checks watching it and the same gates on the link into it (an approve stays an
  approve), kept per folder in your preferences.
* **Nothing waits to be armed.** Commit, push, PR and deploy run as soon as what they wait for is in.
  Where you want to look before something leaves the machine, put a [check](#checks-gates-and-watches)
  on the link before it: *approve* holds it for your OK, *check* has a command pass, *CI* waits for
  green. A push that ran with the commits it had pushes again, on its own, for a commit linked into it
  later. One whose round already holds a commit waits when that commit's agent is set to work again
  (you answered its question, a chat): it goes out once that work is committed too, not without it.

#### Loose ends

A flow from an older weft can still have a push (or PR) that waits to be armed, and that would keep
its flow on the canvas for good: the archive files a flow only once every card in it is finished. So
weft asks git (its refs only: the git tree is what fetches) what such a push has left once its
commits are in:

* **Commits still to send:** it is *ready*. The card says *ready · 1 commit to push*, the top bar
  *N ready to push*; a click there lists them and arms one, or all.
* **Nothing to send** (a later push sent the whole branch, or you pushed by hand) **and nothing linked
  after it:** it is done by itself (*pushed before*), and its flow can go to the archive. With a card
  after it, arming stays your go-ahead for that card.

**What travels.** A commit, a push and a PR each hand on one result: where the work is (branch,
HEAD, project folder), the commits, the files changed, the pull request link if there is one, and
for every agent that took part what it was asked, what it said and which commit came of it
(budgeted to stay inside the 8 000 byte limit of piped results). Whatever you link after the
chain, a tester, a reviewer, a deploy script, starts from that, not from a bare "done".

### Rewind: what a step changed, and undo

Every step an agent takes (a prompt, an answer to a gate, what you typed in its terminal) is
bracketed by two snapshots of its folder. The steps that changed files are a progress bar above the
card's footer (*steps*): a segment per step, up to half the card wide, with what it was asked and how
many files it changed; green once it is in, yellow and blinking while it runs, struck through when
undone. A new step flashes white. Hover a step to see its files; a click shows its diff full size,
coloured as a terminal shows it.

* **Undo this step** puts the files that step changed back as they were before it: edited files
  get their old content, files it made are removed, files it deleted come back. A file that changed
  after the step (another agent, or the same one later) is left as it is, and the toast names it.
* **Rewind to before it** does the same for that step and every step after it.
* **The undo is a step too** (↺): undoing it puts the changes back. Commits are never touched,
  only the files; undone work shows up as uncommitted changes.
* The agent hears about it with its next prompt: which work was undone, which files are back, and
  not to redo it unless asked.

It works the same for every CLI (an agent's own undo, where it has one, knows only its own
edits). The snapshots live in a shadow repository per repository under `.weft/cp/` (git-ignored)
that borrows the real repository's objects. Nothing in the real repository changes: not the
folder, not its index, not its branches or refs. Only content the repository does not have yet is
stored. Ignored files, weft's own `.weft/` and untracked files over 8 MB are left out. A card keeps
its last 24 steps; a step that changed nothing leaves no segment. Deleting a card drops its snapshots.
An agent that is working cannot be rewound: wait for it, or stop it first.

### The archive

Finished work leaves the canvas so what is still going on stays in sight. The archive is the box
in the bottom left corner.

* **Drop cards on it.** Drag a finished card (or a selection) onto it and let go: the cards are
  filed. Their terminals end, their results, prompts, links and worktrees are kept. A card that is
  still at work stays where it is (stop it or let it finish first). `ctrl+z` brings back what you
  just filed; *archive* is in a card's right-click menu too.
* **It files by itself.** A lone card goes **10 minutes** after it is done. A **flow** (cards tied
  by links, a hand-over's baton line or a queued prompt's agent) goes as one, **15 minutes after its
  last step**, once every card in it is finished: done, or a branch whose condition did not come
  true. Nothing in a flow that is still under way, failed or waits for you is filed, and a card whose
  terminal you have open waits until you close it. A note goes along with the one flow it feeds, and
  so does a draft that never ran and was given nothing to do (an empty queued prompt). A finished
  flow that another draft keeps here says so on its cards (*held · push not armed*; a click shows
  that card: arm it or delete it). See [loose ends](#loose-ends) for pushes nobody armed.
  The clock only runs while you work on the canvas: two minutes without a click, a move or a key
  there and it stops (the countdowns dim) until you are back, so nothing leaves while you are away.
* **Keep.** A finished card shows when it will go (`⊟ 24m` in its footer); a click on that keeps it
  on the canvas (`kept`), and with it its whole flow. *Keep on the canvas* is in its menu too.
* **Hold archiving.** The *archiving* lock in the top bar pauses every countdown at once (amber,
  *archiving held*); nothing files itself until a second click lets it go, and the time that was left
  runs from then on. The board remembers it across restarts.
* **Read and bring back.** A click on the archive unfolds what is filed, newest first: what each
  card was asked and what it said, a search once there is more than a handful. *Bring back* puts
  the cards where they were (or next to that, if something is there now) with their results and
  links; nothing is handed on again and the next prompt starts a fresh terminal. Its clock starts
  over. *×* forgets it for good (its worktree goes too, as when a card is deleted).

`archive_s` (600) and `archive_flow_s` (900) in `config.json` set the two times in seconds; `0`
means never. The archive keeps the latest 100 drops. On disk it is a small index
(`.weft/archive.json`: what the list shows) and one gzipped pack per drop (`.weft/archive/<id>.json.gz`:
the cards, links and everything they knew), read only when you open or bring back that drop; an
archive from an older weft is moved into packs on the first start.

**Idle agents rest, and remember.** An agent CLI sitting at its prompt holds a few hundred MB. Once
an agent has finished and its terminal has sat unchanged for `rest_s` (1800 s, wall clock; `0` =
never), weft ends its process the way a person would (Ctrl+C at its prompt), and keeps its session
id: weft gave claude one when it started, codex and agy name theirs on the way out. The card stays
as it is, done with its result (*resting*). The next prompt to it (a follow-up, a queued prompt, a
commit or hand-over request) starts a process on the same conversation (`claude --resume <id>`,
`codex resume <id>`, `agy --conversation <id>`), so it still knows its work and commits only its own
files. The same holds for an agent you stopped and for one brought back from the archive. Where the
conversation cannot be taken up again (a CLI that cannot resume, a session that is gone), the new
session's first prompt says what the card was asked and what it answered last. *restart agent* in a
card's menu starts a clean conversation (told the same). Not an agent that waits for you (an issue,
a dialog), a kept one (or one in a kept flow), or one whose terminal is open, rests. On start weft
also ends agent windows left in its tmux session whose card is gone.

### The git tree

Bottom right, beside the bar: the commits of the branch where you are, drawn as a graph (a lane
per line of work, merges joining them, a ring on what is checked out, the branches and tags where
they point). *Where you are* is the folder the line above the bar names; when that line shows
another folder, the tree does too. It is live: weft looks at the folder every few seconds while a
page is open, and a commit by any agent, a fetch or a new branch shows up by itself, lit up for a
moment. Files nobody committed yet sit on top as *N uncommitted*.

* **Another branch.** Click the name in its head: the checked-out one (it follows a switch),
  *every branch* at once, or any local or remote branch. Only what is shown changes, nothing is
  checked out; the choice is kept per folder. A branch weft cut for an agent's worktree wears that
  card's name.
* **The remote.** The head says how the branch stands against the remote branch it follows:
  `↑2 to push`, `↓1 to pull`, *pushed*, `↑3 new` (never pushed), *gone on origin* (deleted there,
  say after its pull request was merged). That is true of the remote *now*: while a window is in
  front, weft asks the remote every minute where its branches are (`git ls-remote`, one short
  request that writes nothing) and fetches only when one of them moved; a click on it asks at once.
  It never asks for a password: a remote that would need one is shown as not reachable (red dot) and
  the counts are as of the last fetch. The dot beside it: lit = asked lately, hollow = not lately,
  pulsing = asking. Commits not pushed yet have hollow amber dots and an amber bar; commits on the
  remote that are not here yet stand above in their own lane, dashed and violet.
* **What happened.** The line under the head says the latest thing that happened to the branch,
  read from git's own record of it (the reflog), so it does not matter who did it (an agent, a weft
  git step, you in a terminal): *committed "…"*, *pushed 3 commits to origin/main*, *origin/main has
  2 new commits from elsewhere*, *pulled*, *merged*, *rebased*, *reset*. As it happens, a folded tree
  unfolds for a few seconds (pointer on it or a click keeps it open) and it moves: a new commit lands
  with a ripple, a push sends an arrow from its commits up to the remote, what came from the remote
  drops into its lane. A click on the line shows that commit.
* **A commit.** A click unfolds it: hash (click to copy), author, time, the whole message and the
  files with lines added and removed. *older commits…* at the end loads more.
* **Out of the way.** `–` folds it to a box with the branch, how it stands against the remote and
  what happened last (remembered); the launch pad hides it; when the window is too narrow for it
  beside the bar it sits above it.
* **Other repositories.** While the board's agents work in more than one repository (a website and
  its crawler, say), each one the tree does not show is a folded line on top of it: its name, branch,
  how it stands against its remote and what happened there last. When something happens there, its
  line unfolds for a few seconds with its latest commits, the new ones lit. A click shows that
  repository in the tree (until the folder the line above the bar names changes) and the other one
  becomes a line. Folders inside one repository, and an agent's own worktree, count as that
  repository. Where the folder you are in is no repository, only the lines show.

### Other folders

One canvas can drive agents in several repositories: a website and the crawler that feeds it, say.
The canvas (its `.weft/`, its daemon) stays with the folder weft was started in; each agent works
in a folder of its own.

* **New agents.** Click the folder on the launch pad, or the folder line above the bar, or press
  `Ctrl+O`: a picker lists the project, the folders agents already work in, the ones you used
  before and the folders beside the project (sibling repositories, those named like the project
  first). A few letters filter it (`crawl`), a path is completed from the disk (`../`, `~/work/`,
  `Tab` goes into a folder), `Enter` takes it. New agents start there until you pick another; the
  choice is remembered per project, and the pad says when it is not the canvas folder (*back to
  it*). `/folder crawler` in the bar does the same.
* **Agents that exist.** Select one or several and do the same (the folder line then shows *their*
  folder), or click a card's folder tag. A running agent moves at its next start (*restart there*
  in the toast does it now, as a fresh session). The card's options have the folder as text too.
* **Which card is where.** While the cards on the canvas work in more than one folder, each says
  which at the top (the folder's name and colour), the ones in the project folder too. When they all
  work in one folder, whichever it is, no card needs to say so and none does.
* **What goes along.** An agent linked after another one, or taking over from it, works where it
  did. A commit after an agent is made and checked in that agent's repository, and so are gates; an
  own worktree is cut from it. A push or PR fed by work in several repositories pushes each one and
  opens a pull request in each, asked of an agent that worked there (its card names them all). A
  folder that does not exist is an error, not the home folder.

### Boards

Every folder weft was started in is a **board**: a canvas with a daemon of its own. Several run at
once, each with its agents (in as many folders as it likes, see above).

```
  ◎ weft            2 agents · 1 working              the board you are on
  ● shop-api        4 agents · 2 working · 1 needs you
  ○ rubbermetrics   asleep · 6 cards
  + new board
```

* **The launch pad lists them** above its folder, the board you are on first, then the ones you
  opened last. The prompt keeps the keyboard, so the board you were on still starts with no click.
  With only one board there is just *+ new board*.
* **On the canvas**, the board's name in the top bar (or `B`) unfolds the same list under it. When
  an agent on another board needs you, a badge next to the name says so (*shop-api needs you*);
  a click takes you there.
* **A click switches this window** to that board; `Ctrl`+click (or a middle click) opens it in a
  window of its own. A board whose daemon is not running *sleeps*: a click wakes it (its cards come
  back as they were), and its row counts its cards meanwhile.
* **+ new board** asks for a folder and makes it one. A double-click on the name of the board you
  are on, in the top bar's list, renames it (empty: the folder's name again); the name is kept in
  its `flow.json`. The × on a sleeping board leaves it off the list; its cards stay in its folder.
* **`weft` in a folder that belongs to a board opens that board**, with its new agents starting
  in the folder: a folder inside the board's folder, or one its agents work in. So `weft` in
  `rubbermetrics/crawler` opens *rubbermetrics* rather than a second board. `weft new [dir]` makes a
  board of its own anyway. The home folder and `/` never take in folders below them.
* Your settings (last agent, model, addenda, library, networks) are the same on every board: a
  board picks up what you changed on another one when its page connects or comes to the front.

Boards never call each other: each daemon writes what it is doing (agents, working, needs you,
cards, its agents' folders) to `<id>.live.json` beside its record in `~/.local/state/weft/`, and
a board with a page open reads the others' every few seconds.

### Marks from the browser

Show the agent what you mean instead of describing it. A browser extension (Brave, Chrome,
Chromium, Edge, Vivaldi) marks things on any web page, in a PDF or in a Markdown file and makes a
card of it:

```
weft extension                 # once: then "Load unpacked" the folder it names in brave://extensions
```

* **`Alt+Shift+W`** (or the weft button): hover shows what is under the cursor (`button.cta 180×48
  <Hero> src/Hero.tsx:12`), a click marks it, a drag marks an area, `↑`/`↓` make the mark
  bigger/smaller. The page never sees that click.
* **Selected text**: select text first, then `Alt+Shift+W`: that text is the mark, highlighted on
  the page, and the agent gets it word for word with the element it is in.
* A box under the mark takes your note: each mark shows its text or a close-up, a click on its name
  scrolls to it, *to* and *on* say who gets it and on which board, ▾ folds the box away. *+ mark
  another* (or `Alt+Shift+W` again) adds numbered marks, each with its own note. `Ctrl+Enter`
  sends; *card only* places the card without starting it.
* **Which board**: the one whose folder the page's server runs in (the process listening on the
  page's port, its repository), a weft canvas its own board; for a file opened from this machine (a
  `file://` page or PDF) the board that has its folder or repository; else the one you picked for
  that page last time. A folder without a board is offered as a new one (for a file, only a repository). **To whom**: a new agent, set up the
  way the launch pad would (last agent, model, reasoning, addenda), in the server's folder; or an
  agent on the board (the next mark on a page goes to the agent the last one made): typed now when
  it is free, a queued-prompt card after its work while it is busy.
* **What the agent gets**, after your words: the page (URL, title, viewport), and per mark where
  the element is written and the components around it (React, Vue, Svelte, Angular dev builds;
  Astro and inspector plugins' source attributes), selector, text, box and the styles that shape it,
  its HTML, a close-up and a whole-view screenshot with the mark outlined (saved in
  `.weft/shots/`), then the page's console errors and failed requests. On `localhost`,
  `127.0.0.1` and `*.localhost` the errors are collected from the page's start.
* **PDFs**: the browser's own PDF viewer lets no extension in, so the weft button on a PDF opens it
  in weft's viewer (PDF.js, in the extension) and the picker starts there. A click marks a line,
  `↑` its paragraph, then its page; a drag marks an area and takes the text inside it. Zoom with
  `ctrl +`/`ctrl -`/`ctrl 0` or `ctrl`+wheel; back goes to the browser's viewer. The agent gets the
  file, the page, where on it (in points), the text and the screenshots, and is told to change the
  PDF's source (HTML, Markdown, LaTeX, …) and make it again when it has one. Local files need
  *Allow access to file URLs* on the extension (on by default for an unpacked one); a PDF from a
  website asks once to read from that site. A drag over text there highlights it as a mark of its
  own, `Alt`+drag still draws an area.
* **Markdown**: a `.md` file opened from this machine (dropped on the browser, say) opens in weft's
  Markdown viewer by itself, drawn as it reads, the file's line numbers beside each block on hover;
  the weft button on Markdown from a website opens it there too. Select text to highlight it: it is
  a mark at once, with its own note, and you go on reading and selecting (on a wide window the box
  stays in the margin on the right). *mark* (`Alt+Shift+W`) picks a block: a click marks a
  paragraph, heading, list item, table row or code block, `↑` its list, its section, the whole file.
  *source* shows the file as written, line by line, and marks lines there. The agent gets the file,
  per mark the selected text and the lines of the file it is on, as they are written, and is told
  to change the file.

The extension never learns a daemon's address or token: the browser starts `weft` itself as the
extension's native messaging host (only this extension may), and `weft` talks to the board.

## Networks

A network is a whole flow in one gesture: cards, their links, gates, git steps and watches. The two
you use most sit in the **top bar**; all of them are in the **library** (also `/` → *network: …*).

* **Drag one onto the canvas.** Its outline follows the pointer. Let go and a box under it asks what
  it is for; `Enter` places it and starts it, *place only* puts the cards down without starting
  anything, `Esc` drops it. A click on a chip puts it in the middle of the view.
* **Drag one onto an agent** and it carries on from that agent's work instead: the agent is its first
  card, the rest is linked after it at once (and starts as soon as the agent is done). Its task is
  that agent's prompt. This is how you add *review & fix* or *ship it* to work that is already going.
* **Agents are roles.** The first is the agent you work with (the bar's), a second opinion goes to the
  next agent you have (claude → agy → codex); the box lets you pick each, and remembers.
* Nothing waits to be armed: the network's checks (and its *approve*) are where you look before
  anything is pushed.

`[check]` below is a check gate (the folder's check command must pass, red goes back to the agent).

| Built in | Cards | Needs you |
| --- | --- | --- |
| *build and ship* | | |
| **screenshot & ship** | build → [check] → screenshot (confirm, or ask for changes) → commit → push | once |
| **review & fix** | build → a *second* agent reviews the uncommitted change → the builder fixes what it agrees with → [check] → commit → push | – |
| **tests & ship** | build → tests gate (written, run, red goes back twice) → commit → push | – |
| **plan first** | plan without touching files → you approve → the same agent builds it → [check] → commit → push | once |
| **build & ship** | build → [check] → commit → push | – |
| *bigger jobs* | | |
| **issue → PR** | read the issue (asks you if it is unclear) and plan → you approve → build → second agent reviews → fix → [check] → commit on `issue-<n>` → push → open PR | once |
| **tests first** | A writes failing tests only → you look at them → B (another model) implements without touching them → [check] → A audits the code for tricks that only satisfy the tests → B fixes → each commits its own files → push | once |
| **team of three** | split in three parts with file ownership and the interfaces written down → you approve → three builders in their own worktrees → a commit each (merged; a conflict goes back to its agent) → the planner integrates → [check] → commit → push | once |
| **best of three** | three attempts (a model each) in worktrees → [check] each → a judge compares and recommends → you approve one per attempt → only the approved one is committed | once |
| **fix a bug** | reproduce it (failing test) and find the cause → the same agent fixes the cause → [check] → commit → push | – |
| **three plans** | three agents plan on their own → the first compares, takes the best and builds it → [check] → commit → push | – |
| *pull requests* | | |
| **PR + review** | build → [check] → commit on a branch of its own → push → open PR → [CI] (red goes back to the builder, which fixes and pushes) → a second agent reviews the pull request and comments on it | – |
| **answer my review** | check out your PR, sort every comment: do / answer / push back → you approve → do it → [check] → commit → push → reply on every thread, ask for review again | once |
| **review their PR** | correctness ‖ design and security (another model) ‖ a test run in a separate worktree → one merged review → you approve → posted as one review | once |
| *drop on an agent* | | |
| **harden it** | its change → security review ‖ missing tests ‖ docs → the author fixes what they found → [check] → everyone commits their own files → push | – |

Fixes, builds after a plan, integration and the bug fix are *queued prompts* on the agent that did
the work, so it keeps everything it read; reviewers, auditors and judges start fresh, ideally on
another model. A network
with a pull request commits on a branch named after what you type (`issue-42`, else
`weft/<first words>`); the box shows it and you can change it.

**Your own.** Select cards on the canvas, right-click, *save as network*. Links keep their order, the
first agent becomes the role *you work with*, and its prompt becomes `{{task}}`: what you type when
you drop it (a network you placed from another one gets its template back). They are kept in
`config.json` (`networks`), so every project has them; rename, describe or delete them in the library.
The top bar shows the two networks you used most.

## Prompt library

*Standard prompts* are copied in when you insert them (`/`). *Addenda* are referenced, so editing
one changes every agent that uses it. Everything is editable under *library* (*prompts & addenda*) or
in place. The built-in **Report with weft commands** addendum (on by default, see below) teaches an
agent to finish with `weft done`, `weft issue` or `weft fail`.

## The prompt bar

The bar at the bottom is the launch pad's own prompt box in a compact form (one component: the
same choices, box, key hints, start button and addenda, with less padding and smaller type). It
always carries the choices that make an agent, one click each: **agent**, **reasoning**, **yolo**
and, on a second row, the **model**. With nothing selected they are for the next new agent (the
same remembered choices as the launch pad, and its addenda sit under the box). With an agent
selected they are *that agent's* own settings and apply the next time it starts. The keys are the
pad's: `Ctrl+Enter` sends (starts the agent, or messages the selected one; while it is busy the
prompt becomes a queued-prompt card linked after it), a plain `Enter` is a new line, `Tab`, `/` and `Alt+↑/↓` for reasoning; `Alt+Enter`
steers the selected agent now. The footer under the text lists the ones that apply. What the bar
talks to is plain to see: its chip turns solid and names the card (✕ lets go), the box is outlined,
and every selected card carries a *prompt bar → here* tag. Nobody touching the selection (or typing
to it) for 30 s lets go of it, so a prompt typed later starts a new agent instead of landing on an
old card; the tag counts down the last 10 s. Text in the bar, an open flyout, terminal, reader or
menu holds it. Not every agent takes every choice: `agy` accepts
low / medium / high, and only with its default model (its other models carry the level in the
name, and agy drops the whole model choice when the two disagree), so the control is hidden where
it would not work.

## What counts as finished

| Setting | Meaning | Use for |
| --- | --- | --- |
| Screen goes quiet | the screen did not change for N s after it was given work | any agent TUI (default) |
| Process exits | exit code 0 = succeeded, otherwise failed | `cargo test`, scripts |
| Reports done | only an explicit `weft done [--fail]` counts | long, bursty agents |

A card that shows a trust, update or approval dialog, or Claude Code's own question tool (its numbered
choices with *Enter to select*), goes to *needs you* and nothing is typed into it: typing into the
question tool would answer it for you (patterns per agent, configurable; a pattern counts near the bottom of the screen with a choice
under it, or on the last line, so an agent showing the same words in code or a diff is not taken
for a dialog). What it needs unfolds **under the card**: the dialog, with its
numbered options as buttons (plus enter / esc), or an agent's question (below), readable and
scrollable, with an answer box under it. Enter on the selected card puts the cursor there. Answering
in its terminal works too: once nothing is open any more the card shows what it did before.
Something new typed into a finished agent's terminal on the canvas (and Enter) makes it work again,
like a message from the bar: the card is busy, the run counts, and its answer is the new result,
which goes down its links like any other.

### Silence is not an answer

An agent that stops talking may be finished, or it may have said "I will get back to you when the
build is done", or it may be waiting for an answer. Nothing resumes it, handing a promise on would
send nonsense to the next step, and a card that says *done* while it waits for your decision hides
the question (and files itself in the archive half an hour later). So **every prompt weft gives an
agent asks it to end its final message with one line**, and weft reads that line from the output:

```
STATUS: DONE                         finished, and checked
STATUS: ISSUE <your question>        needs an answer or a decision from the person
STATUS: WAITING <what for>           waits for something; weft wakes it to check
STATUS: FAILED <why>                 cannot be done
```

| The last line says | weft does |
| --- | --- |
| `DONE` | its message (without the line) is the result and goes on |
| `FAILED` | the step fails with the reason: the "only if it fails" links fire |
| `ISSUE` | the card says *asks you* and the question unfolds under it with an answer box; a chain waits; your answer (there, in the bar or in its terminal) continues the same run |
| `WAITING` | the card says what it waits for; after `recheck_s` (60 s) weft types a short *check on it now* into the session, repeatedly, until it is DONE, or an ISSUE after `background_s` |
| nothing | a step of a chain is asked once for the line; if that does not help, silence is the end, so a flow never hangs. A lone card (nothing waits for its answer) is simply done, with no extra turn |

The message is read from the screen. Claude Code keeps its own history on the alternate screen and
shows only the last rows of it, so while a step runs weft lays each frame of the screen over the
answer so far (rows the frames share are matched, a line that is still being written is taken in
place): a long answer arrives whole, head included. What the window still shows from before the
prompt (the end of the last answer, with whatever it asked there) is not part of this one.

It is plain text on the screen, so it needs no tool, no shell, no permission and no particular CLI:
claude, codex, agy or anything with a prompt, in any permission mode. It costs one instruction in
the prompt (about 70 tokens the first time in a session, a one-line reminder after) and one line at
the end of each answer (not part of the card's result), no extra turn. What you type straight into
an agent's terminal gets nothing, and neither do weft's own requests (commit, screenshot, handover:
those weft checks itself, with git, a file or a test run). `"chain_status": false` in `config.json`
turns it off.

### Done, but still asking

An agent often finishes and still leaves you something: "Should I push?", a list under *Decisions
left for you*, "...: your call", "want me to fix Safari too?". weft reads the report (`weft done`,
the message before `STATUS: DONE`, or a lone card's last message) for what it asks you. If it asks
anything, the step is **not done**: the card *asks you*, nothing goes on and nothing is filed. Under
the card each question has quick answers: `yes`/`no` or the options it gave, *do it* for a decision
it left to you, *you decide*, *skip*, and *answer…* to write your own. They fill the answer box with
`> question` and the answer, and Enter sends it. The agent goes on in the same session and is told to
report again with a whole summary. A question you quoted does not hold it a second time. *skip all*
(or sending nothing but skips) lets the report go on as it stands: it is the result. *read it all*
opens it in the reader with the questions marked where they stand.

The report is read by a model: the agent's own CLI, with `asks_cmd` (claude `-p --model sonnet
--effort medium`, codex `exec -m gpt-6-luna` at medium, agy `gemini-3.8-flash-medium`). It reads
only the last answer: that report, its last 8,000 characters (every report on the boards fits),
and your last message to the agent, nothing older. claude runs it without MCP servers and with a
one-line system prompt: about 0.6k tokens of context besides the report, where its own system
prompt and your MCP tools were 52k. It also
writes the quick answers (*yes, deploy and apply* / *no, leave it for the next deploy*). Meanwhile
the card is still at work (*reading its report for questions…*, a few seconds); then it is done, or
it asks you. A recap of what you or the agent decided, notes, to-dos, the agent's own choices and
offers ask nothing. Why a model: weft's rules took a recap of your own answers under *Your
decisions:* for decisions left open, and held a finished crawl (rubbermetrics n142). The smallest
models made the same kind of mistakes on the reports saved on every board. An ISSUE gets its
quick answers the same way.

The rules (`src/asks.rs`) still read a report when the CLI has no `asks_cmd`, when the reading
fails, and with `"asks": false`. Only the agent's own prose counts there: code, tables, quotes,
headings and words in quotation marks are not questions, and neither is one it answers itself
("Is it fast? Yes, 3 ms"). *Decisions:* on its own lists what it decided. An offer ("let me know if
you want a PR") is shown but holds nothing. After a restart, a report the rules held is read by the
model once more. weft's own requests (commit, screenshot, handover) are never held. The report
addendum tells agents to ask with `weft issue` in the first place.

A step of a chain also gets one line first: *you are one step in a chain run by weft, what you write
last is handed on, messages starting with `[weft]` are from weft*. A lone agent gets only the status
line, unless it has the addendum below.

Agents that can run a shell say the same words with commands, which carry exact text:
`weft done "..."`, `weft issue "..."`, `weft fail "..."`. The **Report with weft commands** addendum
is on by default, so new agents get these commands and not the status line; switch it off for an
agent to get the status line back. An agent told to use the commands that goes quiet without doing
so is finished by that silence (after a grace of `signal_grace_s`, 12 s); weft types nothing into it.

A prompt is typed only into a process that is **up**: it has drawn something and then gone quiet
for a moment. A shell still loading its rc files, or a TUI still painting its first screen, loses
or garbles what is typed into it, so a slow start just delays the first prompt (a process that
draws nothing at all is given eight seconds).

An agent that ended its turn but says it is still waiting for something it started in the
background (Claude Code's status line reads `1 shell still running`) is **not** finished: the
card stays busy ("waiting on background work") until that ends and the agent has answered, or
`background_s` (10 minutes) have passed, in case a server was left running on purpose.

A turn in progress (`esc to interrupt` on the status line) holds an agent the same way, however
long its output pauses. weft's own copy of the screen is checked against tmux's when that marker
sits on a screen that has not changed for 20 seconds: if the pane does not show it, the copy is
replaced by what tmux shows. A marker on a screen that has not moved for `background_s` is dropped.

### Usage limits

When an agent's CLI stops at its usage limit ("You've hit your session limit · resets 3pm", "try
again at 3:04 PM", "try again in 2 hours"), the card is not done and its message is no result. The
agent sleeps: a violet **LIMIT** countdown sits where an ETA would, the status line says *usage
limit · resumes 15:02*, and the cards after it wait. Nothing is typed into it meanwhile (the CLI
would drop it), queued prompts included. At the reset time (plus a minute and a half) weft tells it
to carry on with its task. A click on the countdown (or its menu) offers **try now**, or **hand
over** to another CLI: a fresh agent of that CLI gets the original task, what the sleeper said and
what git says, and takes its links. The sleeper is stopped, so it never wakes up in the same files.

The patterns are `limits` per agent in `config.json` (Claude Code, Codex and the usual quota
messages for Antigravity are built in). A message that names no time is tried again every
`limit_retry_s` (1200 s); after 30 sleeps in one step the card asks you. A step that gave its
status line finished the normal way, whatever it said about limits.

**How much is used.** Under the agents in the launch pad and the bar, a meter shows how much of the
chosen agent's plan is used: the 5-hour window and the week (amber from 75 %, red from 90 %), plus
the chosen model's own limit where the CLI has one (Claude's *Sonnet only* week, agy's groups of
models). Its tooltip says when each starts over. The CLI is asked by `usage_cmd`, which costs no
tokens (claude and agy answer `-p /usage` themselves; for codex the newest of its own session logs is
read, nothing is asked), at most every 15 minutes and only while a page shows that agent. The answer
is kept in `~/.local/state/weft/usage.json` for every board, so a reload, a second page or another
board does not ask again; a click on the meter asks now, at most once a minute.

### When something fails

A click on **N failed** in the top bar (or *fix it* in a failed card's menu) starts a fresh agent
under the failed cards, one reasoning level above the agents that failed, at least *high* (their
CLI and model; the person's last agent when no agent was behind them). It is told, for each
card: what it is, its folder, the cards before and after it (and their status), what it was asked
and what it said (a tests gate adds what the suite printed). The rest is one `weft read` /
`weft result` away. It looks for the cause (several failures often share one, and a
card can fail only because the one before it did), fixes it, checks the fix, and runs the cards
again:

```
weft retry <card> "<what was wrong and what you changed>"
```

The card does its own work again with the input it had (the latest results of the cards before
it), and what was fixed goes with its task; what it hands on goes down its links as the first
time, so the cards it held up follow by themselves. A gate checks again; a git step takes its
input up again (the agents are asked to commit). *run it again* in a failed card's menu does the
same without a note. Something only the person can solve (access, a decision) comes back as a
question, not as a workaround.

While it works the chip says *fixing*, the failed cards say who is on them, and another click
shows that agent instead of starting a second one. If it gives up, the next click starts a new one,
told about it too.

## CLI

```
weft [dir]                    open the canvas (a folder of a board: that board, new agents starting there)
weft new [dir]                a board of its own for the folder, even inside another board's
weft app [dir] [--pick]       the same in a window of its own (no dir: the project opened last)
weft desktop                  put weft in the app grid, to pin it to the dock
weft extension                the browser extension (marks on a web page become cards) and its link to weft
weft ls                       nodes, status, runs, queued
weft send [--queue|--now] <node> <text|->   a prompt into a node (starts it if needed)
weft result <node>            its latest result
weft read <node> [-n N]       terminal history of a node
weft run                      run the flow
weft retry <node> [text|-]    run a finished card again with the input it had; the text (what was fixed) goes with its task
weft stop [dir] [--keep]      quit: the daemon and every agent of the project end (--keep leaves the agents in tmux)
# inside an agent:
weft done|issue|fail [text]   report this node: finished / cannot go on without the person / cannot be done
weft shot <image> [--note ..] show a screenshot to the reviewer; returns at once
weft eta <90|2m|1m30s> [--note ..]   say how long it will take (what is left); returns at once
weft tell <id[,id]|all> <text|->     say something to the agents it talks with (only they hear it)
```

Everything goes through one small local HTTP API, so scripts can drive a flow too:
`weft send reviewer "$(git diff)"`.

## Files and config

| Path | What |
| --- | --- |
| `<project>/.weft/flow.json` | the graph; plain JSON, safe to commit and hand-edit |
| `<project>/.weft/state.json`, `run/`, `shots/` | run state, prompt files, images; git-ignored |
| `<project>/.weft/archive.json`, `archive/` | filed work: an index, and a gzipped pack per drop (cards, links, results); git-ignored |
| `~/.config/weft/config.json` | **your overrides only**, merged onto the defaults; also your saved networks |
| `~/.local/state/weft/` | per board: the daemon's record (port, token), `<id>.live.json` (what it is doing, for the board list), its log; `recent` |

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
substrings that mean "a human is needed", `limits` the ones that mean "it stopped at its usage limit"
(see [Usage limits](#usage-limits); `limit_retry_s` is how often one without a reset time is tried
again). `gh` names GitHub's CLI for the [CI check](#checks-gates-and-watches) (default `gh`).

**Card names.** A card is named by its own agent's CLI with the smallest model at the lowest
reasoning: `title_cmd` (claude `-p --model haiku --effort low`, without MCP servers or its own
system prompt, codex `exec -m gpt-6-luna -c model_reasoning_effort=low`, agy `-p --model
gemini-3.8-flash-low`), run with `sh -c` outside the
project, the request in `$WEFT_TITLE_REQ`, the last printed line taken. Until it answers (and
for an agent without one) the prompt's first words stand in. Git steps are named after the work
they carry: `commit: <agent>`, `push: <agent> + <agent>`, `PR: <agent> + 2 more`, and follow the
links and names upstream. A name you give yourself stays. `"titles": false` turns the calls off.
`asks_cmd` reads reports the same way (`$WEFT_ASKS_REQ`, JSON back; see *Done, but still asking*);
`"asks": false` turns it off.

**Sessions.** How a card keeps its agent's conversation across processes (see *Idle agents rest*):
`session_arg` gives a new session the id weft picked (claude `--session-id {}`), `resume_arg` takes
an ended one up again (claude `--resume {}`, codex `resume {}`, agy `--conversation {}`), `fork_arg`
starts a twin on a copy (claude `--resume {} --fork-session`, codex `fork {}`), `resume_hint` is what
the CLI prints right before its id when it ends (codex `codex resume `, agy `--conversation=`) and
`exit_keys` end it at its prompt so it does (`["C-c"]`, agy `["C-c", "C-c"]`). They go right after
the agent's own `cmd`, before the flags weft adds. An agent without them starts every process anew,
and its first prompt then says what the card did before.

## How it works

* `tmux -C` (control mode) streams every pane's output as events. The daemon feeds each pane
  into an in-process terminal emulator (`vt100`), so previews, idle detection and the live
  terminal never spawn `capture-pane` and nothing polls tmux.
* Prompts are sent as a bracketed paste (if the app enabled it) followed by Enter, keystrokes as
  literal bytes, so UTF-8 and multi-line text arrive intact.
* Claude Code draws on the alternate screen, where a terminal keeps no history: an answer longer
  than the window is only in Claude Code. So the wheel goes to a program that turned on mouse
  reporting as wheel events (it scrolls itself; Ctrl+End brings it back), and to weft's own history
  of the pane otherwise. A restart re-seeds each screen with the mouse modes tmux reports.
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
src/launch.rs      `weft app` / `weft desktop`: a window of its own, the dock entry, recent projects
src/boards.rs      boards: the live files, the list, which board a folder belongs to, waking a daemon
src/model.rs       the flow graph, node and config types, built-in defaults: plain JSON on disk
src/server.rs      loopback HTTP + WebSocket, token and Host checks
src/web.rs         puts the page together from web/ (embedded in the binary)
src/dev.rs         dev only: the page from disk, following edits (see "Working on weft's UI")
src/tmux.rs        the control-mode client: events in, validated keystrokes out
src/term.rs        screen helpers: HTML rows, previews, telling an answer from terminal furniture
src/words.rs       everything weft says to an agent, and how it reads the status line back
src/asks.rs        what an answer still asks the person, with answers to pick: a model's reading of it, and the rules that stand in
src/client.rs      the small HTTP client the CLI commands use
src/browser.rs     marks from the browser: the native messaging host, which board a page's server or a local file belongs to, `weft extension`
src/engine/        the single actor that owns all state
    mod.rs         the Engine, its events, the loop and its heartbeat
    state.rs       what is known per node while it runs (Rt), and what survives a restart
    graph.rs       nodes and links: add, edit, delete, names, ids
    flow.rs        results travelling along links: joins, triggers, queued prompts, prompt text
    merge.rs       a queued prompt joins its agent when asked: its links and reasoning level become the agent's
    each.rs        "for each": the jobs an answer lists, one copy of the next card per job, joined again
    run.rs         reading a screen, finishing a step, settling, the prompt queue
    reads.rs       a model reads a report for what it leaves to the person; the card waits for it
    agent.rs       the process side: spawn in tmux, type, stop
    checks.rs      watches: screenshots, tests, changes, the ETA countdown
    gates.rs       the gates on a link: approve, screenshot, tests
    ci.rs          the CI check: the runs of the pushed commit (gh), red to the agent that did the work
    limits.rs      an agent at its usage limit sleeps until the reset its CLI names, then carries on
    usage.rs       how much of each agent's plan is used (the meter by the agents), asked every 15 min at most
    rewind.rs      every step snapshotted before and after in a shadow repository: its diff, and undo
    resume.rs      a card keeps its agent's conversation: session ids, an agent that rests and comes back, twins
    loose.rs       loose ends: a push an older weft left waiting to be armed says what it has left, or is done by itself
    deploy.rs      the deploy step: the folder's deploy command on a checkout of the pushed commit
    git.rs         commit, push and open PR as steps; repo.rs is the git plumbing under them
    handover.rs    a fresh agent takes over another one's work
    fix.rs         what failed, fixed: a stronger agent told what failed, and `weft retry`
    talk.rs        agents that talk: who is told what, `weft tell`, messages held until they can be taken
    archive.rs     finished work filed away (by hand or by itself), kept, brought back
    history.rs     the git tree in the corner: a branch's commits, what happened to it (reflog), the remote (ls-remote)
    commands.rs    the command set and its dispatcher (each command lives in its domain's module)
    ui.rs          what browsers are told, and the state file
    info.rs        the folders agents work in (branch, uncommitted files, sibling folders), each CLI's model list
    boards.rs      this board's live file, the other boards for the pages, open / rename / forget, shared settings
    marks.rs       a mark from the browser becomes a card: a new agent set up like the launch pad's, or a queued prompt
web/               index.html (the shell), css/*.css and js/*.js (joined in file order); dev.js (dev only)
extension/         the browser extension: background.js (button, screenshots, native messaging), mark.js (the
                   picker and the note box), probe.js (in the page's own world: components, console),
                   viewer.html/js (the PDF viewer: lines, paragraphs, what a mark is), md.html/js (the
                   Markdown viewer: blocks and their lines, selected text), md-open.js (a local .md goes
                   there), view.mjs + viewer.css (what both viewers share), pdfjs/ (PDF.js 6.3.289) and
                   mdit/ (markdown-it 15.0.2), both gzipped into the binary by build.rs
tests/             e2e.py (real tmux, simulated agent), ui/ (browser), engine/tests.rs (unit)
```

A change starts in the module that owns the idea. Nothing outside `src/engine/` touches a node's
state, and nothing inside `repo.rs` knows what a node is.

## Limits to know about

* "Screen goes quiet" is a heuristic. An agent that thinks silently for longer than the quiet
  time looks finished; raise *Quiet for* or use *Reports done*. The status line catches
  most of what the heuristic misses, but it trusts the agent to write it (and to write it truthfully).
* `weft done`, `weft issue`, `weft fail`, `weft shot` and `weft tell` need the agent to be allowed to reach
  `127.0.0.1` and run a shell command. Sandboxed modes that block that fall back to the quiet
  heuristic and the status line (plain text), and watches then need a capture command or
  pasted images.
* A steering message typed into a TUI while it works is handled the way that TUI handles it
  (most queue it for the next turn).
* Dialogs shown on first start (trust, update, login) are held with *needs you*, but only for
  patterns weft knows. Add yours under `attention`.
* Two agents editing one folder collide. Tick *own git worktree* on agents that run in parallel.
* A step's "before" snapshot is taken as its prompt goes out, not before: an edit the agent makes in
  its first moment can land in it. Two agents working in one folder show each other's edits in
  their steps; undo leaves what changed since alone, but cannot tell whose an edit was.
* The CI check reads GitHub Actions runs with `gh` (`gh` in `config.json` names another one). It
  looks at one repository, the folder of the work, and at the runs of one commit.
* Usage limits are read from the CLI's own words (`limits` per agent). A CLI that words it
  differently is not caught; add its phrase there.
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
stay), and an open browser tab reloads itself to the matching page. Nothing else to restart:
every other running daemon notices the new binary within seconds and replaces itself the same
way, so all boards get the upgrade, not only the one it was installed for (a daemon run from
`target/` does not, as cargo rewrites it under the tests). Two replacements at once cannot start
two daemons: each board's daemon holds `~/.local/state/weft/<id>.lock`.

### From the dock

```
weft desktop                   # once: weft appears in the app grid; right-click it, Pin to Dash
```

The icon runs `weft app`: the canvas opens in a window of its own (a Chromium-family browser in
app mode, with a profile of its own: the default browser if it is one, else Brave, Chrome, Edge
or Chromium; `WEFT_BROWSER` picks another), on the project you opened last. Which folder each
agent works in is chosen on the launch pad. Right-click the icon for the five projects opened
last, or *Other folder…* to pick one; the first start ever asks too. On Wayland the window's id
is `<browser>-127.0.0.1__-Default` for every port, which the entry names as its
`StartupWMClass`, so all weft windows sit under the one icon. Without such a browser, weft opens a
tab. Rerun `weft desktop` after changing the default browser or moving the binary.

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
open (view, selection, flyout, terminal, launch pad). It waits while you type or drag; an amber
tag top left says so then (click it to reload now). An error in the page's script shows in a bar
with its file and line; the follower runs apart from the page's script, so a script that no
longer parses reloads by itself once fixed.
A file added under `css/` or `js/` is part of the next load (and must be listed in `web.rs`
before the next build; `cargo test` says so).

Every other board follows weft's `web/` too, once weft's source folder is a board itself (decided
when a daemon starts), so all open dashboards get the edits, not only weft's own.
Changes to `src/` still need a build and an install (every daemon takes it up; the agents keep running).
`WEFT_WEB_DIR=<folder>` serves another copy of `web/` the same way; `WEFT_WEB_DIR=off` turns it
off. It is a development aid, kept apart so it can go: `src/dev.rs`, `web/dev.js`,
`tests/ui/dev.cjs` and the lines marked `dev:` (`grep -rn 'dev:' src tests`).

Fonts: Chakra Petch and JetBrains Mono, both SIL Open Font License, embedded.
