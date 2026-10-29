#!/usr/bin/env python3
"""Upload the Android app of a version to a Google Play track.

Usage: play-submit.py check 2.25.0                 prints ship=true|false (also to $GITHUB_OUTPUT)
       play-submit.py upload 2.25.0 app-release.aab
Env: PLAY_SERVICE_ACCOUNT (the codync-play service account's JSON key); TRACK (internal, alpha for
closed testing, production; default internal); STATUS (completed, or draft to finish the release in
Play Console); FORCE=1 ships without Android changes; PRS as in asc-submit.py; DRY_RUN=1 prints
writes instead. Runs in a checkout with the release tags.

A release whose tag changes nothing the Android app is built from since the version on the track
is skipped, unless the track is empty or the tag's host asks for a newer app (MIN_APP). The version
code comes from the version (apps/android/app/build.gradle.kts), so the track's version code names
the release it was built from; a version Play already has (from another track) moves to TRACK
instead of being uploaded again. Release notes come from apps/android/WhatsNew.md, else the PRs'
"## What's New: Android" bullets, zh-Hant as zh-TW, cut to Play's 500 characters.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import jwt

import store_release as store

PACKAGE = "com.pokai.Codync.android"
API = f"https://androidpublisher.googleapis.com/androidpublisher/v3/applications/{PACKAGE}"
UPLOAD = f"https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/{PACKAGE}"
TRACK = os.environ.get("TRACK") or "internal"
STATUS = os.environ.get("STATUS") or "completed"
DRY_RUN = os.environ.get("DRY_RUN") == "1"
LOCALES = {"en-US": "en-US", "zh-Hant": "zh-TW"}  # WhatsNew.md locale → Play language
NOTES = Path(__file__).resolve().parent.parent / "apps/android/WhatsNew.md"
# What the Android app is built from: its sources, the shared account configuration, and the
# strings and push relay it takes from the iPhone kit. Tests, the emulator-only release acceptance
# build and the Play notes never reach the bundle.
ANDROID_PATHS = ["apps/android", "apps/shared", "apps/ios/Kit/Sources/CodyncKit/Resources/Languages",
                 "apps/ios/Kit/Sources/CodyncKit/Client/SharedStore.swift",
                 ":(exclude)apps/android/*/src/test/*", ":(exclude)apps/android/*/src/androidTest/*",
                 ":(exclude)apps/android/app/src/releaseAcceptance", ":(exclude)apps/android/README.md",
                 ":(exclude)apps/android/WhatsNew.md"]


def version_code(version):
    """Must match `versionCode` in apps/android/app/build.gradle.kts."""
    major, minor, patch = map(int, version.split("."))
    return major * 1_000_000 + minor * 1_000 + patch


def version_name(code):
    return f"{code // 1_000_000}.{code // 1_000 % 1_000}.{code % 1_000}"


def play_notes(notes):
    """Play's releaseNotes for the locales it lists, each cut at its last whole line within 500
    characters."""
    out = []
    for locale, language in LOCALES.items():
        text = notes.get(locale)
        if not text:
            continue
        while len(text) > 500 and "\n" in text:
            text = text.rsplit("\n", 1)[0]
        out.append({"language": language, "text": text[:500]})
    return out


def token():
    account = json.loads(os.environ["PLAY_SERVICE_ACCOUNT"])
    now = int(time.time())
    assertion = jwt.encode({"iss": account["client_email"], "aud": account["token_uri"], "iat": now,
                            "exp": now + 3600, "scope": "https://www.googleapis.com/auth/androidpublisher"},
                           account["private_key"], algorithm="RS256")
    body = urllib.parse.urlencode({"grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
                                   "assertion": assertion}).encode()
    with urllib.request.urlopen(urllib.request.Request(account["token_uri"], data=body)) as res:
        return json.load(res)["access_token"]


def call(method, url, body=None, data=None, content_type="application/json"):
    # A dry run still opens (and drops) an edit to read the track, but changes nothing in it.
    if DRY_RUN and method in ("POST", "PUT") and "/edits/" in url:
        print(f"DRY_RUN {method} {url} {json.dumps(body) if body else ''}")
        return {}
    req = urllib.request.Request(
        url, method=method, data=data if data is not None else json.dumps(body).encode() if body else None,
        headers={"Authorization": f"Bearer {call.token}", "Content-Type": content_type})
    try:
        with urllib.request.urlopen(req, timeout=600) as res:
            raw = res.read()
    except urllib.error.HTTPError as e:
        sys.exit(f"{method} {url} -> {e.code}\n{e.read().decode()}")
    return json.loads(raw) if raw else {}


def track_version(edit):
    """The newest version on TRACK, from its version codes."""
    releases = call("GET", f"{API}/edits/{edit}/tracks/{TRACK}").get("releases", [])
    codes = [int(code) for release in releases for code in release.get("versionCodes", [])]
    return version_name(max(codes)) if codes else None


def should_ship(version, edit):
    tag = f"v{version}"
    shipped = track_version(edit)
    base = store.release_base(shipped, tag)
    if os.environ.get("FORCE") != "1" and shipped and base and not store.changed(base, tag, ANDROID_PATHS) \
            and not store.locked_out(shipped, tag):
        print(f"nothing the Android app is built from changed since {base}; not shipping Android {version}")
        return False
    print(f"shipping Android {version} (on {TRACK}: {shipped or 'nothing'})")
    return True


def upload(version, bundle, edit):
    code = version_code(version)
    if code in (b["versionCode"] for b in call("GET", f"{API}/edits/{edit}/bundles").get("bundles", [])):
        print(f"Play already has {version} ({code}); moving it to {TRACK}")
    else:
        with open(bundle, "rb") as file:
            uploaded = call("POST", f"{UPLOAD}/edits/{edit}/bundles?uploadType=media", data=file.read(),
                            content_type="application/octet-stream")
        if not DRY_RUN and uploaded.get("versionCode") != code:
            sys.exit(f"{bundle} has version code {uploaded.get('versionCode')}, expected {code} for {version}")

    # Notes cover everything since the track's version, the same base `check` compared with.
    tag = f"v{version}"
    base = store.release_base(track_version(edit), tag)
    notes, source = store.notes_for(NOTES, version, "Android", base, tag)
    if not notes:
        notes, source = store.GENERIC_NOTES, "the generic line"
    print(f"release notes from {source}: {sorted(notes)}")

    call("PUT", f"{API}/edits/{edit}/tracks/{TRACK}", {"track": TRACK, "releases": [{
        "name": version, "versionCodes": [str(code)], "status": STATUS, "releaseNotes": play_notes(notes)}]})
    if DRY_RUN:
        call("DELETE", f"{API}/edits/{edit}")
        return
    call("POST", f"{API}/edits/{edit}:commit")
    print(f"{version} ({code}) is on {TRACK} as {STATUS}")


def main(command, version, bundle=None):
    call.token = token()
    edit = call("POST", f"{API}/edits")["id"]
    if command == "check":
        ship = should_ship(version, edit)
        call("DELETE", f"{API}/edits/{edit}")
        print(f"ship={str(ship).lower()}")
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as out:
                out.write(f"ship={str(ship).lower()}\n")
    elif command == "upload":
        upload(version, bundle, edit)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(*sys.argv[1:])
