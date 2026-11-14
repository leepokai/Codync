#!/usr/bin/env python3
"""Rebuild the offline terminal assets from integrity-pinned MIT npm releases."""
import base64
import hashlib
import io
import json
import pathlib
import subprocess
import tarfile
import tempfile
import urllib.request

DESTINATION = pathlib.Path(__file__).resolve().parents[1] / "app/src/main/assets/terminal"
ESBUILD = "0.28.2"
PACKAGES = [
    ("@xterm/xterm", "6.0.0", "sha512-TQwDdQGtwwDt+2cgKDLn0IRaSxYu1tSUjgKarSDkUM0ZNiSRXFpjxEsvc/Zgc5kq5omJ+V0a8/kIM2WD3sMOYg==",
     {"lib/xterm.js": "xterm.js", "css/xterm.css": "xterm.css", "LICENSE": "LICENSE.xterm"}),
    ("@xterm/addon-fit", "0.11.0", "sha512-jYcgT6xtVYhnhgxh3QgYDnnNMYTcf8ElbxxFzX0IZo+vabQqSPAjC3c1wJrKB5E19VwQei89QCiZZP86DCPF7g==",
     {"lib/addon-fit.js": "addon-fit.js", "LICENSE": "LICENSE.addon-fit"}),
    ("@xterm/addon-web-links", "0.12.0", "sha512-4Smom3RPyVp7ZMYOYDoC/9eGJJJqYhnPLGGqJ6wOBfB8VxPViJNSKdgRYb8NpaM6YSelEKbA2SStD7lGyqaobw==",
     {"lib/addon-web-links.js": "addon-web-links.js", "LICENSE": "LICENSE.addon-web-links"}),
]

def main():
    files = {}
    manifest = []
    with tempfile.TemporaryDirectory(prefix="codync-terminal-vendor-") as temporary:
        for package, version, integrity, selected in PACKAGES:
            name = package.split("/")[-1]
            url = f"https://registry.npmjs.org/{package}/-/{name}-{version}.tgz"
            with urllib.request.urlopen(url, timeout=30) as response:
                archive = response.read()
            actual = "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode()
            if actual != integrity:
                raise ValueError(f"Integrity mismatch for {package}")
            entry = {"package": package, "version": version, "integrity": integrity, "license": "MIT",
                     "source": "https://github.com/xtermjs/xterm.js/tree/6.0.0", "files": {}, "originalFiles": {}}
            with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
                for source, target in selected.items():
                    with tar.extractfile("package/" + source) as stream:
                        data = stream.read()
                    entry["originalFiles"][target] = hashlib.sha256(data).hexdigest()
                    if target.endswith(".js"):
                        # xterm 6 uses class static blocks; old Android WebViews cannot parse them.
                        result = subprocess.run(["npx", "--yes", f"esbuild@{ESBUILD}", "--target=chrome83",
                                                 "--minify", "--legal-comments=inline"], input=data,
                                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True,
                                                cwd=temporary)
                        data = result.stdout
                    files[target] = data
                    entry["files"][target] = hashlib.sha256(data).hexdigest()
            entry["transform"] = {"tool": "esbuild", "version": ESBUILD, "target": "chrome83",
                                  "options": ["--minify", "--legal-comments=inline"]}
            manifest.append(entry)
    for target, data in files.items():
        (DESTINATION / target).write_bytes(data)
    (DESTINATION / "vendor-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("Verified and rebuilt the bundled terminal assets.")

if __name__ == "__main__":
    main()
