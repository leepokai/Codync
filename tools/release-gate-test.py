"""Self-check for release-gate.py's choice: python3 tools/release-gate-test.py"""

import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("release_gate", pathlib.Path(__file__).with_name("release-gate.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

floors = {"v2.17.5": "2.3.0", "v2.18.0": "2.18.0", "v2.19.1": "2.18.0", "v2.19.2": "2.19.2"}
uploaded = {"v2.17.5", "v2.18.0", "v2.19.1"}


def choose(store):
    return gate.choose(store, ["v2.19.2", "v2.19.1", "v2.18.0", "v2.17.5", "v2.19.3-beta"],
                       min_app=floors.get, complete=uploaded.__contains__)


# 2.18.0 raised the floor: while the App Store has 2.17.0, downloads stay on 2.17.5.
assert choose("2.17.0") == "v2.17.5"
# The newest whose floor the App Store meets, even when it isn't the newest release.
assert choose("2.18.0") == "v2.19.1"
# 2.19.2 meets 2.19.2, but it is still uploading.
assert choose("2.19.2") == "v2.19.1"
uploaded.add("v2.19.2")
assert choose("2.19.2") == "v2.19.2"
# Versions compare as numbers, not text.
assert choose("2.100.0") == "v2.19.2"
# No release's floor is met: the latest mark stays where it is.
assert choose("2.0.0") is None
print("ok")
