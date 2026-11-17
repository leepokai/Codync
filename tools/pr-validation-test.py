#!/usr/bin/env python3
"""python3 tools/pr-validation-test.py: checks tools/pr-validation.py against the PR template."""

import importlib.util
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("pr_validation", ROOT / "tools/pr-validation.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

template = (ROOT / ".github/pull_request_template.md").read_text()
ticked = template.replace("- [ ]", "- [x]")
tried = "<!-- Commands, devices, flows walked and their results. Paste the after-change report here. -->"
described = ticked.replace(tried, "cargo test: 1151 passed. Sent a message from the iOS Simulator, the reply arrived.")

assert mod.problems(template), "the untouched template must fail"
assert any("Unticked" in p for p in mod.problems(template))
assert any("What I tried" in p for p in mod.problems(ticked)), "ticked boxes without a description must fail"
assert mod.problems(described) == [], mod.problems(described)
assert mod.problems(described.replace("\n", "\r\n")) == [], "CRLF bodies from the web editor must pass"
assert mod.problems("") == ["The `## Validation` section of the PR template is missing."]
one_unticked = described.replace("- [x] Targets", "- [ ] Targets", 1)
assert len(mod.problems(one_unticked)) == 1
print("ok")
