# Minimal UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign CodePulse popover to minimal Apple-style aesthetic with grayscale base, blue/orange accents, independent dark/light toggle, and dual-mode status indicator (sparkle vs progress ring).

**Architecture:** Add `CodePulseTheme` with `@AppStorage` dark mode toggle. Rewrite `StatusDotView` → `SessionStatusView` with dual-mode logic (sparkle when no tasks, progress ring when tasks exist). Update all views to use grayscale + blue accent + orange warning only.

**Tech Stack:** SwiftUI, `@AppStorage`, `Color` extensions

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| **Create** | `CodePulse-macOS/Views/Theme.swift` | `CodePulseTheme` color definitions + environment key |
| **Rewrite** | `CodePulse-macOS/Views/StatusDotView.swift` | Rename to `SessionStatusView` — dual-mode (sparkle / progress ring) |
| **Modify** | `CodePulse-macOS/Views/SessionRowView.swift` | Grayscale colors, remove `CircularProgressView` (merged into status indicator) |
| **Modify** | `CodePulse-macOS/Views/SessionListView.swift` | Add dark/light toggle, apply theme |
| **Modify** | `CodePulse-macOS/Views/SessionDetailView.swift` | Grayscale stats, blue task icons |
| **Modify** | `CodePulse-macOS/Views/SessionTagView.swift` | Grayscale pill for all models |
| **Modify** | `CodePulse-macOS/App/MenuBarController.swift` | Inject theme into hosting controller |

---

## Chunk 1: Theme + StatusView

### Task 1: Create CodePulseTheme

**Files:**
- Create: `CodePulse-macOS/Views/Theme.swift`

- [ ] **Step 1: Create Theme.swift**

```swift
import SwiftUI

struct CodePulseTheme {
    let isDark: Bool

    var background: Color {
        isDark ? Color(red: 0.11, green: 0.11, blue: 0.12) : .white
    }
    var cardBackground: Color {
        isDark ? Color.white.opacity(0.05) : Color.black.opacity(0.03)
    }
    var primaryText: Color {
        isDark ? Color(red: 0.96, green: 0.96, blue: 0.97) : Color(red: 0.11, green: 0.11, blue: 0.12)
    }
    var secondaryText: Color {
        isDark ? Color(red: 0.56, green: 0.56, blue: 0.58) : Color(red: 0.53, green: 0.53, blue: 0.55)
    }
    var separator: Color {
        isDark ? Color.white.opacity(0.1) : Color.black.opacity(0.08)
    }
    var accent: Color { .blue }
    var warning: Color { .orange }
}

// MARK: - Environment

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = CodePulseTheme(isDark: false)
}

extension EnvironmentValues {
    var theme: CodePulseTheme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
```

- [ ] **Step 2: Add to Xcode project, verify build**

```bash
xcodebuild -scheme CodePulse -destination 'platform=macOS,arch=arm64' build 2>&1 | grep -E "(error:|BUILD)"
```

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Views/Theme.swift CodePulse.xcodeproj/project.pbxproj
git commit -m "feat: add CodePulseTheme with dark/light mode support"
```

---

### Task 2: Rewrite StatusDotView → SessionStatusView (dual-mode)

**Files:**
- Rewrite: `CodePulse-macOS/Views/StatusDotView.swift`

The new view has two modes:
- **No tasks**: sparkle (working), gray dot (idle), orange pulse (needsInput)
- **Has tasks**: blue progress ring with sparkle edge (working), static ring (idle), ring + orange dot (needsInput)

- [ ] **Step 1: Replace StatusDotView.swift entirely**

```swift
import SwiftUI
import CodePulseShared

/// Dual-mode status indicator:
/// - No tasks: sparkle animation (working), gray dot (idle), orange pulse (needsInput)
/// - Has tasks: blue progress ring with optional sparkle edge and orange attention dot
struct SessionStatusView: View {
    let status: SessionStatus
    let completedTasks: Int
    let totalTasks: Int

    private var hasTasks: Bool { totalTasks > 0 }
    private var progress: Double {
        totalTasks > 0 ? Double(completedTasks) / Double(totalTasks) : 0
    }

    var body: some View {
        if hasTasks {
            ProgressRingView(
                progress: progress,
                status: status
            )
            .frame(width: 18, height: 18)
        } else if status == .working {
            ClaudeSparkleView()
                .frame(width: 14, height: 14)
        } else {
            MinimalDotView(status: status)
        }
    }
}

// MARK: - Progress Ring (has tasks)

private struct ProgressRingView: View {
    let progress: Double
    let status: SessionStatus

    @State private var shimmerAngle: Double = 0
    @State private var isPulsing = false

    private var isWorking: Bool { status == .working }
    private var needsAttention: Bool { status == .needsInput || status == .error }

    var body: some View {
        ZStack {
            // Background track
            Circle()
                .stroke(Color.primary.opacity(0.08), lineWidth: 2)

            // Blue progress arc
            Circle()
                .trim(from: 0, to: progress)
                .stroke(Color.blue, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(-90))

            // Sparkle shimmer overlay when working
            if isWorking {
                Circle()
                    .trim(from: max(0, progress - 0.08), to: progress)
                    .stroke(
                        Color.blue.opacity(0.6),
                        style: StrokeStyle(lineWidth: 3, lineCap: .round)
                    )
                    .rotationEffect(.degrees(-90))
                    .blur(radius: 1.5)
                    .opacity(isPulsing ? 1.0 : 0.3)
            }

            // Orange attention dot (top-right)
            if needsAttention {
                Circle()
                    .fill(Color.orange)
                    .frame(width: 5, height: 5)
                    .offset(x: 6, y: -6)
            }
        }
        .animation(
            isWorking
                ? .easeInOut(duration: 1.2).repeatForever(autoreverses: true)
                : .default,
            value: isPulsing
        )
        .onAppear {
            if isWorking { isPulsing = true }
        }
        .onChange(of: status) { _, _ in
            isPulsing = (status == .working)
        }
    }
}

// MARK: - Minimal Dot (no tasks, non-working states)

private struct MinimalDotView: View {
    let status: SessionStatus
    @State private var isPulsing = false

    private var needsPulse: Bool {
        status == .needsInput || status == .error
    }

    var body: some View {
        ZStack {
            if needsPulse {
                Circle()
                    .fill(Color.orange.opacity(0.3))
                    .frame(width: 16, height: 16)
                    .scaleEffect(isPulsing ? 1.4 : 0.8)
                    .opacity(isPulsing ? 0 : 0.6)
            }

            Circle()
                .fill(dotColor)
                .frame(width: 9, height: 9)
                .scaleEffect(isPulsing && needsPulse ? 1.15 : 1.0)
        }
        .frame(width: 14, height: 14)
        .animation(
            needsPulse
                ? .easeInOut(duration: 1.5).repeatForever(autoreverses: true)
                : .default,
            value: isPulsing
        )
        .onAppear {
            if needsPulse { isPulsing = true }
        }
        .onChange(of: status) { _, _ in
            withAnimation { isPulsing = needsPulse }
        }
    }

    private var dotColor: Color {
        switch status {
        case .needsInput, .error: return .orange
        case .idle: return .secondary.opacity(0.4)
        case .completed: return .secondary.opacity(0.3)
        default: return .secondary.opacity(0.4)
        }
    }
}

// MARK: - Claude Sparkle Animation

struct ClaudeSparkleView: View {
    private static let phases: [String] = ["·", "✢", "✳", "✶", "✻", "✽"]
    private static let cycle: [String] = phases + phases.dropFirst().dropLast().reversed()

    @State private var currentIndex = 0
    private let timer = Timer.publish(every: 0.22, on: .main, in: .common).autoconnect()

    var body: some View {
        Text(Self.cycle[currentIndex])
            .font(.system(size: 12))
            .foregroundStyle(.secondary.opacity(opacity))
            .onReceive(timer) { _ in
                currentIndex = (currentIndex + 1) % Self.cycle.count
            }
    }

    private var opacity: Double {
        let pos = Double(currentIndex) / Double(Self.cycle.count - 1)
        let wave = sin(pos * .pi)
        return 0.4 + wave * 0.6
    }
}
```

- [ ] **Step 2: Verify build**

```bash
xcodebuild -scheme CodePulse -destination 'platform=macOS,arch=arm64' build 2>&1 | grep -E "(error:|BUILD)"
```

Build will fail because `SessionRowView` and `SessionDetailView` still reference old `StatusDotView`. That's fine — we fix them in the next tasks.

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Views/StatusDotView.swift
git commit -m "feat: rewrite StatusDotView → SessionStatusView with dual-mode indicator"
```

---

## Chunk 2: Update All Views

### Task 3: Update SessionRowView — grayscale + new status view

**Files:**
- Modify: `CodePulse-macOS/Views/SessionRowView.swift`

- [ ] **Step 1: Replace SessionRowView.swift entirely**

```swift
import SwiftUI
import CodePulseShared

struct SessionRowView: View {
    let session: SessionState
    let onSelect: () -> Void
    @Environment(\.theme) private var theme
    @State private var isHovered = false
    @State private var isPressed = false

    var body: some View {
        Button(action: {
            withAnimation(.spring(duration: 0.15)) { isPressed = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
                withAnimation(.spring(duration: 0.15)) { isPressed = false }
            }
            onSelect()
        }) {
            HStack(spacing: 8) {
                SessionStatusView(
                    status: session.status,
                    completedTasks: session.completedTaskCount,
                    totalTasks: session.totalTaskCount
                )

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        Text(session.summary)
                            .font(.system(size: 13, weight: isHovered ? .medium : .regular))
                            .foregroundStyle(theme.primaryText)
                            .lineLimit(1)
                            .truncationMode(.middle)

                        if !session.model.isEmpty {
                            SessionTagView(tag: session.model)
                        }
                    }

                    if let subtitle = subtitleText {
                        Text(subtitle)
                            .font(.system(size: 11))
                            .foregroundStyle(subtitleColor)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 4)

                Text(relativeTime(session.startedAt))
                    .font(.system(size: 11, design: .rounded))
                    .foregroundStyle(theme.secondaryText)
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(isPressed ? theme.primaryText.opacity(0.1)
                          : isHovered ? theme.primaryText.opacity(0.06)
                          : Color.clear)
            )
            .scaleEffect(isPressed ? 0.98 : 1.0)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 2)
        .onHover { hovering in
            withAnimation(.easeOut(duration: 0.12)) { isHovered = hovering }
        }
    }

    private var subtitleText: String? {
        session.lastEvent ?? session.currentTask
    }

    private var subtitleColor: Color {
        session.status == .needsInput ? theme.warning : theme.secondaryText
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m ago" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h ago" }
        if seconds < 604800 { return "\(Int(seconds / 86400))d ago" }
        return "\(Int(seconds / 604800))w ago"
    }
}
```

Note: `CircularProgressView` is removed — progress is now shown in the left `SessionStatusView`.

- [ ] **Step 2: Commit**

```bash
git add CodePulse-macOS/Views/SessionRowView.swift
git commit -m "refactor: SessionRowView to grayscale + SessionStatusView"
```

---

### Task 4: Update SessionTagView — grayscale

**Files:**
- Modify: `CodePulse-macOS/Views/SessionTagView.swift`

- [ ] **Step 1: Replace SessionTagView.swift**

```swift
import SwiftUI

struct SessionTagView: View {
    let tag: String

    var body: some View {
        Text(tag)
            .font(.system(size: 9, weight: .medium, design: .monospaced))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 3)
            .padding(.vertical, 1)
            .background(
                RoundedRectangle(cornerRadius: 3, style: .continuous)
                    .fill(Color.primary.opacity(0.06))
            )
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add CodePulse-macOS/Views/SessionTagView.swift
git commit -m "refactor: SessionTagView to grayscale pill"
```

---

### Task 5: Update SessionDetailView — grayscale + blue tasks

**Files:**
- Modify: `CodePulse-macOS/Views/SessionDetailView.swift`

- [ ] **Step 1: Replace SessionDetailView.swift**

```swift
import SwiftUI
import CodePulseShared

struct SessionDetailView: View {
    let session: SessionState
    let onBack: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 0) {
            // Navigation header
            HStack(spacing: 6) {
                Button(action: onBack) {
                    HStack(spacing: 2) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 11, weight: .semibold))
                        Text("Back")
                            .font(.system(size: 12))
                    }
                    .foregroundStyle(theme.secondaryText)
                }
                .buttonStyle(.plain)
                .onHover { inside in
                    if inside { NSCursor.pointingHand.push() }
                    else { NSCursor.pop() }
                }

                Spacer()

                SessionStatusView(
                    status: session.status,
                    completedTasks: session.completedTaskCount,
                    totalTasks: session.totalTaskCount
                )
                Text(session.status.label)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(
                        session.status == .needsInput ? theme.warning : theme.secondaryText
                    )
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider().padding(.horizontal, 8)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 12) {
                    // Title section
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 4) {
                            Text(session.summary)
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundStyle(theme.primaryText)
                                .lineLimit(2)

                            if !session.model.isEmpty {
                                SessionTagView(tag: session.model)
                            }
                        }

                        HStack(spacing: 6) {
                            Label(session.projectName, systemImage: "folder")
                            if !session.gitBranch.isEmpty {
                                Text("·")
                                Text(session.gitBranch)
                            }
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(theme.secondaryText)
                    }

                    // Stats row
                    HStack(spacing: 0) {
                        statItem("\(session.contextPct)%", "Context")
                        Divider().frame(height: 24)
                        statItem(String(format: "$%.2f", session.costUSD), "Cost")
                        Divider().frame(height: 24)
                        statItem(formatDuration(session.durationSec), "Duration")
                    }
                    .padding(.vertical, 6)
                    .background(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(theme.cardBackground)
                    )

                    // Tasks section
                    if !session.tasks.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack(spacing: 10) {
                                DetailProgressRing(
                                    completed: session.completedTaskCount,
                                    total: session.totalTaskCount
                                )

                                VStack(alignment: .leading, spacing: 2) {
                                    Text("Tasks")
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundStyle(theme.primaryText)
                                    Text("\(session.completedTaskCount) of \(session.totalTaskCount) completed")
                                        .font(.system(size: 11))
                                        .foregroundStyle(theme.secondaryText)
                                }
                                Spacer()
                            }

                            VStack(alignment: .leading, spacing: 4) {
                                ForEach(session.tasks) { task in
                                    HStack(spacing: 6) {
                                        taskIcon(task.status)
                                        Text(task.content)
                                            .font(.system(size: 12))
                                            .foregroundStyle(
                                                task.status == .pending
                                                    ? theme.secondaryText
                                                    : theme.primaryText
                                            )
                                            .lineLimit(1)
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
            }
        }
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed:
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.blue)
            case .inProgress:
                Image(systemName: "circlebadge.fill")
                    .foregroundStyle(.blue)
            case .pending:
                Image(systemName: "circle")
                    .foregroundStyle(theme.secondaryText)
            }
        }
        .font(.system(size: 11))
    }

    private func statItem(_ value: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .foregroundStyle(theme.primaryText)
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(theme.secondaryText)
        }
        .frame(maxWidth: .infinity)
    }

    private func formatDuration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        let h = seconds / 3600
        let m = (seconds % 3600) / 60
        return "\(h)h \(m)m"
    }
}

// MARK: - Detail Progress Ring (blue, larger)

private struct DetailProgressRing: View {
    let completed: Int
    let total: Int

    @State private var animatedProgress: Double = 0

    private var progress: Double {
        total > 0 ? Double(completed) / Double(total) : 0
    }

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color.primary.opacity(0.06), lineWidth: 3)
            Circle()
                .trim(from: 0, to: animatedProgress)
                .stroke(Color.blue, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int(progress * 100))%")
                .font(.system(size: 10, weight: .semibold, design: .rounded))
                .foregroundStyle(.primary)
        }
        .frame(width: 36, height: 36)
        .onAppear {
            withAnimation(.easeOut(duration: 0.6)) { animatedProgress = progress }
        }
        .onChange(of: progress) { _, newValue in
            withAnimation(.easeOut(duration: 0.4)) { animatedProgress = newValue }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add CodePulse-macOS/Views/SessionDetailView.swift
git commit -m "refactor: SessionDetailView to grayscale + blue task icons"
```

---

### Task 6: Update SessionListView — dark/light toggle + theme injection

**Files:**
- Modify: `CodePulse-macOS/Views/SessionListView.swift`
- Modify: `CodePulse-macOS/App/MenuBarController.swift`

- [ ] **Step 1: Replace SessionListView.swift**

```swift
import SwiftUI
import CodePulseShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    @AppStorage("codepulse_darkMode") private var isDarkMode = false
    @State private var selectedSession: SessionState?

    private var theme: CodePulseTheme { CodePulseTheme(isDark: isDarkMode) }

    var body: some View {
        VStack(spacing: 0) {
            if let selected = selectedSession {
                let liveSession = stateManager.sessions.first { $0.sessionId == selected.sessionId } ?? selected
                SessionDetailView(session: liveSession) {
                    withAnimation(.easeInOut(duration: 0.2)) { selectedSession = nil }
                }
            } else {
                sessionList
            }
        }
        .frame(width: 320)
        .fixedSize(horizontal: true, vertical: true)
        .background(theme.background)
        .environment(\.theme, theme)
        .preferredColorScheme(isDarkMode ? .dark : .light)
    }

    private var sessionList: some View {
        VStack(spacing: 0) {
            if stateManager.sessions.isEmpty {
                emptyState
            } else {
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(spacing: 2) {
                        ForEach(stateManager.sessions) { session in
                            SessionRowView(session: session) {
                                withAnimation(.easeInOut(duration: 0.2)) { selectedSession = session }
                            }
                        }
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 2)
                }
                .frame(maxHeight: 420)
            }

            Divider().padding(.horizontal, 8)

            footer
        }
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "terminal")
                .font(.system(size: 24, weight: .light))
                .foregroundStyle(theme.secondaryText)
            Text("No active sessions")
                .font(.system(size: 13))
                .foregroundStyle(theme.secondaryText)
        }
        .frame(height: 120)
        .frame(maxWidth: .infinity)
    }

    private var footer: some View {
        HStack(alignment: .center, spacing: 8) {
            Text("CodePulse")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(theme.secondaryText.opacity(0.6))

            Spacer()

            // Dark/Light mode toggle
            Button(action: { isDarkMode.toggle() }) {
                Image(systemName: isDarkMode ? "sun.max" : "moon")
                    .font(.system(size: 11))
                    .foregroundStyle(theme.secondaryText)
            }
            .buttonStyle(.plain)

            Button(action: { NSApp.terminate(nil) }) {
                Text("Quit")
                    .font(.system(size: 11))
                    .foregroundStyle(theme.secondaryText)
            }
            .buttonStyle(.plain)
            .onHover { inside in
                if inside { NSCursor.pointingHand.push() }
                else { NSCursor.pop() }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}
```

- [ ] **Step 2: Verify full build**

```bash
xcodebuild -scheme CodePulse -destination 'platform=macOS,arch=arm64' build 2>&1 | grep -E "(error:|BUILD)"
```
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Views/SessionListView.swift
git commit -m "feat: add dark/light mode toggle + theme injection"
```

---

## Chunk 3: Cleanup + Install

### Task 7: Remove unused ProgressBarView + clean up

**Files:**
- Check: `CodePulse-macOS/Views/ProgressBarView.swift`

- [ ] **Step 1: Check if ProgressBarView is still referenced**

```bash
grep -r "ProgressBarView" CodePulse-macOS/
```

If not referenced, delete it and remove from Xcode project.

- [ ] **Step 2: Final build + install**

```bash
xcodebuild -scheme CodePulse -destination 'platform=macOS,arch=arm64' -configuration Release build 2>&1 | grep -E "(error:|BUILD)"
pkill -f "CodePulse.app"; sleep 2
rm -rf /Applications/CodePulse.app
cp -R ~/Library/Developer/Xcode/DerivedData/CodePulse-bwznbmdcofznndglmwocehwromxo/Build/Products/Release/CodePulse.app /Applications/CodePulse.app
open /Applications/CodePulse.app
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: remove unused ProgressBarView, final cleanup"
```
