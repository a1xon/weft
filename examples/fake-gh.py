#!/usr/bin/env python3
"""A stand-in for GitHub's CLI in weft's tests: the two calls a CI check makes.

    fake-gh.py run list --commit <sha> --json <fields> [--limit N]
    fake-gh.py run view <id> --log-failed

Its "CI" is one workflow, "tests": the project's unit tests run on the pushed commit (a clean copy of
it, as CI would), so a commit that breaks them is red. A run shows as in progress for a few seconds
first, like a real one. What it found is kept in the repository's git folder (fake-ci/).
"""
import json, os, subprocess, sys, tempfile, time

RUNNING_S = 4


def state_dir():
    common = subprocess.run(["git", "rev-parse", "--git-common-dir"], capture_output=True, text=True).stdout.strip()
    d = os.path.join(common or ".git", "fake-ci")
    os.makedirs(d, exist_ok=True)
    return d


def run_for(sha):
    p = os.path.join(state_dir(), sha + ".json")
    if not os.path.exists(p):
        d = tempfile.mkdtemp(prefix="fake-ci-")
        arc = subprocess.run(["git", "archive", sha], capture_output=True)
        if arc.returncode != 0:
            return None
        subprocess.run(["tar", "-x", "-C", d], input=arc.stdout)
        r = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"], cwd=d, capture_output=True, text=True)
        json.dump({"start": time.time(), "ok": r.returncode == 0, "log": r.stdout + r.stderr, "id": int(sha[:6], 16)}, open(p, "w"))
    return json.load(open(p))


def main(a):
    if a[:2] == ["run", "list"] and "--commit" in a:
        info = run_for(a[a.index("--commit") + 1])
        if info is None:
            print("[]")
            return 0
        done = time.time() - info["start"] >= RUNNING_S
        print(json.dumps([{
            "databaseId": info["id"], "workflowName": "tests", "name": "tests",
            "status": "completed" if done else "in_progress",
            "conclusion": ("success" if info["ok"] else "failure") if done else "",
            "url": "https://github.com/example/textkit/actions/runs/%d" % info["id"],
        }]))
        return 0
    if a[:2] == ["run", "view"] and "--log-failed" in a:
        for f in os.listdir(state_dir()):
            info = json.load(open(os.path.join(state_dir(), f)))
            if str(info["id"]) == a[2]:
                for line in info["log"].splitlines():
                    print("tests\tRun the unit tests\t2026-10-17T05:01:02.0000000Z " + line)
                return 0
        print("run not found", file=sys.stderr)
        return 1
    print("fake gh: not simulated: " + " ".join(a), file=sys.stderr)
    return 1


sys.exit(main(sys.argv[1:]))
