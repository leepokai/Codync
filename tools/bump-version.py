#!/usr/bin/env python3
"""Set the product version everywhere it lives, after checking they all agree.

    tools/bump-version.py patch | minor | X.Y.Z
    tools/bump-version.py check

The places: apps/project.yml (Android reads it too) and the Xcode project generated from it, host/Cargo.toml and its
Cargo.lock entry, apps/desktop/package.json and package-lock.json. Commit them alone as
`build: bump version to X.Y.Z` (when to bump: CLAUDE.md, "Versioning"). Never a major bump.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
V = r"(\d+\.\d+\.\d+)"
# Each pattern's last group is the version; `count` is how often it occurs (None: one or more, all equal).
SPOTS = [
    ("apps/project.yml", rf"^(\s*MARKETING_VERSION: ){V}$", 1),
    ("apps/Codync.xcodeproj/project.pbxproj", rf"^(\s*MARKETING_VERSION = ){V};$", None),
    ("host/Cargo.toml", rf'^(version = "){V}"$', 1),
    ("host/Cargo.lock", rf'^(name = "codync-host"\nversion = "){V}"$', 1),
    ("apps/desktop/package.json", rf'^(  "version": "){V}",$', 1),
    ("apps/desktop/package-lock.json", rf'^(\s*"name": "codync-desktop",\n\s*"version": "){V}",$', 2),
]


def found() -> dict[str, list[str]]:
    versions = {}
    for path, pattern, count in SPOTS:
        hits = re.findall(pattern, (ROOT / path).read_text(), re.M)
        if not hits or (count is not None and len(hits) != count):
            sys.exit(f"{path}: expected {count or 'some'} version line(s), found {len(hits)}")
        versions[path] = [h[-1] for h in hits]
    return versions


def current() -> str:
    versions = found()
    distinct = {v for vs in versions.values() for v in vs}
    if len(distinct) != 1:
        sys.exit("versions differ:\n" + "\n".join(f"  {p}: {', '.join(vs)}" for p, vs in versions.items()))
    return distinct.pop()


def next_version(now: str, arg: str) -> str:
    major, minor, patch = map(int, now.split("."))
    if arg == "patch":
        return f"{major}.{minor}.{patch + 1}"
    if arg == "minor":
        return f"{major}.{minor + 1}.0"
    if not re.fullmatch(V, arg):
        sys.exit(f"not patch, minor or X.Y.Z: {arg}")
    new = tuple(map(int, arg.split(".")))
    if new[0] != major:
        sys.exit("never a major bump (stay on the current major)")
    if new <= (major, minor, patch):
        sys.exit(f"{arg} is not above {now}")
    return arg


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    now = current()
    if sys.argv[1] == "check":
        print(now)
        return
    new = next_version(now, sys.argv[1])
    for path, pattern, _ in SPOTS:
        file = ROOT / path
        file.write_text(re.sub(pattern, lambda m: m.group(0).replace(m.group(len(m.groups())), new), file.read_text(), flags=re.M))
    if current() != new:
        sys.exit("the bump left versions behind")
    print(f"{now} -> {new}\ncommit: git commit -m 'build: bump version to {new}' -- {' '.join(p for p, _, _ in SPOTS)}")


if __name__ == "__main__":
    main()
