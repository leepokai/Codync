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

A release whose host asks for a newer iPhone app (MIN_APP) than the last version sent ships even
without iPhone changes.

What's New comes from the version's section in apps/ios/WhatsNew.md, else from the
"## What's New: iPhone" bullets of the PRs merged since the last App Store version (PRS: `gh pr list
--json body,mergeCommit` output); without either, empty fields get a generic line.

Latest version wins: a version still waiting for review is pulled back, renamed and
resubmitted with the new build. A version already in review is left alone.
"""

import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

import jwt

import store_release as store
from store_release import git

APP_ID = "6760984418"
API = "https://api.appstoreconnect.apple.com/v1"
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


def ios_changed(base, tag):
    """Whether anything the iPhone app is built from differs between two refs."""
    if store.changed(base, tag, IOS_PATHS):
        return True
    project = git("diff", "-U0", base, tag, "--", "apps/project.yml").splitlines()
    return any(line[:1] in "+-" and line[:3] not in ("+++", "---") and "_VERSION" not in line
               for line in project)


def app_store_text(text):
    """What's New as the App Store takes it: without emoji (it rejects them) or a line their removal
    left repeating another, within 4000 characters."""
    lines = [EMOJI.sub("", line).rstrip() for line in text.splitlines()]
    return "\n".join(line for i, line in enumerate(lines) if not line or line not in lines[:i]).strip()[:4000]


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
    base = store.release_base(shipped, tag)
    if os.environ.get("FORCE") != "1" and base and not ios_changed(base, tag) and not store.locked_out(shipped, tag):
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

    notes, source = store.notes_for(NOTES, version, "iPhone", base, tag)
    notes = {locale: app_store_text(text) for locale, text in notes.items()}
    print(f"What's New from {source or 'nowhere, generic text'}: {sorted(notes)}")
    if version_id != "dry-run":
        for loc in call("GET", f"/appStoreVersions/{version_id}/appStoreVersionLocalizations")["data"]:
            locale = loc["attributes"]["locale"]
            text = notes.get(locale) or notes.get("en-US")
            if not text and not loc["attributes"].get("whatsNew"):
                text = store.GENERIC_NOTES.get(locale, store.GENERIC_NOTES["en-US"])
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
