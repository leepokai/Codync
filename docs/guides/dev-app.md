# Codync Dev and production side by side

Codync Dev uses the same source as Codync, with a separate installed identity and local state. Production identifiers and storage locations remain unchanged. Do not copy production credentials, host identity keys, databases or paired devices into Dev; sign in and pair test devices independently.

Both variants use the original Codync icon. The installed name **Codync Dev** distinguishes the development app.

## Choose a build

| Client | Production | Development |
| --- | --- | --- |
| Xcode iPhone scheme | `iOS` | `iOS Dev` |
| Xcode Screen helper scheme | `Screen` | `Screen Dev` |
| Desktop packaging | `npm run dist:<platform>:main` | `npm run dist:<platform>:dev` |
| Foreground host | `CODYNC_ENV=main cargo run -- serve` | `cargo run -- serve` |

`<platform>` is `mac`, `linux`, or `win`. Desktop `npm run dev` uses the development identity. An unpackaged run needs a matching development host binary (`host/target/debug/codync-host`, or `CODYNC_HOST_BIN`). On Linux, install a development host as `~/.local/bin/codync-dev-host`; the production binary remains `codync-host`.

Xcode configurations separate environment from optimization: `Debug` and `Release` use production; `DevDebug` and `DevRelease` use development. The `iOS` archive remains `Release`, preserving Xcode Cloud's existing production scheme. Configure schemes through `apps/project.yml`, then regenerate with `xcodegen generate --spec apps/project.yml`.

## Resource ownership

| Resource | Production | Development |
| --- | --- | --- |
| Cloud | `https://api.codync.dev` | `https://dev-api.codync.dev` |
| iOS bundle | `com.pokai.Codync.ios` | `com.pokai.Codync.ios.dev` |
| iOS App Group | `group.com.pokai.Codync` | `group.com.pokai.Codync.dev` |
| iOS Keychain group (team prefix omitted) | `com.pokai.Codync` | `com.pokai.Codync.dev` |
| Desktop bundle | `com.pokai.Codync` | `com.pokai.Codync.dev` |
| Linux package / Windows package cache | `codync-desktop` | `codync-desktop-dev` |
| Desktop account callback | `com.pokai.Codync://callback` | `com.pokai.Codync.dev://callback` |
| Pairing and widget links | `codync://` | `codync-dev://` |
| Host default port | `19222` | `19223` |
| Host data | `~/.codync` | `~/.codync-dev` |
| macOS host launch agent | `com.pokai.codync.host` | `com.pokai.codync.dev.host` |
| Linux user service | `codync-host.service` | `codync-dev-host.service` |
| Windows Run key value | `CodyncHost` | `CodyncDevHost` |
| Windows supervisor | `codync-hostw.exe` | `codync-dev-hostw.exe` |
| macOS Screen helper identifier | `com.pokai.Codync.screen` | `com.pokai.Codync.dev.screen` |

The iPhone extensions inherit the parent environment. Electron Dev stores its browser session, account credentials and SSH profiles under its own `Codync Dev` user-data directory. SSH host-key storage and orphan-tunnel cleanup are also separate. Dev never installs production updates and does not rewrite the shared Claude Code status line.

Remote SSH lookup in Dev accepts the host bundled in `/Applications/Codync Dev.app` or `~/.local/bin/codync-dev-host`; it does not fall back to Homebrew's production host. The desktop's missing-host instructions therefore ask for a Dev build. iOS has no SSH installer or local host to manage. The terminal pairing screen names Codync Dev when it emits a Dev pairing link.

Stop only the variant being replaced. Avoid `pkill codync-host`, generic bundle lookups, or commands that stop both variants. macOS Dev's host can be stopped with `launchctl bootout gui/$(id -u)/com.pokai.codync.dev.host`; its app is **Codync Dev**. Keep each worktree's Apple build products under its own `build/dd`.

## Accounts, callbacks and notifications

Register the development app identifiers, App Group and Sign in with Apple capability with the signing team. Configure the Clerk development instance to accept the new iOS identifier and desktop callback. These settings are external to the repository; a successful simulator build does not prove signing or sign-in works on a device.

The cloud adds `/v1/oauth/callback/dev` for connector sign-in, returning `codync-dev://oauth`. The existing callback keeps returning `codync://oauth`. Deploy the added route to the development Worker before testing connector sign-in from a Dev client.

The push relay seals the app bundle identifier into new tickets and selects that APNs topic. Existing tickets and registrations without a bundle identifier retain the production topic. Only the production and Dev iOS identifiers are accepted; alert and Live Activity destinations stay separate. Deploy this additive relay change before testing Dev notifications. APNs sandbox/production is a signing/distribution choice, independent of which Codync cloud is used.

## Troubleshooting a running host shown as Offline

Observed on 2026-10-09: a MacBook development host was online in development D1, while a Mac mini production app showed the same computer identity's old production record as Offline. Both hosts reported healthy relay connections. Their cloud environments differed.

Before resetting keys or pairing again:

1. Run the relevant bundled `codync-host cloud` on each computer. Compare `url`, `registered`, `connected` and `last error`.
2. Read `/health` on that host's port for `environment`, `computerId`, binary path and version.
3. Confirm the desktop app, its host and the iPhone app participating in the test use the same environment. An identical version number or Apple login does not establish this.
4. Refresh the account list. Working SSH or local access does not prove the clouds match.

An environment change is not a migration of the production account or host. The fresh Dev instance starts without production bots or permissions.

## Local verification (2026-10-09)

- iOS Dev built, signed and launched on a physical iPhone. The app and both extensions use the Dev App Group; the signed app uses the separate Dev Keychain group and development APNs entitlement.
- The production iOS Release simulator build succeeded with its existing bundle identifiers, App Group and URL scheme. The Swift package's 55 tests passed.
- A signed macOS Dev app launched into an independent empty workspace. The existing host on 19222 and Dev host on 19223 ran together with different computer identities. Starting and stopping Dev left the original launch agent, token and Claude Code settings unchanged.
- Desktop type checking and regression tests passed; Rust tests and Clippy passed. Windows and Linux identities and package configuration were checked in tests, but their installed apps have not been exercised on those operating systems.
- Live Apple account sign-in, connector callbacks and APNs delivery require the external setup and deployments above. Local build success does not verify those flows.
