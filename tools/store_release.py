"""What the App Store and Google Play submissions share: release notes, the release they compare
with, and change detection. Each store's own rules stay in its script.

Imported by tools/asc-submit.py and tools/play-submit.py (run from tools/, so a plain import works).
"""

import json
import os
import re
import subprocess

# For a release with neither a WhatsNew.md section nor PR bullets.
GENERIC_NOTES = {"en-US": "Bug fixes and improvements.", "zh-Hant": "錯誤修正與改進。"}


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=False).stdout.strip()


def release_base(shipped, tag):
    """The ref a release is compared with: the store's version's tag, else the tag before `tag`."""
    return f"v{shipped}" if shipped and git("tag", "-l", f"v{shipped}") else \
        git("describe", "--tags", "--abbrev=0", "--match", "v*", f"{tag}^")


def changed(base, tag, paths):
    """Whether anything under `paths` (git pathspecs) differs between two refs."""
    return bool(git("diff", "--name-only", base, tag, "--", *paths))


def locked_out(shipped, tag):
    """Whether the host at `tag` turns the store's app version away (its MIN_APP is newer), so the
    app ships even without changes of its own."""
    found = re.search(r'pub const MIN_APP: &str = "([^"]+)";', git("show", f"{tag}:host/src/compat.rs"))
    return bool(shipped and found) and tuple(map(int, found[1].split("."))) > tuple(map(int, shipped.split(".")))


def release_notes(path, version):
    """{locale: text} from the `## <version>` section of a WhatsNew.md, split by `### <locale>` headings."""
    text = path.read_text() if path.exists() else ""
    section = re.search(rf"^## {re.escape(version)}\s*$(.*?)(?=^## |\Z)", text, re.M | re.S)
    if not section:
        return {}
    parts = re.split(r"^### +(\S+)\s*$", section.group(1), flags=re.M)[1:]
    return {loc: body.strip() for loc, body in zip(parts[::2], parts[1::2]) if body.strip()}


def pr_notes(prs, commits, platform):
    """{locale: bullets} from "### <locale>" under "## What's New: <platform>" in the PRs merged
    into this release."""
    notes = {}
    for pr in prs:
        if (pr.get("mergeCommit") or {}).get("oid") not in commits:
            continue
        body = re.sub(r"<!--.*?-->", "", (pr.get("body") or "").replace("\r\n", "\n"), flags=re.S)
        section = re.search(rf"^## What.s New: {platform}[ \t]*$(.*?)(?=^## |\Z)", body, flags=re.M | re.S)
        for loc, text in re.findall(r"^### +(\S+)[ \t]*$(.*?)(?=^### |\Z)", section[1] if section else "",
                                    flags=re.M | re.S):
            # Only filled bullets count: text after the section (a PR footer) is not a note.
            for line in map(str.strip, text.splitlines()):
                if re.match(r"[-*] +\S", line) and line not in notes.setdefault(loc, []):
                    notes[loc].append(line)
    return {loc: "\n".join(lines) for loc, lines in notes.items() if lines}


def notes_for(path, version, platform, base, tag):
    """({locale: text}, source): the version's section in `path`, else the PRs' What's New bullets
    for `platform` merged since `base` (PRS: `gh pr list --json body,mergeCommit` output), else none."""
    if notes := release_notes(path, version):
        return notes, path.name
    if os.environ.get("PRS"):
        with open(os.environ["PRS"]) as prs:
            commits = set(git("rev-list", f"{base}..{tag}" if base else tag).split())
            if notes := pr_notes(json.load(prs), commits, platform):
                return notes, "PRs"
    return {}, None
