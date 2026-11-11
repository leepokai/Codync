"""Import Hermes skill bundles without changing their instructions or running scripts.

Source: NousResearch/hermes-agent skills/ and optional-skills/ at 10d25f8a (MIT).
Adaptation: offline catalog, namespaced folder ids, deterministic archive and license.
Requires PyYAML. Run from the Codync root with the pinned checkout as argument.
"""

import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile

import yaml

COMMIT = "10d25f8ad65fdf71304e9272959421a0c22e5d23"

# The category ids shared by desktop, iPhone, Android and the terminal.
CATEGORIES = {
    "apple": "productivity", "autonomous-ai-agents": "automation",
    "creative": "design", "media": "design", "devops": "coding",
    "email": "productivity", "note-taking": "documents", "productivity": "productivity",
    "research": "research", "social-media": "productivity", "software-development": "coding",
    "web": "coding", "web-development": "coding", "blockchain": "coding",
    "communication": "productivity", "data-science": "data-analytics", "dogfood": "coding",
    "finance": "data-analytics", "mcp": "coding", "mlops": "coding", "security": "coding",
    "smart-home": "automation",
}


def tracked_files(source: Path, *roots):
    tree = subprocess.check_output(["git", "ls-tree", "-r", "-z", COMMIT, "--", *roots], cwd=source)
    files = {}
    for entry in tree.decode("utf-8").split("\0"):
        if not entry:
            continue
        header, name = entry.split("\t", 1)
        mode = header.split()[0]
        if mode not in {"100644", "100755"}:
            raise ValueError(f"Only regular tracked files can be bundled: {name}")
        files[name] = mode
    return files


def import_skills(source: Path, output: Path):
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if actual != COMMIT:
        raise ValueError(f"Expected Hermes {COMMIT}, found {actual}")
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=source):
        raise ValueError("Hermes tracked sources must be unmodified")
    license_text = (source / "LICENSE").read_bytes()
    tracked = tracked_files(source, "skills", "optional-skills")
    skills = []
    archive_bytes = io.BytesIO()
    with tarfile.open(fileobj=archive_bytes, mode="w", format=tarfile.PAX_FORMAT) as archive:
        def add(name, data, executable=False):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o755 if executable else 0o644
            archive.addfile(info, io.BytesIO(data))

        ids = set()
        for surface in ("skills", "optional-skills"):
            for path in sorted(source / name for name in tracked if name.startswith(surface + "/") and name.endswith("/SKILL.md")):
                relative = path.parent.relative_to(source)
                # A short stable hash keeps ids inside Codync's 48-character slug limit.
                suffix = hashlib.sha256(relative.as_posix().encode()).hexdigest()[:8]
                stem = re.sub(r"[^a-z0-9]+", "-", path.parent.name.lower()).strip("-")
                skill_id = f"hermes-{stem[:31]}-{suffix}"
                if skill_id in ids:
                    raise ValueError(f"Duplicate skill id: {skill_id}")
                ids.add(skill_id)
                markdown = path.read_text(encoding="utf-8")
                parts = re.split(r"^---\s*$", markdown, maxsplit=2, flags=re.MULTILINE)
                meta = yaml.safe_load(parts[1]) if len(parts) == 3 else {}
                meta = meta or {}
                body = parts[2].strip() if len(parts) == 3 else markdown.strip()
                skills.append({
                    "id": skill_id, "name": str(meta.get("name") or path.parent.name),
                    "description": str(meta.get("description") or ""),
                    "category": CATEGORIES.get(relative.parts[1], "other"), "upstreamCategory": relative.parts[1],
                    "path": relative.as_posix(),
                    "body": body,
                })
                prefix = relative.as_posix() + "/"
                for filename in sorted(name for name in tracked if name.startswith(prefix)):
                    file = source / filename
                    if file.is_symlink():
                        raise ValueError(f"Symlinks are not bundled: {file}")
                    data = file.read_bytes()
                    if len(data) > 16 * 1024 * 1024:
                        raise ValueError(f"Skill file exceeds 16 MiB: {file}")
                    name = file.relative_to(path.parent).as_posix()
                    add(f"{skill_id}/{name}", data, tracked[filename] == "100755")
                add(f"{skill_id}/HERMES-LICENSE", license_text)
    output.mkdir(parents=True, exist_ok=True)
    (output / "skills.json").write_text(json.dumps({"commit": COMMIT, "skills": skills}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "skills.tar.gz").write_bytes(gzip.compress(archive_bytes.getvalue(), mtime=0))
    (output / "LICENSE").write_bytes(license_text)
    print(f"Imported {len(skills)} Hermes skills at {COMMIT}")


if __name__ == "__main__":
    import_skills(Path(sys.argv[1]).resolve(), Path("host/src/market/hermes"))
