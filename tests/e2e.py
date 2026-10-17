#!/usr/bin/env python3
"""End-to-end test of the daemon against real tmux, using plain shell commands
as stand-ins for agents. Isolated: private tmux socket, config and state dirs.

    cargo build && python3 tests/e2e.py [path/to/weft]
"""
import glob, json, os, shutil, subprocess, sys, tempfile, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "debug", "weft")
TMP = tempfile.mkdtemp(prefix="weft-e2e-")
PROJ = os.path.join(TMP, "proj")
os.makedirs(PROJ)
ENV = dict(os.environ, XDG_CONFIG_HOME=TMP + "/cfg", XDG_STATE_HOME=TMP + "/state", WEFT_SOCKET="weft-e2e-%d" % os.getpid(),
           GIT_AUTHOR_NAME="weft e2e", GIT_AUTHOR_EMAIL="e2e@weft.test", GIT_COMMITTER_NAME="weft e2e", GIT_COMMITTER_EMAIL="e2e@weft.test")
failures = 0
os.makedirs(TMP + "/cfg/weft", exist_ok=True)
SIM = os.path.join(ROOT, "examples", "sim-agent.py")
# recheck_s: an agent that says it is waiting is woken after this long (a minute is too slow for a test)
json.dump({"agents": {"claude": {"cmd": "python3 %s --name claude --think 1" % SIM},
                      # programs that mishandle the first Enter / lose the first paste, as real TUIs do when starting or loaded
                      "eater": {"cmd": "python3 %s --name eater --think 1 --eat-enter 1" % SIM},
                      "loser": {"cmd": "python3 %s --name loser --think 1 --drop-paste 1" % SIM}},
           # the real CLIs would name the cards: names here are the prompts' first words
           "recheck_s": 4, "titles": False},
          open(TMP + "/cfg/weft/config.json", "w"))


def check(cond, msg):
    global failures
    print(("PASS  " if cond else "FAIL  ") + msg)
    if not cond:
        failures += 1


def info():
    f = [x for x in glob.glob(TMP + "/state/weft/*.json") if not x.endswith(".live.json")]
    return json.load(open(f[0]))


def call(op, **kw):
    i = info()
    kw["op"] = op
    r = urllib.request.Request(
        "http://127.0.0.1:%d/api/cmd" % i["port"], data=json.dumps(kw).encode(),
        headers={"X-Weft-Token": i["token"], "Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(r, timeout=10))


def add(**node):
    return call("node_add", node=node)["id"]


def status(nid):
    for n in call("ls")["nodes"]:
        if n["id"] == nid:
            return n["status"], n["runs"]
    return None, None


def wait(pred, what, timeout=25):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if pred():
            return True
        time.sleep(0.2)
    print("      (timed out waiting for %s)" % what)
    return False


def cli(*args):
    return subprocess.run([BIN, *args], cwd=PROJ, env=ENV, capture_output=True, text=True, timeout=20)


def start_daemon():
    r = subprocess.run([BIN, "open", PROJ, "--no-open"], env=ENV, capture_output=True, text=True, timeout=20)
    assert r.returncode == 0, r.stderr
    time.sleep(0.4)


def stop_daemon():
    """Only the daemon goes: the agents carry on in tmux, as after an upgrade."""
    cli("stop", "--keep")
    time.sleep(0.4)


def tmux(*args):
    return subprocess.run(["tmux", "-L", ENV["WEFT_SOCKET"], *args], capture_output=True, text=True).stdout


def windows():
    """The agents' windows (the placeholder window that keeps the session alive is not one)."""
    return [w for w in tmux("list-windows", "-a", "-F", "#{window_name}").split() if w != "weft"]


def window_detail():
    return tmux("list-panes", "-a", "-F", "#{window_name}:#{window_id}:dead=#{pane_dead}:#{pane_current_command}").split()


try:
    subprocess.run(["git", "init", "-q"], cwd=PROJ, check=True)
    subprocess.run(["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "--allow-empty", "-qm", "init"], cwd=PROJ, check=True)
    BASE = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=PROJ, capture_output=True, text=True).stdout.strip()
    start_daemon()

    # 1. fan-in: C waits for A and B, then receives both outputs
    a = add(name="A", agent="sh", cmd="echo result-from-A; sleep 1", prompt="go")
    b = add(name="B", agent="sh", cmd="echo result-from-B; sleep 2", prompt="go")
    c = add(name="C", agent="sh", cmd="cat $WEFT_PROMPT_FILE; sleep 1", prompt="Combine:\n{{inputs}}", addenda=[])
    call("edge_add", **{"from": a, "to": c})
    call("edge_add", **{"from": b, "to": c})
    call("run")
    wait(lambda: status(a)[0] == "done", "A done")
    check(status(c)[0] == "waiting", "join waits while only one input has arrived (%s)" % (status(c),))
    check(wait(lambda: status(c)[0] == "done", "C done"), "join fires once both inputs arrived")
    out = cli("read", "C").stdout
    check("result-from-A" in out and "result-from-B" in out, "downstream received both outputs")

    # 2. the screen going quiet finishes an interactive process; its transcript flows on
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    e = add(name="echoer", agent="sh", cmd="cat", finish="idle", quiet_s=1.5, addenda=[])
    d = add(name="after", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[])
    call("edge_add", **{"from": e, "to": d})
    call("start", id=e)
    wait(lambda: status(e)[0] == "ready", "echoer ready")
    cli("send", "echoer", "hello from the user")
    check(wait(lambda: status(d)[0] == "done", "after done", 30), "idle detection finishes a node and triggers the next")
    check(wait(lambda: "hello from the user" in cli("read", "after").stdout, "transcript", 10), "idle output carries the transcript")

    # 3. a message to a not-yet-started command node is typed, not turned into a prompt file
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    t = add(name="typed", agent="sh", cmd="cat", addenda=[])
    call("send", id=t, text="typed-into-cat")
    check(wait(lambda: "typed-into-cat" in call("read", id=t).get("text", ""), "typed text echoed"), "message to a stopped command node is typed into it")

    # 4. `weft done` from inside the pane + routing on failure
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    w = add(name="worker", agent="sh", finish="signal", addenda=[], prompt="",
            cmd="sleep 1; $WEFT_BIN done --fail <<<'3 tests failed'; sleep 30")
    ok_ = add(name="on-ok", agent="sh", cmd="echo ok; sleep 1", prompt="x", addenda=[])
    bad = add(name="on-fail", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[])
    call("edge_add", **{"from": w, "to": ok_, "on": "ok"})
    call("edge_add", **{"from": w, "to": bad, "on": "fail"})
    call("fire", id=w)
    check(wait(lambda: status(bad)[0] == "done", "fail path"), "failure routes to the 'only if it fails' thread")
    check(status(ok_)[0] == "blocked", "the 'only if it succeeds' link stayed dormant and the agent shows as blocked (%s)" % (status(ok_),))
    check("3 tests failed" in cli("read", "on-fail").stdout, "the failure report is handed on")

    # 5. daemon restart: agents survive, port and token are reused, state is re-adopted
    before = info()
    stop_daemon()
    check(subprocess.run(["tmux", "-L", ENV["WEFT_SOCKET"], "list-windows", "-a"], capture_output=True).returncode == 0,
          "tmux windows outlive the daemon")
    start_daemon()
    after = info()
    check((before["port"], before["token"]) == (after["port"], after["token"]), "port and token are stable across restarts")
    check(status(bad)[0] == "done" and status(w)[0] in ("failed", "busy"), "node state is restored (%s)" % (status(w),))

    # 6. git worktree isolation
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    g = add(name="wt", agent="sh", cmd="pwd; git rev-parse --abbrev-ref HEAD; sleep 30", worktree=True, finish="signal", addenda=[])
    call("start", id=g)
    check(wait(lambda: "weft/" in call("read", id=g).get("text", ""), "worktree branch"), "worktree node runs on its own weft/* branch")
    text = call("read", id=g)["text"]
    check(PROJ not in text.splitlines()[0], "worktree node runs outside the project folder (%s)" % text.splitlines()[0][-40:])
    call("node_del", id=g)
    time.sleep(1.5)
    wts = subprocess.run(["git", "worktree", "list"], cwd=PROJ, capture_output=True, text=True).stdout.strip().splitlines()
    check(len(wts) == 1, "deleting the node removes its worktree (%d left)" % len(wts))
    # 7. a draft collects but does not run until armed; notes ride along; an approval gate holds
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    note = add(name="brief", agent="note", prompt="HOUSE-RULE: tabs only")
    src = add(name="source", agent="sh", cmd="echo from-source; sleep 1", prompt="go", addenda=[])
    dr = add(name="drafty", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[], draft=True)
    call("edge_add", **{"from": note, "to": dr})
    call("edge_add", **{"from": src, "to": dr})
    call("run")
    wait(lambda: status(src)[0] == "done", "source done")
    time.sleep(0.8)
    check(status(dr)[0] == "draft", "a draft does not run when its inputs arrive (%s)" % (status(dr),))
    call("arm", id=dr)
    check(wait(lambda: status(dr)[0] == "done", "drafty done"), "arming a draft runs it with what it collected")
    out = cli("read", "drafty").stdout
    check("from-source" in out and "HOUSE-RULE: tabs only" in out, "the note's text rode along with the real input")

    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    s1 = add(name="plan", agent="sh", cmd="echo the-plan; sleep 1", prompt="go", addenda=[])
    gt = add(name="approve", agent="gate")
    nx = add(name="build", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[])
    call("edge_add", **{"from": s1, "to": gt})
    call("edge_add", **{"from": gt, "to": nx, "on": "ok"})
    call("run")
    check(wait(lambda: status(gt)[0] == "attention", "gate holds"), "an approval gate holds the flow and asks for a human")
    time.sleep(0.5)
    check(status(nx)[0] in ("waiting", "new"), "nothing moves past the gate meanwhile (%s)" % (status(nx),))
    call("approve", id=gt, ok=True)
    check(wait(lambda: status(nx)[0] == "done", "build done"), "approving lets the flow continue")
    check(wait(lambda: "the-plan" in cli("read", "build").stdout, "plan", 10), "the held results were handed on")

    # 8. commit & push: disarmed by default, one commit per agent worktree, a single push at the end
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    remote = TMP + "/remote.git"
    subprocess.run(["git", "init", "-q", "--bare", remote], check=True)
    subprocess.run(["git", "remote", "add", "origin", remote], cwd=PROJ, check=True)
    subprocess.run(["git", "push", "-q", "-u", "origin", "HEAD"], cwd=PROJ, check=True)
    a = add(name="fix slug", agent="sh", cmd="echo slug > slug.txt; echo 'Fixed slugify'", finish="exit", worktree=True)
    b = add(name="fix wrap", agent="sh", cmd="sleep 2; echo wrap > wrap.txt; echo 'Fixed wrap'", finish="exit", worktree=True)
    # (a push no longer waits to be armed; a flow from an older weft can still have one that does)
    g = add(name="ship", agent="git", branch="weft/ship", draft=True)
    call("edge_add", **{"from": a, "to": g})
    call("edge_add", **{"from": b, "to": g})
    call("run")
    wait(lambda: status(a)[0] == "done" and status(b)[0] == "done", "both agents done")
    time.sleep(1.0)
    refs = subprocess.run(["git", "--git-dir", remote, "branch", "--list"], capture_output=True, text=True).stdout
    check(status(g)[0] == "draft" and "weft/ship" not in refs, "a commit & push left waiting to be armed does nothing (%s)" % (status(g),))
    call("arm", id=g)
    check(wait(lambda: status(g)[0] == "done", "shipped"), "arming commits the arrived results and pushes")
    log = subprocess.run(["git", "--git-dir", remote, "log", "--oneline", "weft/ship"], capture_output=True, text=True).stdout
    check("fix slug" in log and "fix wrap" in log, "the remote branch has one commit per agent")
    tree = subprocess.run(["git", "--git-dir", remote, "ls-tree", "-r", "--name-only", "weft/ship"], capture_output=True, text=True).stdout.split()
    check("slug.txt" in tree and "wrap.txt" in tree and not any(t.startswith(".weft") for t in tree), "both agents' files are there, weft's own folder is not")
    # 9-12: gates and agent-made commits, with the simulated agent standing in for a real one
    import shutil as _sh
    subprocess.run(["git", "checkout", "-q", BASE], cwd=PROJ, capture_output=True)
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])

    def reset_project():
        subprocess.run(["git", "checkout", "-q", "-f", BASE], cwd=PROJ, check=True)
        for item in os.listdir(PROJ):
            if item not in (".git", ".weft"):
                (_sh.rmtree if os.path.isdir(os.path.join(PROJ, item)) else os.remove)(os.path.join(PROJ, item))
        _sh.copytree(os.path.join(ROOT, "examples", "textkit"), PROJ, dirs_exist_ok=True)
        subprocess.run(["git", "add", "-A"], cwd=PROJ, check=True)
        subprocess.run(["git", "commit", "-qm", "textkit"], cwd=PROJ, capture_output=True)
        for n in call("ls")["nodes"]:
            call("node_del", id=n["id"])
        for f in glob.glob(PROJ + "/.weft/run/received-*.log"):
            os.remove(f)

    def fetch_shot(name):
        i = info()
        r = urllib.request.Request("http://127.0.0.1:%d/shots/%s" % (i["port"], name), headers={"X-Weft-Token": i["token"]})
        with urllib.request.urlopen(r, timeout=5) as resp:
            return resp.status, resp.headers.get("Content-Type"), resp.read(8)

    reset_project()
    a = add(name="build", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.")
    g = add(name="look", agent="shot", subject="the slugify behaviour")
    b = add(name="deploy", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[])
    call("edge_add", **{"from": a, "to": g})
    call("edge_add", **{"from": g, "to": b})
    call("run")
    check(wait(lambda: status(g)[0] == "attention", "screenshot review", 40), "the screenshot gate asked the agent and now waits for a human (%s)" % (status(g),))
    shots = sorted(f for f in os.listdir(PROJ + "/.weft/shots") if f.endswith(".png"))
    check(len(shots) >= 1, "the agent saved a screenshot where the gate asked: %s" % shots)
    code, ctype, magic = fetch_shot(shots[0])
    check(code == 200 and ctype == "image/png" and magic.startswith(b"\x89PNG"), "the daemon serves it to the app (%s %s)" % (code, ctype))
    try:
        fetch_shot("../../etc/passwd")
        traversal = False
    except Exception:
        traversal = True
    check(traversal, "path traversal on the image route is refused")
    check(status(b)[0] in ("waiting", "new"), "the barrier holds: nothing downstream has started (%s)" % (status(b),))
    # changes requested: the agent is asked again, in its own session, for a second image
    call("gate_changes", id=g, text="make the card bigger")
    check(wait(lambda: len([f for f in os.listdir(PROJ + "/.weft/shots") if "-r2-" in f]) >= 1, "second screenshot", 40), "requesting changes makes the agent take a new screenshot")
    check(wait(lambda: status(g)[0] == "attention", "second review", 20), "and the gate waits again")
    check(status(b)[0] in ("waiting", "new"), "still blocked after the round trip")
    call("approve", id=g, ok=True)
    check(wait(lambda: status(b)[0] == "done", "deploy done", 20), "confirming opens the barrier")
    check(wait(lambda: "Approved screenshot" in cli("read", "deploy").stdout, "approved path", 10), "the approved image is handed on by path")

    reset_project()
    a = add(name="build", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.")
    t = add(name="check", agent="tests", cmd="python3 -m unittest discover -s tests -q")
    b = add(name="release", agent="sh", cmd="cat $WEFT_PROMPT_FILE", prompt="{{inputs}}", addenda=[])
    call("edge_add", **{"from": a, "to": t})
    call("edge_add", **{"from": t, "to": b, "on": "ok"})
    call("run")
    check(wait(lambda: status(b)[0] == "done", "release", 90), "a failing suite is sent back to the agent until it passes, then the barrier opens (%s)" % (status(t),))
    out = cli("read", "release").stdout
    check("Tests passed" in out, "the pass is handed on: %r" % out[-80:].replace("\n", " "))
    check(os.path.exists(PROJ + "/tests/test_extra.py"), "the agent was asked to write tests first and did")

    reset_project()
    subprocess.run(["git", "push", "-q", "origin", "HEAD"], cwd=PROJ, capture_output=True)
    a = add(name="build", agent="claude", prompt="Fix the slugify function in textkit/slug.py and the wrap function in textkit/wrap.py so that all tests pass. Keep the public API.")
    gt = add(name="ship", agent="git", branch="weft/by-agent", draft=False)
    call("edge_add", **{"from": a, "to": gt})
    call("run")
    check(wait(lambda: status(gt)[0] in ("done", "failed"), "ship", 90), "the commit node finished (%s)" % (status(gt),))
    remote_log = subprocess.run(["git", "--git-dir", remote, "log", "--format=%an|%s", "weft/by-agent"], capture_output=True, text=True).stdout.splitlines()
    mine = [l for l in remote_log if l.split("|", 1)[1].startswith("Fix ")]
    check(len(mine) == 1 and "slug.py" in mine[0] and "wrap.py" in mine[0], "the commit was written by the agent that did the work: %s" % (mine,))
    check(not any(l.split("|", 1)[1].startswith("build:") for l in remote_log), "weft did not invent a commit of its own")
    log_text = call("result", id=gt)["text"]
    check("Pushed `weft/by-agent` to `origin`" in log_text and "- build (claude)" in log_text and "commit: " in log_text,
          "the node asked, verified with git, pushed, and says who did what: %r" % log_text[:90])
    check("Commit the work you just did" in open(PROJ + "/.weft/run/received-%s.log" % a).read(), "and it asked the agent that did the work")
    # 13-16: queue on the same agent, or hand the work to a fresh one
    reset_project()
    a = add(name="build", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.")
    call("run")
    check(wait(lambda: status(a)[0] == "done", "build done", 60), "the first agent finishes (%s)" % (status(a),))
    runs0 = status(a)[1]
    # then: the same agent gets one more prompt; no new node appears
    n0 = len(call("ls")["nodes"])
    call("send", id=a, text="Also fix the wrap function in textkit/wrap.py. Keep the public API.", mode="auto")
    check(wait(lambda: status(a)[0] == "busy" or status(a)[1] > runs0, "second prompt running", 20), "a follow-up runs on the same agent (%s)" % (status(a),))
    check(wait(lambda: status(a)[0] == "done" and status(a)[1] > runs0, "second prompt done", 60), "and finishes there")
    check(len(call("ls")["nodes"]) == n0, "no new agent was needed")
    check(os.path.exists(PROJ + "/textkit/wrap.py") and "return" in open(PROJ + "/textkit/wrap.py").read(), "its work is on disk")
    # hand over: a fresh agent takes over, briefed by the old one
    r = call("takeover", **{"from": a, "node": {"agent": "claude", "x": 600.0, "y": 0.0}, "text": "Run the whole test suite and report the result."})
    check(r.get("ok") and r.get("id"), "a takeover creates the new agent (%s)" % (r,))
    b = r["id"]
    check(wait(lambda: status(b)[0] in ("starting", "busy", "done"), "new agent started", 40), "the new agent starts once the note is in (%s)" % (status(b),))
    check(wait(lambda: status(b)[0] == "done", "new agent done", 60), "and finishes (%s)" % (status(b),))
    got = open(PROJ + "/.weft/run/received-%s.log" % b).read()
    check("Taking over from" in got and "taking over unfinished work" in got, "it was handed the handover document")
    check("goal: make the textkit tests pass" in got, "including the note the old agent wrote")
    check("State of the repository" in got and "branch:" in got, "and the state of the repository")
    check("Run the whole test suite and report the result." in got, "and the instruction it was given")
    check("Context from previous steps" not in got, "it is not just a piped result")
    check(any(n["id"] == b for n in call("ls")["nodes"]), "the new agent is a normal node on the canvas")
    old_got = open(PROJ + "/.weft/run/received-%s.log" % a).read()
    check("take over this work from you" in old_got, "the old agent was asked for the note in its own session")
    # 17-19: yolo is the default and a card (or the config) can switch it off
    reset_project()
    on = add(name="yolo-on", agent="claude", prompt="Say hello in one word.")
    off = add(name="yolo-off", agent="claude", prompt="Say hello in one word.", yolo=False)
    call("run")
    check(wait(lambda: status(on)[0] == "done" and status(off)[0] == "done", "both done", 60), "both agents ran (%s %s)" % (status(on), status(off)))
    got_on = open(PROJ + "/.weft/run/received-%s.log" % on).read() if os.path.exists(PROJ + "/.weft/run/received-%s.log" % on) else ""
    seen_on = cli("read", on, "-n", "40").stdout
    seen_off = cli("read", off, "-n", "40").stdout
    check("yolo=on" in seen_on, "agents run in yolo mode by default (%s)" % [l for l in seen_on.splitlines() if "yolo=" in l][:1])
    check("yolo=off" in seen_off, "a card that switched it off starts the agent without the flag (%s)" % [l for l in seen_off.splitlines() if "yolo=" in l][:1])
    # 20-26: an agent that announced background work is not done; the ETA follows what it says
    reset_project()
    bg = add(name="bg", agent="claude", prompt="BACKGROUND=14 say hi", addenda=[])
    call("run")
    check(wait(lambda: status(bg)[0] == "busy", "bg busy", 30), "the agent starts working (%s)" % (status(bg),))
    time.sleep(9)   # far beyond the quiet time, the screen is still, the status line says "1 shell still running"
    check(status(bg)[0] == "busy", "9 s of silence with a shell still running is not done (%s)" % (status(bg),))
    check(wait(lambda: status(bg)[0] == "done", "bg done", 40), "it is done once the shell ended and the agent answered (%s)" % (status(bg),))
    res = cli("result", "bg").stdout
    check("It finished." in res, "and the result is the real answer, not the first sentence: %r" % res[:90].replace("\n", " "))
    check("Cooked for" not in res and "Brewed for" not in res and "▐" not in res, "without the terminal furniture")

    reset_project()
    eta_n = add(name="eta", agent="claude", prompt="LONG=12 count to ten", checks=[{"id": "c1", "kind": "eta"}])
    call("run")
    check(wait(lambda: status(eta_n)[0] == "busy", "eta busy", 30), "an agent with an ETA check runs (%s)" % (status(eta_n),))
    time.sleep(3)
    ls1 = cli("ls").stdout
    got = [l for l in ls1.splitlines() if l.startswith(eta_n)]
    check(got and "eta=" in got[0], "it was asked, and answered with weft eta: %s" % (got[0].split("runs=")[-1] if got else ls1))
    secs = int(got[0].split("eta=")[1].rstrip("s")) if got and "eta=" in got[0] else -999
    check(0 < secs <= 14, "the countdown is in range (%ss)" % secs)
    check(wait(lambda: status(eta_n)[0] == "done", "eta done", 40), "it finishes (%s)" % (status(eta_n),))
    check("eta=" not in "".join(l for l in cli("ls").stdout.splitlines() if l.startswith(eta_n)), "and a finished agent has no countdown")
    # 27-30: a queued prompt is answered by the same agent
    reset_project()
    sa = add(name="alpha", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.")
    call("run")
    check(wait(lambda: status(sa)[0] == "done", "alpha done", 60), "an agent finishes (%s)" % (status(sa),))
    n_before = len(call("ls")["nodes"])
    stp = add(name="more", agent="step", host=sa, prompt="Also explain textkit/wrap.py in one sentence.", addenda=[])
    call("edge_add", **{"from": sa, "to": stp})
    check(wait(lambda: status(stp)[0] == "done", "step done", 60), "a queued prompt is answered (%s)" % (status(stp),))
    check("explain textkit/wrap.py" in open(PROJ + "/.weft/run/received-%s.log" % sa).read(), "by the same agent, in its own session")
    check(not os.path.exists(PROJ + "/.weft/run/received-%s.log" % stp), "nobody else was started for it")
    check(len(call("ls")["nodes"]) == n_before + 1, "the step is the only new node")
    check(status(sa)[0] == "done" and status(sa)[1] == 2, "the agent took it as one more turn of its own (%s)" % (status(sa),))

    # 31-40: one commit per agent, the commits meet in one push, a pull request is opened, and an
    # agent after it tests the result live, starting from everything that was carried on
    reset_project()
    subprocess.run(["git", "push", "-q", "origin", "HEAD"], cwd=PROJ, capture_output=True)
    a1 = add(name="slug", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.", worktree=True)
    a2 = add(name="wrap", agent="claude", prompt="Fix the wrap function in textkit/wrap.py so that tests/test_wrap.py passes. Keep the public API.", worktree=True)
    c1 = add(name="commit slug", agent="git", git_op="commit", branch="weft/two")
    c2 = add(name="commit wrap", agent="git", git_op="commit", branch="weft/two")
    ps = add(name="push", agent="git", git_op="push", draft=False, remote="origin")
    pr = add(name="open PR", agent="git", git_op="pr", draft=False, prompt="label it agent-made")
    tt = add(name="live test", agent="claude", prompt="Test the features live and report.", addenda=[])
    for f, t in [(a1, c1), (a2, c2), (c1, ps), (c2, ps), (ps, pr), (pr, tt)]:
        r = call("edge_add", **{"from": f, "to": t})
        check(r.get("ok"), "link %s -> %s" % (f, t))
    r = call("edge_add", **{"from": a2, "to": c1})
    check(not r.get("ok") and "one agent" in r.get("error", ""), "a commit takes one agent: %s" % r.get("error"))
    call("run")
    check(wait(lambda: status(c1)[0] == "done" and status(c2)[0] == "done", "commits done", 120), "each commit resolves with its own agent (%s, %s)" % (status(c1), status(c2)))
    check(wait(lambda: status(ps)[0] == "done", "push done", 60), "the push waited for both and pushed (%s)" % (status(ps),))
    check(wait(lambda: status(pr)[0] == "done", "pr done", 60), "the pull request is opened by an agent that did the work (%s)" % (status(pr),))
    check(wait(lambda: status(tt)[0] == "done", "tester done", 60), "the agent after it ran (%s)" % (status(tt),))
    remote_log = subprocess.run(["git", "--git-dir", remote, "log", "--format=%s", "weft/two"], capture_output=True, text=True).stdout.splitlines()
    fixes = [l for l in remote_log if l.startswith("Fix ")]
    check(len(fixes) == 2, "both agents' commits are on the remote branch, written by the agents: %s" % fixes)
    got = open(PROJ + "/.weft/run/received-%s.log" % tt).read()
    for want in ["Opened a pull request for `weft/two`: https://example.test/acme/textkit/pull/7", "Files changed (2): textkit/slug.py, textkit/wrap.py",
                 "- slug (claude)", "- wrap (claude)", "was asked: Fix the slugify function", "was asked: Fix the wrap function", "Test the features live and report."]:
        check(want in got, "the tester started from the carried report: %r" % want)
        if want not in got:
            print("      received:", got[-1400:])
    pr_log = open(PROJ + "/.weft/run/received-%s.log" % a2).read() + open(PROJ + "/.weft/run/received-%s.log" % a1).read()
    check("open a pull request" in pr_log.lower() and "label it agent-made" in pr_log, "the agent that wrote the PR got the extra instructions")

    # 41-44: the remote moved on behind weft's back: the push is rejected, the agent brings the branch up to date, once
    reset_project()
    subprocess.run(["git", "push", "-q", "origin", "HEAD"], cwd=PROJ, capture_output=True)
    other = TMP + "/other"
    shutil.rmtree(other, ignore_errors=True)
    subprocess.run(["git", "clone", "-q", remote, other], check=True, capture_output=True)
    subprocess.run(["git", "checkout", "-q", "-b", "weft/three", "origin/" + BASE], cwd=other, check=True, capture_output=True)
    open(other + "/elsewhere.txt", "w").write("someone else\n")
    subprocess.run(["git", "add", "-A"], cwd=other, check=True)
    subprocess.run(["git", "commit", "-qm", "elsewhere"], cwd=other, check=True, capture_output=True, env=ENV)
    subprocess.run(["git", "push", "-q", "origin", "weft/three"], cwd=other, check=True, capture_output=True)
    ar = add(name="slug", agent="claude", prompt="Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.")
    cr = add(name="commit", agent="git", git_op="commit", branch="weft/three")
    pu = add(name="push", agent="git", git_op="push", draft=False, remote="origin")
    call("edge_add", **{"from": ar, "to": cr})
    call("edge_add", **{"from": cr, "to": pu})
    call("run")
    check(wait(lambda: status(pu)[0] in ("done", "failed"), "rejected push", 120), "the push ends (%s)" % (status(pu),))
    check(status(pu)[0] == "done", "after the agent brought the branch up to date it went through")
    if status(pu)[0] != "done":
        print("      push result:", call("result", id=pu).get("text"))
        print("      project:", subprocess.run(["git", "log", "--oneline", "--all", "--graph", "-12"], cwd=PROJ, capture_output=True, text=True).stdout)
        print("      status:", subprocess.run(["git", "status", "--short", "-b"], cwd=PROJ, capture_output=True, text=True).stdout)
        print("      agent:", open(PROJ + "/.weft/run/received-%s.log" % ar).read()[-600:])
    log3 = subprocess.run(["git", "--git-dir", remote, "log", "--format=%s", "weft/three"], capture_output=True, text=True).stdout.splitlines()
    check("elsewhere" in log3 and any(l.startswith("Fix ") for l in log3), "the remote has what was there and the agent's commit: %s" % log3[:4])
    if not any(l.startswith("Fix ") for l in log3):
        print("      push:", call("result", id=pu).get("text"))
        print("      commit:", call("result", id=cr).get("text"))
        print("      project:", subprocess.run(["git", "log", "--oneline", "--all", "--graph", "-14"], cwd=PROJ, capture_output=True, text=True).stdout)
        print("      status:", subprocess.run(["git", "status", "--short", "-b"], cwd=PROJ, capture_output=True, text=True).stdout)
    check("was rejected" in open(PROJ + "/.weft/run/received-%s.log" % ar).read(), "the agent that did the work was asked to rebase")

    # 14. silence is not an answer: a step of a chain ends with a status line that weft reads
    def clear():
        for n in call("ls")["nodes"]:
            call("node_del", id=n["id"])

    def received(nid):
        try:
            return open(PROJ + "/.weft/run/received-%s.log" % nid).read()
        except OSError:
            return ""

    def forget(*nids):
        # node ids are reused after a clean-up: start each log empty
        for nid in nids:
            try:
                os.remove(PROJ + "/.weft/run/received-%s.log" % nid)
            except OSError:
                pass

    # the report addendum is on by default: a new agent is taught the weft commands, not the status line
    clear()
    rd = add(name="reporter", agent="claude", prompt="Explain what textkit/slug.py does in one sentence.")
    forget(rd)
    call("run")
    check(wait(lambda: status(rd)[0] == "done", "reporter done", 60), "an agent with the default addenda finishes (%s)" % (status(rd),))
    got_rd = received(rd).split("=== prompt ===\n", 1)[-1]
    check("done <<'EOF'" in got_rd and "Finish your final message" not in got_rd, "it is taught the weft commands, and no status line")

    # the status line is what these sections test: their agents switch the report addendum off
    clear()
    solo = add(name="solo", agent="claude", prompt="Explain what textkit/slug.py does in one sentence.", addenda=[])
    forget(solo)
    call("run")
    check(wait(lambda: status(solo)[0] == "done", "solo done", 60), "a lone agent finishes (%s)" % (status(solo),))
    got = received(solo).split("=== prompt ===\n", 1)[-1]
    check(not got.startswith("[weft]"), "it is not told it is a step of a chain: %r" % got[:70])
    check(got.rstrip().split("\n")[-1].startswith("[weft] Finish your final message with one last line: STATUS: <WORD>"), "but is asked to end with the status line")
    check("STATUS" not in call("result", id=solo).get("text", ""), "which is not part of its result")

    # a lone agent that ends with a question needs you: it is not done
    clear()
    la = add(name="lone-asker", agent="claude", prompt="ASK_QUESTION about the slug of a title with unicode.")
    forget(la)
    call("run")
    check(wait(lambda: status(la)[0] == "attention", "lone issue", 90), "a lone agent's question shows as 'needs you', not done (%s)" % (status(la),))
    call("send", id=la, text="Transliterate to plain ascii.", mode="auto")
    check(wait(lambda: status(la)[0] == "done", "lone answered", 90), "after the answer it finishes (%s)" % (status(la),))

    clear()
    hi = add(name="intro", agent="claude", prompt="Explain what textkit/slug.py does in one sentence.", addenda=[])
    nx0 = add(name="reader", agent="claude", prompt="Say OK to: {{inputs}}", addenda=[])
    call("edge_add", **{"from": hi, "to": nx0})
    forget(hi, nx0)
    call("run")
    check(wait(lambda: status(hi)[0] == "done", "intro done", 90), "a step with a successor finishes (%s)" % (status(hi),))
    first = received(hi).split("=== prompt ===\n", 1)[-1]
    check(first.startswith("[weft] You are one step in a chain of agents run by weft"), "its first line tells it where it is: %r" % first[:70])
    check("Explain what textkit/slug.py does" in first, "then comes the task as it was written")
    check(first.rstrip().split("\n")[-1].startswith("[weft] Finish your final message with one last line: STATUS: <WORD>"), "and the last line tells it how to end")
    check("done <<'EOF'" not in first, "no shell command is put into the task")
    check("STATUS" not in call("result", id=hi).get("text", ""), "the status line is not part of its result")
    check(wait(lambda: status(nx0)[0] == "done", "reader done", 60), "then the next step ran (%s)" % (status(nx0),))
    check("STATUS" not in received(nx0).split("=== prompt ===\n", 1)[-1].split("Finish your final message")[0], "and was not handed the label")

    # an agent that says it is waiting is woken, not finished
    clear()
    df = add(name="defer", agent="claude", prompt="DEFER run the long build and tell me the result.", addenda=[])
    nx = add(name="after", agent="claude", prompt="Summarise: {{inputs}}", addenda=[])
    call("edge_add", **{"from": df, "to": nx})
    forget(df, nx)
    call("run")
    time.sleep(3.5)
    check(status(df)[0] != "done" and status(nx)[0] != "done", "'I will get back to you' is not the end of the task (%s, next: %s)" % (status(df), status(nx)))
    check(wait(lambda: status(df)[0] == "done", "defer done", 90), "it finishes once it has really finished (%s)" % (status(df),))
    check("You said you were waiting for" in received(df), "weft woke it to check on what it waited for")
    out = call("result", id=df).get("text", "")
    check("build finished" in out.lower() and "STATUS" not in out, "the result has what it found: %r" % out[-80:])
    check(wait(lambda: status(nx)[0] == "done", "after done", 60), "and only then the next step ran (%s)" % (status(nx),))
    check("build finished" in received(nx).lower(), "with the real result as its input")

    # an agent that ignores the rule is asked once for the line
    clear()
    ns = add(name="silent", agent="claude", prompt="DEFER NO_STATUS run the long build and tell me the result.", addenda=[])
    nt = add(name="after-silent", agent="claude", prompt="Summarise: {{inputs}}", addenda=[])
    call("edge_add", **{"from": ns, "to": nt})
    forget(ns, nt)
    call("run")
    check(wait(lambda: "had no STATUS line" in received(ns), "asked", 60), "no status line: it was asked for it")
    check(wait(lambda: status(ns)[0] == "done", "silent done", 90), "and it finished after waiting and being woken (%s)" % (status(ns),))
    check("You said you were waiting for" in received(ns) and "build finished" in call("result", id=ns).get("text", "").lower(), "with the real result")
    check(wait(lambda: status(nt)[0] == "done", "after silent", 60), "then the next step ran (%s)" % (status(nt),))

    # an agent that cannot go on says ISSUE: the flow waits, the person answers in the same session
    clear()
    ia = add(name="asker", agent="claude", prompt="ASK_QUESTION about the slug of a title with unicode.")
    ib = add(name="after-asker", agent="claude", prompt="Summarise: {{inputs}}")
    call("edge_add", **{"from": ia, "to": ib})
    forget(ia, ib)
    call("run")
    check(wait(lambda: status(ia)[0] == "attention", "issue", 90), "its question shows as 'needs you' (%s)" % (status(ia),))
    time.sleep(6)
    check(status(ia)[0] == "attention" and status(ib)[0] != "done", "it stays open: the question is not handed on as a result (%s, next: %s)" % (status(ia), status(ib)))
    runs0 = status(ia)[1]
    call("send", id=ia, text="Transliterate to plain ascii.", mode="auto")
    check(wait(lambda: status(ia)[0] == "done", "answered", 90), "after the answer the same agent finishes (%s)" % (status(ia),))
    check(status(ia)[1] == runs0, "it is the same run, not a new one (%s -> %s)" % (runs0, status(ia)[1]))
    check(wait(lambda: status(ib)[0] == "done", "after asker", 60), "and the flow goes on (%s)" % (status(ib),))

    # an agent that fails says so: the failure links fire
    clear()
    fa = add(name="failing", agent="claude", prompt="FAIL_TASK change the slug function.")
    fb = add(name="rescue", agent="claude", prompt="Look at: {{inputs}}")
    call("edge_add", **{"from": fa, "to": fb, "on": "fail"})
    call("run")
    check(wait(lambda: status(fa)[0] == "failed", "failing", 90), "STATUS: FAILED fails the step (%s)" % (status(fa),))
    check(wait(lambda: status(fb)[0] == "done", "rescue", 60), "and the 'only if it fails' link fires (%s)" % (status(fb),))

    # the optional command channel: an agent taught to report with weft commands may use them
    clear()
    ca = add(name="commander", agent="claude", prompt="Explain what textkit/slug.py does in one sentence.", addenda=["report-done"])
    cb = add(name="after-commander", agent="claude", prompt="Say OK to: {{inputs}}")
    call("edge_add", **{"from": ca, "to": cb})
    forget(ca, cb)
    call("run")
    check(wait(lambda: status(ca)[0] == "done", "commander", 60), "an agent told to use the commands finishes with them (%s)" % (status(ca),))
    log = received(ca)
    check("## How to report back to weft" in log and "STATUS: <WORD>" not in log, "and is not also given the status line")

    # a prompt that was not taken is not a finished agent: the Enter is repeated, a lost paste is pasted again
    clear()
    ea = add(name="eats-enter", agent="eater", prompt="Explain what textkit/slug.py does in one sentence.")
    lo = add(name="loses-paste", agent="loser", prompt="Explain what textkit/wrap.py does in one sentence.")
    forget(ea, lo)
    call("run")
    time.sleep(2.2)
    check(status(ea)[0] != "done" and status(lo)[0] != "done", "while the prompt was not taken the agents are not 'done' (%s, %s)" % (status(ea), status(lo)))
    check(wait(lambda: status(ea)[0] == "done", "eater", 60), "an Enter read as a newline is sent again, and the agent works (%s)" % (status(ea),))
    check("analysis" in call("result", id=ea).get("text", "").lower(), "with a real answer: %r" % call("result", id=ea).get("text", "")[:60])
    check(wait(lambda: status(lo)[0] == "done", "loser", 60), "a lost paste is pasted again, and the agent works (%s)" % (status(lo),))
    check("analysis" in call("result", id=lo).get("text", "").lower(), "with a real answer: %r" % call("result", id=lo).get("text", "")[:60])
    check(received(ea).count("=== prompt ===") == 1 and received(lo).count("=== prompt ===") == 1, "and each got the task exactly once")

    # 15. quitting leaves nothing behind
    for n in call("ls")["nodes"]:
        call("node_del", id=n["id"])
    q1 = add(name="q1", agent="sh", cmd="sleep 600", prompt="go")
    q2 = add(name="q2", agent="sh", cmd="sleep 600", prompt="go")
    call("run")
    check(wait(lambda: len(windows()) == 2, "two windows"), "two agents are running in their own windows (%s)" % window_detail())
    r = cli("stop", "--keep")
    check("2 agents keep running" in r.stdout, "stop --keep leaves them and says so: %r" % r.stdout.strip())
    time.sleep(0.4)
    check(len(windows()) == 2, "their windows are still there")
    start_daemon()
    r = call("quit", end=True)
    check(r.get("ok") and r.get("agents") == 2, "quit with end says how many agents it ended (%s)" % r)
    check(wait(lambda: tmux("list-sessions") == "", "session gone"), "no tmux session is left behind")
    check(wait(lambda: cli("ls").returncode != 0, "daemon gone"), "the daemon is gone too")
    start_daemon()
    check(len(windows()) == 0 and status(q1)[0] != "busy", "nothing came back to life on its own (%s, %s)" % (status(q1), windows()))
    call("node_del", id=q1)
    call("node_del", id=q2)
    q3 = add(name="q3", agent="sh", cmd="sleep 600", prompt="go")
    call("run")
    check(wait(lambda: len(windows()) == 1, "q3 running"), "an agent runs again after the restart")
    os.kill(info()["pid"], 9)
    time.sleep(0.4)
    r = cli("stop")
    check("left behind" in r.stdout and tmux("list-sessions") == "", "stop clears the agents a dead daemon left behind: %r" % r.stdout.strip())
finally:
    subprocess.run(["tmux", "-L", ENV["WEFT_SOCKET"], "kill-server"], capture_output=True)
    cli("stop")
    shutil.rmtree(TMP, ignore_errors=True)

print("\n%s" % ("ALL PASSED" if not failures else "%d FAILED" % failures))
sys.exit(1 if failures else 0)
