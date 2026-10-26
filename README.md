# weft

A canvas for coding agents. Every card is an agent (Claude Code, Codex, Antigravity, or any
terminal program) in a tmux window. You link cards into flows: a result is handed to the next
card, steps run in parallel and join, checks act as gates, and git commits, pushes and opens PRs.
It is one ~7 MB Rust binary with the UI inside. There is no database, no Node and no cloud: it
runs on 127.0.0.1 only.

```sh
curl -fsSL https://raw.githubusercontent.com/a1xon/weft/master/install.sh | sh
cd your-project && weft          # starts this folder's daemon and opens the canvas
```

## Requirements

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Status | main platform, fully tested | core + app window + extension; less tested | WSL2 only (as Linux) |
| Needs | tmux ≥ 3.2, git | `brew install tmux` (+ git) | inside WSL2 |
| Optional | `gh` (CI check, PRs), a Chromium-family browser (app window, extension), zenity (folder picker) | the same; the folder picker uses osascript | |

You also need at least one agent CLI on your PATH: `claude`, `codex`, `agy`, or anything you add
in the config. The canvas works in any browser.

## Install

| How | Command |
| --- | --- |
| Prebuilt binary (latest release) | `curl -fsSL https://raw.githubusercontent.com/a1xon/weft/master/install.sh \| sh` |
| While the repo is private | `gh repo clone a1xon/weft && weft/install.sh` (uses `gh` for the download or the clone) |
| From a checkout | `./install.sh` builds that checkout (needs `cargo`, from rustup.rs) |
| By hand | `cargo build --release && install -m 0755 target/release/weft ~/.local/bin/weft` |

The binary goes to `~/.local/bin/weft` (`WEFT_BIN=<dir>` changes that; `WEFT_FROM_SOURCE=1`
always builds). To update, run it again: running daemons notice the new binary and replace
themselves, and the agents keep running. The script checks tmux, git, PATH and the agent CLIs.

## Use

| Key / gesture | Does |
| --- | --- |
| type + `Ctrl+Enter` | a new agent with that prompt (`Enter` is a new line) |
| type in the bottom bar | a message to the selected agent (queued if it is busy) |
| drag a card's ◇ onto another card | link them: the result is handed on (several links into one card = wait for all) |
| drag a ◇ into empty space | the dial: queue a prompt, hand off to a fresh agent, or a git step (commit → push → PR) |
| click a link's chip | *always* / *only if ok* / *only if it fails* |
| `R` / click an answer | read the full result; questions in it get quick answers |
| `T` | tidy the layout · `N` launch pad · `Esc` canvas · right-click anything for its menu |

An agent reports back with `weft done|issue|fail <<'EOF' … EOF`. weft adds this instruction to
every prompt. A card that goes quiet is never taken as done.

> **yolo is on by default**: agents run with `--dangerously-skip-permissions` or the equivalent.
> One click on the launch pad (or `"yolo": false` in the config) makes them ask first.

## CLI

```
weft [dir]            open the canvas          weft ls                  nodes and status
weft new [dir]        a board of its own       weft send <node> <text>  prompt an agent
weft app [dir]        in its own window        weft result|read <node>  its result / terminal
weft desktop          dock entry (Linux)       weft run | retry <node>  run the flow / a card again
weft extension        browser marks → cards    weft stop [--keep]       quit (--keep: agents stay)
```

The agents live in `tmux -L weft attach`.

## Files

| Path | What |
| --- | --- |
| `<project>/.weft/` | the flow (`flow.json`: plain JSON, fine to commit), run state, archive, screenshots |
| `~/.config/weft/config.json` | your overrides only: agents, models, flags, prompts, networks |
| `~/.local/state/weft/` | per board: port, token, log, live status |

Security: the daemon listens on 127.0.0.1 only. Each board has a random token, and requests need
a loopback `Host` (and `Origin` for WebSockets).

## Develop

```sh
cargo test                   # unit tests
python3 tests/e2e.py         # end to end against real tmux with a simulated agent
tests/ui/run.sh              # browser simulation (Playwright), see tests/ui/README.md
git tag v0.2.0 && git push --tags   # CI builds Linux/macOS x86_64+aarch64 binaries into a release
```

Run weft in its own source folder and the UI is served from `web/` on disk, with live reload.
**[docs/manual.md](docs/manual.md)** covers everything in detail: checks, talks between agents,
networks, boards, rewind, the archive, usage limits, config keys and how it works inside.

## License

[MIT](LICENSE). Bundled: PDF.js (Apache-2.0), markdown-it (MIT), Chakra Petch and JetBrains Mono
([OFL 1.1](web/OFL.txt)).
