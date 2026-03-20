import SwiftUI
import CodyncShared

struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @Environment(\.theme) private var theme
    @State private var previousOrder: [String] = []
    @State private var cardStyles: [String: GlassCardStyle] = [:]

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 1) {
                ForEach(sessions) { session in
                    NavigationLink(destination: IOSSessionDetailView(session: session)) {
                        SessionRowContent(
                            session: session,
                            isTracking: liveActivityManager.isTracking(sessionId: session.sessionId),
                            isPinned: liveActivityManager.isPinned(sessionId: session.sessionId),
                            onTogglePin: { liveActivityManager.togglePin(session.sessionId) }
                        )
                    }
                    .buttonStyle(.plain)
                    .tint(theme.primaryText)
                    .glassCard(cardStyles[session.sessionId] ?? .normal, isDark: theme.isDark)
                }
            }
        }
        .background(theme.background)
        .navigationTitle("Codync")
        .toolbarColorScheme(.dark, for: .navigationBar)
        .onChange(of: sessions.map(\.sessionId)) { _, newOrder in
            var newStyles: [String: GlassCardStyle] = [:]
            for (newIdx, id) in newOrder.enumerated() {
                if let oldIdx = previousOrder.firstIndex(of: id) {
                    if newIdx < oldIdx { newStyles[id] = .elevated }
                    else if newIdx > oldIdx { newStyles[id] = .receded }
                }
            }
            cardStyles = newStyles
            previousOrder = newOrder

            // Reset after animation completes
            Task {
                try? await Task.sleep(for: .seconds(2.0))
                withAnimation(.easeOut(duration: 0.3)) { cardStyles = [:] }
            }
        }
    }
}

private struct SessionRowContent: View {
    let session: SessionState
    let isTracking: Bool
    let isPinned: Bool
    let onTogglePin: () -> Void
    @Environment(\.theme) private var theme

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
                    Spacer(minLength: 4)
                }
                if session.statusDescription != nil || !session.tasks.isEmpty {
                    HStack(spacing: 4) {
                        if let desc = session.statusDescription {
                            Text(desc)
                                .font(.system(size: 13))
                                .foregroundStyle(subtitleColor)
                                .lineLimit(1)
                        }
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

            Button(action: onTogglePin) {
                Image(systemName: isPinned ? "pin.fill" : "pin")
                    .font(.system(size: 13))
                    .foregroundStyle(isPinned ? theme.primaryText : theme.tertiaryText)
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    private var subtitleColor: Color {
        guard session.status == .needsInput else { return theme.secondaryText }
        return theme.waitingColor(for: session.waitingReason)
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
