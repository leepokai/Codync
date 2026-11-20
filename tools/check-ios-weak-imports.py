#!/usr/bin/env python3
"""Fail when an iPhone build reaches Foundation through a weak import.

Usage: check-ios-weak-imports.py <Codync.app or a Mach-O binary> (macOS: uses dyld_info)

An SDK's @backDeployed API compiles to a thunk: "if the OS is at least X, call the OS's copy, else run
the copy built into the app". The call has no nil check, so an OS that reports X without having the
symbol (an early beta seed of X) jumps to address 0. Xcode 27.2's SDK does this to Date's <= and >=
(before: iOS 27.2), which killed the app at launch on such seeds once an account was signed in (#350).
CodyncKit's DateComparisons.swift now defines those two in the binary. SDK 27.1 builds import nothing
weakly from Foundation, so any such import is new and gets a look. upload-dsyms.py runs this on every
release's archive. Local builds on an older SDK always pass: only an Xcode Cloud archive proves anything
(docs/guides/iphone-crash-tracing.md).
"""

import re
import subprocess
import sys
from pathlib import Path

IMPORT = re.compile(r"^\s*0x[0-9a-fA-F]+\s+(\S+)\s+(\[weak-import\]\s+)?\(from (\S+)\)")


def foundation_weak_imports(binary):
    """The symbols `binary` imports weakly from Foundation."""
    output = subprocess.run(["dyld_info", "-imports", str(binary)], capture_output=True, text=True, check=True).stdout
    imports = [m for m in map(IMPORT.match, output.splitlines()) if m]
    if not imports:
        sys.exit(f"{binary}: found no imports in dyld_info's output; has its format changed?")
    return sorted(m[1] for m in imports if m[2] and m[3] == "Foundation")


def binaries(path):
    """The Mach-O files of a .app (the app and its extensions), or `path` itself."""
    if path.suffix != ".app":
        return [path]
    appexes = sorted(path.glob("PlugIns/*.appex"))
    return [bundle / bundle.stem for bundle in [path, *appexes]]


def main(path):
    failed = False
    for binary in binaries(Path(path)):
        found = foundation_weak_imports(binary)
        print(f"{binary.name}: {len(found)} weak imports from Foundation")
        for symbol in found:
            print(f"  {symbol}")
        failed = failed or bool(found)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main(sys.argv[1])
