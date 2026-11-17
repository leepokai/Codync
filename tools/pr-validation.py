#!/usr/bin/env python3
"""Checks a pull request body's Validation section (.github/pull_request_template.md) is filled in.

PR_BODY=<body> python3 tools/pr-validation.py

Exit 0: every checklist box is ticked and "What I tried" describes what was run. Exit 1: prints what is missing.
"""

import os
import re
import sys

MIN_DESCRIPTION = 40  # characters of "What I tried", past which it is more than "tested it"


def problems(body):
    body = re.sub(r"<!--.*?-->", "", body.replace("\r\n", "\n"), flags=re.S)
    section = re.search(r"^## Validation[ \t]*$(.*?)(?=^## |\Z)", body, re.M | re.S)
    if not section:
        return ["The `## Validation` section of the PR template is missing."]
    validation = section[1]
    found = []
    unticked = re.findall(r"^\s*[-*] \[ \] (.+)$", validation, re.M)
    found += [f"Unticked: {item.strip()} (tick it, adding \"N/A: <why>\" when it doesn't apply)" for item in unticked]
    tried = re.search(r"^### What I tried[ \t]*$(.*)", validation, re.M | re.S)
    if not tried:
        found.append("The `### What I tried` heading is missing.")
    elif len(re.sub(r"\s+", "", tried[1])) < MIN_DESCRIPTION:
        found.append("`### What I tried` doesn't describe what you ran (commands, devices, flows and results).")
    return found


def main():
    found = problems(os.environ.get("PR_BODY", ""))
    for line in found:
        print(f"::error::{line}")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    main()
