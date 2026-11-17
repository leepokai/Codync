"""Self-check for bump-version.py: python3 tools/bump-version-test.py"""

import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("bump_version", pathlib.Path(__file__).with_name("bump-version.py"))
bump = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bump)


def refused(now, arg):
    try:
        bump.next_version(now, arg)
    except SystemExit:
        return True
    return False


assert bump.next_version("2.43.0", "patch") == "2.43.1"
assert bump.next_version("2.43.9", "minor") == "2.44.0"
assert bump.next_version("2.43.0", "2.43.2") == "2.43.2"
# Versions compare as numbers, not text.
assert bump.next_version("2.9.0", "2.10.0") == "2.10.0"
assert refused("2.43.0", "3.0.0"), "never a major bump"
assert refused("2.43.0", "2.43.0"), "not above the current version"
assert refused("2.43.0", "2.42.9")
assert refused("2.43.0", "major")
# Every place the version lives still matches its pattern, and they agree.
assert bump.current()
print("ok")
