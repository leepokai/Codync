# Codync Dev and production side by side

Codync Dev uses the same source as Codync, with a separate installed identity and local state. Production identifiers and storage locations remain unchanged. Do not copy production credentials, host identity keys, databases or paired devices into Dev; sign in and pair test devices independently.

Both variants use the original Codync icon. The installed name **Codync Dev** distinguishes the development app.

Desktop development and builds on macOS generate SF Symbols automatically, including the Apple sign-in logo. Packaging rejects a macOS build without the Apple logo assets; a fresh worktree does not need a separate `npm run icons` step.

## Choose a build

| Client | Production | Development |
| --- | --- | --- |
| Xcode iPhone scheme | `iOS` | `iOS Dev` |
| Xcode Screen helper scheme | `Screen` | `Screen Dev` |
| Desktop packaging | `npm run dist:<platform>:main` | `npm run dist:<platform>:dev` |
| Foreground host | `CODYNC_ENV=main cargo run -- serve` | `cargo run -- serve` |

`<platform>` is `mac`, `linux`, or `win`. Desktop `npm run dev` uses the development identity; in a linked worktree it runs a [sandbox](#worktree-sandboxes). In the main checkout, an unpackaged run needs a matching development host binary; it takes the first that exists of `CODYNC_HOST_BIN`, `apps/desktop/build/native/codync-host` (left by `dist:mac:dev`, or rebuilt alone with `node tools/native-host.mjs dev`) and `host/target/debug/codync-host`, so after a `dist:mac:dev` a plain `cargo build` isn't picked up. On Linux, install a development host as `~/.local/bin/codync-dev-host`; the production binary remains `codync-host`.

Xcode configurations separate environment from optimization: `Debug` and `Release` use production; `DevDebug` and `DevRelease` use development. The `iOS` archive remains `Release`, preserving Xcode Cloud's existing production scheme. Configure schemes through `apps/project.yml`, then regenerate with `xcodegen generate --spec apps/project.yml`.

## Worktree sandboxes

`npm run dev` decides by checkout. The main checkout (its `.git` is a folder) runs the default Codync Dev: the launchd/systemd host on 19223 and `~/.codync-dev`, the one a physical iPhone pairs with. A linked worktree (its `.git` is a file) runs a sandbox beside it, so any number of worktrees and agents test at once:

- Host data in `~/.codync-dev-<worktree folder>`, which also holds the app's session, account and single-instance lock (`desktop/`). The window and menu bar read `Codync Dev (<name>)`.
- A port from 19300–19399, picked free on the first run and kept in that folder's `sandbox.json`, so a paired simulator stays paired.
- A host built from the worktree's own code (`node tools/native-host.mjs dev`; on Linux `cargo build`, honoring `CARGO_TARGET_DIR`; or `CODYNC_HOST_BIN`), started and stopped with the app, logging to `host.log`. No launchd/systemd service, and the dev cloud is off.
- The run prints the commands that pair a phone or simulator (`codync-host pair`) and open the terminal UI against the sandbox. Give each sandbox its own simulator: a simulator holds one Codync Dev.

Flags go after `npm run dev --`: `--shared` takes the default Codync Dev from a worktree (quit the main checkout's first); `--sandbox` runs a sandbox from the main checkout, for a second session on `dev`; `--name <name>` runs the sandbox of that name, for another copy of the same checkout; `--cloud` lets the sandbox host use the dev cloud (each sandbox then shows as another computer); `--cdp` opens the DevTools protocol on the sandbox port + 10000; `--list` shows every sandbox; `--prune` deletes the ones whose worktree is gone (and their Clerk test accounts), so run it after removing worktrees.

A sandbox starts past onboarding (welcome and computer-access setup, only where the window hasn't chosen yet); `--onboarding` clears those two settings once per start, so the flow comes back (a signed-in sandbox passes the welcome screen by itself and lands on computer-access setup). With `CODYNC_CLERK_SECRET_KEY` (the Clerk **development** instance's `sk_test_...` key) in the git-ignored `.env.development` at the main checkout's root (copy `.env.development.example`) (every worktree reads that one file) or the environment, it also signs in its own Clerk test account `sandbox-<name>+clerk_test@example.com`: `tools/dev-account.mjs` finds or creates that user through the Backend API and mints a one-time sign-in token, which `account.ts` trades for a session (`ticket` strategy, unpackaged Dev runs only). The token is minted at every start and used only while the sandbox has no active session; the app also requires a `pk_test_` publishable key, and the key never reaches the host or the agents. Keys that aren't `sk_test_` are refused. Without a key a sandbox starts signed out; it never borrows Codync Dev's login.

Still shared: macOS sees every unpackaged run as the same Electron app, so permissions and notifications are shared and a `codync-dev://` sign-in callback may reach another copy (test sign-in on the default Codync Dev); Codync Screen and computer use need the packaged `Codync Dev.app`; a global shortcut (the approval island's) goes to whichever app registered it first.

## Local secrets

Secrets for running the project locally live in git-ignored env files at the main checkout's root, one per environment: `.env.development` for the Clerk and Cloudflare **development** instances, `.env.production` for production. `.gitignore` covers `.env` and `.env.*` but not `.env.*.example`, so each file has a committed template (`.env.development.example`) listing its variables with placeholders; copy it and fill it in. Linked worktrees read the main checkout's file (through `git rev-parse --git-common-dir`), so a new worktree needs no setup. Real values never go in a template, a commit or a log.

- Dev tooling reads `.env.development` only and never loads `.env.production`; a variable already in the environment wins over the file.
- A development key is checked before use (`CODYNC_CLERK_SECRET_KEY` must be `sk_test_...`), so a production key put there by mistake is refused.
- Deployed secrets (the Worker's `CLERK_SECRET_KEY` and the like) are set with `wrangler secret put`, not kept in these files.
- Variables so far: `CODYNC_CLERK_SECRET_KEY` (see [Worktree sandboxes](#worktree-sandboxes)).

## Resource ownership

| Resource | Production | Development |
| --- | --- | --- |
| Cloud | `https://api.codync.dev` | `https://dev-api.codync.dev` |
| iOS bundle | `com.pokai.Codync.ios` | `com.pokai.Codync.ios.dev` |
| iOS App Group | `group.com.pokai.Codync` | `group.com.pokai.Codync.dev` |
| iOS Keychain group (team prefix omitted) | `com.pokai.Codync` | `com.pokai.Codync.dev` |
| Desktop bundle | `com.pokai.Codync` | `com.pokai.Codync.dev` |
| Android application ID | `com.pokai.Codync.android` | `com.pokai.Codync.android.dev` |
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

Since 2026-10-10 the Clerk development instance lists `7FUM8A8H72.com.pokai.Codync.ios.dev` and `7FUM8A8H72.com.pokai.Codync.dev` under Native applications, with their `://callback` redirects. Without the iOS entry, Clerk rejects the Dev app's native Apple token: the log shows `sign_up.failed` with "You are not authorized to perform this request", and the app shows only "Couldn't sign in". Check the instance's Logs page first when sign-in fails.

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

A local build from before the Dev split (2.11.x and earlier) uses the development cloud even when it is named **Codync**. Its `~/.codync` identity stays claimed in the development account, so Codync Dev phones list it and request access to it. Replace such a build with a release build, then unclaim that identity from the development account. Observed on 2026-10-10: a 2.11.2 local build ran as this Mac's production host.

## Local verification (2026-10-09)

- iOS Dev built, signed and launched on a physical iPhone. The app and both extensions use the Dev App Group; the signed app uses the separate Dev Keychain group and development APNs entitlement.
- The production iOS Release simulator build succeeded with its existing bundle identifiers, App Group and URL scheme. The Swift package's 55 tests passed.
- A signed macOS Dev app launched into an independent empty workspace. The existing host on 19222 and Dev host on 19223 ran together with different computer identities. Starting and stopping Dev left the original launch agent, token and Claude Code settings unchanged.
- Desktop type checking and regression tests passed; Rust tests and Clippy passed. Windows and Linux identities and package configuration were checked in tests, but their installed apps have not been exercised on those operating systems.
- Live Apple account sign-in, connector callbacks and APNs delivery require the external setup and deployments above. Local build success does not verify those flows.
