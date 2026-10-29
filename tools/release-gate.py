#!/usr/bin/env python3
"""Point every download at the newest release the App Store's iPhone app works with.

Usage: release-gate.py
Env: GH_TOKEN (gh: reads the releases, marks the latest), TAP_TOKEN (pushes the Homebrew tap);
DRY_RUN=1 prints what it would do instead; ALLOW_BACK=1 may mark an older release than the latest.
Runs in a checkout with the release tags.

Releases are published without GitHub's "latest" mark. The website's download links,
packaging/install.sh, the desktop app's and the host's updaters all read the latest release, and the
Homebrew tap follows it, so this decides what anyone can get: the newest complete release whose
host serves the iPhone app the App Store has (its `MIN_APP`). A newer one waits for App Review;
offered earlier, a computer on it would tell every paired iPhone to update to a version still in
review. The mark only moves forward on its own: an App Store lookup that briefly answers with an
older version doesn't take downloads back. Runs after each release, and every 5 minutes while
a newer release waits or the tap trails the latest one (.github/workflows/release-gate.yml, dispatched by
cloud/src/release-gate.ts).
"""

import json
import os
import re
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

REPO = "leepokai/Codync"
TAP = "leepokai/homebrew-codync"
APP_STORE_LOOKUP = "https://itunes.apple.com/lookup?id=6760984418"
DRY_RUN = os.environ.get("DRY_RUN") == "1"
# In the order of the formula template's sha256 lines.
HOST_PLATFORMS = ("macos-arm64", "macos-x86_64", "linux-arm64", "linux-x86_64")
# Each platform's update feed goes up after the files it points at, so with all of them there the
# release is downloadable everywhere.
FEEDS = {"latest-mac.yml", "latest.yml", "latest-linux.yml", "latest-linux-arm64.yml"} | {
    f"codync-host-{p}.update.json" for p in HOST_PLATFORMS}


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, stdout=subprocess.PIPE, text=True).stdout.strip()


def gh_json(*args):
    return json.loads(run("gh", *args, "--repo", REPO))


def parse(version):
    """`[major, minor, patch]` of `2.19.1` or `v2.19.1`; None for anything else (a pre-release)."""
    match = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", version)
    return [int(n) for n in match.groups()] if match else None


def app_store_version():
    # Apple's CDN keeps the bare URL for up to a day after a release; a query string of its own skips it.
    with urllib.request.urlopen(f"{APP_STORE_LOOKUP}&_={time.time_ns()}", timeout=20) as response:
        results = json.load(response)["results"]
    return results[0]["version"] if results else None


def min_app(tag):
    """The iPhone app the tag's host asks for. Fails rather than guess: the latest mark stays."""
    source = run("git", "show", f"{tag}:host/src/compat.rs")
    return re.search(r'pub const MIN_APP: &str = "([^"]+)";', source).group(1)


def complete(tag):
    names = {a["name"] for a in gh_json("release", "view", tag, "--json", "assets")["assets"]}
    missing = FEEDS - names
    if missing:
        print(f"{tag} isn't fully uploaded yet (no {', '.join(sorted(missing))})")
    return not missing


def choose(store, releases, min_app=min_app, complete=complete):
    """The newest of `releases` (published tags) that is complete and whose MIN_APP the App Store
    version meets."""
    for tag in sorted((t for t in releases if parse(t)), key=parse, reverse=True):
        needs = min_app(tag)
        if parse(needs) > parse(store):
            print(f"{tag} needs iPhone {needs}; the App Store has {store}")
        elif complete(tag):
            return tag
    return None


def tap(tag):
    """The Mac app's cask and the host's formula, at `tag`, from that release's templates."""
    version = tag.removeprefix("v")
    show = lambda path: run("git", "show", f"{tag}:{path}") + "\n"
    with tempfile.TemporaryDirectory() as tmp:
        run("gh", "release", "download", tag, "--repo", REPO, "--pattern", "*.sha256", "--dir", tmp)

        def filled(template, *archives):
            """`template` at `version`, its sha256 placeholders filled with those of `archives`, in order."""
            text = re.sub(r'(?m)^  version "[^"]*"', f'  version "{version}"', show(template))
            for name in archives:
                text = text.replace("REPLACE_WITH_SHA256", Path(tmp, f"{name}.sha256").read_text().split()[0], 1)
            return text

        files = {
            "Casks/codync.rb": filled("packaging/homebrew/codync.rb", "codync-macos.dmg"),
            "Formula/codync-host.rb": filled("packaging/homebrew/codync-host.rb",
                                             *(f"codync-host-{p}.tar.gz" for p in HOST_PLATFORMS)),
            "README.md": show("packaging/homebrew/README.md"),
        }
        checkout = Path(tmp, "tap")
        auth = f"x-access-token:{os.environ['TAP_TOKEN']}@" if os.environ.get("TAP_TOKEN") else ""
        run("git", "clone", "--depth", "1", f"https://{auth}github.com/{TAP}.git", str(checkout))
        for path, text in files.items():
            target = Path(checkout, path)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
        if not run("git", "status", "--porcelain", cwd=checkout):
            return
        print(f"Homebrew tap → {version}")
        if DRY_RUN:
            print(run("git", "diff", cwd=checkout))
            return
        run("git", "add", "-A", cwd=checkout)
        run("git", "-c", "user.name=github-actions[bot]",
            "-c", "user.email=github-actions[bot]@users.noreply.github.com",
            "commit", "-m", f"Codync {version}", cwd=checkout)
        run("git", "push", cwd=checkout)


def main():
    store = app_store_version()
    if not parse(store or ""):
        print(f"::warning::The App Store lookup gave no version ({store!r}); the latest release stays as it is")
        return
    releases = gh_json("release", "list", "--limit", "30", "--exclude-drafts", "--exclude-pre-releases",
                       "--json", "tagName,isLatest")
    current = next((r["tagName"] for r in releases if r["isLatest"]), None)
    tag = choose(store, [r["tagName"] for r in releases])
    if not tag:
        print(f"::warning::No release works with iPhone {store}; the latest release stays as it is")
        return
    if current and parse(tag) < parse(current) and os.environ.get("ALLOW_BACK") != "1":
        print(f"::warning::{current} needs a newer iPhone app than the App Store's {store}; {tag} becomes latest "
              "only on a run with allow_back (Actions → Release gate → Run workflow)")
        return
    print(f"App Store: {store}; latest release: {current} → {tag}")
    # The mark first: a tap that fails mustn't hold everyone's downloads back. The Worker runs the gate
    # again while the tap trails the mark.
    if tag != current and not DRY_RUN:
        run("gh", "release", "edit", tag, "--repo", REPO, "--latest")
    tap(tag)


if __name__ == "__main__":
    main()
