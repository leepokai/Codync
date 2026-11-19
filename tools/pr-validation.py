#!/usr/bin/env python3
"""Checks a pull request body's Validation section (.github/pull_request_template.md) is filled in.

PR_BODY=<body> python3 tools/pr-validation.py

Exit 0: every checklist item of the template is there and ticked. Exit 1: prints what is missing.
"""

import os
import pathlib
import re
import sys

TEMPLATE = pathlib.Path(__file__).resolve().parent.parent / ".github/pull_request_template.md"
ITEM = r"^\s*[-*] \[([ xX])\] (.+)$"


def checklist(body):
    """The (box mark, text) of each item in `body`'s Validation section, or None without one."""
    body = re.sub(r"<!--.*?-->", "", body.replace("\r\n", "\n"), flags=re.S)
    section = re.search(r"^## Validation[ \t]*$(.*?)(?=^## |\Z)", body, re.M | re.S)
    return [(mark, text.strip()) for mark, text in re.findall(ITEM, section[1], re.M)] if section else None


def required():
    items = checklist(TEMPLATE.read_text())
    if items is None:
        sys.exit(f"::error::{TEMPLATE.name} has no `## Validation` section to check against.")
    return [text for _, text in items]


def problems(body):
    items = checklist(body)
    if items is None:
        return ["The `## Validation` section of the PR template is missing."]
    unticked = [text for mark, text in items if mark == " "]
    found = [f"Unticked: {text} (tick it, adding \"N/A: <why>\" when it doesn't apply)" for text in unticked]
    need = required()
    # Counted, not matched word for word: rewording an item in the template doesn't fail the PRs already open.
    if len(items) < len(need):
        missing = [item for item in need if not any(text.startswith(item) for _, text in items)]
        found.append(f"Missing: {'; '.join(missing) or 'an item'} (keep every checklist item of the template)")
    return found


def main():
    found = problems(os.environ.get("PR_BODY", ""))
    for line in found:
        print(f"::error::{line}")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    main()
