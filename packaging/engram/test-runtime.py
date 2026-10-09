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
with urllib.request.urlopen(url, timeout=120) as response:
    data = response.read(128 * 1024 * 1024 + 1)
if hashlib.sha256(data).hexdigest() != release["assets"][target]:
    raise SystemExit("Engram release checksum mismatch")
destination = root / "host" / "target" / "engram-test-runtime" / version / name
destination.parent.mkdir(parents=True, exist_ok=True)
if extension == "zip":
    with zipfile.ZipFile(io.BytesIO(data)) as files:
        binary = files.read(name)
else:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as files:
        member = files.getmember(name)
        if not member.isfile():
            raise SystemExit("Engram release does not contain a regular binary")
        binary = files.extractfile(member).read()
destination.write_bytes(binary)
destination.chmod(0o755)
if "GITHUB_ENV" in os.environ:
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as env:
        env.write(f"CODYNC_TEST_ENGRAM={destination}\n")
print(destination)
