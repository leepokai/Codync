#!/usr/bin/env python3
"""Fail release packaging if Sparkle metadata does not describe the built app."""
import base64
from pathlib import Path
import plistlib
import sys
import xml.etree.ElementTree as ET
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey


def main():
    feed, app, archive = map(Path, sys.argv[1:])
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    root = ET.parse(feed).getroot()
    namespace = "http://www.andymatuschak.org/xml-namespaces/sparkle"
    items = root.findall("./channel/item")
    assert len(items) == 1, "expected one current release"
    item = items[0]
    enclosure = item.find("enclosure")
    assert enclosure is not None
    version = item.findtext(f"{{{namespace}}}version") or enclosure.get(f"{{{namespace}}}version")
    assert version == info["CFBundleVersion"], "appcast build version differs from app"
    assert enclosure.get("length") == str(archive.stat().st_size), "appcast archive size is wrong"
    assert len(base64.b64decode(enclosure.attrib[f"{{{namespace}}}edSignature"], validate=True)) == 64
    expected = f"https://github.com/leepokai/Codync/releases/download/v{info['CFBundleShortVersionString']}/{archive.name}"
    assert enclosure.get("url") == expected, "appcast must point to this immutable release archive"
    public_key = Path(__file__).with_name("macos-public-key.txt").read_text().strip()
    Ed25519PublicKey.from_public_bytes(base64.b64decode(public_key, validate=True)).verify(
        base64.b64decode(enclosure.attrib[f"{{{namespace}}}edSignature"], validate=True),
        archive.read_bytes(),
    )
    assert info["SUPublicEDKey"] == public_key, "app has the wrong Sparkle public key"
    print("Sparkle appcast matches the signed app and release archive")


if __name__ == "__main__":
    main()
