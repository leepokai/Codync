#!/usr/bin/env python3
"""Checks a pull request body's Validation section (.github/pull_request_template.md) is filled in.

PR_BODY=<body> python3 tools/pr-validation.py

Exit 0: every checklist box is ticked. Exit 1: prints what is missing.
"""

import os
import re
import sys


def problems(body):
    body = re.sub(r"<!--.*?-->", "", body.replace("\r\n", "\n"), flags=re.S)
    section = re.search(r"^## Validation[ \t]*$(.*?)(?=^## |\Z)", body, re.M | re.S)
    if not section:
        return ["The `## Validation` section of the PR template is missing."]
    unticked = re.findall(r"^\s*[-*] \[ \] (.+)$", section[1], re.M)
    return [f"Unticked: {item.strip()} (tick it, adding \"N/A: <why>\" when it doesn't apply)" for item in unticked]


def main():
    found = problems(os.environ.get("PR_BODY", ""))
    for line in found:
        print(f"::error::{line}")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    main()
