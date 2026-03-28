import ActivityKit
import WidgetKit
import SwiftUI
import CodyncShared

/// Monochrome palette — dark navy for light mode, white for dark mode
private let widgetFgLight = Color(red: 47/255, green: 59/255, blue: 84/255)
private let widgetFgDark = Color(red: 0.93, green: 0.93, blue: 0.93)

/// Legacy alias used in Dynamic Island (always dark background)
private let widgetFg = widgetFgLight

/// Shared SF Symbol name for the code icon used across all DI/Watch views
private let codeIconName = "chevron.left.forwardslash.chevron.right"

/// Code icon opacity based on session state
private func codeIconOpacity(isBusy: Bool, isCompleted: Bool) -> Double {
    isBusy ? 0.8 : (isCompleted ? 0.3 : 0.5)
}

/// Treat empty strings as nil — CloudKit stores nil optionals as ""
private func nonEmpty(_ value: String?) -> String? {
    guard let value, !value.isEmpty else { return nil }
    return value
}

/// Simplify tool text: "Running: cd /Users/foo/bar && cmd" → "Running command"
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
    if lower.contains("mcp") { return "Using tool" }
    if text.count > 40 { return String(text.prefix(37)) + "…" }
    return text
}

/// Status label shared across all widgets
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

struct CodyncLiveActivityWidget: Widget {
    let kind: String = "CodyncLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodyncAttributes.self) { context in
            ActivityFamilyRouter {
                WatchIndividualView(attributes: context.attributes, state: context.state)
            } medium: {
                lockScreenBanner(context: context)
            }
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Image(systemName: codeIconName)
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(.white.opacity(codeIconOpacity(isBusy: context.state.isBusy, isCompleted: context.state.isCompleted)))
                        .padding(.top, 4)
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(context.attributes.projectName)
                            .font(.headline)
                        Text(simplifyToolText(context.state.currentTask) ?? statusLabel(context.state.status))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                            .id(context.state.currentTask ?? context.state.status)
                            .transition(.push(from: .bottom))
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
                    } else if context.state.costUSD > 0 {
                        Text(String(format: "$%.2f", context.state.costUSD))
                            .font(.system(size: 11, weight: .semibold).monospacedDigit())
                            .foregroundStyle(.white.opacity(0.5))
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
                        HStack(spacing: 6) {
                            let completed = context.state.tasks.filter { $0.status == .completed }
                            if let prev = completed.last, !context.state.isCompleted {
                                Image(systemName: "checkmark")
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.white.opacity(0.7))
                                Text(prev.truncatedContent)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            Spacer()
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
                } else {
                    Image(systemName: codeIconName)
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white.opacity(codeIconOpacity(isBusy: context.state.isBusy, isCompleted: context.state.isCompleted)))
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
                } else {
                    Image(systemName: codeIconName)
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white.opacity(codeIconOpacity(isBusy: context.state.isBusy, isCompleted: context.state.isCompleted)))
                }
            }
        }
        .supplementalActivityFamilies([.small])
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
        let dark = state.isDark
        let fg = dark ? widgetFgDark : widgetFgLight
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
                            .foregroundStyle(fg)
                        VStack(alignment: .leading, spacing: 1) {
                            Text("All tasks complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(fg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.caption)
                                .foregroundStyle(fg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 6)
                } else if let task = nonEmpty(state.currentTask) {
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: "arrow.turn.down.right")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(fg.opacity(0.4))
                            .frame(width: 16, height: 16)
                            .padding(.top, 1)
                        Text(task)
                            .font(.callout)
                            .foregroundStyle(fg)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, 6)
                    .id(task)
                    .transition(.push(from: .bottom))
                }

                VStack(spacing: 6) {
                    HStack(spacing: 2) {
                        ForEach(Array(state.tasks.enumerated()), id: \.offset) { _, task in
                            RoundedRectangle(cornerRadius: 3)
                                .fill(taskColor(task.status, isDark: dark))
                                .frame(height: 6)
                        }
                    }
                    .padding(.horizontal, 6)

                    HStack {
                        Text("\(state.completedCount)/\(state.totalCount) tasks")
                            .font(.caption)
                            .foregroundStyle(fg.opacity(0.5))
                        Spacer()
                        Text("\(Int(progress * 100))%")
                            .font(.caption.bold())
                            .foregroundStyle(fg.opacity(0.6))
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
        .background(dark ? Color.black.opacity(0.8) : Color.white.opacity(state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Card Stack Mode

    @ViewBuilder
    private func cardBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state
        let dark = state.isDark
        let fg = dark ? widgetFgDark : widgetFgLight

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
                            .foregroundStyle(dark ? Color.black : Color.white)
                            .frame(width: 28, height: 28)
                            .background(fg, in: Circle())

                        VStack(spacing: 2) {
                            Text("Session Complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(fg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.footnote)
                                .foregroundStyle(fg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 12)
                } else if isWaiting {
                    let prompt = nonEmpty(context.attributes.summary) ?? "Analyzing code…"
                    Text(prompt)
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .foregroundStyle(dark ? .white : .blue)
                        .padding(12)
                        .frame(minWidth: 52)
                        .background(
                            (dark ? Color.white : Color.blue).opacity(0.12),
                            in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                        )
                        .padding(.leading, 48)
                        .padding(.trailing, 8)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .transition(.push(from: .bottom))
                } else {
                    // Stacked cards: secondPrevious behind, previous in front
                    let secondPrev = nonEmpty(state.secondPreviousTask)
                    ZStack {
                        if let sp = secondPrev {
                            TaskCard(
                                text: simplifyToolText(sp) ?? sp,
                                icon: toolIcon(for: sp),
                                isBehind: true,
                                isDark: dark
                            )
                        }
                        TaskCard(
                            text: simplifyToolText(prevTool) ?? prevTool ?? "",
                            icon: toolIcon(for: prevTool),
                            isBehind: false,
                            isDark: dark
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
        .background(dark ? Color.black.opacity(0.8) : Color.white.opacity(isWaiting || state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Shared Components

    @ViewBuilder
    private func bannerHeader(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state
        let fg = state.isDark ? widgetFgDark : widgetFgLight

        HStack(spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: codeIconName)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(fg.opacity(0.6))
                    .frame(width: 21, height: 21)
                    .background(fg.opacity(0.08), in: Circle())

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
                            .stroke(fg.opacity(alert ? 0.06 : 0.12))
                    }
                    .background(fg.opacity(alert ? 0.12 : 0), in: Capsule())
                    .monospacedDigit()
            } else {
                HStack(spacing: 5) {
                    Circle()
                        .fill(fg)
                        .frame(width: 5, height: 5)
                    Text(statusLabel(state.status))
                        .opacity(0.48)
                }
                .transition(.push(from: .bottom))
            }
        }
        .font(.subheadline.bold())
        .frame(height: 28)
        .padding(.horizontal, 6)
        .foregroundStyle(fg)
    }

    @ViewBuilder
    private func bannerFooter(context: ActivityViewContext<CodyncAttributes>, isWaiting: Bool) -> some View {
        let state = context.state
        let fg = state.isDark ? widgetFgDark : widgetFgLight

        HStack(spacing: 6) {
            if state.isCompleted {
                Text("^[\(state.totalCount) task](inflect: true) completed")
                    .transition(.push(from: .bottom))
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
                .transition(.push(from: .bottom))
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
        .foregroundStyle(fg)
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


    private func compactTrailingIcon(_ status: String) -> String {
        switch status {
        case "working", "compacting": codeIconName
        case "needsInput": "hand.raised.fill"
        case "completed": "checkmark"
        case "idle": "pause"
        default: codeIconName
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

    /// Lock Screen task segments — adapts to light/dark mode
    private func taskColor(_ status: TaskStatus, isDark: Bool = false) -> Color {
        let fg = isDark ? widgetFgDark : widgetFgLight
        switch status {
        case .completed: return fg
        case .inProgress: return fg.opacity(0.4)
        case .pending: return fg.opacity(0.12)
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
    var isDark: Bool = false

    private var cornerRadius: CGFloat { isBehind ? 10 : 16 }
    private var fg: Color { isDark ? widgetFgDark : widgetFgLight }
    private var cardBg: Color {
        if isBehind {
            return isDark ? Color.white.opacity(0.08) : Color.white.opacity(0.85)
        }
        // Front card must be opaque to hide the card behind it
        return isDark ? Color(red: 0.14, green: 0.14, blue: 0.16) : Color.white
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: isInProgress ? icon : "checkmark")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(fg.opacity(0.5))
                .frame(width: 21, height: 21)
                .background(
                    Circle()
                        .foregroundStyle(fg.opacity(0.12))
                )

            Text(text)
                .font(.callout)
                .foregroundStyle(fg)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 60)
        .background(cardBg, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
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
            ActivityFamilyRouter {
                WatchOverallView(state: context.state)
            } medium: {
                overallLockScreen(context: context)
            }
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                }
                DynamicIslandExpandedRegion(.center) {
                    let primary = self.primarySession(from: context.state)
                    let anyWorking = context.state.sessions.contains { $0.status == .working }
                    HStack(spacing: 6) {
                        if let p = primary, p.totalCount > 0 {
                            ProgressRing(
                                progress: Double(p.completedCount) / Double(max(p.totalCount, 1)),
                                size: 14,
                                lineWidth: 2
                            )
                        } else {
                            Image(systemName: codeIconName)
                                .font(.system(size: 11, weight: .bold))
                                .foregroundStyle(.white.opacity(anyWorking ? 0.8 : 0.3))
                        }
                        Text(primary?.projectName ?? "Codync")
                            .font(.headline)
                        if let task = simplifyToolText(primary?.currentTask) {
                            Text(task)
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(0.5))
                                .lineLimit(1)
                                .id(task)
                                .transition(.push(from: .bottom))
                        }
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                }
                DynamicIslandExpandedRegion(.bottom) {
                    let primaryId = context.state.primarySessionId
                    HStack(spacing: 6) {
                        ForEach(context.state.sessions, id: \.sessionId) { s in
                            let isPrimary = s.sessionId == primaryId
                            let isWorking = s.status == .working
                            HStack(spacing: 4) {
                                if s.totalCount > 0 {
                                    ProgressRing(
                                        progress: Double(s.completedCount) / Double(max(s.totalCount, 1)),
                                        size: 8,
                                        lineWidth: 1.5
                                    )
                                } else {
                                    Circle()
                                        .fill(.white.opacity(isWorking ? 1.0 : 0.3))
                                        .frame(width: isWorking ? 6 : 5, height: isWorking ? 6 : 5)
                                }
                                Text(s.projectName)
                                    .font(.system(size: 11, weight: isPrimary ? .semibold : .regular))
                                    .foregroundStyle(.white.opacity(isWorking ? 0.9 : 0.4))
                                    .lineLimit(1)
                            }
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(.white.opacity(isPrimary ? 0.15 : 0.06), in: Capsule())
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
            } compactLeading: {
                let primary = self.primarySession(from: context.state)
                if let p = primary, p.totalCount > 0 {
                    ProgressRing(
                        progress: Double(p.completedCount) / Double(max(p.totalCount, 1)),
                        size: 11,
                        lineWidth: 2
                    )
                } else {
                    let anyWorking = context.state.sessions.contains { $0.status == .working }
                    Image(systemName: codeIconName)
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white.opacity(anyWorking ? 0.8 : 0.4))
                }
            } compactTrailing: {
                Text("\(context.state.sessions.count)")
                    .font(.caption2.bold())
            } minimal: {
                let primary = self.primarySession(from: context.state)
                if let p = primary, p.totalCount > 0 {
                    ProgressRing(
                        progress: Double(p.completedCount) / Double(max(p.totalCount, 1)),
                        size: 11,
                        lineWidth: 2
                    )
                } else {
                    let anyWorking = context.state.sessions.contains { $0.status == .working }
                    Image(systemName: codeIconName)
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white.opacity(anyWorking ? 0.8 : 0.4))
                }
            }
        }
        .supplementalActivityFamilies([.small])
    }

    @ViewBuilder
    private func overallLockScreen(context: ActivityViewContext<OverallAttributes>) -> some View {
        let sessions = context.state.sessions
        let primaryId = context.state.primarySessionId
        let dark = context.state.isDark

        VStack(spacing: 0) {
            ForEach(sessions, id: \.sessionId) { session in
                let isPrimary = session.sessionId == primaryId
                overallSessionRow(session: session, isPrimary: isPrimary, isDark: dark)
                    .id(session.sessionId)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(dark ? Color.black.opacity(0.8) : Color.white)
        .activityBackgroundTint(.clear)
    }

    @ViewBuilder
    private func overallSessionRow(session: SessionSummary, isPrimary: Bool, isDark: Bool) -> some View {
        let fg: Color = isDark ? .white : widgetFg

        OverallSessionRow(
            projectName: session.projectName,
            model: session.model,
            currentTask: session.currentTask,
            status: session.status,
            isPrimary: isPrimary,
            fg: fg,
            completedCount: session.completedCount,
            totalCount: session.totalCount
        )
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(
            isPrimary
                ? RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(fg.opacity(0.06))
                : nil
        )
    }

    private func primarySession(from state: OverallAttributes.ContentState) -> SessionSummary? {
        state.sessions.first { $0.sessionId == state.primarySessionId }
            ?? state.sessions.first
    }

}

// MARK: - Activity Family Router

/// Routes between Apple Watch Smart Stack (.small) and iPhone Lock Screen (.medium)
struct ActivityFamilyRouter<Small: View, Medium: View>: View {
    @Environment(\.activityFamily) var activityFamily
    let small: () -> Small
    let medium: () -> Medium

    init(@ViewBuilder small: @escaping () -> Small, @ViewBuilder medium: @escaping () -> Medium) {
        self.small = small
        self.medium = medium
    }

    var body: some View {
        switch activityFamily {
        case .small:
            small()
        default:
            medium()
        }
    }
}

// MARK: - Watch Individual View (Apple Watch Smart Stack)

struct WatchIndividualView: View {
    let attributes: CodyncAttributes
    let state: CodyncAttributes.ContentState

    var body: some View {
        Group {
            if state.isCompleted {
                watchCompletedView
            } else if state.totalCount > 0 {
                watchProgressView
            } else {
                watchMinimalView
            }
        }
        .padding(12)
    }

    // MARK: Completed — checkmark + ProjectName + $cost / Session Complete

    @ViewBuilder
    private var watchCompletedView: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 13))
                    .foregroundStyle(.white.opacity(0.6))
                Text(attributes.projectName)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer()
                Text(String(format: "$%.2f", state.costUSD))
                    .font(.caption.bold())
                    .foregroundStyle(.white.opacity(0.5))
                    .monospacedDigit()
            }
            Text("Session Complete")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(0.4))
        }
    }

    // MARK: Progress (Design A) — sparkle + Project + timer / task / bars + count

    @ViewBuilder
    private var watchProgressView: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 6) {
                watchStatusIndicator
                Text(attributes.projectName)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer()
                watchTimer
            }

            if let task = nonEmpty(state.currentTask) {
                Text(task)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.7))
                    .lineLimit(1)
                    .id(task)
                    .transition(.push(from: .bottom))
            }

            HStack(spacing: 2) {
                ForEach(Array(state.tasks.enumerated()), id: \.offset) { _, task in
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(watchTaskColor(task.status))
                        .frame(height: 3)
                }

                Text("\(state.completedCount)/\(state.totalCount)")
                    .font(.system(size: 10, weight: .medium).monospacedDigit())
                    .foregroundStyle(.white.opacity(0.4))
                    .fixedSize()
                    .padding(.leading, 4)
            }
        }
    }

    // MARK: Minimal (Design C) — sparkle + Project + timer / icon + tool

    @ViewBuilder
    private var watchMinimalView: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                watchStatusIndicator
                Text(attributes.projectName)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer()
                watchTimer
            }

            if let task = nonEmpty(state.currentTask) {
                Text(task)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.7))
                    .lineLimit(1)
                    .id(task)
                    .transition(.push(from: .bottom))
            } else {
                Text(statusLabel(state.status))
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.4))
            }
        }
    }

    // MARK: Shared Components

    @ViewBuilder
    private var watchStatusIndicator: some View {
        Image(systemName: codeIconName)
            .font(.system(size: 9, weight: .bold))
            .foregroundStyle(.white.opacity(codeIconOpacity(isBusy: state.isBusy, isCompleted: state.isCompleted)))
    }

    @ViewBuilder
    private var watchTimer: some View {
        Text(state.sessionStartDate, style: .timer)
            .font(.caption2.bold().monospacedDigit())
            .foregroundStyle(.white.opacity(0.5))
            .contentTransition(.numericText(countsDown: false))
    }

    // MARK: Helpers

    private func watchTaskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: .white
        case .inProgress: .white.opacity(0.4)
        case .pending: .white.opacity(0.12)
        }
    }

}

// MARK: - Watch Overall View (Apple Watch Smart Stack)

struct WatchOverallView: View {
    let state: OverallAttributes.ContentState

    var body: some View {
        VStack(spacing: 0) {
            ForEach(state.sessions.prefix(3), id: \.sessionId) { session in
                let isPrimary = session.sessionId == state.primarySessionId
                watchSessionRow(session: session, isPrimary: isPrimary)
                    .id(session.sessionId)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    @ViewBuilder
    private func watchSessionRow(session: SessionSummary, isPrimary: Bool) -> some View {
        HStack(spacing: 6) {
            Circle()
                .fill(watchDotColor(session: session, isPrimary: isPrimary))
                .frame(width: 6, height: 6)

            Text(session.projectName)
                .font(.system(size: 13, weight: isPrimary ? .semibold : .medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .layoutPriority(1)

            Text(modelDisplayLabel(session.model))
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(.white.opacity(0.4))
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(.white.opacity(0.08), in: Capsule())

            Spacer(minLength: 0)

            if let task = session.currentTask, !task.isEmpty {
                Text(task)
                    .font(.system(size: 10))
                    .foregroundStyle(.white.opacity(0.35))
                    .lineLimit(1)
            } else if session.status == .working {
                Image(systemName: codeIconName)
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(.white.opacity(0.5))
            }
        }
        .padding(.vertical, 5)
    }

    private func watchDotColor(session: SessionSummary, isPrimary: Bool) -> Color {
        if isPrimary { return .white }
        switch session.status {
        case .working: return .white.opacity(0.8)
        case .idle: return .white.opacity(0.3)
        default: return .white.opacity(0.5)
        }
    }
}
