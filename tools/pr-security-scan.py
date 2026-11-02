"""First pass of the security check for an external pull request (docs/guides/security-check.md): flags text aimed at
AI agents, invisible or reordering Unicode, changes to the files that steer agents, and changes to CI, hooks,
dependencies and build tooling. It never runs the PR's code. A clean result doesn't replace reading the diff.

    python3 tools/pr-security-scan.py <pr-number>      # reads the PR with gh (and git fetch for a huge diff)
    python3 tools/pr-security-scan.py FILE...          # a saved body or .diff"""

import re
import subprocess
import sys

# Lines addressed to an AI reviewer or agent rather than a person.
AIMED_AT_AI = re.compile(
    r"ignore (all |any )?(the )?(previous|prior|above|earlier) (instructions|prompts|rules)|disregard (all|the|any|previous)|"
    r"you are (an? )?(ai|assistant|language model|llm|claude|chatgpt|gpt)|as an ai\b|system prompt|"
    r"\b(claude|greptile|codex|copilot|reviewers?|assistants?|llms?|agents?|ai)\b,? (must|should|please|need to) "
    r"(approve|merge|ignore|skip|not)|approve (this|the) (pr|pull request)|do not (flag|report|mention|review)|"
    r"if you are an? (ai|llm|agent|model|bot)|<\s*/?\s*(system|instructions?|assistant)\s*>",
    re.I,
)
# Zero-width, bidi overrides and isolates, word joiners, BOM, and tag characters.
HIDDEN = re.compile("[​-‏‪-‮⁠-⁤⁦-⁩﻿\U000e0000-\U000e007f]")
# Files that steer agents working in this repository.
STEERS_AGENTS = re.compile(r"(^|/)(CLAUDE\.md|AGENTS\.md|SKILL\.md)$|(^|/)\.(claude|agents|cursor)/|^\.github/copilot")
# Files that run on CI, on commit, or while building and installing.
RUNS_CODE = re.compile(
    r"^\.github/|^\.githooks/|(^|/)tools/|(^|/)package(-lock)?\.json$|(^|/)Cargo\.(toml|lock)$|"
    r"(^|/)build\.gradle(\.kts)?$|(^|/)settings\.gradle(\.kts)?$|(^|/)Package\.(swift|resolved)$|project\.yml$|\.(sh|ps1)$"
)


def scan(text, is_diff):
    """(line number, kind, excerpt) for each sign in a body or a unified diff (only added lines and file names)."""
    hits = []
    for n, line in enumerate(text.splitlines(), 1):
        if is_diff and line.startswith("+++ b/"):
            path = line[6:]
            if STEERS_AGENTS.search(path):
                hits.append((n, "changes agent instructions", path))
            elif RUNS_CODE.search(path):
                hits.append((n, "changes CI, hooks, dependencies or build tooling", path))
            continue
        if is_diff and not line.startswith("+"):
            continue
        if HIDDEN.search(line):
            hits.append((n, "invisible or bidi Unicode", repr(line)[:160]))
        if AIMED_AT_AI.search(line):
            hits.append((n, "text aimed at an AI", line.strip()[:160]))
        if not is_diff and "<!--" in line:
            hits.append((n, "HTML comment (hidden when rendered)", line.strip()[:160]))
    return hits


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def pull_request(number):
    """The PR's title, body and commit messages, and its diff against its base."""
    about = run("gh", "pr", "view", number, "--json", "title,body,commits,baseRefName",
                "--jq", '.title + "\\n" + .body + "\\n" + ([.commits[] | .messageHeadline + "\\n" + .messageBody] | join("\\n"))')
    try:
        diff = run("gh", "pr", "diff", number)
    except subprocess.CalledProcessError:
        # GitHub won't serve a diff past 20,000 lines: fetch the head without checking it out.
        base = run("gh", "pr", "view", number, "--json", "baseRefName", "--jq", ".baseRefName").strip()
        run("git", "fetch", "-q", "origin", f"pull/{number}/head:refs/remotes/pr/{number}", base)
        diff = run("git", "diff", f"origin/{base}...pr/{number}")
    return about, diff


def report(name, hits):
    print(f"== {name}: {len(hits)} sign(s)")
    for n, kind, excerpt in hits:
        print(f"  {n}: {kind}: {excerpt}")
    return len(hits)


def main(args):
    if len(args) == 1 and args[0].isdigit():
        about, diff = pull_request(args[0])
        found = report(f"#{args[0]} title, body and commits", scan(about, False))
        found += report(f"#{args[0]} diff", scan(diff, True))
    else:
        found = sum(report(path, scan(open(path, encoding="utf-8", errors="replace").read(), path.endswith(".diff")))
                    for path in args)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]) if sys.argv[1:] else print(__doc__) or 2)
