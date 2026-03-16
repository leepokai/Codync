# CodePulse Design Spec

## Overview

CodePulse is a macOS menu bar app that monitors Claude Code sessions in real-time and syncs session state to iPhone via iCloud, displaying progress on Dynamic Island and Lock Screen through Live Activities. Apple Watch receives the Live Activity automatically via watchOS 11+ mirroring.

## Goals

- Zero-config monitoring of Claude Code sessions from the macOS menu bar
- Real-time task progress displayed as a segmented progress bar
- iPhone Live Activity (Dynamic Island + Lock Screen) showing session progress
- Automatic Live Activity switching when a session completes
- Haptic notification on session completion (iPhone vibration + Watch taptic)
- Open source + App Store distribution (Universal Purchase)
- Zero server infrastructure — all sync via CloudKit Private Database

## Non-Goals

- Monitoring non-Claude-Code terminal sessions (V1)
- Hooks integration for faster updates (V2)
- Multiple simultaneous Live Activities (V2)
- watchOS native app (automatic mirroring is sufficient)
- AI-generated summaries (Claude Code already provides structured data)

## Architecture

### Approach: FSEvents + CloudKit (V1), Hooks optional (V2)

V1 uses FSEvents file watching for zero-config operation. V2 adds optional Claude Code hooks for lower-latency updates.

### Filesystem Access

The macOS app must read files under `~/.claude/`, which is outside the App Sandbox container. Options:

1. **Disable App Sandbox** (recommended for V1) — common for developer tools. Requires entitlement exception justification for App Store review.
2. **Security-scoped bookmark** — prompt user to grant access to `~/.claude/` via Open Panel on first launch, persist the bookmark. More complex but sandbox-compatible.

V1 will disable App Sandbox. This is standard practice for developer tools (e.g., Command, Tower, iTerm2).

### Data Sources (Claude Code files on macOS)

Path encoding: `~/.claude/projects/<mangled-cwd>/` where `<mangled-cwd>` is the session's `cwd` with all `/` replaced by `-` (e.g., `/Users/foo/myproject` → `-Users-foo-myproject`).

| File | Path | Data |
|------|------|------|
| Active sessions | `~/.claude/sessions/<PID>.json` | pid, sessionId, cwd, startedAt |
| Tasks (primary) | `~/.claude/tasks/<sessionId>/` | Per-task files with id, subject, description, status, activeForm, blocks, blockedBy |
| Todos (fallback) | `~/.claude/todos/<sessionId>-agent-*.json` | [{content, status, activeForm}] — may be empty |
| Session index | `~/.claude/projects/<mangled-cwd>/sessions-index.json` | summary, firstPrompt, messageCount, gitBranch |
| Conversation log | `~/.claude/projects/<mangled-cwd>/<sessionId>.jsonl` | message.model, usage.input_tokens/output_tokens (read tail only) |

**Computed fields (not directly available in files):**

- **context%**: Compute from `usage.input_tokens + usage.cache_read_input_tokens` in the latest assistant message, divided by model context window size (e.g., 200K for Opus, 200K for Sonnet). This is an estimate.
- **cost (USD)**: Compute by accumulating token usage across assistant messages × per-model pricing. Pricing is hardcoded and must be updated when Anthropic changes rates. Consider making this configurable or fetching from a bundled config.

### Data Flow

```
Claude Code writes files to ~/.claude/
  → macOS app watches via FSEvents (debounce 1s)
    → Parses tasks/ + session JSON + JSONL tail
      → Computes context% and cost from token usage
        → Aggregates into SessionState
          → Writes to CloudKit Private DB (debounce 2s, only on change)
            → CKSubscription pushes to iPhone
              → iPhone updates Live Activity
              → Watch mirrors automatically
```

Total latency: ~5-10 seconds from Claude Code state change to iPhone display. CKSubscription delivery is best-effort and may be delayed by iOS, especially in low-power mode. When a Live Activity is active, the iOS app also polls CloudKit every 30 seconds as a fallback to ensure updates are not missed.

### CloudKit Schema

Single record type:

```
Record Type: SessionState
├── sessionId    (String, indexed)
├── projectName  (String)
├── gitBranch    (String)
├── status       (String: working/idle/needsInput/error/completed)
├── model        (String: Opus/Sonnet/Haiku)
├── summary      (String: from sessions-index.json)
├── currentTask  (String: activeForm of in_progress task)
├── tasks        (Data: JSON encoded [TaskItem], truncated to last 10 tasks for 4KB limit)
├── contextPct   (Int, computed)
├── costUSD      (Double, computed)
├── startedAt    (Date)
├── durationSec  (Int, computed from startedAt)
├── deviceId     (String, identifies which Mac — supports multi-Mac setups)
└── updatedAt    (Date)
```

## Project Structure

```
CodePulse/
├── CodePulse-macOS/              ← macOS Menu Bar app
│   ├── App/
│   │   ├── CodePulseApp.swift
│   │   └── MenuBarController.swift
│   ├── Services/
│   │   ├── SessionScanner.swift      ← FSEvents watcher
│   │   ├── SessionStateManager.swift ← Aggregation + debounce
│   │   └── CloudKitSync.swift        ← Write to CloudKit
│   ├── Views/
│   │   ├── SessionListView.swift     ← Command-style session list
│   │   ├── SessionRowView.swift      ← Status dot + summary + time
│   │   ├── SessionDetailView.swift   ← Progress bar + tasks + stats
│   │   └── StatusDotView.swift       ← Animated status indicator
│   └── Utilities/
│       ├── PIDChecker.swift
│       └── JSONLTailReader.swift
│
├── CodePulse-iOS/                ← iPhone app (shell + Live Activity control)
│   ├── App/
│   │   └── CodePulseIOSApp.swift
│   ├── Services/
│   │   ├── CloudKitReceiver.swift    ← CKSubscription listener
│   │   └── LiveActivityManager.swift ← Activity lifecycle
│   └── Views/
│       ├── SessionListView.swift
│       ├── SessionDetailView.swift
│       └── OnboardingView.swift
│
├── CodePulseLiveActivity/        ← Widget Extension target (separate Xcode target)
│   ├── CodePulseLiveActivity.swift
│   ├── LockScreenView.swift
│   └── DynamicIslandView.swift
│
└── CodePulseShared/              ← Shared framework
    ├── Models/
    │   ├── SessionState.swift
    │   ├── TaskItem.swift
    │   └── SessionStatus.swift
    └── CloudKit/
        ├── CloudKitManager.swift
        └── CKRecordMapper.swift
```

No watchOS target needed — watchOS 11+ mirrors iPhone Live Activities automatically.

## macOS App Design

### Menu Bar

- Icon: pulse symbol + active session count badge
- Click or ⌘. to open popover
- Menu bar only (no Dock icon)
- Optional: Login item for auto-start

### Session List (Command-style)

Each row shows:
- Status dot (animated): working (green), idle (cyan), needs input (orange), error (red pulse), completed (gray)
- 5-word summary (from sessions-index.json summary or firstPrompt)
- Claude tag badge
- Relative time ("now", "2m ago", "1h ago")
- Click to navigate to detail view

### Session Detail

- Back button to session list
- Status indicator
- Project name, git branch, model
- Segmented progress bar: each segment = one task, colored by status
  - Green: completed
  - Cyan (animated): in_progress
  - Gray: pending
- Full task list with status icons
- Stats row: context %, cost (USD), duration

### Status Detection Logic

```
PID not alive                          → completed
JSONL last entry has tool error/exit≠0 → error
Has in_progress task                   → working
JSONL growing (new bytes in last 10s)  → working
No JSONL growth for >30s              → needsInput (likely waiting for user)
Otherwise                              → idle
```

Note: "needsInput" is a best-effort heuristic. Claude Code may pause for various reasons (thinking, permission prompt, rate limit). This is acceptable for V1 — hooks in V2 will provide precise state.

### First Launch Behavior

On first launch, the macOS app scans all existing `~/.claude/sessions/*.json` files, checks which PIDs are still alive, and begins monitoring those sessions. Sessions started before CodePulse was installed are picked up immediately.

### Performance Targets

- Memory: < 30MB
- Binary size: < 2MB
- JSONL reading: tail only (last ~50 lines), never full file
- FSEvents debounce: 1 second
- CloudKit write debounce: 2 seconds

## iPhone App Design

### Screens

1. **Onboarding** — shown when CloudKit has no data. "Install CodePulse on your Mac to start monitoring."
2. **Session List** — similar to Mac, with mini progress bar per row. LIVE badge on tracked session.
3. **Session Detail** — full progress bar + task list + stats + Live Activity toggle switch.

### Live Activity

#### Dynamic Island — Compact
- Status dot + segmented progress bar + count (e.g., "5/8")

#### Dynamic Island — Expanded (long press)
- Session name + status
- Full progress bar
- Current task name
- Model + context% + cost

#### Lock Screen
- App icon + session name + status
- Full progress bar
- Current task + count
- Model + context% + cost + duration

#### ActivityAttributes

```swift
struct CodePulseAttributes: ActivityAttributes {
    let sessionId: String
    let projectName: String

    struct ContentState: Codable, Hashable {
        let status: String
        let model: String
        let tasks: [TaskItem]
        let completedCount: Int
        let totalCount: Int
        let currentTask: String?
        let contextPct: Int
        let costUSD: Double
        let durationSec: Int
    }
}
```

#### Content Size Limit

Apple enforces a 4KB limit on `ActivityAttributes.ContentState` updates. The `tasks` array is truncated to the 10 most recent tasks, with task content capped at 50 characters each. If more detail is needed, the user taps to open the full app.

### Auto-Switch Logic

1. Default: track the most recently started session
2. User can manually select a session to track in the app
3. When tracked session completes:
   - Send haptic notification (iPhone vibration + Watch .success taptic)
   - Auto-switch to next working session
4. When all sessions complete:
   - End Live Activity
   - Final notification: "All N sessions completed · Total $X.XX"

## Shared Layer

### Models

- `SessionState` — core data model shared between macOS and iOS
- `TaskItem` — individual task with content, status, activeForm
- `SessionStatus` — enum: working, idle, needsInput, error, completed (with color/icon helpers)

### CloudKit

- `CloudKitManager` — CRUD operations (save on macOS, fetch on iOS, subscribe on iOS)
- `CKRecordMapper` — SessionState ↔ CKRecord conversion

### Sync Strategy

- Mac writes: debounce 2s, diff check, only write on actual change
- iOS reads: CKSubscription push triggers background fetch → update Live Activity
- Cleanup: completed sessions auto-delete after 24 hours, max 20 records
- Offline: Mac queues locally, iPhone shows last known state + "offline" label

## CloudKit Configuration

- Container identifier: `iCloud.com.pokai.CodePulse`
- Must be created in Apple Developer portal
- Both macOS and iOS targets include the CloudKit entitlement
- Bundle IDs: `com.pokai.CodePulse` (macOS), `com.pokai.CodePulse.ios` (iOS), `com.pokai.CodePulse.ios.LiveActivity` (widget extension)

## Privacy

- All data in CloudKit Private Database (user's own iCloud)
- No conversation content synced — only summary, task names, and stats
- Developer has zero access to user data
- No analytics, no telemetry, no server

## Distribution

- Open source on GitHub (MIT license)
- App Store via Universal Purchase (macOS + iOS in one listing)
- Free

## V2 Roadmap (out of scope for V1)

- Claude Code hooks integration for lower-latency updates (~instant vs ~3-5s)
- Multiple simultaneous Live Activities (user selects multiple sessions to track)
- Mac desktop WidgetKit widget
- Session history / statistics dashboard on iPhone
- iPad support
