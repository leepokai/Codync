# CodePulse Minimal UI Redesign Spec

## Goal

Redesign CodePulse menu bar popover to a minimal Apple-style aesthetic with grayscale base, blue accent for progress, orange for attention, independent dark/light mode toggle, and a dual-mode status indicator (sparkle vs progress ring).

## Color System

- **Base**: Grayscale only (backgrounds, text, dividers, borders)
- **Blue accent**: Progress ring fill, active state indicators (system blue `Color.accentColor`)
- **Orange warning**: needsInput indicator dot (`.orange`)
- **No other colors** — SessionTagView (Opus/Sonnet/Haiku) becomes grayscale pills, stats bar is grayscale

## Dark/Light Mode

- Independent toggle stored in `UserDefaults` (key: `codepulse_darkMode`)
- NOT following system appearance — CodePulse has its own switch
- Dark: `#1C1C1E` background, `#F5F5F7` primary text, `#8E8E93` secondary text
- Light: `#FFFFFF` background, `#1D1D1F` primary text, `#86868B` secondary text
- Toggle location: small icon button in the list view header (sun/moon)

## Row Status Indicator (Left Circle) — Dual Mode

### Mode A: No Tasks
- **working** → Claude sparkle animation (grayscale `· ✢ ✳ ✶ ✻ ✽` cycle, matching text color)
- **idle** → Static gray dot
- **needsInput** → Orange pulsing dot
- **completed** → Dim gray dot (low opacity)

### Mode B: Has Tasks (>0 tasks)
- Circular progress ring (blue arc, proportion = completedTasks / totalTasks)
- **working** → Ring edge has sparkle/shimmer effect (subtle blue glow animation)
- **idle** → Static ring, no animation
- **needsInput** → Static ring + small orange dot overlay (top-right of ring)
- **completed** → Full blue ring (100%), static

Ring size: same diameter as current StatusDotView (~14pt)

## Views to Modify

### SessionListView
- Add dark/light mode toggle button in header
- Apply theme colors via environment
- Remove colored accents from footer

### SessionRowView
- Grayscale text, remove hover color effects or make them subtle gray
- SessionTagView → gray pill with gray text (no purple/blue/green)
- Time label → secondary gray
- Subtitle (lastEvent) → secondary gray, no status-colored text

### StatusDotView → SessionStatusView (rename)
- Implement dual-mode logic: check if session has tasks
- Mode A: sparkle/dot (grayscale + orange for needsInput)
- Mode B: progress ring (blue + sparkle edge when working + orange dot when needsInput)

### SessionDetailView
- Stats bar (Context/Cost/Duration) → grayscale
- Task list icons: completed=blue checkmark, inProgress=blue dot, pending=gray circle
- Progress circle → blue ring (matching row indicator style)

### SessionTagView
- Remove purple/blue/green backgrounds
- Use gray background + gray text for all models

## Theme Implementation

```swift
struct CodePulseTheme {
    let isDark: Bool
    var background: Color
    var primaryText: Color
    var secondaryText: Color
    var separator: Color
    var cardBackground: Color
    var accent: Color = .blue
    var warning: Color = .orange
}
```

Injected via `.environment()` or `@AppStorage("codepulse_darkMode")`.

## What NOT to Change

- Data flow (SessionStateManager, TranscriptWatcher, etc.)
- Popover size and behavior
- MenuBarController icon and badge
- CloudKit sync
