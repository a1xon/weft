#!/usr/bin/env python3
"""A simulated terminal coding agent for weft demos and tests.

It looks and behaves like the real TUIs weft talks to: bracketed paste, an inline
transcript with a redrawn input box, a thinking spinner, "[Pasted text]" placeholders,
an optional trust dialog, clarifying questions, and the `weft done` completion
protocol. It is deterministic, instant and free, and it really edits files in the
textkit example so downstream test steps genuinely pass or fail.

    sim-agent.py [--name NAME] [--think SECONDS] [--trust] [--chatty]
                 [--model M | -m M] [--effort LEVEL | -c model_reasoning_effort=LEVEL]
                 [--dangerously-skip-permissions | --dangerously-bypass-approvals-and-sandbox]

Prompt directives it understands (case-insensitive):
    ASK_QUESTION   raise an ISSUE (`weft issue`) with a question and wait for the user
    DEFER          end the turn with "I will get back to you": STATUS: WAITING, and the job
                   is done when weft wakes it to check
    NO_STATUS      do not end with the status line this turn, as an agent that ignores the
                   rule would; weft then asks for the line and it answers

    FAIL_TASK      do the work badly: report failure
    SLOW           think three times longer

A prompt from weft asks it to end with "STATUS: <WORD>" (DONE, ISSUE, WAITING, FAILED); it does.

It also answers weft's own requests like a competent agent would: commit what it
changed (on the branch it is told to use), save a screenshot at the given path, write
tests, or run the suite and report pass/fail.
"""
import argparse, os, re, select, struct, subprocess, sys, termios, textwrap, time, tty, zlib

ap = argparse.ArgumentParser()
ap.add_argument("--name", default="sim")
ap.add_argument("--think", type=float, default=2.5)
ap.add_argument("--trust", action="store_true")
ap.add_argument("--chatty", action="store_true")
# a TUI that is still starting up or reads the Enter as a newline, like the real ones under load
ap.add_argument("--eat-enter", type=int, default=0, help="read the first N Enters as a newline in the input box")
ap.add_argument("--drop-paste", type=int, default=0, help="lose the first N pastes (the program was not reading yet)")
ap.add_argument("--model", "-m", default="")
ap.add_argument("--effort", default="")
ap.add_argument("-c", action="append", default=[])
# the "do not ask" switches of the real CLIs
ap.add_argument("--dangerously-skip-permissions", action="store_true")
ap.add_argument("--dangerously-bypass-approvals-and-sandbox", action="store_true")
args = ap.parse_args()
YOLO = args.dangerously_skip_permissions or args.dangerously_bypass_approvals_and_sandbox
for kv in args.c:
    if kv.startswith("model_reasoning_effort="):
        args.effort = kv.split("=", 1)[1].strip('"')

SLUG_FIXED = '''import re
import unicodedata


def slugify(text, max_length=None):
    """Turn a title into a URL slug: lowercase ascii words joined by single dashes."""
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
    text = re.sub(r"[^a-zA-Z0-9]+", "-", text).strip("-").lower()
    if max_length:
        text = text[:max_length].rstrip("-")
    return text
'''
WRAP_FIXED = '''def wrap(text, width):
    """Greedy word wrap. No returned line may be longer than `width` characters."""
    lines, current = [], ""
    for word in text.split():
        if current and len(current) + 1 + len(word) > width:
            lines.append(current)
            current = word
        else:
            current = f"{current} {word}".strip()
    if current:
        lines.append(current)
    return lines
'''

W = lambda s: (sys.stdout.write(s), sys.stdout.flush())
cols = lambda: os.get_terminal_size().columns if sys.stdout.isatty() else 100

fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
tty.setraw(fd)
W("\x1b[?2004h")  # bracketed paste on, like the real ones


def restore():
    W("\x1b[?2004l")
    termios.tcsetattr(fd, termios.TCSADRAIN, old)


typed = ""          # the input box content
box_h = 0           # how many lines the box + status currently occupy
status = ""         # spinner line


def draw(new_lines=()):
    """Erase the old box, print transcript lines, redraw the box (inline, like real TUIs)."""
    global box_h
    out = ""
    if box_h:
        out += f"\x1b[{box_h}A\r\x1b[J"
    for l in new_lines:
        out += l + "\r\n"
    c = cols()
    bar = "\x1b[2m" + "─" * (c - 1) + "\x1b[0m"
    lines = []
    if status:
        lines.append(status)
    lines += [bar, "\x1b[1m❯\x1b[0m " + typed, bar, "\x1b[2m  ⏵⏵ bypass permissions on (shift+tab to cycle) · ? for shortcuts\x1b[0m"]
    out += "\r\n".join(lines)
    # park the cursor at the end of what is typed, like the real ones do (Claude Code, Codex and
    # Antigravity all leave it in the input box, two rows above the bottom: the rule and the hint)
    rows = len(lines) + typed.count("\r\n")
    col = 2 + len(typed.split("\r\n")[-1])
    out += "\x1b[2A\r" + (f"\x1b[{col}C" if col else "")
    box_h = rows - 3
    W(out)


def say(*lines):
    draw([textwrap.fill(l, cols() - 4) if len(l) > cols() - 4 else l for l in lines])


EDITED = []   # files this agent changed, so a commit request can stage exactly those


def png(w=240, h=140):
    """A small valid PNG (a gradient) so screenshot gates have a real image to show."""
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    rows = b"".join(b"\x00" + b"".join(bytes((x * 255 // w, y * 255 // h, 170)) for x in range(w)) for y in range(h))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def git(*a):
    return subprocess.run(["git", *a], capture_output=True, text=True)


def do_commit(prompt):
    """Stage only what this agent changed and commit it, like the request asks."""
    m = re.search(r"Commit on branch `([^`]+)`", prompt)
    if m and "own git worktree" not in prompt:
        b = m.group(1)
        if git("checkout", b).returncode != 0:
            git("checkout", "-b", b)
    if not EDITED:
        return True, "nothing to commit: I changed no files"
    git("add", "--", *EDITED)
    if git("diff", "--cached", "--quiet").returncode == 0:
        return True, "nothing to commit"
    subject = "Fix " + ", ".join(sorted(set(os.path.basename(f) for f in EDITED)))
    r = git("commit", "-q", "-m", subject, "-m", "Reason: the tests in tests/ failed before this change.")
    if r.returncode != 0:
        return False, "git commit failed: " + r.stderr.strip()
    h = git("rev-parse", "--short", "HEAD").stdout.strip()
    return True, f"{h} {subject}"


def show_shot(prompt):
    """A watch asked for it: make a picture and hand it over with `weft shot`, like a real agent would."""
    import tempfile
    path = os.path.join(tempfile.gettempdir(), "sim-shot-%d.png" % int(time.time() * 1000))
    open(path, "wb").write(png())
    exe = os.environ.get("WEFT_BIN", "weft")
    note = "sim: front page" if "login" not in prompt.lower() else "sim: login page"
    subprocess.run([exe, "shot", path, "--note", note], capture_output=True, text=True)


def do_pr(prompt):
    """What an agent with `gh` reports once the pull request is open: its link, then its title."""
    m = re.search(r"just pushed \(`([^`]+)`\)", prompt)
    branch = m.group(1) if m else "the branch"
    return True, "https://example.test/acme/textkit/pull/7\nsim: " + branch


def do_rebase(prompt):
    """The remote moved on: pull it in, the way the request spells out."""
    m = re.search(r"project folder `([^`]+)` run `(git pull --rebase[^`]*)`", prompt)
    if not m:
        return False, "no folder given"
    r = subprocess.run(["git", "-C", m.group(1), *m.group(2).split()[1:]], capture_output=True, text=True)
    if r.returncode != 0:
        return False, "rebase failed: " + (r.stderr or r.stdout).strip()[-160:]
    return True, "rebased onto the remote; nothing conflicted"


def do_handover():
    """Write the note a fresh agent would need: goal, state, what is open."""
    files = ", ".join(sorted(set(EDITED))) or "nothing yet"
    return True, "goal: make the textkit tests pass. state: changed " + files + ". open: run the whole suite once more."


def do_screenshot(prompt):
    # the path is quoted inline (a bare path on its own line is turned into an image by Claude Code)
    m = re.search(r"`(/[^`\n]+\.png)`", prompt) or re.search(r"^(/\S+\.png)\s*$", prompt, re.M)
    if not m:
        return False, "no path given"
    os.makedirs(os.path.dirname(m.group(1)), exist_ok=True)
    open(m.group(1), "wb").write(png())
    return True, "a gradient test card, 240x140"


def do_write_tests():
    os.makedirs("tests", exist_ok=True)
    p = "tests/test_extra.py"
    open(p, "w").write("import unittest\n\nfrom textkit import slugify\n\n\nclass Extra(unittest.TestCase):\n    def test_empty(self):\n        self.assertEqual(slugify(''), '')\n\n\nif __name__ == '__main__':\n    unittest.main()\n")
    EDITED.append(p)
    return True, "added tests/test_extra.py"


def do_run_tests():
    r = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"], capture_output=True, text=True)
    out = (r.stdout + r.stderr).strip()
    return r.returncode == 0, ("all tests pass" if r.returncode == 0 else out[-1200:])


def apply_work(prompt):
    """Really do the edits the prompt asks for, in the current directory."""
    low = prompt.lower()
    done = []
    if re.search(r"slug", low) and re.search(r"fix|implement|repair|make", low):
        p = "textkit/slug.py"
        if os.path.exists(p):
            open(p, "w").write(SLUG_FIXED)
            done.append(p)
            EDITED.append(p)
    if re.search(r"wrap", low) and re.search(r"fix|implement|repair|make", low):
        p = "textkit/wrap.py"
        if os.path.exists(p):
            open(p, "w").write(WRAP_FIXED)
            done.append(p)
            EDITED.append(p)
    return done


# how the last turn went, for the status line: ("done"|"issue"|"fail"|"waiting", detail)
outcome = ("done", "")


def work(prompt):
    """Think, 'use tools', answer, and honour the weft completion protocol."""
    global status, outcome
    low = prompt.lower()
    # a real TUI folds pasted text away; keep what arrived so a test can look at it
    proj, node = os.environ.get("WEFT_PROJECT"), os.environ.get("WEFT_NODE")
    if proj and node:
        try:
            d = os.path.join(proj, ".weft", "run")
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, "received-%s.log" % node), "a") as f:
                f.write("=== prompt ===\n" + prompt + "\n")
        except OSError:
            pass
    # linked with agents to coordinate with: tell them once what it works on, as a real agent would
    # (a message from one of them only says how to answer: that line is not at the start of a line)
    mt = re.search(r'^\s*(\S+) tell (\S+) "<message>"', prompt, re.M)
    if mt:
        say("\x1b[2m●\x1b[0m Bash(weft tell " + mt.group(2) + ")")
        subprocess.run([mt.group(1), "tell", mt.group(2), "%s here: I am working on textkit/slug.py" % args.name],
                       capture_output=True, text=True)
    # the status line weft asks a prompt to end with (the literal "status: <word>" is the rule)
    rule = "status: <word>" in low or "end with the status line" in low
    if "had no status line" in low:
        # weft asks for the line it did not get: answer the way the turn went
        time.sleep(0.4)
        kind, detail = outcome
        line = {"issue": "STATUS: ISSUE " + detail, "fail": "STATUS: FAILED " + detail,
                "waiting": "STATUS: WAITING " + detail}.get(kind, "STATUS: DONE")
        say("", "● " + line, "")
        return
    if "you said you were waiting for" in low:
        # weft woke it to check: the deferred job is done by now
        time.sleep(0.6)
        outcome = ("done", "")
        say("", "● The build finished: everything is green.", "", "● STATUS: DONE", "")
        return
    outcome = ("done", "")
    think = args.think * (3 if "slow" in low else 1)
    ml = re.search(r"LONG=(\d+)", prompt)
    if ml:
        think = float(ml.group(1))      # LONG=<seconds>: work for that long
    spin = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
    t0 = time.time()
    i = 0
    shown_tool = False
    # an estimate was asked for ("weft eta <seconds>"): give one, then update it halfway
    wants_eta = "eta <seconds> --note" in prompt
    eta_sent = [False, False]
    exe_ = os.environ.get("WEFT_BIN", "weft")
    while time.time() - t0 < think:
        if wants_eta and not eta_sent[0] and time.time() - t0 > 0.4:
            eta_sent[0] = True
            subprocess.run([exe_, "eta", str(int(think)), "--note", "sim plan: read, fix, test"], capture_output=True, text=True)
        if wants_eta and not eta_sent[1] and time.time() - t0 > think / 2:
            eta_sent[1] = True
            subprocess.run([exe_, "eta", str(max(1, int(think / 2) + 2)), "--note", "halfway: fixing"], capture_output=True, text=True)
        status = f"\x1b[33m{spin[i % len(spin)]}\x1b[0m Thinking… ({int(time.time() - t0)}s · esc to interrupt)"
        draw()
        if not shown_tool and time.time() - t0 > think / 2:
            shown_tool = True
            say("\x1b[2m●\x1b[0m Read(" + ("textkit/slug.py" if "slug" in low else "textkit/wrap.py" if "wrap" in low else ".") + ")")
        if select.select([fd], [], [], 0.12)[0]:
            handle_keys(os.read(fd, 4096), busy=True)
        i += 1
    status = ""
    action = None
    # NO_STATUS: an agent that does not follow the rule this turn (weft then asks for the line)
    label = rule and "no_status" not in low
    if "defer" in low:
        outcome = ("waiting", "for the full build")
        say("", "● I started the full build. I will get back to you when it is done.",
            *(["  STATUS: WAITING for the full build"] if label else []), "", "\x1b[2m✻ Cooked for %ds\x1b[0m" % int(time.time() - t0))
        return
    if "shot <path-to-the-png>" in prompt:
        show_shot(prompt)
    if "commit the work you just did" in low:
        action = do_commit(prompt)
    elif "take over this work from you" in low:
        action = do_handover()
    elif "open a pull request for the work" in low:
        action = do_pr(prompt)
    elif "was rejected: the remote has commits" in low:
        action = do_rebase(prompt)
    elif ("take a screenshot" in low or "take a new screenshot" in low) and "shot <path-to-the-png>" not in prompt:
        action = do_screenshot(prompt)
    elif "write automated tests" in low:
        action = do_write_tests()
    elif "run the project's test suite now" in low:
        action = do_run_tests()
    edited = apply_work(prompt) if ("fail_task" not in low and action is None) else []
    if "ask_question" in low:
        outcome = ("issue", "Should the slug keep unicode letters such as ü, or transliterate them to plain ascii (u)?")
        say("", "● Before I change anything: should the slug keep unicode letters such as ü,",
            "  or transliterate them to plain ascii (u)?",
            *(["  STATUS: ISSUE should the slug keep unicode letters, or transliterate them to ascii?"] if label else []),
            "", "\x1b[2m✻ Cooked for %ds\x1b[0m" % int(time.time() - t0))
        mi = re.search(r"^(\S+) done <<'EOF'", prompt, re.M)
        if mi:
            # taught the protocol: it raises an ISSUE instead of just stopping
            say("\x1b[2m●\x1b[0m Bash(" + os.path.basename(mi.group(1)) + " issue)")
            subprocess.run([mi.group(1), "issue", "Should the slug keep unicode letters such as ü, or transliterate them to plain ascii (u)?"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return
    if action is not None:
        ok, said = action
        body = ["● " + said.replace("\n", " ")]
    elif "fail_task" in low:
        body = ["● I could not complete this: the requested change conflicts with the tests."]
        ok = False
        outcome = ("fail", "the requested change conflicts with the tests")
    else:
        ok = True
        body = ["● " + ("Updated " + ", ".join(edited) + "." if edited else
                       "Here is my analysis: the failing tests point at normalisation of the input and"
                       " an off-by-one in the line-length comparison.")]
        if args.chatty:
            body += ["  - read the module", "  - compared with the tests", "  - kept the public API unchanged"]
    # BACKGROUND=<seconds>: like an agent that started a shell and will answer when it ends:
    # the turn is over, the screen is still, and the status line says what it is waiting for
    mb = re.search(r"BACKGROUND=(\d+)", prompt)
    last = "  STATUS: DONE" if ok else "  STATUS: FAILED " + (outcome[1] or "the step did not succeed")
    if label and not mb:
        body.append(last)
    say("", *body, "", "\x1b[2m✻ Cooked for %ds\x1b[0m" % int(time.time() - t0))
    if mb:
        say("", "● I started it in the background and will answer again when it finishes.")
        # the status line above the input box names what it is waiting for, like the real one
        status = "\x1b[2m✻ Brewed for %ds · done 7:07 · 1 shell still running\x1b[0m" % int(time.time() - t0)
        draw()
        tb = time.time()
        while time.time() - tb < int(mb.group(1)):
            if select.select([fd], [], [], 0.2)[0]:
                handle_keys(os.read(fd, 4096), busy=True)
        status = ""
        # the status line ends its final message, the one after the shell
        say("", "● Background command \"sleep\" completed (exit code 0)", "● It finished.", *([last] if label else []),
            "\x1b[2m✻ Worked for 1s · done 7:08\x1b[0m")
    m = re.search(r"^(\S+) done <<'EOF'", prompt, re.M)
    if m:
        summary = ("Fixed: " + ", ".join(edited)) if edited else "Analysis: normalisation and an off-by-one in the width check."
        if action is not None:
            summary = said
        elif "have not reported to weft" in low:
            summary = "Build finished: everything is green."
        elif not ok:
            summary = "Could not complete: conflicts with the tests."
        say("\x1b[2m●\x1b[0m Bash(" + os.path.basename(m.group(1)) + (" done" if ok else " fail") + ")")
        cmd = [m.group(1), "done" if ok else "fail"]
        subprocess.run(cmd, input=summary.encode(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


buf = b""
in_paste = False
paste = b""
queued_prompts = []


def handle_keys(data, busy=False):
    """Feed raw bytes: bracketed paste, printable chars, Enter, Backspace, ^C."""
    global typed, buf, in_paste, paste
    data = buf + data
    buf = b""
    i = 0
    while i < len(data):
        if data[i:i + 6] == b"\x1b[200~":
            in_paste, paste, i = True, b"", i + 6
            continue
        if data[i:i + 6] == b"\x1b[201~":
            in_paste = False
            txt = paste.decode("utf8", "replace")
            if lost_pastes[0] > 0:
                lost_pastes[0] -= 1
                draw()
                i += 6
                continue
            if txt.count("\n") >= 1:
                typed += f"[Pasted text +{txt.count(chr(10)) + 1} lines]"
                pasted.append(txt)
            else:
                typed += txt
            draw()
            i += 6
            continue
        if in_paste:
            paste += data[i:i + 1]
            i += 1
            continue
        b = data[i:i + 1]
        if b == b"\r" and eaten_enters[0] > 0:
            eaten_enters[0] -= 1
            typed += "\r\n"
            draw()
        elif b == b"\r":
            submit(busy)
        elif b in (b"\x7f", b"\x08"):
            typed = typed[:-1]
            draw()
        elif b == b"\x03":
            restore()
            sys.exit(130)
        elif b == b"\x1b":
            pass
        elif b[0] >= 0x20:
            # utf8 continuation handling
            n = 1
            lead = b[0]
            need = 4 if lead >= 0xF0 else 3 if lead >= 0xE0 else 2 if lead >= 0xC0 else 1
            chunk = data[i:i + need]
            if len(chunk) < need:
                buf = chunk
                return
            typed += chunk.decode("utf8", "replace")
            draw()
            i += need
            continue
        i += 1


pasted = []
lost_pastes = [args.drop_paste]
eaten_enters = [args.eat_enter]


def submit(busy):
    global typed
    text = typed
    # expand paste placeholders back to the real text
    for p in pasted:
        text = re.sub(r"\[Pasted text \+\d+ lines\]", lambda _m: p, text, count=1)
    pasted.clear()
    typed = ""
    if not text.strip():
        draw()
        return
    first = text.strip().splitlines()[0]
    shown = typed_preview(text, first)
    if busy:
        queued_prompts.append(text)
        say("\x1b[2m  (queued) " + shown + "\x1b[0m")
        return
    say("", "\x1b[1m❯\x1b[0m " + shown)
    work(text)
    while queued_prompts:
        nxt = queued_prompts.pop(0)
        say("", "\x1b[1m❯\x1b[0m " + typed_preview(nxt, nxt.splitlines()[0]))
        work(nxt)


def typed_preview(text, first):
    n = len(text.splitlines())
    return f"[Pasted text +{n} lines]" if n > 1 else first


def main():
    global status
    if args.trust:
        W("\r\n Accessing workspace:\r\n " + os.getcwd() + "\r\n\r\n Quick safety check: do you trust this folder?\r\n\r\n"
          " \x1b[1m❯ 1. Yes, I trust this folder\x1b[0m\r\n   2. No, exit\r\n\r\n Enter to confirm · Esc to cancel\r\n")
        while True:
            ch = os.read(fd, 16)
            if b"\r" in ch or b"1" in ch:
                break
            if b"2" in ch:
                restore()
                sys.exit(1)
        W("\x1b[2J\x1b[H")
    tune = "  model=%s effort=%s yolo=%s" % (args.model or "default", args.effort or "default", "on" if YOLO else "off")
    say("\x1b[1m ▐▛███▜▌  SimAgent (%s)\x1b[0m" % args.name, " ▝▜█████▛▘ " + os.getcwd() + tune, "")
    while True:
        r = select.select([fd], [], [], 1.0)[0]
        if r:
            data = os.read(fd, 65536)
            if not data:
                break
            handle_keys(data)


try:
    main()
finally:
    restore()
