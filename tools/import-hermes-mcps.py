"""Source: Hermes optional-mcps/*/manifest.yaml at 10d25f8a (MIT).
Adaptation: Codync app cards using the existing remote MCP and vault OAuth flow.
"""

import json
from pathlib import Path
import subprocess
import sys
from urllib.parse import urlsplit

import yaml

from importlib import import_module

skill_import = import_module("import-hermes-skills")
COMMIT = skill_import.COMMIT


def import_mcps(source: Path, output: Path):
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() != COMMIT:
        raise ValueError("Hermes must stay at the pinned commit")
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=source):
        raise ValueError("Hermes tracked sources must be unmodified")
    apps = []
    for path in sorted(source / name for name in skill_import.tracked_files(source, "optional-mcps") if name.endswith("/manifest.yaml")):
        manifest = yaml.safe_load(path.read_text(encoding="utf-8"))
        transport, auth = manifest["transport"], manifest["auth"]
        if transport["type"] != "http" or auth["type"] not in {"oauth", "none"}:
            raise ValueError(f"Unsupported manifest: {path}")
        url = transport["url"]
        fields = []
        for env in auth.get("env", []):
            marker = "${" + env["name"] + "}"
            if url == marker:
                fields.append({"id": env["name"], "label": env.get("prompt", env["name"]), "url": True, "required": True})
                url = ""
            elif marker in auth.get("oauth", {}).values():
                fields.append({"id": env["name"], "label": env.get("prompt", env["name"]), "env": env["name"], "required": True})
            else:
                raise ValueError(f"Unsupported manifest variable: {path}")
        if url and urlsplit(url).scheme not in {"https", "http"}:
            raise ValueError(f"Invalid MCP URL: {path}")
        origin = f"https://github.com/NousResearch/hermes-agent/blob/{COMMIT}/optional-mcps/{path.parent.name}/manifest.yaml"
        name = manifest["name"]
        apps.append({
            "id": f"hermes-{name}", "name": name,
            "tagline": manifest["description"].strip(),
            "description": manifest["description"].strip(),
            "category": "other", "creator": "Nous Research / Hermes", "version": COMMIT[:8],
            "updated": "2026-11-03", "website": manifest.get("source"), "featured": False,
            "setup": "key" if fields else "link" if auth["type"] == "oauth" else "open",
            "server": {"name": name, "transport": "http", "url": url or None, "args": []},
            "fields": fields, "steps": [], "docsUrl": manifest.get("source"), "prompts": [],
            "origin": origin, "auth": auth["type"],
        })
    output.mkdir(parents=True, exist_ok=True)
    (output / "apps.json").write_text(json.dumps({"commit": COMMIT, "apps": apps}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "LICENSE").write_bytes((source / "LICENSE").read_bytes())
    print(f"Imported {len(apps)} Hermes MCP manifests")


if __name__ == "__main__":
    import_mcps(Path(sys.argv[1]).resolve(), Path("host/src/market/hermes"))
