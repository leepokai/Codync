#!/usr/bin/env python3
"""Export the shared SwiftUI widget cards for visual review (macOS + Xcode)."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
package = root / "kit"
subprocess.run(["swift", "build", "--package-path", str(package), "--target", "CodyncKit"], check=True)
build = Path(subprocess.check_output(
    ["swift", "build", "--package-path", str(package), "--show-bin-path"], text=True).strip())
output = root / "build" / "widget-previews"
output.mkdir(parents=True, exist_ok=True)
executable = output / "render-widgets"
subprocess.run([
    "xcrun", "swiftc", "-parse-as-library", "-I", str(build / "Modules"),
    str(root / "tools" / "render-widgets.swift"),
    *map(str, sorted((build / "CodyncKit.build").glob("*.swift.o"))),
    "-o", str(executable),
], check=True)
subprocess.run([str(executable), str(output)], check=True)

