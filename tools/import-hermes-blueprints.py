"""Source: Hermes cron/blueprint_catalog.py at 10d25f8a (MIT).
Adaptation: extract only literal catalog constructors without importing or executing Hermes.
"""

import ast
from importlib import import_module
import json
from pathlib import Path
import subprocess
import sys

COMMIT = import_module("import-hermes-skills").COMMIT


def extract(source: Path):
    nodes = ast.parse(source.read_text(encoding="utf-8")).body
    values = {}

    def evaluate(node, bindings=None):
        bindings = bindings or {}
        if isinstance(node, ast.Constant):
            return node.value
        if isinstance(node, (ast.List, ast.Tuple)):
            return [evaluate(n, bindings) for n in node.elts]
        if isinstance(node, ast.Dict):
            return {evaluate(k, bindings): evaluate(v, bindings) for k, v in zip(node.keys, node.values)}
        if isinstance(node, ast.Name):
            return bindings[node.id] if node.id in bindings else values[node.id]
        if isinstance(node, ast.Lambda):
            return node
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
            name = node.func.id
            if name in {"AutomationBlueprint", "BlueprintSlot"} and not node.args:
                return {k.arg: evaluate(k.value, bindings) for k in node.keywords}
            if name == "_TIME" and not node.keywords:
                definition = values[name]
                default = evaluate(definition.args.defaults[0])
                return evaluate(definition.body, {"default": evaluate(node.args[0]) if node.args else default})
            if name == "tuple" and len(node.args) == 1:
                keys = node.args[0]
                if isinstance(keys, ast.Call) and isinstance(keys.func, ast.Attribute) and isinstance(keys.func.value, ast.Name) and keys.func.value.id == "WEEKDAY_PRESETS" and keys.func.attr == "keys":
                    return list(values["WEEKDAY_PRESETS"])
        raise ValueError(f"Unsupported catalog expression: {ast.dump(node)}")

    for node in nodes:
        target = node.target if isinstance(node, ast.AnnAssign) else node.targets[0] if isinstance(node, ast.Assign) and len(node.targets) == 1 else None
        if isinstance(target, ast.Name) and target.id in {"WEEKDAY_PRESETS", "_TIME", "_DELIVER", "CATALOG"}:
            values[target.id] = evaluate(node.value)
    return values["CATALOG"]


if __name__ == "__main__":
    source = Path(sys.argv[1]).resolve()
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip() != COMMIT:
        raise ValueError("Hermes must stay at the pinned commit")
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=source):
        raise ValueError("Hermes tracked sources must be unmodified")
    catalog = extract(source / "cron/blueprint_catalog.py")
    output = Path("host/src/market/hermes/blueprints.json")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"commit": COMMIT, "blueprints": catalog}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Imported {len(catalog)} Hermes routine blueprints")
