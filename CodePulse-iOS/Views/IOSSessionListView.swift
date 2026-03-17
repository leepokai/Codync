import SwiftUI
import CodePulseShared

struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @Environment(\.theme) private var theme

    var body: some View {
        List {
            trackingModeSection
            sessionsSection
        }
        .listStyle(.plain)
        .navigationTitle("CodePulse")
    }

    private var trackingModeSection: some View {
        Section {
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
            .padding(.vertical, 2)
        }
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }

    private var sessionsSection: some View {
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
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
        }
    }
}

private struct SessionRowContent: View {
    let session: SessionState
    let trackingMode: TrackingMode
    let isTracking: Bool
    let isPinned: Bool
    let onTogglePin: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 10) {
            SessionStatusView(
                status: session.status,
                completedTasks: session.completedTaskCount,
                totalTasks: session.totalTaskCount
            )

            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 5) {
                    Text(session.summary)
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if !session.model.isEmpty {
                        SessionTagView(tag: session.model)
                    }
                }
                HStack(spacing: 4) {
                    if let subtitle = session.lastEvent ?? session.currentTask {
                        Text(subtitle)
                            .font(.system(size: 13))
                            .foregroundStyle(session.status == .needsInput ? theme.warning : theme.secondaryText)
                            .lineLimit(1)
                    } else {
                        Text("\(session.projectName) · \(session.status.label)")
                            .font(.system(size: 13))
                            .foregroundStyle(theme.secondaryText)
                            .lineLimit(1)
                    }
                    Spacer()
                    Text(relativeTime(session.startedAt))
                        .font(.system(size: 12, design: .rounded))
                        .foregroundStyle(theme.secondaryText)
                }
                if !session.tasks.isEmpty {
                    MiniProgressBar(tasks: session.tasks)
                }
            }

            liveActivityIndicator
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var liveActivityIndicator: some View {
        switch trackingMode {
        case .auto:
            // Auto mode: just show indicator, no interaction
            if isTracking {
                Image(systemName: "dot.radiowaves.up.forward")
                    .font(.system(size: 13))
                    .foregroundStyle(theme.accent)
                    .frame(width: 24, height: 24)
            }
        case .manual:
            // Manual mode: tappable pin button
            Button(action: onTogglePin) {
                Image(systemName: isPinned ? "pin.fill" : "pin")
                    .font(.system(size: 13))
                    .foregroundStyle(isPinned ? theme.accent : theme.secondaryText.opacity(0.4))
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
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
        case .completed: return .blue
        case .inProgress: return .blue.opacity(0.4)
        case .pending: return theme.secondaryText.opacity(0.2)
        }
    }
}
