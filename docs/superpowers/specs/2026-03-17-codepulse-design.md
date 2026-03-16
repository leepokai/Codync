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

### Data Sources (Claude Code files on macOS)

| File | Path | Data |
|------|------|------|
| Active sessions | `~/.claude/sessions/<PID>.json` | pid, sessionId, cwd, startedAt |
| Todo/task progress | `~/.claude/todos/<sessionId>-agent-*.json` | [{content, status, activeForm}] |
| Session index | `~/.claude/projects/<project>/sessions-index.json` | summary, firstPrompt, messageCount, gitBranch |
| Conversation log | `~/.claude/projects/<project>/<sessionId>.jsonl` | model, context%, cost (read tail only) |

### Data Flow

```
Claude Code writes files to ~/.claude/
  → macOS app watches via FSEvents (debounce 2s)
    → Parses todos JSON + session JSON + JSONL tail
      → Aggregates into SessionState
        → Writes to CloudKit Private DB (debounce 2s, only on change)
          → CKSubscription pushes to iPhone
            → iPhone updates Live Activity
            → Watch mirrors automatically
```

Total latency: ~3-5 seconds from Claude Code state change to iPhone display.

### CloudKit Schema

Single record type:

```
Record Type: SessionState
├── sessionId    (String, indexed)
├── projectName  (String)
├── gitBranch    (String)
├── status       (String: working/idle/needsInput/completed)
├── model        (String: Opus/Sonnet/Haiku)
├── summary      (String: from sessions-index.json)
├── currentTask  (String: activeForm of in_progress task)
├── tasks        (Data: JSON encoded [TaskItem])
├── contextPct   (Int)
├── costUSD      (Double)
├── durationSec  (Int)
├── deviceId     (String)
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
│   ├── Views/
│   │   ├── SessionListView.swift
│   │   ├── SessionDetailView.swift
│   │   └── OnboardingView.swift
│   └── CodePulse-LiveActivity/   ← Widget Extension target
│       ├── CodePulseLiveActivity.swift
│       ├── LockScreenView.swift
│       └── DynamicIslandView.swift
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
- Status dot (animated): working (green), idle (cyan), needs input (orange), error (red), completed (gray)
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
PID not alive         → completed
Has in_progress task  → working
Last message is assistant + >30s idle → needsInput
Otherwise             → idle
```

### Performance Targets

- Memory: < 30MB
- Binary size: < 2MB
- JSONL reading: tail only (last ~50 lines), never full file
- FSEvents debounce: 2 seconds
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
- `SessionStatus` — enum: working, idle, needsInput, completed (with color/icon helpers)

### CloudKit

- `CloudKitManager` — CRUD operations (save on macOS, fetch on iOS, subscribe on iOS)
- `CKRecordMapper` — SessionState ↔ CKRecord conversion

### Sync Strategy

- Mac writes: debounce 2s, diff check, only write on actual change
- iOS reads: CKSubscription push triggers background fetch → update Live Activity
- Cleanup: completed sessions auto-delete after 24 hours, max 20 records
- Offline: Mac queues locally, iPhone shows last known state + "offline" label

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
