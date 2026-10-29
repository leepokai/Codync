#!/usr/bin/env python3
"""Create a signed manifest for one standalone host release archive.

HOST_UPDATE_SIGNING_KEY contains a base64 Ed25519 seed. Only the public key is
committed. Requires cryptography (installed by the release workflow).
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", required=True, choices=["macos-arm64", "macos-x86_64", "linux-arm64", "linux-x86_64"])
    args = parser.parse_args()
    version = args.version.removeprefix("v")
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version):
        parser.error("version must be a stable major.minor.patch release")
    name = f"codync-host-{args.platform}.tar.gz"
    if args.archive.name != name:
        parser.error(f"expected archive name {name}")
    key = Ed25519PrivateKey.from_private_bytes(base64.b64decode(os.environ["HOST_UPDATE_SIGNING_KEY"].strip(), validate=True))
    expected = Path(__file__).with_name("host-public-key.txt").read_text().strip()
    actual = base64.b64encode(key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)).decode()
    if actual != expected:
        raise SystemExit("HOST_UPDATE_SIGNING_KEY does not match the committed host public key")
    archive = args.archive.read_bytes()
    manifest = {
        "version": version,
        "platform": args.platform,
        "url": f"https://github.com/leepokai/Codync/releases/download/v{version}/{name}",
        "sha256": hashlib.sha256(archive).hexdigest(),
        "size": len(archive),
    }
    data = (json.dumps(manifest, indent=2) + "\n").encode()
    output = args.archive.with_name(f"codync-host-{args.platform}.update.json")
    output.write_bytes(data)
    output.with_suffix(output.suffix + ".sig").write_text(base64.b64encode(key.sign(data)).decode() + "\n")
    print(f"Signed {output.name}")


if __name__ == "__main__":
    main()
