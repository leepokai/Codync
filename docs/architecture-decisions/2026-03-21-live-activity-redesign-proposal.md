# Live Activity Redesign Proposal (2026-03-21)

## Current Issues
- Multiple Live Activities (up to 4) with complex tracking, grace periods, auto-fill
- iOS controls which one shows on Dynamic Island (unpredictable)
- No way to show all sessions at once

## Proposed Design

### Two Modes

**Mode A: Overall (unified Live Activity)**
- Single Live Activity showing ALL tracked sessions as a compact list
- User selects how many sessions to show (1-N, system adapts layout)
- Lock Screen shows all sessions with status indicators

**Mode B: Individual (current approach)**
- Separate Live Activity per session
- Same as current but simplified

### Primary Session
- User designates ONE session as "primary"
- Primary session shown on:
  - Dynamic Island (compact + expanded)
  - Apple Watch (future)
- If primary session killed (not found in CloudKit) → auto-select next working session
- Only ONE primary at a time, persisted via CloudKit

### Layout Adaptation
- 1 session: full detail view (tool cards, stacking, timer)
- 2-3 sessions: medium rows (status + project + current tool)
- 4+ sessions: compact rows (status dot + project name only)

### Data Model Changes
```swift
// New ContentState for Overall mode
struct OverallContentState {
    let sessions: [SessionSummary]
    let primarySessionId: String?
    let totalCost: Double
}

struct SessionSummary {
    let sessionId: String
    let projectName: String
    let status: String
    let currentTask: String?
    let model: String
    let costUSD: Double
}
```

### Primary Session Detection
- Stored as CloudKit record: `PrimarySession`
- macOS checks if PID is alive → if dead, CloudKit record deleted
- iOS detects deletion → auto-promotes next working session

### Implementation Order
1. Design the two Live Activity layouts (overall + individual)
2. Add primary session CloudKit record
3. Rewrite LiveActivityManager for dual-mode support
4. Add mode selector UI in iOS app
5. Apple Watch support (future)

## Questions to Resolve
- Max sessions to display? (Lock Screen height ~160pt limits to ~5-6 rows)
- Should overall mode show tool stacking for primary session?
- Watch complication design?
