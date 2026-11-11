"""Verify bundled Hermes data byte-for-byte against the pinned source, without executing it."""

import hashlib
from importlib import import_module
import json
from pathlib import Path
import sys
import subprocess
import tarfile

import yaml


def verify(source: Path):
    skill_import = import_module("import-hermes-skills")
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() == skill_import.COMMIT
    subprocess.check_call(["git", "diff", "--quiet", skill_import.COMMIT, "--"], cwd=source)
    bundled = Path("host/src/market/hermes")
    catalog = json.loads((bundled / "skills.json").read_text(encoding="utf-8"))
    tracked = skill_import.tracked_files(source, "skills", "optional-skills")
    expected = {Path(p).parent.as_posix() for p in tracked if p.endswith("/SKILL.md")}
    assert catalog["commit"] == skill_import.COMMIT
    assert {s["path"] for s in catalog["skills"]} == expected, "skill inventory differs"
    assert len({s["id"] for s in catalog["skills"]}) == len(expected), "skill ids collide"
    with tarfile.open(bundled / "skills.tar.gz", "r:gz") as archive:
        members = archive.getmembers()
        assert len({m.name for m in members}) == len(members), "archive entries collide"
        by_id = {s["id"]: s for s in catalog["skills"]}
        checked = set()
        for member in members:
            assert member.isfile() and ".." not in Path(member.name).parts
            skill_id, name = member.name.split("/", 1)
            skill = by_id[skill_id]
            expected_file = source / ("LICENSE" if name == "HERMES-LICENSE" else f"{skill['path']}/{name}")
            data = archive.extractfile(member).read()
            assert hashlib.sha256(data).digest() == hashlib.sha256(expected_file.read_bytes()).digest(), member.name
            checked.add((skill_id, name))
        for skill in catalog["skills"]:
            prefix = skill["path"] + "/"
            for file in tracked:
                if file.startswith(prefix):
                    assert (skill["id"], file.removeprefix(prefix)) in checked
    apps = json.loads((bundled / "apps.json").read_text(encoding="utf-8"))
    manifests = {Path(name).parent.name: yaml.safe_load((source / name).read_text(encoding="utf-8")) for name in skill_import.tracked_files(source, "optional-mcps") if name.endswith("/manifest.yaml")}
    assert apps["commit"] == skill_import.COMMIT
    assert len(apps["apps"]) == len(manifests) == 66
    for app in apps["apps"]:
        manifest = manifests[app["id"].removeprefix("hermes-")]
        assert app["server"]["name"] == manifest["name"]
        assert app["auth"] == manifest["auth"]["type"]
        assert app["server"]["url"] == manifest["transport"]["url"] or any(f.get("url") for f in app["fields"])
        assert {f["id"] for f in app["fields"]} == {e["name"] for e in manifest["auth"].get("env", [])}
    blueprints = json.loads((bundled / "blueprints.json").read_text(encoding="utf-8"))
    assert blueprints["commit"] == skill_import.COMMIT
    assert blueprints["blueprints"] == import_module("import-hermes-blueprints").extract(source / "cron/blueprint_catalog.py")
    print(f"PASS: {len(expected)} skills, {len(checked)} archive files, {len(manifests)} MCP manifests and {len(blueprints['blueprints'])} blueprints match pinned Hermes")


if __name__ == "__main__":
    verify(Path(sys.argv[1]).resolve())
