"""Self-check for asc-submit.py: python3 tools/asc-submit-test.py"""

import importlib.util
import pathlib
import sys
import tempfile
import types

sys.modules.setdefault("jwt", types.ModuleType("jwt"))
spec = importlib.util.spec_from_file_location("asc_submit", pathlib.Path(__file__).with_name("asc-submit.py"))
asc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(asc)

template = pathlib.Path(__file__).parent.parent.joinpath(".github/pull_request_template.md").read_text()
filled = template.replace("### zh-Hant\n\n-", "### zh-Hant\n\n- 例行任務改用同一個表單").replace(
    "### en-US\n\n-", "### en-US\n\n- Routines use one form")
prs = [
    {"body": filled.replace("\n", "\r\n"), "mergeCommit": {"oid": "a"}},
    {"body": filled, "mergeCommit": {"oid": "b"}},  # duplicate bullet is dropped
    {"body": template, "mergeCommit": {"oid": "c"}},  # untouched template adds nothing
    {"body": filled.replace("Routines", "Old"), "mergeCommit": {"oid": "z"}},  # not in this release
    {"body": None, "mergeCommit": None},
    # an empty template followed by a PR footer (shipped as "🤖 ..." in v2.4.3) adds nothing
    {"body": template + "\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nhttps://claude.ai/code/x",
     "mergeCommit": {"oid": "c"}},
    # emoji are stripped (the App Store rejects them), so this repeats a bullet above
    {"body": filled.replace("- Routines use one form", "- Routines use one form 🚀"), "mergeCommit": {"oid": "b"}},
]
assert asc.pr_notes(prs, {"a", "b", "c"}) == {
    "zh-Hant": "- 例行任務改用同一個表單", "en-US": "- Routines use one form"}, asc.pr_notes(prs, {"a", "b", "c"})
assert asc.pr_notes(prs, {"c"}) == {}
print("ok")

# WhatsNew.md loses its emoji too: v2.19.2's ❤️ and 🎉 failed the submission after 2.19.1 was pulled back.
with tempfile.TemporaryDirectory() as tmp:
    asc.NOTES = pathlib.Path(tmp, "WhatsNew.md")
    asc.NOTES.write_text("## 2.19.2\n\n### en-US\n\n- Bots react with a ❤️ or a 🎉\n\n## 2.19.1\n\n### en-US\n\n- Old\n")
    assert asc.release_notes("2.19.2") == {"en-US": "- Bots react with a  or a"}, asc.release_notes("2.19.2")
print("ok")

# Release history: v2.2.3 changed only the Mac app, host and website; v2.3.1 changed kit/.
assert not asc.ios_changed("v2.2.2", "v2.2.3")
assert asc.ios_changed("v2.3.0", "v2.3.1")
assert not asc.ios_changed("v2.19.2", "v2.19.3")  # only WhatsNew.md under apps/ios
assert asc.ios_changed("v2.19.1", "v2.19.2")
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
