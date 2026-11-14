"""Self-check for play-submit.py: python3 tools/play-submit-test.py"""

import importlib.util
import pathlib
import re
import sys
import types

sys.modules.setdefault("jwt", types.ModuleType("jwt"))
spec = importlib.util.spec_from_file_location("play_submit", pathlib.Path(__file__).with_name("play-submit.py"))
play = importlib.util.module_from_spec(spec)
spec.loader.exec_module(play)

# The version code is the Gradle build's, and names its version back.
gradle = pathlib.Path(__file__).parent.parent.joinpath("apps/android/app/build.gradle.kts").read_text()
assert re.search(r"major \* 1_000_000 \+ minor \* 1_000 \+ patch", gradle), "versionCode formula drifted from play-submit.py"
assert play.version_code("2.24.0") == 2_024_000
assert play.version_name(play.version_code("2.9.13")) == "2.9.13"
assert play.version_code("2.10.0") > play.version_code("2.9.999")
print("ok")

# Release notes: Play's languages only, zh-Hant as zh-TW, cut at a whole line within 500 characters.
long = "\n".join(f"- line {i} " + "x" * 40 for i in range(20))
notes = play.play_notes({"en-US": long, "zh-Hant": "- 新功能", "ja": "- 新機能"})
assert [n["language"] for n in notes] == ["en-US", "zh-TW"], notes
assert len(notes[0]["text"]) <= 500 and notes[0]["text"].endswith("x") and notes[1]["text"] == "- 新功能", notes
assert play.play_notes({"en-US": "y" * 600})[0]["text"] == "y" * 500
print("ok")

# Release history: v2.20.4 left the Android app alone; v2.23.1 changed its chat.
assert not play.store.changed("v2.20.3", "v2.20.4", play.ANDROID_PATHS)
assert play.store.changed("v2.23.0", "v2.23.1", play.ANDROID_PATHS)
# The bundle's sources count; tests, the release acceptance build, the README and the Play notes don't.
files = play.store.git("ls-files", "--", *play.ANDROID_PATHS).splitlines()
assert {"apps/android/app/build.gradle.kts", "apps/android/design/build.gradle.kts", "apps/shared/Config/main.plist"} <= set(files)
assert not [f for f in files if re.search(r"/src/(test|androidTest|releaseAcceptance)/|README\.md$|WhatsNew\.md$", f)]
print("ok")

# An empty track (a new one, such as alpha) takes the release; otherwise it's compared with the
# track's newest version, which a rerun finds already there.
def track(*codes):
    play.call = lambda *args, **kwargs: {"releases": [{"versionCodes": list(codes)}]}


track()
assert play.should_ship("2.20.4", "edit")
track("2020003")
assert not play.should_ship("2.20.4", "edit")
track("2020003", "2020004")
assert not play.should_ship("2.20.4", "edit")
print("ok")
