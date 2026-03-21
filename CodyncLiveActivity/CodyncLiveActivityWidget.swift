import ActivityKit
import WidgetKit
import SwiftUI
import CodyncShared

/// Monochrome palette — dark navy on light Lock Screen background
private let widgetFg = Color(red: 47/255, green: 59/255, blue: 84/255)

struct CodyncLiveActivityWidget: Widget {
    let kind: String = "CodyncLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodyncAttributes.self) { context in
            lockScreenBanner(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    if context.state.isBusy {
                        IslandSparkle(durationSec: context.state.durationSec, size: 12)
                            .padding(.top, 4)
                    } else {
                        Circle()
                            .fill(.white)
                            .frame(width: 8, height: 8)
                            .opacity(context.state.isCompleted ? 0.5 : 1)
                            .padding(.top, 6)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(context.attributes.projectName)
                            .font(.headline)
                        Text(nonEmpty(context.state.currentTask) ?? statusLabel(context.state.status))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                            .id(context.state.currentTask ?? context.state.status)
                            .transition(.blurReplace)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if context.state.totalCount > 0 {
                        ProgressRing(
                            progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                            size: 18,
                            lineWidth: 2.5
                        )
                        .padding(.top, 4)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    if context.state.totalCount > 0 {
                        HStack(spacing: 2) {
                            ForEach(Array(context.state.tasks.enumerated()), id: \.offset) { _, task in
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(islandTaskColor(task.status))
                                    .frame(height: 4)
                            }
                        }
                    } else {
                        let completed = context.state.tasks.filter { $0.status == .completed }
                        if let prev = completed.last, !context.state.isCompleted {
                            HStack(spacing: 6) {
                                Image(systemName: "checkmark")
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.white.opacity(0.7))
                                Text(prev.truncatedContent)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                    }
                }
            } compactLeading: {
                if context.state.totalCount > 0 {
                    ProgressRing(
                        progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                        size: 14,
                        lineWidth: 2
                    )
                } else if context.state.isBusy {
                    IslandSparkle(durationSec: context.state.durationSec, size: 10)
                } else {
                    Circle()
                        .fill(.white)
                        .frame(width: 6, height: 6)
                        .opacity(context.state.isCompleted ? 0.5 : 1)
                }
            } compactTrailing: {
                Image(systemName: compactTrailingIcon(context.state.status))
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.white.opacity(compactTrailingOpacity(context.state.status)))
            } minimal: {
                if context.state.totalCount > 0 {
                    ProgressRing(
                        progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                        size: 11,
                        lineWidth: 2
                    )
                } else if context.state.isBusy {
                    IslandSparkle(durationSec: context.state.durationSec, size: 9)
                } else {
                    Circle()
                        .fill(.white)
                        .frame(width: 6, height: 6)
                        .opacity(context.state.isCompleted ? 0.5 : 1)
                }
            }
        }
    }

    // MARK: - Lock Screen Banner

    @ViewBuilder
    private func lockScreenBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        if context.state.totalCount > 0 {
            progressBanner(context: context)
        } else {
            cardBanner(context: context)
        }
    }

    // MARK: - Progress Bar Mode

    @ViewBuilder
    private func progressBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state
        let progress = state.totalCount > 0
            ? Double(state.completedCount) / Double(state.totalCount)
            : 0

        VStack(alignment: .leading, spacing: 0) {
            bannerHeader(context: context)

            VStack(alignment: .leading, spacing: 10) {
                if state.isCompleted {
                    HStack(spacing: 8) {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 20))
                            .foregroundStyle(widgetFg)
                        VStack(alignment: .leading, spacing: 1) {
                            Text("All tasks complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(widgetFg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.caption)
                                .foregroundStyle(widgetFg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 6)
                } else if let task = nonEmpty(state.currentTask) {
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: "arrow.turn.down.right")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(widgetFg.opacity(0.4))
                            .frame(width: 16, height: 16)
                            .padding(.top, 1)
                        Text(task)
                            .font(.callout)
                            .foregroundStyle(widgetFg)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, 6)
                }

                VStack(spacing: 6) {
                    HStack(spacing: 2) {
                        ForEach(Array(state.tasks.enumerated()), id: \.offset) { _, task in
                            RoundedRectangle(cornerRadius: 3)
                                .fill(taskColor(task.status))
                                .frame(height: 6)
                        }
                    }
                    .padding(.horizontal, 6)

                    HStack {
                        Text("\(state.completedCount)/\(state.totalCount) tasks")
                            .font(.caption)
                            .foregroundStyle(widgetFg.opacity(0.5))
                        Spacer()
                        Text("\(Int(progress * 100))%")
                            .font(.caption.bold())
                            .foregroundStyle(widgetFg.opacity(0.6))
                    }
                    .padding(.horizontal, 6)
                }
            }
            .frame(maxHeight: .infinity)

            bannerFooter(context: context, isWaiting: false)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Card Stack Mode

    @ViewBuilder
    private func cardBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state

        let prevTool = nonEmpty(state.previousTask)
        let hasToolActivity = prevTool != nil
        let isWaiting = !hasToolActivity && !state.isCompleted

        VStack(alignment: .leading, spacing: 4) {
            bannerHeader(context: context)

            ZStack {
                if state.isCompleted {
                    VStack(alignment: .center, spacing: 8) {
                        Image(systemName: "checkmark")
                            .font(.system(size: 16, weight: .bold))
                            .foregroundStyle(.white)
                            .frame(width: 28, height: 28)
                            .background(widgetFg, in: Circle())

                        VStack(spacing: 2) {
                            Text("Session Complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(widgetFg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.footnote)
                                .foregroundStyle(widgetFg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 12)
                } else if isWaiting {
                    let prompt = nonEmpty(context.attributes.summary) ?? "Analyzing code…"
                    Text(prompt)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .foregroundStyle(.blue)
                        .padding(12)
                        .frame(minWidth: 52)
                        .background(
                            Color.blue.opacity(0.12),
                            in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                        )
                        .padding(.leading, 48)
                        .padding(.trailing, 8)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .transition(.blurReplace)
                } else {
                    // Stacked cards: secondPrevious behind, previous in front
                    let secondPrev = nonEmpty(state.secondPreviousTask)
                    ZStack {
                        if let sp = secondPrev {
                            TaskCard(
                                text: simplifyToolText(sp) ?? sp,
                                icon: toolIcon(for: sp),
                                isBehind: true,
                                isInProgress: false
                            )
                        }
                        TaskCard(
                            text: simplifyToolText(prevTool) ?? prevTool ?? "",
                            icon: toolIcon(for: prevTool),
                            isBehind: false,
                            isInProgress: false
                        )
                        .id(prevTool)
                        .transition(.asymmetric(
                            insertion: .offset(y: 120),
                            removal: .opacity
                        ))
                    }
                    .compositingGroup()
                }
            }
            .frame(height: 80)
            .padding(.bottom, 8)
            .frame(maxHeight: .infinity)
            .zIndex(10)

            bannerFooter(context: context, isWaiting: isWaiting)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(isWaiting || state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Shared Components

    @ViewBuilder
    private func bannerHeader(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state

        HStack(spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(widgetFg.opacity(0.6))
                    .frame(width: 21, height: 21)
                    .background(widgetFg.opacity(0.08), in: Circle())

                Text(context.attributes.projectName)
                    .font(.callout.bold())
                    .opacity(0.72)
                    .lineLimit(1)
            }

            Spacer()

            if state.costUSD > 0 {
                let costStr = String(format: "$%.2f", state.costUSD)
                let alert = state.costUSD >= 1.0
                Text(costStr)
                    .font(.subheadline)
                    .fontWeight(.regular)
                    .padding(.vertical, 4)
                    .padding(.horizontal, 8)
                    .overlay {
                        Capsule()
                            .stroke(widgetFg.opacity(alert ? 0.06 : 0.12))
                    }
                    .background(widgetFg.opacity(alert ? 0.12 : 0), in: Capsule())
                    .monospacedDigit()
            } else {
                HStack(spacing: 5) {
                    Circle()
                        .fill(widgetFg)
                        .frame(width: 5, height: 5)
                    Text(statusLabel(state.status))
                        .opacity(0.48)
                }
                .transition(.blurReplace)
            }
        }
        .font(.subheadline.bold())
        .frame(height: 28)
        .padding(.horizontal, 6)
        .foregroundStyle(widgetFg)
    }

    @ViewBuilder
    private func bannerFooter(context: ActivityViewContext<CodyncAttributes>, isWaiting: Bool) -> some View {
        let state = context.state

        HStack(spacing: 6) {
            if state.isCompleted {
                Text("^[\(state.totalCount) task](inflect: true) completed")
                    .transition(.blurReplace)
                    .padding(.leading, 8)
            } else {
                HStack(spacing: 4) {
                    Image(systemName: toolIcon(for: state.currentTask))
                        .font(.system(size: 10, weight: .semibold))
                        .frame(width: 20, height: 18)
                    Text(isWaiting ? "Thinking…" : (nonEmpty(state.currentTask) ?? statusLabel(state.status)))
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .multilineTextAlignment(.leading)
                }
                .id(state.currentTask)
                .transition(.blurReplace)
            }

            Spacer()

            if !isWaiting {
                Text("00:00")
                    .opacity(0)
                    .overlay(alignment: .trailing) {
                        Text(state.sessionStartDate, style: .timer)
                            .contentTransition(.numericText(countsDown: false))
                            .opacity(0.5)
                    }
                    .font(.footnote.bold())
                    .monospacedDigit()
                    .multilineTextAlignment(.trailing)
                    .layoutPriority(1)
            }
        }
        .foregroundStyle(widgetFg)
        .padding(.leading, 4)
        .padding(.trailing, 12)
        .font(.footnote.bold())
        .opacity(isWaiting || state.isCompleted ? 0.36 : 1)
    }

    // MARK: - Helpers

    /// Map Claude Code tool names to SF Symbol icons
    private func toolIcon(for task: String?) -> String {
        guard let task = task?.lowercased() else { return "arrow.turn.down.right" }
        if task.contains("read") || task.contains("reading") { return "doc.text" }
        if task.contains("edit") || task.contains("editing") || task.contains("write") || task.contains("writing") { return "pencil.line" }
        if task.contains("bash") || task.contains("command") || task.contains("running") { return "terminal" }
        if task.contains("grep") || task.contains("search") || task.contains("glob") { return "magnifyingglass" }
        if task.contains("agent") || task.contains("dispatch") { return "person.2" }
        if task.contains("notebook") { return "book" }
        if task.contains("web") || task.contains("fetch") { return "globe" }
        if task.contains("git") || task.contains("commit") || task.contains("push") { return "arrow.triangle.branch" }
        if task.contains("test") { return "checkmark.diamond" }
        if task.contains("todo") || task.contains("task") { return "checklist" }
        return "arrow.turn.down.right"
    }

    /// Simplify tool text for card display: "Running: cd /Users/foo/bar && cmd" → "Running command"
    private func simplifyToolText(_ text: String?) -> String? {
        guard let text, !text.isEmpty else { return nil }
        let lower = text.lowercased()
        if lower.hasPrefix("running:") || lower.contains("bash") { return "Running command" }
        if lower.hasPrefix("reading") {
            let file = URL(fileURLWithPath: text.replacingOccurrences(of: "Reading ", with: "")).lastPathComponent
            return file.isEmpty ? "Reading file" : "Reading \(file)"
        }
        if lower.hasPrefix("editing") || lower.hasPrefix("writing") {
            let file = URL(fileURLWithPath: text.replacingOccurrences(of: "Editing ", with: "").replacingOccurrences(of: "Writing ", with: "")).lastPathComponent
            return file.isEmpty ? "Editing file" : "Editing \(file)"
        }
        if lower.contains("grep") || lower.contains("search") { return "Searching code" }
        if lower.contains("glob") { return "Finding files" }
        if lower.contains("agent") { return "Dispatching agent" }
        if lower.contains("git") { return "Git operation" }
        if lower.contains("web") || lower.contains("fetch") { return "Fetching web" }
        if text.count > 40 { return String(text.prefix(37)) + "…" }
        return text
    }

    /// Treat empty strings as nil — CloudKit stores nil optionals as ""
    private func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    private func compactTrailingIcon(_ status: String) -> String {
        switch status {
        case "working", "compacting": "chevron.left.forwardslash.chevron.right"
        case "needsInput": "hand.raised.fill"
        case "completed": "checkmark"
        case "idle": "pause"
        default: "chevron.left.forwardslash.chevron.right"
        }
    }

    private func compactTrailingOpacity(_ status: String) -> Double {
        switch status {
        case "working", "compacting": 0.8
        case "needsInput": 1.0
        case "completed": 0.4
        case "idle": 0.4
        default: 0.6
        }
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "working": "Working"
        case "idle": "Idle"
        case "needsInput": "Needs Input"
        case "compacting": "Compacting"
        case "error": "Error"
        case "completed": "Complete"
        default: "Working"
        }
    }

    /// Lock Screen task segments — monochrome navy at different opacities
    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: widgetFg
        case .inProgress: widgetFg.opacity(0.4)
        case .pending: widgetFg.opacity(0.12)
        }
    }

    /// Dynamic Island task segments — monochrome white at different opacities
    private func islandTaskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: .white
        case .inProgress: .white.opacity(0.4)
        case .pending: .white.opacity(0.12)
        }
    }
}

// MARK: - Task Card

struct TaskCard: View {
    let text: String
    let icon: String
    let isBehind: Bool
    var isInProgress: Bool = false

    private var cornerRadius: CGFloat { isBehind ? 10 : 16 }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: isInProgress ? icon : "checkmark")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(widgetFg.opacity(0.5))
                .frame(width: 21, height: 21)
                .background(
                    Circle()
                        .foregroundStyle(widgetFg.opacity(0.12))
                )

            Text(text)
                .font(.callout)
                .foregroundStyle(widgetFg)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 60)
        .background(Color.white, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        .scaleEffect(isBehind ? 0.9 : 1)
        .offset(y: isBehind ? 10 : 0)
        .opacity(isBehind ? 0.72 : 1)
        .zIndex(isBehind ? 0 : 1)
        .transition(.asymmetric(
            insertion: .offset(y: 120),
            removal: .opacity
        ))
    }
}

// MARK: - Island Sparkle

/// Sparkle indicator for working state in Dynamic Island.
/// Cycles through phases driven by durationSec (updated every second by LiveActivityManager tick timer).
struct IslandSparkle: View {
    let durationSec: Int
    let size: CGFloat

    // Same phases as ClaudeSparkleView: ·✢✶✻✽✻✶✢ (ping-pong)
    private static let cycle: [String] = ["·", "✢", "✶", "✻", "✽", "✻", "✶", "✢"]

    private var phase: Int { abs(durationSec) % Self.cycle.count }

    private var opacity: Double {
        let pos = Double(phase) / Double(Self.cycle.count - 1)
        return 0.5 + sin(pos * .pi) * 0.5
    }

    var body: some View {
        Text(Self.cycle[phase])
            .font(.system(size: size))
            .foregroundStyle(.white.opacity(opacity))
            .contentTransition(.interpolate)
            .animation(.easeInOut(duration: 0.3), value: phase)
    }
}

// MARK: - Progress Ring

struct ProgressRing: View {
    let progress: Double
    let size: CGFloat
    let lineWidth: CGFloat

    var body: some View {
        ZStack {
            Circle()
                .stroke(.white.opacity(0.15), lineWidth: lineWidth)
            Circle()
                .trim(from: 0, to: min(progress, 1.0))
                .stroke(.white, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
    }
}

// MARK: - Overall Live Activity Widget

struct OverallLiveActivityWidget: Widget {
    let kind: String = "CodyncOverallLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: OverallAttributes.self) { context in
            overallLockScreen(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    let primary = self.primarySession(from: context.state)
                    if let p = primary, p.status == .working {
                        Circle().fill(.white).frame(width: 8, height: 8)
                    } else {
                        Circle().fill(.white.opacity(0.5)).frame(width: 8, height: 8)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    let primary = self.primarySession(from: context.state)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(primary?.projectName ?? "Codync")
                            .font(.headline)
                        Text(primary?.currentTask ?? overallStatusLabel(primary?.status.rawValue ?? "idle"))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(String(format: "$%.2f", context.state.totalCost))
                        .font(.caption2.bold())
                        .foregroundStyle(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(spacing: 4) {
                        ForEach(context.state.sessions, id: \.sessionId) { s in
                            HStack(spacing: 3) {
                                Circle()
                                    .fill(s.sessionId == context.state.primarySessionId ? .white : .white.opacity(0.5))
                                    .frame(width: 5, height: 5)
                                Text(s.projectName)
                                    .font(.caption2)
                                    .lineLimit(1)
                            }
                        }
                    }
                }
            } compactLeading: {
                Circle().fill(.white).frame(width: 6, height: 6)
            } compactTrailing: {
                Text("\(context.state.sessions.count)")
                    .font(.caption2.bold())
            } minimal: {
                Circle().fill(.white).frame(width: 6, height: 6)
            }
        }
    }

    @ViewBuilder
    private func overallLockScreen(context: ActivityViewContext<OverallAttributes>) -> some View {
        let sessions = context.state.sessions
        let primaryId = context.state.primarySessionId

        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(widgetFg.opacity(0.6))
                        .frame(width: 21, height: 21)
                        .background(widgetFg.opacity(0.08), in: Circle())
                    Text("Codync")
                        .font(.callout.bold())
                        .opacity(0.72)
                }
                Spacer()
                Text(String(format: "$%.2f", context.state.totalCost))
                    .font(.subheadline)
                    .monospacedDigit()
                    .opacity(0.5)
            }
            .foregroundStyle(widgetFg)
            .padding(.horizontal, 6)
            .frame(height: 28)

            // Session rows — layout adapts by count
            VStack(spacing: 2) {
                ForEach(sessions, id: \.sessionId) { session in
                    let isPrimary = session.sessionId == primaryId
                    if sessions.count <= 3 {
                        mediumRow(session: session, isPrimary: isPrimary)
                    } else {
                        compactRow(session: session, isPrimary: isPrimary)
                    }
                }
            }
            .frame(maxHeight: .infinity)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(0.75))
        .activityBackgroundTint(.clear)
    }

    @ViewBuilder
    private func mediumRow(session: SessionSummary, isPrimary: Bool) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(isPrimary ? widgetFg : widgetFg.opacity(0.4))
                .frame(width: 6, height: 6)
            Text(session.projectName)
                .font(.subheadline.bold())
                .foregroundStyle(widgetFg)
                .lineLimit(1)
            Text(session.model)
                .font(.caption2)
                .foregroundStyle(widgetFg.opacity(0.4))
            Spacer()
            if let task = session.currentTask, !task.isEmpty {
                Text(task)
                    .font(.caption2)
                    .foregroundStyle(widgetFg.opacity(0.6))
                    .lineLimit(1)
                    .frame(maxWidth: 120, alignment: .trailing)
            } else {
                Text(overallStatusLabel(session.status.rawValue))
                    .font(.caption2)
                    .foregroundStyle(widgetFg.opacity(0.4))
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private func compactRow(session: SessionSummary, isPrimary: Bool) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(isPrimary ? widgetFg : widgetFg.opacity(0.4))
                .frame(width: 5, height: 5)
            Text(session.projectName)
                .font(.caption.bold())
                .foregroundStyle(widgetFg)
                .lineLimit(1)
            Spacer()
            Text(overallStatusLabel(session.status.rawValue))
                .font(.caption2)
                .foregroundStyle(widgetFg.opacity(0.4))
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 4)
    }

    private func primarySession(from state: OverallAttributes.ContentState) -> SessionSummary? {
        state.sessions.first { $0.sessionId == state.primarySessionId }
            ?? state.sessions.first
    }

    private func overallStatusLabel(_ status: String) -> String {
        switch status {
        case "working": "Working"
        case "idle": "Idle"
        case "needsInput": "Needs Input"
        case "compacting": "Compacting"
        case "error": "Error"
        case "completed": "Complete"
        default: "Working"
        }
    }
}
