"""Self-check for release-gate.py's store versions and choice: python3 tools/release-gate-test.py"""

import importlib.util
import io
import pathlib
import sys
import urllib.error

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("release_gate", pathlib.Path(__file__).with_name("release-gate.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

floors = {"v2.17.5": "2.3.0", "v2.18.0": "2.18.0", "v2.19.1": "2.18.0", "v2.19.2": "2.19.2"}
uploaded = {"v2.17.5", "v2.18.0", "v2.19.1"}


def choose(store, play=None):
    stores = {"the App Store": store} | ({"Google Play": play} if play else {})
    return gate.choose(stores, ["v2.19.2", "v2.19.1", "v2.18.0", "v2.17.5", "v2.19.3-beta"],
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
# Google Play holds a release the same way: 2.19.2 waits while its Android app is in review.
assert choose("2.19.2", play="2.19.1") == "v2.19.1"
assert choose("2.19.2", play="2.19.2") == "v2.19.2"
# The listing's version sits before the target and minimum SDK; a version elsewhere on the page isn't it.
assert gate.play_version('null,[[["2.49.1"]],[[[36]],[[[23,"6.0"]]]]],null') == "2.49.1"
assert gate.play_version('"]]],"12.4.1",null,[[["Varies with device"]],[[[36]]') is None


def raises(error, call):
    try:
        call()
    except error:
        return True
    return False


def play_answers(status, page=""):
    """Google Play answers the listing request with `status` and `page`."""
    def urlopen(url, timeout):
        if status != 200:
            raise urllib.error.HTTPError(url, status, "", None, None)
        return io.BytesIO(page.encode())
    gate.urllib.request.urlopen = urlopen


gate.app_store_version = lambda: "2.49.1"
# No listing: Google Play holds nothing.
play_answers(404)
assert gate.store_versions() == {"the App Store": "2.49.1"}
# Any other refusal fails the run: Google Play must not drop out of the gate without a word.
play_answers(429)
assert raises(urllib.error.HTTPError, gate.store_versions)
play_answers(200, "<html>no version here</html>")
assert raises(SystemExit, gate.store_versions)
play_answers(200, 'null,[[["2.49.0"]],[[[36]],[[[23,"6.0"]]]]],null')
assert gate.store_versions() == {"the App Store": "2.49.1", "Google Play": "2.49.0"}
print("ok")
