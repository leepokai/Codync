#!/usr/bin/env python3
"""Bot memory regression: scripted conversations against a real host, graded by keywords.

Each case in cases.json is one bot: its sessions (lists of user messages, a new session between them), then a
probe question in a fresh session once dreaming has had its turn. A reply passes when it holds one of `expect`
and none of `forbid`; `memory_forbid` must appear in none of the bot's memory entries or notes, and
`expect_file` wants the detail kept in a long-form note. Keyword grading is coarse but can't be talked into a
pass, which an LLM judge can.

Run it against a disposable host, never your own (it creates and deletes bots):
    CODYNC_HOME=/tmp/eval-home codync-host serve --port 19555
    tools/memory-eval/run.py --home /tmp/eval-home --backend claude [--only ex-01,tm] [--jobs 4]
"""
import argparse
import datetime
import json
import re
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent


class Host:
    def __init__(self, home, port):
        self.base = f"http://127.0.0.1:{port}/api"
        self.token = (Path(home) / "token").read_text().strip()

    def call(self, method, body, timeout=300):
        req = urllib.request.Request(f"{self.base}/{method}", data=json.dumps(body).encode(),
                                     headers={"Authorization": f"Bearer {self.token}",
                                              "content-type": "application/json"})
        try:
            return json.loads(urllib.request.urlopen(req, timeout=timeout).read().decode())
        except urllib.error.HTTPError as e:
            raise RuntimeError(f"{method}: {e.read().decode()}") from e

    def finals(self, bot):
        return [e for e in self.call("history", {"botId": bot})["entries"]
                if e["kind"] == "agent" and e["data"].get("final")]

    def say(self, bot, message, wait=300):
        """Sends a message (text, or {text, pasted}) and returns the bot's reply."""
        body = message if isinstance(message, dict) else {"text": message}
        before = len(self.finals(bot))
        self.call("send", {"botId": bot, "clientNonce": f"eval-{time.time_ns()}", **body})
        deadline = time.time() + wait
        while time.time() < deadline:
            time.sleep(1)
            finals = self.finals(bot)
            if len(finals) > before:
                return finals[-1]["data"].get("text", "")
        raise TimeoutError(f"no reply to {body['text'][:40]!r}")

    def settle_dreams(self, bot, before, wait=90):
        """Waits for dreaming to log a run after a new session, or for it to stay quiet."""
        deadline = time.time() + wait
        while time.time() < deadline:
            time.sleep(3)
            if len(self.call("memory", {"botId": bot})["dreams"]) > before:
                time.sleep(3)  # its file notes follow the run
                return True
        return False


def dates(text, today):
    """Expands {d+N:fmt}, {d-N:fmt}, {next_friday:fmt} and {next_month:fmt} in an expected answer."""
    def day(anchor):
        if anchor == "next_friday":
            return today + datetime.timedelta(days=(7 - today.weekday()) + 4)
        if anchor == "next_month":
            return (today.replace(day=1) + datetime.timedelta(days=32)).replace(day=1)
        return today + datetime.timedelta(days=int(anchor[1:]))
    return re.sub(r"\{([a-z_]+|d[+-]\d+):([^}]+)\}", lambda m: day(m.group(1)).strftime(m.group(2)), text)


def squash(text):
    """Lowercase without whitespace: "3 月 14 日" and "3月14日" are the same answer."""
    return "".join(text.lower().split())


def live_files(view):
    """(layer, name) of every live long-form note the view lists."""
    return [(layer, f["name"]) for layer in ("bot", "yours")
            for f in view.get("files", {}).get(layer, {}).get("files", []) if not f["deleted"]]


def run_case(host, case, backend, today, keep):
    bot = host.call("createBot", {"name": f"eval {case['id']}", "backend": backend, "connectors": [],
                                  "notify": False, "permission": "auto", "sharedMemory": False})["bot"]["id"]
    try:
        for session in case["sessions"]:
            for message in session:
                host.say(bot, message)
            dreams = len(host.call("memory", {"botId": bot})["dreams"])
            host.call("newSession", {"botId": bot})
            host.settle_dreams(bot, dreams)
        reply = host.say(bot, case["probe"])
        view = host.call("memory", {"botId": bot})
        entries = [e["text"] for layer in ("yours", "bot") for e in view[layer]["entries"] if e["state"] == "active"]
        files = live_files(view)
        notes = [host.call("memoryFile", {"botId": bot, "file": name, "shared": layer == "yours"}).get("content") or ""
                 for layer, name in files]
        low = squash(reply)
        expect = [dates(x, today) for x in case.get("expect", [])]
        failures = []
        if expect and not any(squash(x) in low for x in expect):
            failures.append(f"reply lacks any of {expect}")
        failures += [f"reply has {x!r}" for x in case.get("forbid", []) if squash(x) in low]
        remembered = squash("\n".join(entries + notes))
        failures += [f"memory has {x!r}" for x in case.get("memory_forbid", []) if squash(x) in remembered]
        if case.get("expect_file") and not notes:
            failures.append("no long-form note was kept")
        return {"id": case["id"], "kind": case["kind"], "pass": not failures, "failures": failures, "reply": reply,
                "entries": entries, "notes": [name for _, name in files]}
    except Exception as e:  # one case failing to run doesn't stop the others
        return {"id": case["id"], "kind": case["kind"], "pass": False, "failures": [f"error: {e}"]}
    finally:
        if not keep:
            try:
                host.call("deleteBot", {"botId": bot})
            except Exception:
                pass


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--home", required=True, help="the disposable host's CODYNC_HOME (for its token)")
    p.add_argument("--port", type=int, default=19555)
    p.add_argument("--backend", default="claude")
    p.add_argument("--only", default="", help="comma-separated case ids or id prefixes (ex, tm-01, …)")
    p.add_argument("--jobs", type=int, default=3)
    p.add_argument("--keep", action="store_true", help="keep the bots for inspection")
    p.add_argument("--out", default=str(HERE / "results.json"))
    args = p.parse_args()
    cases = json.loads((HERE / "cases.json").read_text())
    if args.only:
        wanted = [w.strip() for w in args.only.split(",") if w.strip()]
        cases = [c for c in cases if any(c["id"] == w or c["id"].startswith(w + "-") or c["id"].startswith(w) for w in wanted)]
    host = Host(args.home, args.port)
    today = datetime.date.today()
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(lambda c: run_case(host, c, args.backend, today, args.keep), cases))
    Path(args.out).write_text(json.dumps(results, ensure_ascii=False, indent=1))
    kinds = {}
    for r in results:
        kinds.setdefault(r["kind"], []).append(r["pass"])
    for kind, passes in kinds.items():
        print(f"{kind:14} {sum(passes)}/{len(passes)}")
    print(f"{'total':14} {sum(r['pass'] for r in results)}/{len(results)}")
    for r in results:
        if not r["pass"]:
            print(f"✗ {r['id']}: {'; '.join(r['failures'])} | {r.get('reply', '')[:120]!r}")
    return 0 if all(r["pass"] for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
