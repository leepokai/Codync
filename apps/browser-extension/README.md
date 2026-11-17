# Codync Browser

Codync Browser is a Manifest V3 extension that lets a bot operate a dedicated foreground tab on
the Windows computer it is running on. The host remains the authority: every request carries the
exact host, bot, user message, browser tab and generation, and a missing host never selects a
different computer.

## Permissions

- `nativeMessaging`: connects to the local, same-user `codync.browser` helper (or
  `codync.browser.dev` for Codync Dev). The helper reads the loopback token and transfers protocol
  messages; the bearer token never enters the extension.
- `debugger`: sends real mouse and keyboard input and captures the page from the dedicated tab.
- `tabs`: creates, navigates, identifies and closes the one tab bound to a user message.
- `storage`: remembers the current bridge status and tab binding across service-worker restarts.

The extension requests no web-origin host permission. It opens only `http` and `https` URLs.

## Local installation

For a development (`codync.browser.dev`) build, or a main (`codync.browser`, set `$env:CODYNC_ENV = 'main'` before `cargo build`) build, install the native helper first:

```powershell
$repo = (Resolve-Path .).Path # Run this from the repository root.
Push-Location "$repo\host"
# Omit CODYNC_ENV for dev (codync.browser.dev / port 19223), or set $env:CODYNC_ENV = 'main' before building for release (codync.browser / port 19222).
cargo build --release --bin codync-browser-native-host
Pop-Location
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$repo\apps\browser-extension\install-native-host.ps1" `
  -Executable "$repo\host\target\release\codync-browser-native-host.exe"
```

The helper built at `host\target\release\codync-browser-native-host.exe` finds `apps\browser-extension` through its relative fallback. For a different layout, set `CODYNC_BROWSER_EXTENSION_DIR` in Chrome's environment before starting Chrome; setting it only while registering the helper does not save the override.

Then open `chrome://extensions`, enable Developer mode, choose **Load unpacked**, and select this
folder once. The manifest's fixed public key gives the extension ID
`dgejialhpalhoigbkmgeabgpphlipiek`. On its first connection, the helper opens Codync Browser in a
separate Chrome profile that loads this extension and carries the helper-owned isolated-profile
marker. The ordinary profile is refused, so Codync bots never share its tabs, cookies or sign-ins.
Confirm that the Codync Browser window's popup says **Connected to Codync**.

The native helper manifest is written under the current user's local app data and registered with
both Chrome and Edge. Release and development builds use separate host names, manifests, profiles
and registry keys, so installing or removing one does not overwrite the other. To remove a build's
registration, run that build's helper with `--uninstall-native-messaging`, then remove the extension
from `chrome://extensions`.

## Safety behavior

- A bot gets one dedicated foreground tab per user message in the dedicated Codync profile.
  Existing signed-in tabs are never selected implicitly.
- A stale generation or a different host, bot, message or tab is rejected.
- A click or text insertion requires a selector that matches exactly one element. Every action runs
  once. Timeouts are reported and never replayed.
- Stopping the bot, ending the turn or calling `browser_stop` closes the dedicated tab. A start that
  is still in flight is cancelled and any tab it creates late is closed.
- With no native connection, browser tools fail explicitly. There is no alternate-profile,
  alternate-browser or cross-computer fallback.
