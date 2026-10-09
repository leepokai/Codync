# App and host updates

A client and a host on versions that can't work together say which one to update:
[Client and host compatibility](../reference/compatibility.md).

## Desktop app

Release builds update through electron-updater from GitHub releases
(`apps/desktop/src/main/updates.ts`). Open **Settings → Updates** in the menu bar
to check for a release, enable scheduled checks, or opt into automatic downloads
and installation. The chat window's Settings has the same **Check for updates**
action on its Updates page. Only packaged `main` builds update: `npm run dev` and builds made
with `tools/account-config.mjs dev` never replace themselves with a release.

The app checks daily (hourly while a release waits for the iPhone app, below).
Automatic installation waits until no Codync window is focused, there has been no
keyboard/mouse input for ten minutes, and the local host reports that it is idle.
A staged update also installs when quitting. Manual installation can interrupt work.
A release whose `latest-mac.yml` names a `minApp` newer than the App Store's iPhone
app waits while an iPhone is paired with this host.

Before replacement on macOS, Codync unregisters its screen helper and stops the host
(`codync-host stop`), which runs from inside the app; on Linux the installed host keeps
running. If stopping fails, installation pauses with a retry action, and an install that
fails afterwards starts the host again. A
persistent restart marker makes the next app launch reinstall the host service from
the new bundle; remote screen registration is restored when the host reports that
it is enabled. Ordinary Quit keeps the host running when no update is staged.

The desktop window's close-to-background handler also listens to Electron's native
`before-quit-for-update` event. On macOS, `quitAndInstall()` closes windows before
`app.before-quit`, so waiting for the latter blocks installation after the host has
already stopped. If an older release is stuck on **Update to …** after downloading,
choose **Codync → Quit Codync**: the staged installer can finish and relaunch the app.
This lifecycle fix is desktop-only; iOS uses the App Store, and the terminal client
uses the independent host updater without Electron windows.

Installation requests are single-flight while the host stops and the installer
starts. An updater error during shutdown cancels that pending installation and
restores the host after the stop finishes; an installer exception also restores
the host and leaves the staged release available to retry.

### Recovering from a failed update

If **Update to …** appears to do nothing, first quit Codync completely (on macOS,
**Codync → Quit Codync**, not just the window's close button). A downloaded update
may finish installing when the old app exits. Reopen Codync if it does not relaunch,
then check the version in **Settings → Updates**.

If it still fails, uninstall and reinstall the application from the
[latest release](https://github.com/leepokai/Codync/releases/latest).
Replace the application while keeping its data and settings. **Do not use Reset
all data, delete `~/.codync` (or a custom `CODYNC_HOME`), remove Codync's application
support/settings folders, or use a cleanup utility.** Those are data resets, not
update repairs; deleting the host's keys also changes its identity and requires
devices to pair again.

For a Mac installed from the DMG or one-line installer:

1. Quit Codync. Before moving the old app, remove its background host service:

   ```sh
   /Applications/Codync.app/Contents/Resources/codync-host uninstall
   ```

   This removes the service and restores the wrapped Claude Code status line;
   it does not delete the host's database or keys. If the app is installed
   elsewhere, use that app's path.
2. Move only `/Applications/Codync.app` to the Trash. Download `codync-macos.dmg`
   from the latest release and drag the new Codync app into Applications.
3. Open the new app. It reinstalls the host service. If the host remains stopped,
   run the new bundled host explicitly:

   ```sh
   /Applications/Codync.app/Contents/Resources/codync-host install
   /Applications/Codync.app/Contents/Resources/codync-host status
   ```

For a Homebrew app installation, quit Codync and use
`brew reinstall --cask leepokai/codync/codync`, then open Codync. The cask removes
the old service before replacement. Do **not** add `--zap`, which removes data.

On Windows, quit Codync and stop its bundled `codync-host.exe` with the `stop`
command before reinstalling the desktop app. On Linux, replace the AppImage or
reinstall the desktop package using the original installation method. For a
standalone host, run `codync-host stop`, reinstall using the original installer
or `brew reinstall leepokai/codync/codync-host`, then run `codync-host install`.
`codync-host uninstall` removes the background service, not the executable or
saved bots, so it is not a complete application uninstall.

After reinstalling, confirm the app version and that the computer reconnects.
Existing bots and pairings should remain when the original data and keys are
kept. Repair the app on the affected computer: an SSH-connected Mac mini must
be reinstalled on the Mac mini, not on the computer displaying its SSH entry.

### Desktop updater regression tests

Run `cd apps/desktop && npm run typecheck && npm test`. The updater tests load the
actual main-process modules with fake Electron, service commands, network and
timers; they never stop a real host or replace an installed app. Coverage includes
manual and idle installation, renderer state broadcasts, download errors, repeated
clicks, errors during shutdown, staged Quit, host restart markers and binary health
checks, development-build exclusion, and iPhone compatibility holds. The native
quit event ordering has a separate regression test and an updater integration test.

The macOS and Windows bundled-host branches and Linux's externally managed host
branch run against simulated OS boundaries. These tests do not replace a signed
release upgrade on each OS, nor do they verify download progress rendering: the
current Updates page exposes checking, available/staged and error states but has
no download percentage or installation progress UI yet.

The release workflow publishes `latest-mac.yml` and `latest-linux*.yml` with the
app archives. macOS build versions follow the marketing version; the iOS build
counter is independent. Installs of the SwiftUI Mac app that preceded the desktop
app still read the Sparkle appcast; the release workflow generates one for the
desktop zip, so those installs move to the desktop app.

### Deprecated: the Sparkle migration appcast (remove after 2026-11-06)

The appcast exists only so installs of the SwiftUI Mac app (2.6.x and earlier) can
move to the desktop app; 2.7.1 is the first release they accept. Support ends
2026-11-06, one month after 2.7.1. Releases after that date drop it, and those installs
need a manual download of the desktop app. Remove together:

- the Sparkle part of **Update feeds** in `.github/workflows/release-desktop.yml`
  (`generate_appcast`, `annotate-appcast.py`, `verify-appcast.py`) and `appcast.xml`
  in **Publish**
- `SUPublicEDKey` in `apps/desktop/electron-builder.yml`
- `packaging/updates/annotate-appcast.py`, `verify-appcast.py`, `macos-public-key.txt`
- the `SPARKLE_PRIVATE_KEY` secret (row below) and the appcast mentions in
  `docs/architecture/desktop-app.md` and `docs/reference/compatibility.md`

## Standalone hosts on Linux and macOS

```sh
codync-host update --check
codync-host update
codync-host update --status --json
codync-host update --auto on
codync-host update --auto off
```

Use `--port` for a host on a nondefault port. `--force` explicitly allows a manual
update to interrupt bots. Stop a manually launched host before replacing it.
Automatic updates require an installed background service and are off by default.
They check daily while idle; failures are reported in update status.

The TUI's action
list (`^k`) has **Check for updates**, which reports the result in the status line.
The API operations (`hostUpdateStatus`, `checkHostUpdate`, `installHostUpdate`,
`setHostAutomaticUpdates`) require a local connection, including an SSH tunnel.

Installed services schedule the updater as an independent launchd job or systemd
user unit, so stopping the daemon does not kill its updater. The CLI returns after
scheduling; inspect progress with `--status`. A lock prevents concurrent updates.
The updater verifies the manifest's Ed25519 signature, platform, stable version,
URL, archive size and SHA-256 before extracting only the host executable.

It stages beside the installed executable, keeps a rollback copy, stops the old
service, atomically replaces the binary and starts the service with its existing
configuration. Health must report the expected version, binary fingerprint and
computer identity within 30 seconds. Startup failure restores and restarts the
previous binary. Data is preserved; this is binary rollback, not database rollback
or recovery from power loss during installation.

The host bundled in the Mac desktop app is updated with the app. Homebrew hosts use
`brew upgrade leepokai/codync/codync-host`, followed by `codync-host install`.
Development builds must be rebuilt. The independent updater refuses to overwrite
those installations. The Linux desktop app uses the installed host, which updates
with the commands above, independently of the app.

## Release configuration

Both workflows (`host.yml`, `release-desktop.yml`) run for a `vMAJOR.MINOR.PATCH` tag. Keep the host Cargo version,
`apps/desktop/package.json` and the marketing version aligned before tagging; the
desktop workflow fails when the app and host versions differ. The host workflow refuses a
tag/version mismatch. Existing clients without this updater need one manual
upgrade to adopt it.

Public keys are committed in `packaging/updates/`. Keep their corresponding
private keys backed up securely; replacing a public key breaks updates for
already distributed clients. Repository Actions secrets:

| Secret | Contents |
| --- | --- |
| `SPARKLE_PRIVATE_KEY` | Sparkle's base64 Ed25519 private key export (the migration appcast; deprecated, see above) |
| `HOST_UPDATE_SIGNING_KEY` | Separate base64 32-byte Ed25519 seed for host manifests |

The desktop workflow signs and notarizes the Mac app, builds its DMG and zip, adds
`minApp` to `latest-mac.yml` and generates `appcast.xml` for the zip; it builds the
Linux AppImage, deb and tar.gz on x86_64 and arm64. The host
workflow emits `codync-host-<platform>.update.json` and `.update.json.sig` alongside
each archive. Signing fails if a private key does not match the committed public
key. Never commit private keys or put them in command-line arguments.

A published release must include these assets before clients can update. A
missing feed or manifest is reported as a check failure; no unsigned fallback is
used. Building locally or configuring secrets does not publish a release.

## References

- [electron-updater](https://www.electron.build/auto-update)
- [Sparkle publishing documentation](https://sparkle-project.org/documentation/publishing/)
- [Grok Bot's safe relaunch gate](https://github.com/b-nnett/grok-bot-0.18-reconstructed/blob/main/source/electron-main/update/safe-relaunch-gate.ts)

Grok Bot supplied the reference for opt-in, staged updates and idle-gated restart.
Codync uses window focus and input idle time; it does not require a locked screen.
