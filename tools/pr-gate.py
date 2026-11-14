#!/usr/bin/env python3
"""Merge gate for a pull request: CI on its head commit, and Greptile's confidence score for that same commit.

python3 tools/pr-gate.py <pr> [--wait SECONDS] [--approve-ci SHA]

Prints one JSON object. Exit 0: ready (mergeable, CI passed, Greptile scored the head commit 4/5 or higher). Exit 1:
settled but not ready (a check failed, a conflict with the base, or the score is lower). Exit 2: not settled (checks
or workflow runs not finished, fork runs awaiting approval or just approved, or Greptile hasn't reviewed the head
commit yet). Exit 3: a GitHub call failed; rerun. Exit 4: the head is not --approve-ci's SHA.
--wait polls every 30 s until settled or SECONDS pass. --approve-ci SHA pins the gate to SHA, a commit whose whole
diff has had its security check (docs/guides/security-check.md), and approves fork CI runs awaiting approval while
the PR's head is SHA.
"""

import argparse
import json
import re
import subprocess
import sys
import time

REPO = "leepokai/Codync"
GREPTILE = "greptile-apps[bot]"
MIN_SCORE = 4
FAILED = {"FAILURE", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE", "ERROR"}
REVIEWED = re.compile(r"\]\(https://github\.com/[^/\s]+/[^/\s]+/commit/([0-9a-f]{40})\)")


def gh(*args):
    out = subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else None


def greptile(comments, head):
    """Score and reviewed commit from Greptile's summary comment (it edits one comment per PR on each review).

    The summary, findings and diagram above the footer paraphrase the untrusted PR, so both come from the footer:
    the last score marker, and the last commit link on the last "Last reviewed commit:" line."""
    for c in reversed(comments):
        scores = re.findall(r"greptile_confidence_score:(\d)", c["body"])
        if c["user"]["login"] != GREPTILE or not scores:
            continue
        lines = [line for line in c["body"].splitlines() if "Last reviewed commit:" in line]
        shas = REVIEWED.findall(lines[-1]) if lines else []
        sha = shas[-1] if shas else None
        return {"score": int(scores[-1]), "commit": sha, "current": sha == head}
    return {"score": None, "commit": None, "current": False}


def ci(rollup):
    """("pass" | "fail" | "pending", failing check names); Greptile's own check is the score, not CI."""
    failing, pending, seen = [], False, False
    for c in rollup:
        name = c.get("name") or c.get("context") or ""
        if "greptile" in name.lower():
            continue
        seen = True
        if c.get("__typename") == "StatusContext":
            state = c.get("state")
            pending |= state in ("PENDING", "EXPECTED")
            if state in ("FAILURE", "ERROR"):
                failing.append(name)
        elif c.get("status") != "COMPLETED":
            pending = True
        elif c.get("conclusion") in FAILED:
            failing.append(name)
    return ("fail" if failing else "pending" if pending or not seen else "pass"), failing


def decide(s):
    """(ready, settled) from a gathered state."""
    if s["head_moved"] or s["mergeable"] == "CONFLICTING":
        return False, True
    # Runs approved by this call haven't started yet, whatever the rollup says.
    if s["approved"]:
        return False, False
    ci_done = s["ci"] == "pass" and not s["running"] and not s["awaiting_approval"] and s["mergeable"] == "MERGEABLE"
    reviewed = s["greptile"]["current"]
    ready = ci_done and reviewed and s["greptile"]["score"] >= MIN_SCORE
    return ready, s["ci"] == "fail" or (ci_done and reviewed)


def exit_code(s):
    if "error" in s:
        return 3
    if s["head_moved"]:
        return 4
    return 0 if s["ready"] else 1 if s["settled"] else 2


def state(pr, expected):
    view = gh("pr", "view", str(pr), "--repo", REPO, "--json", "headRefOid,mergeable,statusCheckRollup")
    head = view["headRefOid"]
    runs = gh("api", f"repos/{REPO}/actions/runs?head_sha={head}&per_page=100",
              "--jq", "[.workflow_runs[] | {id, status, conclusion}]") or []
    # A fork run awaiting approval reads status "completed", conclusion "action_required"; approved, it is queued.
    waiting = [r["id"] for r in runs if r["conclusion"] == "action_required"]
    running = [r["id"] for r in runs if r["status"] != "completed"]
    approved = []
    if waiting and expected == head:
        for run in waiting:
            gh("api", "-X", "POST", f"repos/{REPO}/actions/runs/{run}/approve")
        approved, waiting = waiting, []
    checks, failing = ci(view["statusCheckRollup"])
    pages = gh("api", "--paginate", "--slurp", f"repos/{REPO}/issues/{pr}/comments")
    s = {"pr": pr, "head": head, "head_moved": bool(expected) and head != expected, "mergeable": view["mergeable"],
         "ci": checks, "failing": failing, "running": running, "awaiting_approval": waiting, "approved": approved,
         "greptile": greptile([c for page in pages for c in page], head)}
    s["ready"], s["settled"] = decide(s)
    return s


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("pr", type=int)
    p.add_argument("--wait", type=int, default=0, metavar="SECONDS")
    p.add_argument("--approve-ci", metavar="SHA")
    a = p.parse_args()
    deadline = time.monotonic() + a.wait
    while True:
        try:
            s = state(a.pr, a.approve_ci)
        except subprocess.CalledProcessError as e:
            # A transient GitHub error is retried while --wait allows, never reported as a gate result.
            s = {"pr": a.pr, "error": (e.stderr or str(e)).strip()}
        # Runs awaiting an approval this call may not give never start: stop instead of waiting them out.
        if s.get("settled") or s.get("awaiting_approval") or time.monotonic() >= deadline:
            break
        time.sleep(30)
    print(json.dumps(s, indent=2))
    sys.exit(exit_code(s))


if __name__ == "__main__":
    main()
