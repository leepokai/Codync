"""Read-only Hermes profile metadata. No provider import, initialization or credential output."""

from __future__ import annotations

import importlib.metadata
import json
from pathlib import Path
import re

from acp_owner import configure_profile, source_root


def probe(root: Path) -> dict:
    configure_profile()
    from hermes_constants import get_hermes_home
    from utils import fast_safe_load

    home = get_hermes_home()
    # Do not call entry._load_env: a configured remote config backend may bootstrap network
    # access. Only parse local configuration text; never expand or print secret references.
    path = home / "config.yaml"
    raw = fast_safe_load(path.read_text(encoding="utf-8-sig")) if path.is_file() else {}
    if raw is not None and not isinstance(raw, dict):
        raise ValueError("Hermes profile configuration is not an object")
    raw = raw or {}
    memory = raw.get("memory") or {}
    terminal = raw.get("terminal") or {}
    provider = memory.get("provider", "builtin") if isinstance(memory, dict) else "unknown"
    if not isinstance(provider, str) or not re.fullmatch(r"[a-zA-Z0-9_-]{0,64}", provider):
        provider = "unknown"
    provider = provider or "builtin"
    backend = terminal.get("backend", "local") if isinstance(terminal, dict) else "unknown"
    if backend not in {"local", "docker", "ssh", "modal", "daytona", "vercel_sandbox"}:
        backend = "unknown"
    candidates = (home / "plugins" / "honcho", root / "plugins" / "memory" / "honcho")
    installed = any((directory / "__init__.py").is_file() for directory in candidates)
    entry_points = importlib.metadata.entry_points()
    entries = entry_points.select(group="hermes_agent.memory_providers") if hasattr(entry_points, "select") else entry_points.get("hermes_agent.memory_providers", [])
    installed = installed or any(entry.name == "honcho" for entry in entries)
    return {
        "profileHome": str(home),
        "memoryProvider": provider,
        "honchoInstalled": installed,
        "terminal": {"backend": backend},
        "configuration": "local-profile-metadata",
        "authenticated": None,
        "healthy": None,
    }


if __name__ == "__main__":
    try:
        print(json.dumps(probe(source_root())))
    except Exception:
        # Profile parsing errors may contain credential-bearing YAML snippets. Return a generic
        # message only; this probe deliberately has no native provider/network diagnostics.
        print(json.dumps({"error": "Hermes profile metadata could not be read"}))
        raise SystemExit(1)
