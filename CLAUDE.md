# Codync

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (e.g., `MenuBarExtra`, `@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine where possible
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance

## Architecture

- macOS menu bar app using `NSStatusItem` + `NSPopover`
- **Hook-driven status detection** — 7 Claude Code hooks (Notification, Stop, UserPromptSubmit, PreToolUse, PostToolUse, SessionStart, SessionEnd) provide instant, accurate state
- JSONL transcript parsing retained for supplementary data only (model, tokens, cost, tool display)
- Command hook script (`~/.codync/notify.sh`) reads stdin, POSTs to local server, exits instantly (~20ms)
- Shell script hook uses `curl --max-time 1 || true` — never blocks Claude Code even if app is not running
- Shared code in `CodyncShared` Swift Package (macOS 14+ / iOS 17+)
- **Do NOT register HTTP hooks** — they block Claude Code on ECONNREFUSED when app is down
- Command hooks for PreToolUse/PostToolUse are safe — script exits in <20ms, imperceptible delay

## App Store Upload

- **IMPORTANT**: Every time you archive and upload a new build to App Store Connect, you MUST increment `CURRENT_PROJECT_VERSION` in the Xcode project first. App Store Connect rejects duplicate build numbers.

## Targets

- `Codync-macOS` — macOS menu bar app
- `Codync-iOS` — iOS companion app
- `CodyncLiveActivity` — iOS Live Activity widget
- `CodyncShared` — shared models and CloudKit logic
