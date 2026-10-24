#!/usr/bin/env python3
"""Upload an iPhone release's dSYMs to PostHog, so its crash reports show function, file and line.

Usage: upload-dsyms.py v2.19.2 (a release tag or its commit)
Env: ASC_KEY_ID, ASC_ISSUER_ID, ASC_PRIVATE_KEY (as asc-submit.py); POSTHOG_CLI_API_KEY (a personal
API key with error tracking write) and POSTHOG_CLI_PROJECT_ID for posthog-cli, which must be on PATH.
macOS only: posthog-cli reads dSYMs with Xcode's dwarfdump. Runs in a checkout with the release tags.

Finds the Xcode Cloud "Release" run on that commit, waits for it, downloads its archive and uploads
the dSYMs inside. A release that didn't build the iPhone app has no run and nothing to upload.
"""

import importlib.util
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("asc_submit", Path(__file__).with_name("asc-submit.py"))
asc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(asc)


def release_run(sha):
    """The newest Xcode Cloud "Release" run on `sha`, or None."""
    runs = asc.call("GET", f"/ciWorkflows/{asc.WORKFLOW_ID}/buildRuns?sort=-number&limit=30"
                    "&fields[ciBuildRuns]=number,sourceCommit,executionProgress,completionStatus")["data"]
    return next((r for r in runs if (r["attributes"].get("sourceCommit") or {}).get("commitSha") == sha), None)


def archive_url(run_id):
    """Where the run's .xcarchive (zipped) can be downloaded."""
    for action in asc.call("GET", f"/ciBuildRuns/{run_id}/actions")["data"]:
        for artifact in asc.call("GET", f"/ciBuildActions/{action['id']}/artifacts")["data"]:
            if artifact["attributes"]["fileType"] == "ARCHIVE":
                return asc.call("GET", f"/ciArtifacts/{artifact['id']}")["data"]["attributes"]["downloadUrl"]
    sys.exit(f"Xcode Cloud run {run_id} has no archive")


def main(ref):
    sha = asc.git("rev-list", "-n", "1", ref)
    if not sha:
        sys.exit(f"{ref} isn't a commit in this checkout")
    run = release_run(sha)
    if not run:
        print(f"no Xcode Cloud archive of {ref} among the latest 30; nothing to upload")
        return
    number = run["attributes"]["number"]

    def finished():
        attributes = asc.call("GET", f"/ciBuildRuns/{run['id']}")["data"]["attributes"]
        return attributes if attributes["executionProgress"] == "COMPLETE" else None

    status = asc.wait_for(f"Xcode Cloud build {number}", finished, 120)["completionStatus"]
    if status != "SUCCEEDED":
        sys.exit(f"Xcode Cloud build {number} {status.lower()}; no dSYMs to upload")

    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp, "archive.zip")
        urllib.request.urlretrieve(archive_url(run["id"]), archive)
        dsyms = Path(tmp, "dSYMs")
        with zipfile.ZipFile(archive) as z:
            z.extractall(dsyms, [n for n in z.namelist() if ".dSYM/" in n])
        names = sorted(p.name for p in dsyms.rglob("*.dSYM"))
        if not names:
            sys.exit(f"Xcode Cloud build {number}'s archive has no dSYMs")
        print(f"uploading the dSYMs of {ref} (Xcode Cloud build {number}): {', '.join(names)}", flush=True)
        # posthog-cli finds every .dSYM bundle under the directory.
        subprocess.run(["posthog-cli", "dsym", "upload", "--directory", str(dsyms)], check=True)


if __name__ == "__main__":
    main(sys.argv[1])
