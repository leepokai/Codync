import SwiftUI
import CodyncShared

struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @Environment(\.theme) private var theme
    @State private var showSettings = false

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 1) {
                // Collapsible settings
                if showSettings {
                    modeSection
                    if liveActivityManager.mode == .overall {
                        primarySection
                    }
                }

                // Session list
                ForEach(sessions) { session in
                    NavigationLink(destination: IOSSessionDetailView(session: session)) {
                        SessionRowContent(
                            session: session,
                            isTracking: liveActivityManager.isTracking(sessionId: session.sessionId),
                            isPinned: liveActivityManager.isPinned(sessionId: session.sessionId),
                            isPrimary: primarySessionManager.primarySessionId == session.sessionId,
                            showPrimary: liveActivityManager.mode == .overall,
                            onTogglePin: { liveActivityManager.togglePin(session.sessionId) },
                            onSetPrimary: { primarySessionManager.manualLock(session.sessionId) }
                        )
                    }
                    .buttonStyle(.plain)
                    .tint(theme.primaryText)
                }
            }
        }
        .background(theme.background)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    withAnimation(.easeInOut(duration: 0.25)) {
                        showSettings.toggle()
                    }
                } label: {
                    Image(systemName: showSettings ? "gearshape.fill" : "gearshape")
                        .font(.system(size: 15))
                        .foregroundStyle(showSettings ? theme.accent : theme.secondaryText)
                }
            }
        }
        .toolbarColorScheme(.dark, for: .navigationBar)
    }

    @ViewBuilder
    private var modeSection: some View {
        Picker("Live Activity Mode", selection: Binding(
            get: { liveActivityManager.mode },
            set: { newMode in Task { await liveActivityManager.switchMode(to: newMode) } }
        )) {
            Text("Overall").tag(LiveActivityMode.overall)
            Text("Individual").tag(LiveActivityMode.individual)
        }
        .pickerStyle(.segmented)
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var primarySection: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Max sessions picker
            HStack {
                Text("Max Sessions")
                    .font(.caption.bold())
                    .foregroundStyle(theme.secondaryText)
                Spacer()
                Picker("Max", selection: Binding(
                    get: { liveActivityManager.maxOverallSessions },
                    set: { newVal in
                        liveActivityManager.maxOverallSessions = newVal
                        Task { await liveActivityManager.savePreference() }
                    }
                )) {
                    ForEach(1...4, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 160)
            }

            // Primary session
            HStack {
                Text("Primary Session")
                    .font(.caption.bold())
                    .foregroundStyle(theme.secondaryText)
                Spacer()
                if primarySessionManager.isManuallyLocked {
                    Button("Unlock") {
                        primarySessionManager.unlock()
                        primarySessionManager.autoSelect(from: sessions)
                    }
                    .font(.caption)
                }
            }
            if let primaryId = primarySessionManager.primarySessionId,
               let session = sessions.first(where: { $0.sessionId == primaryId }) {
                HStack(spacing: 6) {
                    Image(systemName: "star.fill")
                        .font(.system(size: 10))
                        .foregroundStyle(.yellow)
                    Text(session.projectName)
                        .font(.subheadline.bold())
                        .foregroundStyle(theme.primaryText)
                    if primarySessionManager.isManuallyLocked {
                        Image(systemName: "lock.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(theme.tertiaryText)
                    }
                }
            } else {
                Text("No active session")
                    .font(.subheadline)
                    .foregroundStyle(theme.tertiaryText)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }
}

private struct SessionRowContent: View {
    let session: SessionState
    let isTracking: Bool
    let isPinned: Bool
    let isPrimary: Bool
    let showPrimary: Bool
    let onTogglePin: () -> Void
    let onSetPrimary: () -> Void
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
                    if showPrimary && isPrimary {
                        Image(systemName: "star.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(.yellow)
                    }
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

            if showPrimary {
                Button(action: onSetPrimary) {
                    Image(systemName: isPrimary ? "star.fill" : "star")
                        .font(.system(size: 13))
                        .foregroundStyle(isPrimary ? .yellow : theme.tertiaryText)
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            } else {
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
