#!/usr/bin/env python3
"""Fetch the same pinned Engram release as the host for isolated integration tests."""

import hashlib
import io
import json
import os
from pathlib import Path
import platform
import tarfile
import urllib.request
import zipfile

root = Path(__file__).resolve().parents[2]
release = json.loads((Path(__file__).parent / "release.json").read_text())
system = {"Darwin": "darwin", "Linux": "linux", "Windows": "windows"}[platform.system()]
arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "amd64", "amd64": "amd64"}[platform.machine().lower()]
target = f"{system}_{arch}"
version = release["version"]
extension = "zip" if system == "windows" else "tar.gz"
name = "engram.exe" if system == "windows" else "engram"
archive = f"engram_{version}_{target}.{extension}"
url = f"https://github.com/Gentleman-Programming/engram/releases/download/v{version}/{archive}"
destination = root / "host" / "target" / "engram-test-runtime" / version / name


def fetch():
    with urllib.request.urlopen(url, timeout=120) as response:
        data = response.read(128 * 1024 * 1024 + 1)
    if hashlib.sha256(data).hexdigest() != release["assets"][target]:
        raise SystemExit("Engram release checksum mismatch")
    if extension == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as files:
            binary = files.read(name)
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as files:
            member = files.getmember(name)
            if not member.isfile():
                raise SystemExit("Engram release does not contain a regular binary")
            binary = files.extractfile(member).read()
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(name + ".part")
    partial.write_bytes(binary)
    partial.chmod(0o755)
    partial.replace(destination)


# A copy fetched earlier is reused: the pre-commit hook asks for it on every host commit.
if not destination.exists():
    fetch()
if "GITHUB_ENV" in os.environ:
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as env:
        env.write(f"CODYNC_TEST_ENGRAM={destination}\n")
print(destination)
