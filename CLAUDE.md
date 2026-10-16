# Codync

Bot-based remote for coding agents: persistent named bots on your computer, messaged from the iPhone (UI patterns from Grok Bot).

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (`@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance
- Desktop app (`apps/desktop/`) is Electron + React + strict TypeScript: state in `Observable` models (`src/renderer/store/`), host calls through the `window.codync` bridge (`src/shared/ipc.ts`), main-process code in `src/main/`; CI runs `npm run typecheck` and `npm test`
- Host is Rust 2024 edition and follows the `rust-skills` rules (`~/.agents/skills/rust-skills`). Lints live in `host/Cargo.toml` (`[lints]`: default groups + pedantic, `unwrap_used`); CI runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- Host conventions: no `unwrap()` outside tests (`expect("why this can't fail")` for true invariants); lock std mutexes with `LockExt::locked()` (poison-tolerant); enums, not strings, for states and modes (`BotStatus`, `Permission`, `EntryKind`, `AlertKind`); `tracing` with structured fields (`error = format!("{e:#}")` keeps the context chain); blocking fs/process work goes through `spawn_blocking`; registry JSON is untrusted (paths are validated)

## Clean code

- Keep code modular and clean on every change: reuse before writing, one responsibility per file/type, no new code appended to files past ~500 lines (split first), views/components hold no I/O or logic, short single-purpose functions, enums over flags, no dead code, no swallowed errors. Full rules: [docs/guides/clean-code.md](docs/guides/clean-code.md).

## Architecture

Clients share host API behavior across iOS, desktop and terminal UI; screen
capture uses native helpers. Architecture, runtime and environment details:
[agent architecture](docs/reference/agent-architecture.md).
UI controls and platform exceptions: [UI conventions](docs/design/ui-conventions.md).

## Cross-platform UI changes

- Any UI change in any client must include the corresponding updates to the other clients in the same change: iOS (`apps/ios/`, `apps/ios/Kit/Sources/CodyncUI/`), desktop (`apps/desktop/src/renderer/`, macOS, Linux and Windows), and terminal UI (`host/src/tui/`). This applies in every direction.
- Keep shared features, actions, terminology, displayed information, and loading, empty, error, and permission states consistent. Adapt layout, controls, and input to each platform, including terminal keyboard interaction, while preserving the same user-facing behavior.
- Inspect every client's corresponding implementation before finishing a UI task. Implement applicable changes together; do not silently defer another client. For a platform-only change or an unsupported capability, document which clients are unaffected and the concrete reason in the change summary.
- Validate each affected client with its relevant build/tests and UI checks. Report any checks that could not run and why.
- The desktop app is one codebase for macOS, Linux and Windows: platform differences go through `window.codync.platform` checks or the main process, never a fork of a view.

## Codync 1.x does not exist for us

- Ignore everything from Codync 1.x (the Claude Code hooks + CloudKit session monitor): no migration, no compatibility shims, no cleanup of its files or hooks, no keeping old workers or App Store copy alive for it. Don't mention 1.x in code, docs or release notes.
- Build only the current design; don't reintroduce hooks or CloudKit.
- Don't carry legacy along. Old names, settings, schemes, files or code paths left from earlier designs get renamed or deleted outright when you meet them, not kept "for compatibility". Put full effort into the new design.

## Installing a new build: kill the old one first

Always stop the old desktop app, host and iPhone process before running a new build (old host = old protocol, old app = old UI); commands in [development: desktop app](docs/guides/development.md#desktop-app) and [iOS app](docs/guides/development.md#ios-app).

Keep only the latest build: in this checkout, Apple builds go to `build/dd` only (no other `-derivedDataPath`, no copies in scratchpads, `/tmp` or Xcode's DerivedData); delete any older Codync build right away, so macOS never launches a stale copy. A git worktree may keep its own single build inside that worktree.

## End-to-end test every change

After every change, test it end to end on the real apps, not just builds and unit tests: install the new build (old one stopped first) and walk through the changed flow. Drive the desktop app with Computer Use, and the iPhone app on an iOS Simulator through Device Hub (Xcode's `DeviceHub.app`). Report any check that could not run and why.
- **Minimal e2e** (default): only the platform of the computer you test on (on the Mac: macOS).
- **Full e2e** (large changes: platform code, packaging, the desktop main process; or when asked): macOS, Linux (Lima `codync-linux`) and Windows (UTM, [Windows testing](docs/guides/windows-testing.md)). A platform whose VM isn't installed is skipped and reported, not set up.

## Project generation

- `apps/project.yml` + `xcodegen generate --spec apps/project.yml` produce `apps/Codync.xcodeproj`. Edit `project.yml`, not the pbxproj.

## App Store Upload

- Xcode Cloud archives the `iOS` scheme on every `v*` tag and uploads it to App Store Connect with its own build number, then the `Submit iOS` workflow submits it to App Review for the App Store; don't upload from the local machine or edit `CURRENT_PROJECT_VERSION` for it. Details: [development guide](docs/guides/development.md#ios-releases-xcode-cloud).

## Versioning

- Phone ↔ host compatibility: each side names the oldest version of the other it works with (`minApp` in the host's `hello`, `minHost` in each client). Additive changes need nothing; a rename/removal raises `minApp` in the same release, and hosts hold that release until the App Store has the iPhone app: [docs/reference/compatibility.md](docs/reference/compatibility.md)
- **Every source change ships as a release.** A version bump on `main` is the only release trigger (Auto Tag → desktop app for macOS, Linux and Windows, host, Homebrew, in-app update), so any change to shipped code (`apps/`, `host/`, `packaging/`) bumps the version before it reaches `main`, without being asked. Docs, `web/`, `cloud/`, `relay/` and CI-only changes don't bump.
- Pick the bump yourself: patch for fixes and small tweaks, minor for new features or protocol additions. Never bump major (stay on 2.x). One bump per merge into `main`: if `MARKETING_VERSION` is already ahead of the latest `v*` tag, leave it.
- How: set `MARKETING_VERSION` in `apps/project.yml` and `version` in `host/Cargo.toml` (+ its `Cargo.lock` entry) and `apps/desktop/package.json` (+ `package-lock.json`) to the same value, run `xcodegen generate --spec apps/project.yml`, commit as `build: bump version to X.Y.Z`. Never touch `CURRENT_PROJECT_VERSION` (Xcode Cloud sets the iOS build number). When the iOS app changes, also write that version's section in `apps/ios/WhatsNew.md` (zh-Hant + en-US, what iPhone users notice); it becomes the App Store "What's New".

## Layout & naming

Build targets, folder layout, file naming and shared terms: [docs/architecture/file-structure.md](docs/architecture/file-structure.md). Follow it when adding or moving files.

## Several agents share `dev`

- Several agents often work in this checkout on `dev` at once. When you start a task, name your session after its area (`/rename kit-markdown`, `host-voice`, …; ask the user if you can't rename yourself) so others can find you in `ListAgents`.
- Before starting any task, sync with origin: `git pull --rebase --autostash origin dev`, then push any finished local commits that aren't on `origin/dev` yet (`git push origin dev`). The pull is tree-wide, so the next rule applies; if the autostash conflicts, keep both sides' intent and never drop another agent's hunk.
- Before touching files with someone else's uncommitted changes, or anything tree-wide (renames, `xcodegen`, version bump, `git stash`/`reset`/`checkout`), `SendMessage` the agents involved with what you'll change and wait for or answer their replies. Never discard, revert or reformat hunks that aren't yours.
- External contributors open PRs against `dev`, never `main`. Retarget a contributor PR aimed at `main` to `dev` before reviewing or merging it.
- An external PR is merged only after its author has verified it end to end locally (real host + the clients it touches, not just builds/tests) and says so in the PR's *Validation*; ask for it in the review if missing.

## Commit messages

Follow the Conventional Commit and PR rules in [AGENTS.md](AGENTS.md#commit--pull-request-guidelines); stage only your own changes. The pre-commit hook (`git config core.hooksPath .githooks`) runs CI's checks for the parts a commit touches ([pre-commit checks](docs/guides/development.md#pre-commit-checks)); fix what it reports instead of `--no-verify`.

## Keeping this file short

- CLAUDE.md and AGENTS.md hold only rules an agent needs on every task. Reference material (file structure, naming tables, API details, audits, how-tos) goes in `docs/` as its own file, with a one-line pointer here.
- Whenever you edit either file, check its length: past ~100 lines, or a section past a few lines of reference detail, refactor that detail into `docs/` and leave the pointer, without being asked.
- Keep `docs/` current: update the doc in the same change that makes it stale.
