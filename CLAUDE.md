# CodePulse

## Language & Syntax

- **Swift 6** strict concurrency mode — use latest Swift 6 syntax throughout
- Prefer SwiftUI lifecycle and modern APIs (e.g., `MenuBarExtra`, `@Observable`, `@State`, `@Environment`)
- Use structured concurrency (`async/await`, `TaskGroup`) over Combine where possible
- Use `sending`, `nonisolated`, `@MainActor` correctly per Swift 6 rules
- Avoid `@unchecked Sendable` — prefer proper `Sendable` conformance

## Architecture

- macOS menu bar app using `NSStatusItem` + `NSPopover`
- **JSONL transcript parsing** (like Pixel Agents) for state detection — zero impact on Claude Code
- Single `Notification` command hook (`~/.codepulse/notify.sh`) for instant permission detection only
- Shell script hook uses `curl --max-time 1 || true` — never blocks Claude Code even if app is not running
- Shared code in `CodePulseShared` Swift Package (macOS 14+ / iOS 17+)
- **Do NOT register HTTP hooks** — they block Claude Code on ECONNREFUSED when app is down
- **Do NOT register PreToolUse or PostToolUse hooks** — they are synchronous and block tool execution

## Targets

- `CodePulse-macOS` — macOS menu bar app
- `CodePulse-iOS` — iOS companion app
- `CodePulseLiveActivity` — iOS Live Activity widget
- `CodePulseShared` — shared models and CloudKit logic
