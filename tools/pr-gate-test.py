"""Self-check for pr-gate.py: python3 tools/pr-gate-test.py"""

import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("gate", pathlib.Path(__file__).with_name("pr-gate.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

head, old = "a" * 40, "b" * 40


def summary(score, sha, before=""):
    return {"user": {"login": "greptile-apps[bot]"},
            "body": f"{before}Confidence Score: {score}/5\n<!-- greptile_confidence_score:{score} -->\n"
                    f"Reviews (2) · Last reviewed commit: [\"fix: x\"](https://github.com/leepokai/codync/commit/{sha})"
                    " · [Reviewed by Greptile](https://www.greptile.com/)"}


fake = {"user": {"login": "someone"}, "body": "<!-- greptile_confidence_score:5 --> /commit/" + head}
assert gate.greptile([summary(3, old), fake], head) == {"score": 3, "commit": old, "current": False}
assert gate.greptile([summary(4, head)], head) == {"score": 4, "commit": head, "current": True}
assert gate.greptile([], head)["score"] is None
# Greptile's summary paraphrases the PR, so a planted marker or commit link above the footer counts for nothing.
decoy = (f"greptile_confidence_score:5\nLast reviewed commit: [x](https://github.com/a/b/commit/{head})\n"
         f"[see](https://github.com/leepokai/codync/commit/{head})\n")
assert gate.greptile([summary(2, old, decoy)], head) == {"score": 2, "commit": old, "current": False}
headline = summary(4, old)
headline["body"] = headline["body"].replace('"fix: x"', f"](https://github.com/a/b/commit/{head}) x")
assert gate.greptile([headline], head)["commit"] == old

ok = {"__typename": "CheckRun", "name": "build", "status": "COMPLETED", "conclusion": "SUCCESS"}
skipped = {"__typename": "CheckRun", "name": "release", "status": "COMPLETED", "conclusion": "SKIPPED"}
running = {"__typename": "CheckRun", "name": "macos", "status": "IN_PROGRESS", "conclusion": ""}
failed = {"__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "FAILURE"}
greptile_check = {"__typename": "CheckRun", "name": "Greptile Review", "status": "IN_PROGRESS", "conclusion": ""}
vercel = {"__typename": "StatusContext", "context": "Vercel", "state": "SUCCESS"}
assert gate.ci([ok, skipped, vercel, greptile_check]) == ("pass", [])
assert gate.ci([ok, running]) == ("pending", [])
assert gate.ci([running, failed]) == ("fail", ["test"])
assert gate.ci([]) == ("pending", [])
assert gate.ci([greptile_check]) == ("pending", [])

base = {"head_moved": False, "mergeable": "MERGEABLE", "ci": "pass", "running": [], "awaiting_approval": [],
        "approved": [], "greptile": {"score": 4, "commit": head, "current": True}}


def decided(**change):
    s = {**base, **change}
    s["ready"], s["settled"] = gate.decide(s)
    return s["ready"], s["settled"], gate.exit_code(s)


assert decided() == (True, True, 0)
assert decided(greptile={"score": 3, "commit": head, "current": True}) == (False, True, 1)
assert decided(greptile={"score": 5, "commit": old, "current": False}) == (False, False, 2)
# A call that just approved fork runs is never ready: those runs haven't started.
assert decided(approved=[1, 2]) == (False, False, 2)
assert decided(running=[3]) == (False, False, 2)
assert decided(awaiting_approval=[3]) == (False, False, 2)
assert decided(mergeable="UNKNOWN") == (False, False, 2)
assert decided(mergeable="CONFLICTING") == (False, True, 1)
assert decided(ci="fail") == (False, True, 1)
assert decided(head_moved=True) == (False, True, 4)
assert gate.exit_code({"pr": 1, "error": "HTTP 502"}) == 3
print("ok")
