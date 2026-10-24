#!/usr/bin/env python3
"""Submit the iOS build of a version for App Review once Xcode Cloud has uploaded it.

Usage: asc-submit.py 2.3.0
Env: ASC_KEY_ID, ASC_ISSUER_ID, ASC_PRIVATE_KEY (.p8 contents); DRY_RUN=1 prints writes instead;
SINCE (ISO 8601) ignores builds uploaded before it, so a tag run never submits an older build.
Runs in a checkout with the release tags.

A release whose tag changes nothing the iPhone app is built from since the last version sent
to App Review is skipped (FORCE=1 ships it anyway). Otherwise, when SINCE is set or no build of
the version exists yet, it starts the Xcode Cloud archive on the tag (or on main while main is the
tagged commit), then waits for the build.

What's New comes from the version's section in apps/ios/WhatsNew.md, else from the
"## What's New" bullets of the PRs merged since the last App Store version (PRS: `gh pr list
--json body,mergeCommit` output); without either, empty fields get a generic line.

Latest version wins: a version still waiting for review is pulled back, renamed and
resubmitted with the new build. A version already in review is left alone.
"""

import json
import subprocess
import os
import sys
import time
import urllib.error
import re
import urllib.request
from datetime import datetime
from pathlib import Path

import jwt

APP_ID = "6760984418"
API = "https://api.appstoreconnect.apple.com/v1"
WHATS_NEW = {"en-US": "Bug fixes and improvements.", "zh-Hant": "錯誤修正與改進。"}
UNSENT = {"PREPARE_FOR_SUBMISSION", "READY_FOR_REVIEW", "DEVELOPER_REJECTED"}  # never sent, or pulled back
EDITABLE = UNSENT | {"REJECTED", "METADATA_REJECTED", "INVALID_BINARY"}
DONE = {"READY_FOR_DISTRIBUTION", "REPLACED_WITH_NEW_VERSION", "REMOVED_FROM_SALE"}
DRY_RUN = os.environ.get("DRY_RUN") == "1"
NOTES = Path(__file__).resolve().parent.parent / "apps/ios/WhatsNew.md"
EMOJI = re.compile("[\U00010000-\U0010FFFF\u2600-\u27BF\uFE0F\u200D]")
WORKFLOW_ID = "44342080-ce08-4dc1-be35-34ce6cbb41b5"  # Xcode Cloud "Release": Archive iOS for the App Store
# What the iPhone app is built from; apps/project.yml counts except its version lines.
# The App Store notes and the Kit tests never reach the binary.
# ponytail: project.yml's macOS Screen targets count too; diff only its iOS targets if that misfires.
IOS_PATHS = ["apps/ios", "apps/shared", "apps/Codync.xcodeproj/project.xcworkspace",
             ":(exclude)apps/ios/WhatsNew.md", ":(exclude)apps/ios/Kit/Tests"]


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=False).stdout.strip()


def ios_changed(base, tag):
    """Whether anything the iPhone app is built from differs between two refs."""
    if git("diff", "--name-only", base, tag, "--", *IOS_PATHS):
        return True
    project = git("diff", "-U0", base, tag, "--", "apps/project.yml").splitlines()
    return any(line[:1] in "+-" and line[:3] not in ("+++", "---") and "_VERSION" not in line
               for line in project)


def release_notes(version):
    """{locale: text} from the `## <version>` section, split by `### <locale>` headings, without
    emoji (the App Store rejects them)."""
    text = EMOJI.sub("", NOTES.read_text()) if NOTES.exists() else ""
    section = re.search(rf"^## {re.escape(version)}\s*$(.*?)(?=^## |\Z)", text, re.M | re.S)
    if not section:
        return {}
    parts = re.split(r"^### +(\S+)\s*$", section.group(1), flags=re.M)[1:]
    return {loc: body.strip() for loc, body in zip(parts[::2], parts[1::2]) if body.strip()}


def pr_notes(prs, commits):
    """{locale: bullets} from "### <locale>" under "## What's New" in the PRs merged into this release."""
    notes = {}
    for pr in prs:
        if (pr.get("mergeCommit") or {}).get("oid") not in commits:
            continue
        body = re.sub(r"<!--.*?-->", "", (pr.get("body") or "").replace("\r\n", "\n"), flags=re.S)
        section = re.search(r"^## What.s New[ \t]*$(.*?)(?=^## |\Z)", body, flags=re.M | re.S)
        for loc, text in re.findall(r"^### +(\S+)[ \t]*$(.*?)(?=^### |\Z)", section[1] if section else "",
                                    flags=re.M | re.S):
            # Only filled bullets count: text after the section (a PR footer) is not a note,
            # and the App Store rejects emoji in What's New.
            for line in map(str.strip, text.splitlines()):
                line = EMOJI.sub("", line).strip()
                if re.match(r"[-*] +\S", line) and line not in notes.setdefault(loc, []):
                    notes[loc].append(line)
    return {loc: "\n".join(lines)[:4000] for loc, lines in notes.items() if lines}


def token():
    now = int(time.time())
    return jwt.encode(
        {"iss": os.environ["ASC_ISSUER_ID"], "iat": now, "exp": now + 1200, "aud": "appstoreconnect-v1"},
        os.environ["ASC_PRIVATE_KEY"], algorithm="ES256", headers={"kid": os.environ["ASC_KEY_ID"]})


def call(method, path, body=None):
    if method != "GET" and DRY_RUN:
        print(f"DRY_RUN {method} {path} {json.dumps(body)}")
        return {"data": {"id": "dry-run"}}
    req = urllib.request.Request(
        API + path, method=method, data=json.dumps(body).encode() if body else None,
        headers={"Authorization": f"Bearer {token()}", "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req) as res:
            raw = res.read()
    except urllib.error.HTTPError as e:
        sys.exit(f"{method} {path} -> {e.code}\n{e.read().decode()}")
    return json.loads(raw) if raw else {}


def wait_for(what, check, minutes):
    for _ in range(minutes):
        if result := check():
            return result
        print(f"waiting for {what}…", flush=True)
        time.sleep(60)
    sys.exit(f"timed out waiting for {what}")


def all_pages(path):
    items = []
    while path:
        page = call("GET", path)
        items += page["data"]
        path = page.get("links", {}).get("next", "").removeprefix(API)
    return items


def store_versions():
    """The app's last ten iOS App Store versions with their states."""
    return call("GET", f"/apps/{APP_ID}/appStoreVersions?filter[platform]=IOS&limit=10"
                "&fields[appStoreVersions]=versionString,appVersionState")["data"]


def last_sent(versions, version):
    """The newest other version sent to App Review. One never sent or pulled back (such as one a
    failed run left behind) doesn't count, so its changes still ship; a rejected one does, so a
    release without iPhone changes doesn't resubmit the rejected code."""
    return next((v["attributes"]["versionString"] for v in versions
                 if v["attributes"]["versionString"] != version
                 and v["attributes"]["appVersionState"] not in UNSENT), None)


def archive_ref(tag, refs):
    """Which of Xcode Cloud's refs ({canonical name: id}) to archive `tag` from: the tag, else main
    while main is the tagged commit. Xcode Cloud hears of a new tag through a GitHub webhook that
    sometimes never comes (v2.20.4)."""
    if f"refs/tags/{tag}" in refs:
        return f"refs/tags/{tag}"
    sha = git("rev-list", "-n", "1", tag)
    main = git("ls-remote", "origin", "refs/heads/main").split("\t")[0]
    return "refs/heads/main" if sha and sha == main and "refs/heads/main" in refs else None


def start_archive(tag):
    repo = call("GET", f"/ciWorkflows/{WORKFLOW_ID}/repository")["data"]["id"]

    def source():
        refs = {r["attributes"]["canonicalName"]: r["id"]
                for r in all_pages(f"/scmRepositories/{repo}/gitReferences?limit=200")}
        name = archive_ref(tag, refs)
        return name and (name, refs[name])

    name, ref = wait_for(f"Xcode Cloud to see {tag}", source, 15)
    run = call("POST", "/ciBuildRuns", {"data": {"type": "ciBuildRuns", "relationships": {
        "workflow": {"data": {"type": "ciWorkflows", "id": WORKFLOW_ID}},
        "sourceBranchOrTag": {"data": {"type": "scmGitReferences", "id": ref}}}}})["data"]
    print(f"started Xcode Cloud build {run.get('attributes', {}).get('number', run['id'])} on {name}")


def main(version):
    tag = f"v{version}"
    since = datetime.fromisoformat(os.environ.get("SINCE", "2000-01-01T00:00:00Z"))
    shipped = last_sent(store_versions(), version)
    base = f"v{shipped}" if shipped and git("tag", "-l", f"v{shipped}") else \
        git("describe", "--tags", "--abbrev=0", "--match", "v*", f"{tag}^")
    if os.environ.get("FORCE") != "1" and base and not ios_changed(base, tag):
        print(f"nothing the iPhone app is built from changed since {base}; not shipping iOS {version}")
        return
    print(f"iOS changes since {base or 'the start'}")

    builds = call("GET", f"/builds?filter[app]={APP_ID}&filter[preReleaseVersion.version]={version}&limit=1")
    if "SINCE" in os.environ or not builds["data"]:
        start_archive(tag)
    build = wait_for(f"build {version} to finish processing", lambda: next(iter(
        b for b in call("GET", f"/builds?filter[app]={APP_ID}&filter[preReleaseVersion.version]={version}"
                        "&filter[processingState]=VALID&sort=-uploadedDate&limit=1")["data"]
        if datetime.fromisoformat(b["attributes"]["uploadedDate"]) >= since), None), 150)
    print(f"build {build['attributes']['version']} ({build['id']})")

    def in_flight():
        return next((v for v in store_versions() if v["attributes"]["appVersionState"] not in DONE), None)

    current = in_flight()
    state = current and current["attributes"]["appVersionState"]
    # An invalid binary leaves its submission waiting too; pull it back so the new build can go.
    if state in ("WAITING_FOR_REVIEW", "INVALID_BINARY"):
        for s in call("GET", f"/reviewSubmissions?filter[app]={APP_ID}&filter[platform]=IOS"
                      "&filter[state]=WAITING_FOR_REVIEW")["data"]:
            print(f"pulling {current['attributes']['versionString']} ({state}) back from review")
            call("PATCH", f"/reviewSubmissions/{s['id']}",
                 {"data": {"type": "reviewSubmissions", "id": s["id"], "attributes": {"canceled": True}}})
        if not DRY_RUN:
            current = wait_for("the pulled version to become editable",
                               lambda: (v := in_flight()) and v["attributes"]["appVersionState"] in EDITABLE and v, 30)
            state = current["attributes"]["appVersionState"]
    if current and state not in EDITABLE and not DRY_RUN:
        print(f"{current['attributes']['versionString']} is {state}; leaving it alone. "
              "Rerun this workflow once it is released.")
        return

    if current:
        if current["attributes"]["versionString"] != version:
            call("PATCH", f"/appStoreVersions/{current['id']}", {"data": {
                "type": "appStoreVersions", "id": current["id"], "attributes": {"versionString": version}}})
        version_id = current["id"]
    else:
        version_id = call("POST", "/appStoreVersions", {"data": {
            "type": "appStoreVersions",
            "attributes": {"platform": "IOS", "versionString": version, "releaseType": "AFTER_APPROVAL"},
            "relationships": {"app": {"data": {"type": "apps", "id": APP_ID}}}}})["data"]["id"]

    notes, source = release_notes(version), "WhatsNew.md"
    if not notes and os.environ.get("PRS"):
        with open(os.environ["PRS"]) as prs:
            commits = set(git("rev-list", f"{base}..{tag}" if base else tag).split())
            notes, source = pr_notes(json.load(prs), commits), "PRs"
    print(f"What's New from {source}: {sorted(notes) or 'none, generic text'}")
    if version_id != "dry-run":
        for loc in call("GET", f"/appStoreVersions/{version_id}/appStoreVersionLocalizations")["data"]:
            locale = loc["attributes"]["locale"]
            text = notes.get(locale) or notes.get("en-US")
            if not text and not loc["attributes"].get("whatsNew"):
                text = WHATS_NEW.get(locale, WHATS_NEW["en-US"])
            if text:
                call("PATCH", f"/appStoreVersionLocalizations/{loc['id']}", {"data": {
                    "type": "appStoreVersionLocalizations", "id": loc["id"], "attributes": {"whatsNew": text}}})

    call("PATCH", f"/appStoreVersions/{version_id}/relationships/build",
         {"data": {"type": "builds", "id": build["id"]}})

    submissions = call("GET", f"/reviewSubmissions?filter[app]={APP_ID}&filter[platform]=IOS"
                       "&filter[state]=READY_FOR_REVIEW,UNRESOLVED_ISSUES")["data"]
    submission_id = submissions[0]["id"] if submissions else call("POST", "/reviewSubmissions", {"data": {
        "type": "reviewSubmissions", "attributes": {"platform": "IOS"},
        "relationships": {"app": {"data": {"type": "apps", "id": APP_ID}}}}})["data"]["id"]
    items = [] if submission_id == "dry-run" else call(
        "GET", f"/reviewSubmissions/{submission_id}/items?include=appStoreVersion")["data"]
    if not any(i["relationships"]["appStoreVersion"]["data"] for i in items):
        call("POST", "/reviewSubmissionItems", {"data": {"type": "reviewSubmissionItems", "relationships": {
            "reviewSubmission": {"data": {"type": "reviewSubmissions", "id": submission_id}},
            "appStoreVersion": {"data": {"type": "appStoreVersions", "id": version_id}}}}})
    call("PATCH", f"/reviewSubmissions/{submission_id}",
         {"data": {"type": "reviewSubmissions", "id": submission_id, "attributes": {"submitted": True}}})
    print(f"submitted {version} ({build['attributes']['version']}) for review")


if __name__ == "__main__":
    main(sys.argv[1])
