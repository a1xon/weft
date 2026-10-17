/* ------------------------------------------------------------ networks */
/* A network is cards and their links placed in one go. Drag one from the top bar (the two you use
   most) or from the library onto the canvas and say what it is for; dropped on an agent it carries
   on from that agent's work instead. In a network an agent is a role: @main is the agent you work
   with (the bar's), @alt and @alt2 the next ones, for a second opinion. {{task}} is what you type at
   the drop, {{agent}} (in a name) the agent a role went to. The built-in networks are here; the ones
   you save from the canvas are kept in the config (`networks`). */
const NET_COL = NW + 150, NET_ROW = NH + 80;
const NET_ROLES = ['@main', '@alt', '@alt2'];
/* what a saved card keeps (no place, no state: those belong to where it is put down) */
const NET_FIELDS = ['name', 'prompt', 'cmd', 'finish', 'quiet_s', 'join', 'worktree', 'model', 'effort', 'yolo', 'addenda',
  'git_op', 'branch', 'remote', 'push', 'message', 'subject', 'write_tests', 'retries', 'confirm', 'ci', 'w', 'h'];
/* {{check}} is the folder's check command (in a gate's command; in a prompt it reads "`cmd`", or
   "the build and tests" when there is none), {{branch}} a branch named after the task */
const NB_ = (c, r = 0) => ({ key: 'a', agent: '@main', prompt: '{{task}}', c, r, checks: ['eta'], does: 'builds what you ask' });
const NK_ = (key, c, r = 0, x = {}) => ({ key, agent: 'tests', name: 'check', write_tests: false, cmd: '{{check}}', c, r, does: 'the check command must pass, red goes back', ...x });
const NC_ = (c, r = 0, key = 'c', x = {}) => ({ key, agent: 'git', git_op: 'commit', c, r, does: 'the agent that did the work commits it', ...x });
const NP_ = (c, r = 0, key = 'p') => ({ key, agent: 'git', git_op: 'push', c, r, does: 'pushes the branch once its commits are in' });
const NG_ = (key, c, r, does) => ({ key, agent: 'gate', c, r, does });
const NI_ = (key, c, r = 0) => ({ key, agent: 'tests', name: 'CI', write_tests: false, ci: true, c, r, does: 'the CI runs of the pushed commit must pass, red goes back' });
const T_PLAN = 'Work out how to do this before any code is written. Read what you need, then write a short, concrete plan: the approach, the files you will change, the risks, and how you will check that it works. Do not modify any files yet.\n\nThe task: {{task}}';
const T_REVIEW = 'Review the change another agent just made. It is not committed yet: `git status` and `git diff` show it.\n\nThe task was: {{task}}\n\nLook for bugs, missed edge cases, regressions and anything the task asked for that is missing. Do not modify any files. List concrete findings, most important first, each with file:line and how to fix it. If it is good as it is, say so.';
const T_FIX = 'A second agent reviewed your change; its findings are below. Fix the ones you agree with, and say in one line each why you leave any out. Keep the build and the tests green.';
const NET_BUILTIN = [
  // ---------------------------------------------------------------- build and ship
  {
    id: 'look', group: 'build and ship', title: 'build → check → screenshot → ship', short: 'screenshot & ship', ask: 'what should it build?',
    about: 'An agent builds it; the check command must pass (red goes back to it); then it shows you a screenshot. You confirm it (or ask for changes: they go back to the same agent), it commits its own work and the push goes out.',
    nodes: [NB_(0), NK_('k', 1), { key: 's', agent: 'shot', c: 2, r: 0, does: 'asks it for a screenshot, waits for you' }, NC_(3), NP_(4)],
    links: [['a', 'k'], ['k', 's', 'ok'], ['s', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'review', group: 'build and ship', title: 'build → review → fix → check → ship', short: 'review & fix', ask: 'what should it build?',
    about: 'An agent builds it; a second agent reviews the uncommitted change without touching it; the first one fixes what it agrees with, in its own session (it still knows everything). The check command must pass, then it commits and the push goes out.',
    nodes: [
      NB_(0),
      { key: 'r', agent: '@alt', name: 'review', role: 'reviewer', c: 1, r: 1, does: 'reviews the change, edits nothing', prompt: T_REVIEW },
      { key: 'f', agent: 'step', host: 'a', name: 'fix findings', c: 2, r: 0, does: 'the builder fixes what it agrees with', prompt: T_FIX },
      NK_('k', 3), NC_(4), NP_(5),
    ],
    // a queued prompt is asked of the agent its last link comes from: the builder's link goes last
    links: [['a', 'r'], ['r', 'f'], ['a', 'f'], ['f', 'k'], ['k', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'tested', group: 'build and ship', title: 'build → tests → ship', short: 'tests & ship', ask: 'what should it build?',
    about: 'An agent builds it, then writes tests for it and the check command is run; red goes back to it (twice, then you decide). Green: it commits its work and the push goes out.',
    nodes: [NB_(0), { key: 't', agent: 'tests', cmd: '{{check}}', c: 1, r: 0, does: 'tests written, the check run, red goes back' }, NC_(2), NP_(3)],
    links: [['a', 't'], ['t', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'plan', group: 'build and ship', title: 'plan → approve → build → check → ship', short: 'plan first', ask: 'what should it do?',
    about: 'The agent reads the code and writes a plan without changing anything. You read it and approve (message it first to change the plan). Then the same agent builds it with everything it learnt; the check must pass; it commits and the push goes out.',
    nodes: [
      { key: 'a', agent: '@main', prompt: T_PLAN, c: 0, r: 0, checks: ['eta'], does: 'reads and plans, changes nothing' },
      NG_('g', 1, 0, 'you approve the plan'),
      { key: 'b', agent: 'step', host: 'a', name: 'build it', c: 2, r: 0, does: 'the same agent builds it',
        prompt: 'The plan is approved. Build it now, check that it works ({{check}}, and a look at the result where there is one), and say what you did.' },
      NK_('k', 3), NC_(4), NP_(5),
    ],
    links: [['a', 'g'], ['g', 'b'], ['b', 'k'], ['k', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'ship', group: 'build and ship', title: 'build → check → ship', short: 'build & ship', ask: 'what should it build?',
    about: 'An agent builds it; the check command must pass (red goes back to it); it commits its own work and the push goes out.',
    nodes: [NB_(0), NK_('k', 1), NC_(2), NP_(3)],
    links: [['a', 'k'], ['k', 'c', 'ok'], ['c', 'p']],
  },
  // ---------------------------------------------------------------- bigger jobs
  {
    id: 'issue', group: 'bigger jobs', title: 'issue → plan → build → review → PR', short: 'issue → PR', ask: 'which issue? (number, link, or what it says)',
    about: 'The agent reads the issue and the code (and asks you if what is wanted is unclear), plans, and builds it after you approve the plan. A second agent reviews the change, the first fixes what it agrees with, the check must pass. It commits on a branch named after the issue, pushes, and opens the pull request.',
    nodes: [
      { key: 'a', agent: '@main', c: 0, r: 0, checks: ['eta'], does: 'reads the issue, plans (asks you if unclear)',
        prompt: 'Work on this issue: {{task}}\n\nIf it is a GitHub issue, `gh issue view <number> --comments` shows it. Read it and the code it touches. If what is wanted is unclear, ask before you plan. Then write a short plan: the approach, the files you will change, and how you will check it ({{check}}). Do not modify any files yet.' },
      NG_('g', 1, 0, 'you approve the plan'),
      { key: 'b', agent: 'step', host: 'a', name: 'build it', c: 2, r: 0, does: 'the same agent builds it',
        prompt: 'The plan is approved. Build it now and check that it works ({{check}}). Say what you did.' },
      { key: 'r', agent: '@alt', name: 'review', role: 'reviewer', c: 3, r: 1, does: 'reviews the change, edits nothing', prompt: T_REVIEW },
      { key: 'f', agent: 'step', host: 'a', name: 'fix findings', c: 4, r: 0, does: 'the builder fixes what it agrees with', prompt: T_FIX },
      NK_('k', 5), NC_(6, 0, 'c', { branch: '{{branch}}', does: 'commits on a branch named after the issue' }), NP_(7),
      { key: 'q', agent: 'git', git_op: 'pr', c: 8, r: 0, does: 'the builder opens the pull request',
        prompt: 'If the work is for a GitHub issue, write "Fixes #<number>" in the description.' },
    ],
    links: [['a', 'g'], ['g', 'b'], ['b', 'r'], ['r', 'f'], ['b', 'f'], ['f', 'k'], ['k', 'c', 'ok'], ['c', 'p'], ['p', 'q']],
  },
  {
    id: 'tdd', group: 'bigger jobs', title: 'tests first → implement → audit → ship', short: 'tests first', ask: 'what should it do? (the behaviour, as precisely as you can)',
    about: 'One agent writes the tests from your description, and nothing else; you look at them. A second agent (another model) implements until they pass, without touching them. The test author then audits the code for tricks that only satisfy the tests, and the implementer fixes what it finds. Tests and code are committed by their authors.',
    nodes: [
      { key: 'a', agent: '@main', role: 'writes the tests', c: 0, r: 0, does: 'writes failing tests, no code',
        prompt: 'Write automated tests for the behaviour described below, and nothing else: no production code. Cover the main cases and the edge cases. Run them and show that they fail because the behaviour is missing (not because of a typo). Say which test files you added.\n\nThe behaviour: {{task}}' },
      NG_('g', 1, 0, 'you look at the tests'),
      { key: 'b', agent: '@alt', name: 'implement', role: 'implements', c: 2, r: 0, checks: ['eta'], does: 'implements until the tests pass',
        prompt: 'Make the new tests described below pass. Another agent wrote them from this description; they define what is wanted:\n\n{{task}}\n\nDo not change or weaken the tests (if one is wrong, say so instead of editing it). Run the whole suite ({{check}}) and say what you did.' },
      NK_('k', 3, 0, { retries: 3 }),
      { key: 'd', agent: 'step', host: 'a', name: 'audit', c: 4, r: 1, does: 'the test author audits the code',
        prompt: 'Another agent implemented the behaviour your tests describe; its summary is below and `git diff` shows the change. Check that it really implements the behaviour and does not game the tests: tests changed or weakened, special cases that only fit the test data, hard-coded answers. Do not modify files. List what is wrong, or say it is sound.' },
      { key: 'e', agent: 'step', host: 'b', name: 'fix the audit', c: 5, r: 0, does: 'the implementer fixes what the audit found',
        prompt: 'The author of the tests audited your implementation; its findings are below. Fix what is right, keep the tests as they are, and say why for anything you leave.' },
      NC_(6, 0, 'cb', { does: 'the implementer commits the code' }), NC_(6, 1, 'ca', { does: 'the test author commits the tests' }), NP_(7),
    ],
    // each queued prompt's own agent links last: the audit is asked of the test author (via its
    // approval), the fix of the implementer (via the check)
    links: [['a', 'g'], ['g', 'b'], ['b', 'k'], ['k', 'd', 'ok'], ['g', 'd'], ['d', 'e'], ['k', 'e', 'ok'], ['e', 'cb'], ['d', 'ca'], ['cb', 'p'], ['ca', 'p']],
  },
  {
    id: 'team', group: 'bigger jobs', title: 'split → three build at once → integrate → ship', short: 'team of three', ask: 'what should they build?',
    about: 'One agent splits the work into three parts that do not touch the same files, with the shared interfaces written down; you approve the split. Three agents build a part each, in their own worktrees, and commit it. The first agent puts it together, fixes the seams, the check must pass, and the push goes out.',
    nodes: [
      { key: 'a', agent: '@main', role: 'plans, builds, integrates', c: 0, r: 1, checks: ['eta'], does: 'splits it in three parts',
        prompt: 'Split this into exactly three parts that three agents can build at the same time without editing the same files. For each part: what it does, the files it owns (only it writes them), and what it needs from the others. Write the shared interfaces (types, function signatures, API shapes) down in full, and say which part creates each of them. If it really has fewer independent parts, say "nothing to do" for the rest. Number them PART 1, PART 2, PART 3. Do not modify any files.\n\nThe task: {{task}}' },
      NG_('g', 1, 1, 'you approve the split'),
      ...[1, 2, 3].map(i => ({ key: 'b' + i, agent: '@main', name: 'part ' + i, worktree: true, c: 2, r: i - 1, does: `builds PART ${i} in its own worktree`,
        prompt: `You are one of three agents building the plan below at the same time, each in its own worktree. Build PART ${i}, and only that: edit only the files it owns, and build against the shared interfaces exactly as written. Check that your part builds. If PART ${i} says there is nothing to do, say so and stop.` })),
      ...[1, 2, 3].map(i => NC_(3, i - 1, 'c' + i, { does: `commits PART ${i} (merged; a conflict goes back to its agent)` })),
      { key: 'i', agent: 'step', host: 'a', name: 'integrate', c: 4, r: 1, does: 'puts it together, fixes the seams',
        prompt: 'The three parts of your plan are built and merged here (the commits are below). Make it work as a whole: build it, run the checks ({{check}}), fix the seams between the parts (interfaces that do not match, missing wiring), and check that the task is done.' },
      NK_('k', 5, 1), NC_(6, 1, 'c'), NP_(7, 1),
    ],
    links: [['a', 'g'], ['g', 'b1'], ['g', 'b2'], ['g', 'b3'], ['b1', 'c1'], ['b2', 'c2'], ['b3', 'c3'], ['c1', 'i'], ['c2', 'i'], ['c3', 'i'], ['i', 'k'], ['k', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'race', group: 'bigger jobs', title: 'three attempts → checks → judge → you pick', short: 'best of three', ask: 'what is the hard problem?',
    about: 'Three agents (a different model each, if you have them) solve the same task in their own worktrees; each attempt has to pass the check. A judge compares them and recommends one. You approve the attempt you want (and reject the others): only that one is committed. For hard bugs and performance work; it costs three times the tokens.',
    nodes: [
      ...[['@main', 'attempt 1, judge'], ['@alt', 'attempt 2'], ['@alt2', 'attempt 3']].map(([ag, role], i) => ({
        key: 'x' + (i + 1), agent: ag, role, name: 'attempt · {{agent}}', worktree: true, c: 0, r: i, checks: i ? [] : ['eta'], does: 'solves it in its own worktree',
        prompt: '{{task}}\n\nWork in your worktree. When it is done, check it ({{check}}) and say what you did and how you checked it.' })),
      ...[1, 2, 3].map(i => NK_('k' + i, 1, i - 1)),
      { key: 'j', agent: '@main', name: 'judge', c: 2, r: 1, does: 'compares the attempts, recommends one',
        prompt: 'Three agents solved the same task, each in its own git worktree; their reports and check results are below. `git worktree list` shows the worktrees, `git diff HEAD...<branch>` each change. Compare them: does it solve the task, did its check pass, how was it verified, size and clarity of the change. Do not modify any files. Recommend one, say why in a few lines, and what the others did better.\n\nThe task: {{task}}' },
      ...[1, 2, 3].map(i => NG_('g' + i, 3, i - 1, `you approve attempt ${i} (or reject it)`)),
      ...[1, 2, 3].map(i => NC_(4, i - 1, 'c' + i, { does: `commits attempt ${i} if you approved it` })),
    ],
    links: [
      ...[1, 2, 3].map(i => ['x' + i, 'k' + i]), ...[1, 2, 3].map(i => ['k' + i, 'j']),
      ...[1, 2, 3].flatMap(i => [['k' + i, 'g' + i, 'ok'], ['j', 'g' + i]]), ...[1, 2, 3].map(i => ['g' + i, 'c' + i, 'ok']),
    ],
  },
  {
    id: 'bug', group: 'bigger jobs', title: 'reproduce → fix → check → ship', short: 'fix a bug', ask: 'what is wrong? (what happens, what should happen)',
    about: 'The agent first reproduces the bug (with a failing test where it can) and finds the root cause without fixing it. Then it fixes the cause in the same session, the check must pass (red goes back), and the fix is committed and pushed.',
    nodes: [
      { key: 'a', agent: '@main', c: 0, r: 0, checks: ['eta'], does: 'reproduces it, finds the cause',
        prompt: 'Find the cause of this bug before you change anything. Reproduce it first, with a failing automated test if the project has tests, and show that it fails. Then explain the root cause with file:line. Do not fix it yet.\n\nThe bug: {{task}}' },
      { key: 'f', agent: 'step', host: 'a', name: 'fix the cause', c: 1, r: 0, does: 'fixes the cause, same session',
        prompt: 'Now fix the root cause, not the symptom. The test that reproduces it must pass, and the rest of the suite too. Say what you changed and why that fixes the cause.' },
      NK_('t', 2), NC_(3), NP_(4),
    ],
    links: [['a', 'f'], ['f', 't'], ['t', 'c', 'ok'], ['c', 'p']],
  },
  {
    id: 'council', group: 'bigger jobs', title: 'three plans → best build → ship', short: 'three plans', ask: 'what should they work out?',
    about: 'Three agents (a different one each, if you have them) plan it on their own. The first then compares the plans, takes the best ideas and builds it in its own session; the check must pass; it commits and the push goes out.',
    nodes: [
      { key: 'p1', agent: '@main', name: 'plan · {{agent}}', role: 'plans, then builds', prompt: T_PLAN, c: 0, r: 0, checks: ['eta'], does: 'plans, then builds the best' },
      { key: 'p2', agent: '@alt', name: 'plan · {{agent}}', role: 'plans too', prompt: T_PLAN, c: 0, r: 1, does: 'plans on its own' },
      { key: 'p3', agent: '@alt2', name: 'plan · {{agent}}', role: 'plans too', prompt: T_PLAN, c: 0, r: 2, does: 'plans on its own' },
      { key: 'b', agent: 'step', host: 'p1', name: 'build the best', c: 1, r: 1, does: 'compares the plans, builds the best',
        prompt: 'Other agents planned the same task on their own; their plans are below. Compare them with yours and take the best ideas (say which, and why, in a few lines). Then build it and check that it works ({{check}}).' },
      NK_('k', 2, 1), NC_(3, 1), NP_(4, 1),
    ],
    links: [['p2', 'b'], ['p3', 'b'], ['p1', 'b'], ['b', 'k'], ['k', 'c', 'ok'], ['c', 'p']],
  },
  // ---------------------------------------------------------------- pull requests
  {
    id: 'pr', group: 'pull requests', title: 'build → check → PR → CI → review', short: 'PR + review', ask: 'what should it build?',
    about: 'An agent builds it, the check must pass, and it commits on a branch of its own; the branch is pushed and the agent opens a pull request. CI must pass on it (red goes back to the builder, which fixes and pushes). A second agent reviews the pull request and posts its review there.',
    nodes: [
      NB_(0), NK_('k', 1), NC_(2, 0, 'c', { branch: '{{branch}}', does: 'commits on a branch named after the task' }), NP_(3),
      { key: 'q', agent: 'git', git_op: 'pr', c: 4, r: 0, does: 'the builder opens the pull request' },
      NI_('i', 5),
      { key: 'v', agent: '@alt', name: 'review the PR', role: 'reviewer', c: 6, r: 0, does: 'reviews it, comments on the PR',
        prompt: 'A pull request was just opened for the work described below. Review it: `gh pr diff` and `gh pr view` show it. Look for bugs, missing tests and anything that does not do what the task asked. Post your review on the pull request with `gh pr review --comment` (do not approve or merge it), then sum it up here.\n\nThe task was: {{task}}' },
    ],
    links: [['a', 'k'], ['k', 'c', 'ok'], ['c', 'p'], ['p', 'q'], ['q', 'i'], ['i', 'v', 'ok']],
  },
  {
    id: 'answer', group: 'pull requests', title: 'review comments → sort → do → reply', short: 'answer my review', ask: 'which pull request? (number or link)',
    about: 'The agent checks out your pull request, reads every review comment and sorts them: to do, to answer, to push back on. You approve that (message it first to change it). It makes the changes, the check must pass, it commits and pushes, then replies on every thread and asks for another review.',
    nodes: [
      { key: 'a', agent: '@main', c: 0, r: 0, checks: ['eta'], does: 'reads the comments, sorts them',
        prompt: 'Address the review on this pull request: {{task}}\n\nCheck out its branch (`gh pr checkout <number>`). Read every review comment: `gh pr view <number> --comments` and the line comments (`gh api repos/{owner}/{repo}/pulls/<number>/comments`). Sort them into: to do (and how), questions to answer, and points you would push back on (and why). Do not change any files yet.' },
      NG_('g', 1, 0, 'you approve what it will do'),
      { key: 'b', agent: 'step', host: 'a', name: 'do it', c: 2, r: 0, does: 'makes the changes',
        prompt: 'Go ahead: make the changes you listed (and any I asked for since), and check that it works ({{check}}).' },
      NK_('k', 3), NC_(4, 0, 'c', { does: 'it commits on the pull request’s branch' }), NP_(5),
      { key: 'r', agent: 'step', host: 'a', name: 'reply', c: 6, r: 0, does: 'replies on every thread, asks for review',
        prompt: 'The changes are pushed. Reply on each review thread of the pull request: what you changed (with the commit), the answer to each question, and your reasoning where you pushed back. Then ask the reviewers for another review (`gh pr edit <number> --add-reviewer <them>`).' },
    ],
    links: [['a', 'g'], ['g', 'b'], ['b', 'k'], ['k', 'c', 'ok'], ['c', 'p'], ['p', 'r']],
  },
  {
    id: 'their', group: 'pull requests', title: 'review a PR: two reviewers + a test run → one review', short: 'review their PR', ask: 'which pull request? (number or link)',
    about: 'Two agents (different models) review someone else’s pull request, one for correctness and tests, one for design and security; a third tries it out in a separate worktree. The first merges everything into one review. You read it (message it to change something) and approve: only then is it posted.',
    nodes: [
      { key: 'a', agent: '@main', role: 'correctness, then the review', c: 0, r: 0, does: 'correctness, edge cases, tests',
        prompt: 'Review this pull request: {{task}}\n\nRead it with `gh pr view <number>` and `gh pr diff <number>`, and as much of the code around it as you need. Look for bugs, missed edge cases, missing or weak tests, and anything that does not do what the pull request says it does. Do not modify any files and do not post anything. List concrete findings with file:line, most important first.' },
      { key: 'b', agent: '@alt', name: 'design review', role: 'design and security', c: 0, r: 1, does: 'design, security, performance',
        prompt: 'Review the design of this pull request: {{task}}\n\nRead it with `gh pr view <number>` and `gh pr diff <number>`. Does it fit the codebase, is it the simplest thing that works, naming and structure; security (input handling, secrets, permissions); performance. Do not modify any files and do not post anything. List concrete findings with file:line, most important first.' },
      { key: 't', agent: '@alt2', name: 'try it', role: 'runs it', c: 0, r: 2, does: 'runs it in a separate worktree',
        prompt: 'Try out this pull request: {{task}}\n\nCheck it out in a separate worktree so nothing here changes (`git fetch origin pull/<number>/head:review-<number>` and `git worktree add ../review-<number> review-<number>`), run its checks there ({{check}}), and try what it changes where you can. Report what passed, what failed (with the output) and what you saw. Remove the worktree when you are done (`git worktree remove`).' },
      { key: 's', agent: 'step', host: 'a', name: 'one review', c: 1, r: 1, does: 'merges everything into one review',
        prompt: 'Two more reports on the same pull request are below: a design review and a test run. Merge them with your findings into one review: no duplicates, most important first, each finding with file:line and a suggested fix, and a one-line verdict at the top. Do not post it yet.' },
      NG_('g', 2, 1, 'you approve the review'),
      { key: 'o', agent: 'step', host: 'a', name: 'post it', c: 3, r: 1, does: 'posts it on the pull request',
        prompt: 'Post the review (with any changes I asked for) on the pull request as one review that comments, with `gh pr review <number> --comment --body-file <file>`; put findings that point at a line as line comments where you can (`gh api`). Do not approve and do not request changes.' },
    ],
    links: [['b', 's'], ['t', 's'], ['a', 's'], ['s', 'g'], ['g', 'o']],
  },
  // ---------------------------------------------------------------- drop on an agent
  {
    id: 'harden', group: 'drop on an agent', title: 'security ‖ tests ‖ docs → fix → check → ship', short: 'harden it', ask: 'what should it build? (or drop this on an agent)',
    about: 'Best dropped on an agent that has built something. Three agents look at its uncommitted change at once: a security review, new tests (they may fail where the code is wrong), and the docs. The agent that built it fixes what they found, the check must pass, everyone commits their own files and the push goes out.',
    nodes: [
      { key: 'a', agent: '@main', prompt: '{{task}}', role: 'builds, writes the docs', c: 0, r: 1, checks: ['eta'], does: 'builds it (or the agent you drop it on)' },
      { key: 's', agent: '@alt', name: 'security', role: 'security review', c: 1, r: 0, does: 'security review, edits nothing',
        prompt: 'Review the change another agent just made for security. It is not committed yet: `git status` and `git diff` show it. Look at input handling and validation, injection, secrets, permissions, unsafe defaults, dependencies. Do not modify any files. List concrete findings with file:line and how to fix them, or say there are none.\n\nThe task was: {{task}}' },
      { key: 't', agent: '@alt2', name: 'tests', role: 'writes the tests', c: 1, r: 1, does: 'writes the missing tests',
        prompt: 'Another agent just made the change that `git diff` shows (not committed yet). Write the tests it is missing: the main behaviour and the edge cases. Add test files only; do not change production code. Run them. If one fails because the code is wrong, keep it and say so.\n\nThe task was: {{task}}' },
      { key: 'd', agent: '@main', name: 'docs', c: 1, r: 2, does: 'updates the docs',
        prompt: 'Another agent just made the change that `git diff` shows (not committed yet). Update the documentation it affects (README, docs, comments that are now wrong, the changelog if there is one). Edit documentation only.\n\nThe task was: {{task}}' },
      { key: 'f', agent: 'step', host: 'a', name: 'fix what they found', c: 2, r: 1, does: 'the builder fixes what they found',
        prompt: 'Three agents looked at your change: a security review, new tests (some may fail where the code is wrong) and the docs. Their reports are below. Fix the security findings you agree with and every bug the new tests expose (do not weaken the tests). Say in one line each why you leave anything out.' },
      NK_('k', 3, 1), NC_(4, 0, 'ca', { does: 'the builder commits its code' }), NC_(4, 1, 'ct', { does: 'the test writer commits the tests' }),
      NC_(4, 2, 'cd', { does: 'the docs are committed' }), NP_(5, 1),
    ],
    links: [['a', 's'], ['a', 't'], ['a', 'd'], ['s', 'f'], ['t', 'f'], ['d', 'f'], ['a', 'f'], ['f', 'k'], ['k', 'ca', 'ok'], ['t', 'ct'], ['d', 'cd'], ['ca', 'p'], ['ct', 'p'], ['cd', 'p']],
  },
];
/* one shape for built-in and saved networks: x/y in canvas units, links as {from, to, on} */
function netNorm(t, own) {
  const nodes = (t.nodes || []).map(n => {
    const o = { ...n, x: n.c != null ? n.c * NET_COL : (+n.x || 0), y: n.r != null ? n.r * NET_ROW : (+n.y || 0) };
    delete o.c; delete o.r;
    o.checks = (n.checks || []).map(c => (typeof c === 'string' ? { kind: c } : c));
    return o;
  });
  const links = (t.links || []).map(l => (Array.isArray(l) ? { from: l[0], to: l[1], on: l[2] || 'done' } : { on: 'done', ...l }));
  return { ...t, nodes, links, own: !!own, short: t.short || t.title };
}
const NETS = NET_BUILTIN.map(t => netNorm(t, false));
/* what each card of a network placed here was filled in with: saved again, it turns back into
   {{task}}, {{check}} and {{branch}} */
const netTask = new Map();
ICON.check = I('<path d="M3 4.5l3.2 3.5L3 11.5M8.5 12h4.5"/>');
ICON.ci = I('<path d="M12.8 6.2A5 5 0 0 0 3.6 5.4M3.2 9.8a5 5 0 0 0 9.2.8"/><path d="M12.9 2.6v3.6H9.3M3.1 13.4V9.8h3.6"/>');
/* the command a folder's work must pass ('' = the agent runs the checks itself) */
const checkFor = dir => folderCmd('check', dir);
/* a check gate as a card: the folder's command must pass, red goes back to the agent */
const checkGate = dir => ({ name: 'check', write_tests: false, cmd: checkFor(dir || newDir()) });
/* a CI check as a card: the runs of the pushed commit must pass (engine/ci.rs) */
const ciGate = () => ({ name: 'CI', write_tests: false, ci: true });
/* a branch named after the task: issue-42 for an issue, else weft/<first words> */
function branchFor(task) {
  const m = /(?:issues?\/|#|^\s*)(\d+)\b/.exec(task || '');
  if (m) return 'issue-' + m[1];
  const w = (task || '').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim().split(' ').filter(Boolean).slice(0, 5).join('-').slice(0, 40).replace(/-+$/, '');
  return w ? 'weft/' + w : '';
}
const usesCheck = net => net.nodes.some(t => /\{\{check\}\}/.test((t.cmd || '') + (t.prompt || '')));
const usesBranch = net => net.nodes.some(t => (t.branch || '').includes('{{branch}}'));
const allNets = () => [...NETS, ...(Array.isArray(S.nets) ? S.nets.map(t => netNorm(t, true)) : [])];
const netUses = id => (S.prefs.netUse || {})[id] || 0;
/* the top bar holds the two you use most (before you have used any: the first two) */
const topNets = () => allNets().map((n, i) => [n, i]).sort((a, b) => netUses(b[0].id) - netUses(a[0].id) || a[1] - b[1]).slice(0, 2).map(x => x[0]);
/* a tests gate that only runs a command: a check */
const isCheck = t => t.agent === 'tests' && t.write_tests === false && !t.ci;
const netKind = t => (t.agent === 'step' ? 'step' : isGate(t.agent) ? 'check' : KINDS.includes(t.agent) ? t.agent : 'agent');
const isRole = a => NET_ROLES.includes(a);
/* the agents that start the network: agents nothing but a note leads into */
function netEntries(net) {
  const notes = new Set(net.nodes.filter(t => t.agent === 'note').map(t => t.key));
  const into = new Set(net.links.filter(l => !notes.has(l.from)).map(l => l.to));
  return net.nodes.filter(t => netKind(t) === 'agent' && !into.has(t.key)).map(t => t.key);
}
/* who plays which role: @main the agent you work with (or the one dropped on), the others the next
   agents in the list (what you picked for them last time) */
function netRoles(net, main) {
  const ks = agentList().map(x => x[0]);
  const m = ks.includes(main) ? main : ks.includes(S.lastAgent) ? S.lastAgent : ks[0];
  const saved = (S.prefs.netRole || {})[net.id] || {}, i = Math.max(0, ks.indexOf(m)), out = {};
  NET_ROLES.forEach((r, j) => { out[r] = j === 0 ? m : ks.includes(saved[r]) ? saved[r] : ks[(i + j) % Math.max(1, ks.length)]; });
  return out;
}
const roleAgent = (a, roles) => (isRole(a) ? roles[a] : a);
const netName = (t, ag) => (t.name || '').split('{{agent}}').join(ag || 'agent');
function netLabel(t, roles) {
  const k = netKind(t);
  if (k === 'git') return GIT_VERB[t.git_op || ''];
  if (k === 'step') return '↻ ' + (t.name || 'then');
  if (k === 'note') return t.name || 'note';
  if (k === 'check') return t.ci ? 'CI' : isCheck(t) ? 'check' : { gate: 'approve', shot: 'screenshot', tests: 'tests' }[t.agent];
  const ag = roleAgent(t.agent, roles), nm = netName(t, ag);
  return nm && !nm.includes(ag) ? `${ag} · ${nm}` : nm || ag;
}
function netDoes(t) {
  if (t.does) return t.does;
  const line = s => (s || '').split('\n').map(x => x.trim()).find(Boolean) || '';
  const k = netKind(t), p = line(t.prompt);
  if (k === 'git') return { commit: 'the agent commits its work', push: 'pushes the branch', pr: 'opens a pull request', '': 'commits and pushes' }[t.git_op || ''];
  if (k === 'check') return { gate: 'waits for your approval', shot: 'asks for a screenshot, waits for you', tests: 'tests are run, red goes back' }[t.agent];
  if (p === '{{task}}' || !p) return k === 'agent' ? 'does what you ask' : 'carries on';
  return p.length > 70 ? p.slice(0, 69) + '…' : p;
}
/* "{{task}}" becomes what was typed; with nothing typed the lines that need it go */
function fillTask(s, task) {
  if (!(s || '').includes('{{task}}')) return s;
  if (task) return s.split('{{task}}').join(task);
  return s.split('\n').filter(l => !l.includes('{{task}}')).join('\n').replace(/\n{3,}/g, '\n\n').trim();
}
/* a small picture of it: one square per card, the links between them */
function netGlyph(net, W, H) {
  const xs = net.nodes.map(t => t.x), ys = net.nodes.map(t => t.y);
  const x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
  const s = Math.max(4, Math.min(7, H / 2.6)), p = s / 2 + 1;
  const at = t => [x1 > x0 ? p + (t.x - x0) / (x1 - x0) * (W - 2 * p) : W / 2, y1 > y0 ? p + (t.y - y0) / (y1 - y0) * (H - 2 * p) : H / 2];
  const pos = new Map(net.nodes.map(t => [t.key, at(t)]));
  const svg = sv('svg', { class: 'nglyph', viewBox: `0 0 ${W} ${H}`, width: W, height: H, 'aria-hidden': 'true' });
  for (const l of net.links) {
    const a = pos.get(l.from), b = pos.get(l.to); if (!a || !b) continue;
    const m = (a[0] + b[0]) / 2;
    svg.append(sv('path', { class: 'gl', d: `M${a[0]} ${a[1]}C${m} ${a[1]} ${m} ${b[1]} ${b[0]} ${b[1]}` }));
  }
  for (const t of net.nodes) { const [x, y] = pos.get(t.key); svg.append(sv('rect', { class: 'gn ' + netKind(t), x: x - s / 2, y: y - s / 2, width: s, height: s })); }
  return svg;
}
/* the bigger picture in the library: a box per card with what it is */
function netDiagram(net, roles) {
  const cw = 132, rh = 64, bw = 116, bh = 38;
  const xs = net.nodes.map(t => t.x / NET_COL), ys = net.nodes.map(t => t.y / NET_ROW);
  const mx = Math.min(...xs), my = Math.min(...ys);
  const W = (Math.max(...xs) - mx) * cw + bw + 8, H = (Math.max(...ys) - my) * rh + bh + 8;
  const pos = new Map(net.nodes.map(t => [t.key, [4 + (t.x / NET_COL - mx) * cw, 4 + (t.y / NET_ROW - my) * rh]]));
  const svg = sv('svg', { class: 'ndia', viewBox: `0 0 ${W} ${H}`, width: W, height: H });
  for (const l of net.links) {
    const a = pos.get(l.from), b = pos.get(l.to); if (!a || !b) continue;
    const x1 = a[0] + bw, y1 = a[1] + bh / 2, x2 = b[0], y2 = b[1] + bh / 2, m = (x1 + x2) / 2;
    svg.append(sv('path', { class: 'gl' + (l.on === 'ok' ? ' ok' : l.on === 'fail' ? ' fail' : ''), d: `M${x1} ${y1}C${m} ${y1} ${m} ${y2} ${x2} ${y2}` }));
  }
  for (const t of net.nodes) {
    const [x, y] = pos.get(t.key), g = sv('g', { class: 'gb ' + netKind(t) });
    const lab = netLabel(t, roles);
    g.append(sv('rect', { x, y, width: bw, height: bh }));
    const tx = sv('text', { x: x + 8, y: y + 23 }); tx.textContent = lab.length > 17 ? lab.slice(0, 16) + '…' : lab;
    const tip = sv('title'); tip.textContent = lab + ': ' + netDoes(t);
    g.append(tx, tip);
    svg.append(g);
  }
  return svg;
}

/* ------------------------------------------------------------ the top bar: the two you use most */
function renderNetChips() {
  const box = $('#nets'); if (!box) return;
  const top = topNets(), key = JSON.stringify(top.map(n => [n.id, n.short]));
  if (box._key === key) return;
  box._key = key;
  box.replaceChildren(...top.map(net => {
    const b = h('button', { class: 'netchip', 'data-net': net.id,
      title: `${net.title}\n${net.about}\n\nDrag it onto the canvas and say what it is for, or onto an agent to carry on from its work. A click puts it in the middle.` },
    netGlyph(net, 30, 14), h('span', {}, net.short));
    b.addEventListener('pointerdown', e => dragNet(e, net));
    return b;
  }));
}

/* ------------------------------------------------------------ the outline while it is dragged */
const netGhost = h('div', { id: 'netghost', hidden: true });
let ghostKey = '';
function drawGhost(net, at, roles, skip) {
  if (!netGhost.isConnected) world.append(netGhost);
  const key = JSON.stringify([net.id, net.title, skip, roles]);
  if (key !== ghostKey) {
    ghostKey = key;
    const svg = sv('svg', { class: 'nglinks' });
    const by = new Map(net.nodes.map(t => [t.key, t]));
    for (const l of net.links) {
      const a = by.get(l.from), b = by.get(l.to); if (!a || !b) continue;
      svg.append(sv('path', { class: l.on === 'ok' ? 'ok' : '', d: pathBetween(a.x + (a.w || NW), a.y + PY, b.x, b.y + PY) }));
    }
    netGhost.replaceChildren(svg, ...net.nodes.filter(t => t.key !== skip).map(t =>
      h('div', { class: 'ngc ' + netKind(t), style: `left:${t.x}px;top:${t.y}px;width:${t.w || NW}px;height:${t.h || NH}px` },
        h('b', {}, h('i', { html: ICON[t.ci ? 'ci' : isCheck(t) ? 'check' : { agent: 'ready', step: 'play', check: t.agent, git: 'git', note: 'note' }[netKind(t)]] }), netLabel(t, roles)),
        h('span', {}, netDoes(t)))));
  }
  netGhost.style.transform = `translate(${at.x}px,${at.y}px)`;
  netGhost.hidden = false;
  $('#app').classList.add('netting');
}
function hideGhost() { netGhost.hidden = true; ghostKey = ''; $('#app').classList.remove('netting'); }

/* ------------------------------------------------------------ where it goes */
function netBox(net, at) {
  return {
    x0: at.x + Math.min(...net.nodes.map(t => t.x)), y0: at.y + Math.min(...net.nodes.map(t => t.y)),
    x1: at.x + Math.max(...net.nodes.map(t => t.x + (t.w || NW))), y1: at.y + Math.max(...net.nodes.map(t => t.y + (t.h || NH))),
  };
}
/* the network's origin when its first card sits where the pointer is (as a single card would) */
function pointerAt(net, w) {
  const e = net.nodes.find(t => t.key === netEntries(net)[0]) || net.nodes[0];
  return { x: w.x - NW / 2 - e.x, y: w.y - 30 - e.y };
}
function centerAt(net) {
  const c = toWorld(innerWidth / 2, (42 + barTop()) / 2), b = netBox(net, { x: 0, y: 0 });
  return { x: c.x - (b.x0 + b.x1) / 2, y: c.y - (b.y0 + b.y1) / 2 };
}
/* the same place, or the nearest one below or above where no card of it covers another */
function netSpot(net, at, skip) {
  const boxes = [...S.nodes.values()].filter(n => n.id !== skip).map(cardRoom);
  const cards = net.nodes.filter(t => t.key !== skip);
  const clash = dy => cards.some(t => {
    const x = at.x + t.x, y = at.y + dy + t.y, w = t.w || NW, hh = t.h || NH;
    return boxes.some(b => x - JX < b.x + b.w && x + w > b.x && y < b.y + b.h && y + hh > b.y);
  });
  const snap = dy => ({ x: Math.round(at.x / 10) * 10, y: Math.round((at.y + dy) / 10) * 10 });
  for (let i = 0; i < 40; i++) for (const s of i ? [1, -1] : [1]) if (!clash(s * i * NET_ROW / 2)) return snap(s * i * NET_ROW / 2);
  return snap(0);
}
/* dropped on an agent: that agent is the network's first card, the rest goes after it */
function attachAt(net, card, key) {
  const e = net.nodes.find(t => t.key === key);
  const at = { x: card.x - e.x, y: card.y - e.y };
  const spot = netSpot(net, at, key);
  // (the dropped-on card stays where it is: only the rest moves out of the way)
  return { x: at.x, y: spot.y };
}
/* the view shows all of it (and the box under it): pan if it fits, zoom out if it does not */
function showNet(net, at, below = 0) {
  const b = netBox(net, at), x0 = b.x0 - JX, k0 = S.v.k;
  const top = 58, bottom = Math.min(innerHeight - 110, barTop() - 10) - below, left = 24, right = innerWidth - 24;
  let v;
  if ((b.x1 - x0) * k0 <= right - left && (b.y1 - b.y0) * k0 <= bottom - top) {
    const sx0 = S.v.x + x0 * k0, sx1 = S.v.x + b.x1 * k0, sy0 = S.v.y + b.y0 * k0, sy1 = S.v.y + b.y1 * k0;
    v = { k: k0, x: S.v.x + (sx0 < left ? left - sx0 : sx1 > right ? right - sx1 : 0), y: S.v.y + (sy0 < top ? top - sy0 : sy1 > bottom ? bottom - sy1 : 0) };
  } else {
    const k = Math.max(0.15, Math.min((right - left) / (b.x1 - x0), (bottom - top) / (b.y1 - b.y0)));
    v = { k, x: (left + right) / 2 - (x0 + b.x1) / 2 * k, y: (top + bottom) / 2 - (b.y0 + b.y1) / 2 * k };
  }
  if (Math.abs(v.x - S.v.x) > 1 || Math.abs(v.y - S.v.y) > 1 || v.k !== k0) glide(new Map(), v);
}

/* ------------------------------------------------------------ the gesture */
/* drag a network: its outline follows the pointer; over an agent it shows what would come after
   that agent. A click puts it in the middle of the view (or whatever `o.click` says). */
function dragNet(e, net, o = {}) {
  if (e.button !== 0) return;
  e.preventDefault(); closeMenu(); closeFly(); closeNetBox();
  const sx = e.clientX, sy = e.clientY;
  let moved = false, onto = null;
  const tag = h('div', { class: 'gghost' }, net.short);
  const one = netEntries(net).length === 1 ? netEntries(net)[0] : null;
  const roles = netRoles(net);
  const place = ev => { tag.style.left = ev.clientX + 14 + 'px'; tag.style.top = ev.clientY + 12 + 'px'; };
  const under = ev => document.elementFromPoint(ev.clientX, ev.clientY);
  const off = t => !t || !t.closest || !!t.closest('#top, #dock, #tray, #arch, #zoom, #modal, #pad, #fly, #toasts');
  const cardAt = t => {
    const nd = t && t.closest && t.closest('.nw'), n = nd && S.nodes.get(nd.dataset.id);
    return n && one && kindOf(n) === 'agent' ? n : null;
  };
  const mark = id => {
    if (onto === id) return;
    if (onto && nodeEls.get(onto)) nodeEls.get(onto).root.classList.remove('drop');
    onto = id;
    if (onto && nodeEls.get(onto)) nodeEls.get(onto).root.classList.add('drop');
  };
  const mv = ev => {
    if (!moved && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
    if (!moved) { moved = true; document.body.append(tag); if (o.lift) o.lift(); }
    place(ev);
    const t = under(ev);
    if (off(t)) { mark(null); hideGhost(); tag.textContent = net.short; return; }
    const n = cardAt(t);
    mark(n ? n.id : null);
    if (n) {
      drawGhost(net, attachAt(net, n, one), netRoles(net, hostOf(n).agent), one);
      tag.textContent = `${net.short}: after ${n.name}`;
    } else {
      drawGhost(net, pointerAt(net, toWorld(ev.clientX, ev.clientY)), roles, null);
      tag.textContent = net.short;
    }
  };
  const up = ev => {
    removeEventListener('pointermove', mv);
    tag.remove();
    const n = onto && S.nodes.get(onto);
    mark(null);
    if (!moved) { hideGhost(); return o.click ? o.click() : dropNet(net, null); }
    if (off(under(ev))) return hideGhost();
    if (n) { hideGhost(); return attachNet(net, n); }
    dropNet(net, pointerAt(net, toWorld(ev.clientX, ev.clientY)));
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
/* on the canvas: the outline stays where it was let go and the box under it asks what it is for */
function dropNet(net, at) {
  if (!agentList().length) return toast('err', 'no agent is configured to run it');
  const spot = netSpot(net, at || centerAt(net), null);
  openNetBox(net, spot);
}
/* on an agent: it carries on from that agent's work at once; its task is that agent's prompt */
async function attachNet(net, card) {
  const key = netEntries(net)[0], host = hostOf(card);
  const task = (host.prompt || card.prompt || '').trim();
  const ids = await buildNet(net, { at: attachAt(net, card, key), onto: card.id, entry: key, task, check: checkFor(workDir(host)), branch: branchFor(task),
    roles: netRoles(net, host.agent), start: true });
  const r = S.rt.get(host.id) || {}, done = r.status === 'done' || r.status === 'failed';
  if (ids) toast('ok', `${net.short} after ${card.name}: ${done ? 'it goes on now' : `it goes on when ${card.name} is done`}.`,
    { ms: 7000, action: { label: 'undo', run: undo } });
}

/* ------------------------------------------------------------ the box: what is it for? */
const netbox = $('#netbox');
const NBX = { net: null, at: null, roles: null, check: '', branch: '', branched: false };
function closeNetBox() { if (!NBX.net) return; NBX.net = null; netbox.hidden = true; netbox.replaceChildren(); hideGhost(); }
function openNetBox(net, at) {
  closeNext(); closeMenu();
  NBX.net = net; NBX.at = at; NBX.roles = netRoles(net);
  NBX.check = checkFor(newDir()); NBX.branch = ''; NBX.branched = false;
  renderNetBox('');
  showNet(net, at, netbox.offsetHeight + 24);
}
function roleLabel(net, r) {
  const ts = net.nodes.filter(t => t.agent === r);
  const named = ts.map(t => t.role || (t.name || '').split('{{agent}}').join('').replace(/[·\s]+$/, '').trim()).find(Boolean);
  return named || (r === '@main' ? (ts.length > 1 ? 'agents' : 'agent') : r === '@alt' ? 'second agent' : 'third agent');
}
function renderNetBox(text) {
  const net = NBX.net; if (!net) return;
  const used = NET_ROLES.filter(r => net.nodes.some(t => t.agent === r)), ks = agentList();
  const ta = h('textarea', { rows: 3, spellcheck: 'false', placeholder: net.ask || 'what is it for?', 'aria-label': 'What is it for?' });
  ta.value = text;
  const pick = (r, k) => {
    NBX.roles[r] = k;
    if (r !== '@main') { const m = S.prefs.netRole = S.prefs.netRole || {}; (m[net.id] = m[net.id] || {})[r] = k; savePrefs(); }
    renderNetBox(ta.value); drawGhost(net, NBX.at, NBX.roles, null);
  };
  const roleRow = r => h('div', { class: 'nbr' }, h('span', { class: 'dl' }, roleLabel(net, r)),
    h('div', { class: 'seg' }, ks.map(([k, c]) => h('button', { 'aria-pressed': String(NBX.roles[r] === k), title: c.label || k, onclick: () => pick(r, k) }, k))));
  // the command the check gates run (remembered for the folder) and the branch a pull request comes from
  const keys = e => {
    e.stopPropagation();
    if (e.key === 'Enter') { e.preventDefault(); submitNetBox(true); }
    else if (e.key === 'Escape') { e.preventDefault(); closeNetBox(); }
  };
  const field = (label, tip, value, ph, set) => {
    const f = h('input', { class: 'fld', value, placeholder: ph, spellcheck: 'false', 'aria-label': label, oninput: e => set(e.target.value), onkeydown: keys });
    return h('div', { class: 'nbr', title: tip }, h('span', { class: 'dl' }, label), f);
  };
  const D = newDir();
  const check = usesCheck(net) ? field('must pass', `the command its check gates run in ${D} (remembered for that folder); empty: the agent runs the checks`,
    NBX.check, 'empty: the agent runs the checks', v => { NBX.check = v; }) : null;
  const branch = usesBranch(net) ? field('branch', 'the branch it commits on and the pull request comes from (named after what you type)',
    NBX.branch, 'named after what you type', v => { NBX.branch = v; NBX.branched = true; }) : null;
  netbox.replaceChildren(
    h('div', { class: 'nh' }, netGlyph(net, 34, 16), h('span', { class: 'nt' }, net.title),
      h('button', { class: 'ib', 'aria-label': 'Cancel', title: 'cancel (esc)', html: ICON.x, onclick: closeNetBox })),
    h('p', { class: 'nx' }, net.about),
    ...used.map(roleRow),
    ...nn([check, branch]),
    ta,
    h('div', { class: 'nf' }, ...nn([h('span', { class: 'sp' }), h('span', { class: 'kh' }, h('kbd', {}, 'enter'), ' start'),
      h('button', { class: 'btn sm dim', title: 'put the cards down without starting anything', onclick: () => submitNetBox(false) }, 'place only'),
      h('button', { class: 'btn sm pri', onclick: () => submitNetBox(true) }, 'start')])));
  ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.isComposing) return;
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submitNetBox(true); }
    else if (e.key === 'Escape') { e.preventDefault(); closeNetBox(); }
    else if (e.key === 'Tab' && used.includes('@main') && ks.length > 1) {
      e.preventDefault();
      const i = ks.findIndex(([k]) => k === NBX.roles['@main']);
      pick('@main', ks[(i + (e.shiftKey ? ks.length - 1 : 1)) % ks.length][0]);
      netbox.querySelector('textarea').focus();
    }
  });
  ta.addEventListener('input', () => {
    ta.style.height = 'auto'; ta.style.height = Math.min(220, Math.max(70, ta.scrollHeight)) + 'px';
    if (branch && !NBX.branched) { NBX.branch = branchFor(ta.value); branch.querySelector('input').value = NBX.branch; }
  });
  netbox.hidden = false;
  drawGhost(net, NBX.at, NBX.roles, null);
  placeNetBox();
  ta.focus();
  ta.setSelectionRange(ta.value.length, ta.value.length);
}
/* under the outline, left-aligned with it */
function placeNetBox() {
  if (!NBX.net || netbox.hidden) return;
  const b = netBox(NBX.net, NBX.at), k = S.v.k, w = netbox.offsetWidth, hh = netbox.offsetHeight;
  const x = S.v.x + b.x0 * k, y = S.v.y + b.y1 * k + 14;
  netbox.style.left = Math.max(12, Math.min(x, innerWidth - w - 12)) + 'px';
  netbox.style.top = Math.max(50, Math.min(y, barTop() - hh - 10)) + 'px';
}
async function submitNetBox(start) {
  const net = NBX.net; if (!net) return;
  const ta = netbox.querySelector('textarea'), task = ta.value.trim();
  if (start && !task && net.nodes.some(t => (t.prompt || '').includes('{{task}}'))) return shake(ta);
  const at = NBX.at, roles = { ...NBX.roles }, check = NBX.check.trim(), branch = NBX.branch.trim() || branchFor(task);
  if (usesCheck(net)) rememberFolderCmd('check', newDir(), check);
  closeNetBox();
  if (roles['@main'] && net.nodes.some(t => t.agent === '@main')) setAgent(roles['@main']);
  const ids = await buildNet(net, { at, task, check, branch, roles, start });
  if (!ids) return;
  toast('ok', start ? `${net.title}: started.` : `${net.title}: placed. Nothing runs until you start its first agent.`, { ms: 7000, action: { label: 'undo', run: undo } });
}

/* ------------------------------------------------------------ making it */
/* every card, then the links in their order, then the checks; the first agents start last. Each
   batch goes out at once: one socket, one actor, so the daemon takes them in the order sent.
   o: at (origin), task, roles, start, onto/entry (an agent that is the first card) */
let netBuilding = 0;
async function buildNet(net, o) {
  const ids = new Map(), made = [];
  const entries = netEntries(net);
  const hasTask = net.nodes.some(t => (t.prompt || '').includes('{{task}}'));
  const onto = o.onto ? S.nodes.get(o.onto) : null;
  const dir = onto ? (hostOf(onto).cwd || '') : cwdField(newDir());
  const undoMade = () => made.forEach(id => send('node_del', { id }));
  if (onto) ids.set(o.entry, onto.id);
  // the queued prompts last: the agent they are asked of has to be there first
  for (const pass of [0, 1]) {
    const todo = net.nodes.filter(t => (t.agent === 'step') === (pass === 1) && !ids.has(t.key));
    const res = await Promise.all(todo.map(t => req('node_add', { node: netNode(t, o, entries.includes(t.key), hasTask, dir, ids) })));
    res.forEach((r, i) => { if (r.ok) { ids.set(todo[i].key, r.id); made.push(r.id); netTask.set(r.id, { task: o.task || '', check: o.check || '', branch: o.branch || '' }); } });
    const bad = res.find(r => !r.ok);
    if (bad) { toast('err', bad.error || 'could not place the network'); undoMade(); return null; }
  }
  // (a queued prompt follows each link drawn into it: the daemon says so every time, which is news
  // only when a person rewires one)
  netBuilding++;
  const links = await Promise.all(net.links.filter(l => ids.get(l.from) && ids.get(l.to))
    .map(l => req('edge_add', { from: ids.get(l.from), to: ids.get(l.to), on: l.on || 'done', each: !!l.each })));
  netBuilding--;
  const bad = links.find(r => !r.ok);
  if (bad) toast('warn', bad.error || 'a link of the network could not be drawn');
  // the agent dropped on keeps what watches it
  await Promise.all(net.nodes.filter(t => !(onto && t.key === o.entry)).flatMap(t => (t.checks || []).map(c =>
    req('check_add', { node: ids.get(t.key), kind: c.kind, subject: c.subject || '', cmd: c.cmd || '', every_s: c.every_s || 0 }))));
  if (o.start) for (const k of entries) if (!(onto && k === o.entry)) send('arm', { id: ids.get(k) });
  const u = S.prefs.netUse = S.prefs.netUse || {};
  u[net.id] = (u[net.id] || 0) + 1; savePrefs(); renderNetChips();
  S.undo.push(undoMade);
  for (const id of made) await whenNode(id);
  select(made.length ? made : [o.onto]);
  if (!o.start && !onto) { const first = ids.get(entries[0]); if (first && !fillTask(S.nodes.get(first)?.prompt || '', '').trim()) editAsk(first); }
  return made;
}
/* one card as node_add takes it */
function netNode(t, o, entry, hasTask, dir, ids) {
  let agent = roleAgent(t.agent, o.roles);
  const k = netKind({ agent });
  // a saved network can name an agent this machine does not have: the main one plays it
  if (k === 'agent' && agent !== 'step' && agent !== 'sh' && !S.cfg.agents[agent]) agent = o.roles['@main'];
  const node = { agent, x: Math.round(o.at.x + t.x), y: Math.round(o.at.y + t.y) };
  for (const f of NET_FIELDS) if (t[f] !== undefined) node[f] = t[f];
  if (node.name) node.name = netName(t, agent);
  if (node.prompt != null) node.prompt = fillTask(node.prompt, o.task).split('{{check}}').join(o.check ? '`' + o.check + '`' : 'the build and tests');
  if (typeof node.cmd === 'string') node.cmd = node.cmd.split('{{check}}').join(o.check || '');
  if (typeof node.branch === 'string') node.branch = node.branch.split('{{branch}}').join(o.branch || '');
  if (entry && !hasTask && o.task) node.prompt = ((node.prompt || '') + '\n\n' + o.task).trim();
  if (k === 'agent' && agent !== 'step' && agent !== 'sh') {
    const ac = S.cfg.agents[agent] || {};
    // its own model and reasoning only where the role went to the agent it was saved with
    const own = !isRole(t.agent) || t.was === agent;
    node.model = own && t.model != null ? t.model : (ac.model_arg ? tuned(agent, 'model') : '');
    node.effort = own && t.effort != null ? t.effort : (ac.effort_arg ? tuned(agent, 'effort') : '');
    if (t.addenda == null) node.addenda = addendaIds();
    if (ac.yolo_arg && t.yolo == null) node.yolo = yoloDefault();
  }
  if (k === 'agent' && agent !== 'step' && dir) node.cwd = dir;
  // the first agents wait for the start; the rest run when what they wait for arrives
  if (k === 'agent') node.draft = !!entry;
  if (agent === 'step') {
    const host = S.nodes.get(ids.get(t.host));
    node.host = host ? hostOf(host).id : '';
    // no addenda of its own: the daemon gives it its agent's (this page may not have that agent yet)
  }
  return node;
}

/* ------------------------------------------------------------ saving one */
/* the agent a card speaks for, the way the daemon's `agent_behind` finds it */
function agentBehind(id, seen = new Set()) {
  const n = S.nodes.get(id); if (!n || seen.has(id)) return null;
  seen.add(id);
  if (n.agent === 'step') return n.host ? agentBehind(n.host, seen) : null;
  if (isGate(n.agent)) { const e = incoming(id)[0]; return e ? agentBehind(e.from, seen) : null; }
  return kindOf(n) === 'agent' ? id : null;
}
/* the selected cards as a network: the task they were given (or else the first agents' prompts)
   becomes {{task}}, the first agent's agent becomes @main (so it follows the agent you work with),
   the links keep their order */
function captureNet(ids) {
  // (what a "for each" link copied is made again when the network runs)
  const ns = ids.map(id => S.nodes.get(id)).filter(n => n && !n.each_of);
  if (!ns.some(n => kindOf(n) === 'agent')) return null;
  const sel = new Set(ns.map(n => n.id)), key = new Map(ns.map((n, i) => [n.id, 'k' + (i + 1)]));
  const x0 = Math.min(...ns.map(n => n.x)), y0 = Math.min(...ns.map(n => n.y));
  const edges = [...S.edges.values()].filter(e => sel.has(e.from) && sel.has(e.to));
  const last = e => { const t = S.nodes.get(e.to); return t && t.agent === 'step' && agentBehind(e.from) === t.host ? 1 : 0; };
  edges.sort((a, b) => last(a) - last(b));
  const into = new Set(edges.filter(e => kindOf(S.nodes.get(e.from)) !== 'note').map(e => e.to));
  const entries = ns.filter(n => kindOf(n) === 'agent' && n.agent !== 'step' && !into.has(n.id));
  const first = [...entries].sort((a, b) => a.x - b.x || a.y - b.y)[0];
  const main = first && first.agent !== 'sh' && S.cfg.agents[first.agent] ? first.agent : null;
  const nodes = ns.map(n => {
    const t = { key: key.get(n.id), agent: n.agent === main ? '@main' : n.agent, x: Math.round(n.x - x0), y: Math.round(n.y - y0) };
    if (n.agent === main) t.was = main;
    for (const f of NET_FIELDS) { const v = n[f]; if (v !== undefined && v !== null && v !== '' && !(Array.isArray(v) && !v.length)) t[f] = v; }
    if (n.auto_name) delete t.name;
    if (n.agent === main && t.name) t.name = t.name.split(main).join('{{agent}}');
    const was = netTask.get(n.id) || {};
    if (was.task && t.prompt) t.prompt = t.prompt.split(was.task).join('{{task}}');
    if (was.check && t.prompt) t.prompt = t.prompt.split('`' + was.check + '`').join('{{check}}');
    if (was.check && t.cmd === was.check) t.cmd = '{{check}}';
    if (was.branch && t.branch === was.branch) t.branch = '{{branch}}';
    if (entries.includes(n) && n.agent !== 'sh' && !(t.prompt || '').includes('{{task}}')) t.prompt = '{{task}}';
    if (n.agent === 'step' && sel.has(n.host)) t.host = key.get(n.host);
    if ((n.checks || []).length) t.checks = n.checks.map(c => ({ kind: c.kind, subject: c.subject || '', cmd: c.cmd || '', every_s: c.every_s || 0 }));
    return t;
  });
  return { id: 'my-' + Math.random().toString(36).slice(2, 8), title: '', about: '', nodes,
    links: edges.map(e => ({ from: key.get(e.from), to: key.get(e.to), on: e.on || 'done', ...(e.each ? { each: true } : {}) })) };
}
function saveNetFrom(ids, x, y) {
  if (!Array.isArray(S.nets)) return toast('warn', 'Saving networks needs a newer weft: build and install it, then run weft once.', { ms: 8000 });
  const net = captureNet(ids);
  if (!net) return toast('info', 'Select the cards of the network first (at least one agent).', { ms: 5000 });
  askText(x, y, `name the network (${ids.length} card${ids.length > 1 ? 's' : ''})`, '', title => {
    if (!title) return;
    net.title = title;
    net.about = `Saved from the canvas. The first agent${net.nodes.filter(t => t.prompt === '{{task}}').length > 1 ? 's get' : ' gets'} what you type when you drop it.`;
    S.nets = [...S.nets, net];
    send('net_set', { networks: S.nets });
    renderNetChips();
    if (!modal.hidden) renderLibrary();
    toast('ok', `saved "${title}": it is in the library (and in the top bar once you use it most).`, { ms: 7000, action: { label: 'show', run: () => { netSel = net.id; openLibrary('nets'); } } });
  });
}
function setNets(list) {
  S.nets = list;
  send('net_set', { networks: list });
  renderNetChips();
}
