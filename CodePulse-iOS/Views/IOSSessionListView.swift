import SwiftUI
import CodePulseShared

struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 1) {
                trackingModeRow
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 4)

                ForEach(sessions) { session in
                    NavigationLink(destination: IOSSessionDetailView(session: session)) {
                        SessionRowContent(
                            session: session,
                            trackingMode: liveActivityManager.trackingMode,
                            isTracking: liveActivityManager.isTracking(sessionId: session.sessionId),
                            isPinned: liveActivityManager.isPinned(sessionId: session.sessionId),
                            onTogglePin: { liveActivityManager.togglePin(session) }
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .background(theme.background)
        .navigationTitle("CodePulse")
        .toolbarColorScheme(.dark, for: .navigationBar)
    }

    private var trackingModeRow: some View {
        HStack(spacing: 8) {
            Image(systemName: liveActivityManager.trackingMode.icon)
                .font(.system(size: 13))
                .foregroundStyle(theme.accent)
                .frame(width: 20)

            Text("Live Activity")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(theme.primaryText)

            Spacer()

            Picker("", selection: $liveActivityManager.trackingMode) {
                ForEach(TrackingMode.allCases, id: \.self) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 140)
            .onChange(of: liveActivityManager.trackingMode) { _, _ in
                liveActivityManager.onModeChanged(sessions: sessions)
            }
        }
        .padding(.vertical, 6)
    }
}

private struct SessionRowContent: View {
    let session: SessionState
    let trackingMode: TrackingMode
    let isTracking: Bool
    let isPinned: Bool
    let onTogglePin: () -> Void
    @Environment(\.theme) private var theme

    /// Matches macOS SessionRowView.statusDescription
    private var statusDescription: String? {
        if session.status == .working, let event = session.lastEvent, !event.isEmpty {
            return event
        }
        let desc = session.summary
        return desc != session.projectName ? desc : nil
    }

    var body: some View {
        HStack(spacing: 10) {
            SessionStatusView(
                status: session.status,
                completedTasks: session.completedTaskCount,
                totalTasks: session.totalTaskCount,
                waitingReason: session.waitingReason
            )

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    Text(session.projectName)
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                        .lineLimit(1)
                    if !session.model.isEmpty {
                        SessionTagView(tag: session.model)
                    }
                }
                if let desc = statusDescription {
                    HStack(spacing: 4) {
                        Text(desc)
                            .font(.system(size: 13))
                            .foregroundStyle(subtitleColor)
                            .lineLimit(1)
                        Spacer()
                        Text(relativeTime(session.updatedAt))
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(theme.tertiaryText)
                    }
                }
                if !session.tasks.isEmpty {
                    MiniProgressBar(tasks: session.tasks)
                }
            }

            liveActivityIndicator
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private var subtitleColor: Color {
        guard session.status == .needsInput else { return theme.secondaryText }
        return theme.waitingColor(for: session.waitingReason)
    }

    @ViewBuilder
    private var liveActivityIndicator: some View {
        switch trackingMode {
        case .auto:
            EmptyView()
        case .manual:
            Button(action: onTogglePin) {
                Image(systemName: isPinned ? "pin.fill" : "pin")
                    .font(.system(size: 13))
                    .foregroundStyle(isPinned ? theme.primaryText : theme.tertiaryText)
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h" }
        if seconds < 604800 { return "\(Int(seconds / 86400))d" }
        return "\(Int(seconds / 604800))w"
    }
}

private struct MiniProgressBar: View {
    let tasks: [TaskItem]
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 2) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 2)
                    .fill(color(for: task.status))
                    .frame(height: 3)
            }
        }
        .padding(.top, 1)
    }

    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return theme.accent
        case .inProgress: return theme.accent.opacity(0.4)
        case .pending: return theme.secondaryText.opacity(0.15)
        }
    }
}
