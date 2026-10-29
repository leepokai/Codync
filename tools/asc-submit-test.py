"""Self-check for asc-submit.py and store_release.py: python3 tools/asc-submit-test.py"""

import importlib.util
import pathlib
import sys
import tempfile
import types

sys.modules.setdefault("jwt", types.ModuleType("jwt"))
spec = importlib.util.spec_from_file_location("asc_submit", pathlib.Path(__file__).with_name("asc-submit.py"))
asc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(asc)
import store_release  # noqa: E402, already loaded from tools/ by asc-submit.py

template = pathlib.Path(__file__).parent.parent.joinpath(".github/pull_request_template.md").read_text()


def fill(platform, zh, en):
    """The PR template with a bullet per language in `platform`'s What's New section."""
    head, rest = template.split(f"## What's New: {platform}\n")
    rest = rest.replace("### zh-Hant\n\n-", f"### zh-Hant\n\n- {zh}", 1).replace("### en-US\n\n-", f"### en-US\n\n- {en}", 1)
    return f"{head}## What's New: {platform}\n{rest}"


filled = fill("iPhone", "例行任務改用同一個表單", "Routines use one form")
prs = [
    {"body": filled.replace("\n", "\r\n"), "mergeCommit": {"oid": "a"}},
    {"body": filled, "mergeCommit": {"oid": "b"}},  # duplicate bullet is dropped
    {"body": template, "mergeCommit": {"oid": "c"}},  # untouched template adds nothing
    {"body": filled.replace("Routines", "Old"), "mergeCommit": {"oid": "z"}},  # not in this release
    {"body": None, "mergeCommit": None},
    # an empty template followed by a PR footer (shipped as "🤖 ..." in v2.4.3) adds nothing
    {"body": template + "\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nhttps://claude.ai/code/x",
     "mergeCommit": {"oid": "c"}},
    # Android's bullets never reach the App Store, nor the iPhone's Google Play
    {"body": fill("Android", "通知改經由 Firebase 送達", "Notifications arrive through Firebase"), "mergeCommit": {"oid": "c"}},
]
assert store_release.pr_notes(prs, {"a", "b", "c"}, "iPhone") == {
    "zh-Hant": "- 例行任務改用同一個表單", "en-US": "- Routines use one form"}, store_release.pr_notes(prs, {"a", "b", "c"}, "iPhone")
assert store_release.pr_notes(prs, {"c"}, "iPhone") == {}
assert store_release.pr_notes(prs, {"a", "b", "c"}, "Android") == {
    "zh-Hant": "- 通知改經由 Firebase 送達", "en-US": "- Notifications arrive through Firebase"}
print("ok")

# A version's WhatsNew.md section, split by locale.
with tempfile.TemporaryDirectory() as tmp:
    notes = pathlib.Path(tmp, "WhatsNew.md")
    notes.write_text("## 2.19.2\n\n### en-US\n\n- Bots react\n\n### zh-Hant\n\n## 2.19.1\n\n### en-US\n\n- Old\n")
    assert store_release.release_notes(notes, "2.19.2") == {"en-US": "- Bots react"}
    assert store_release.release_notes(notes, "2.19.3") == {}
# The App Store rejects emoji: v2.19.2's ❤️ and 🎉 failed the submission after 2.19.1 was pulled back.
# A bullet that repeats another once they're gone is dropped.
assert asc.app_store_text("- Bots react with a ❤️ or a 🎉") == "- Bots react with a  or a"
assert asc.app_store_text("- Routines use one form\n- Routines use one form 🚀\n\n- Next") == \
    "- Routines use one form\n\n- Next"
print("ok")

# Release history: v2.2.3 changed only the Mac app, host and website; v2.3.1 changed kit/.
assert not asc.ios_changed("v2.2.2", "v2.2.3")
assert asc.ios_changed("v2.3.0", "v2.3.1")
assert not asc.ios_changed("v2.19.2", "v2.19.3")  # only WhatsNew.md under apps/ios
assert asc.ios_changed("v2.19.1", "v2.19.2")
# v2.24.0's host asks for 2.21.0 (MIN_APP): a store version below it ships again without changes.
assert store_release.locked_out("2.20.1", "v2.24.0")
assert not store_release.locked_out("2.21.0", "v2.24.0")
assert not store_release.locked_out(None, "v2.24.0")
assert not store_release.locked_out("2.1.0", "v2.2.0")  # predates MIN_APP
print("ok")

# The base version: skip the run's own version and any never sent or pulled back, keep a rejected one.
def store(*vs):
    return [{"attributes": {"versionString": v, "appVersionState": s}} for v, s in vs]


assert asc.last_sent(store(("2.19.3", "PREPARE_FOR_SUBMISSION"), ("2.19.2", "READY_FOR_DISTRIBUTION")), "2.19.4") == "2.19.2"
assert asc.last_sent(store(("2.19.3", "WAITING_FOR_REVIEW"), ("2.19.2", "READY_FOR_DISTRIBUTION")), "2.19.3") == "2.19.2"
assert asc.last_sent(store(("2.20.0", "REJECTED"), ("2.19.2", "READY_FOR_DISTRIBUTION")), "2.20.1") == "2.20.0"
assert asc.last_sent(store(("2.19.3", "DEVELOPER_REJECTED")), "2.19.4") is None
print("ok")

# Xcode Cloud never heard of v2.20.4's tag (tagged commit abc): main on that commit builds the same source.
def git_with_main_at(sha):
    return lambda cmd, *args: {"rev-list": "abc", "ls-remote": f"{sha}\trefs/heads/main"}[cmd]


refs = {"refs/heads/main": "m", "refs/tags/v2.20.3": "t"}
asc.git = git_with_main_at("abc")
assert asc.archive_ref("v2.20.3", refs) == "refs/tags/v2.20.3"
assert asc.archive_ref("v2.20.4", refs) == "refs/heads/main"
asc.git = git_with_main_at("def")
assert asc.archive_ref("v2.20.4", refs) is None  # main moved on: wait for the tag
asc.git = lambda *args: ""
assert asc.archive_ref("v2.20.4", refs) is None  # tag or main unknown: never guess main
print("ok")
