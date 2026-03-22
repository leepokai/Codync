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
            LazyVStack(spacing: 4) {
                // Collapsible settings
                if showSettings {
                    modeSection
                }

                // Session list
                ForEach(Array(sessions.enumerated()), id: \.element.sessionId) { index, session in
                    NavigationLink(destination: IOSSessionDetailView(session: session)) {
                        SessionRowContent(
                            session: session,
                            isPrimary: primarySessionManager.primarySessionId == session.sessionId,
                            depth: index,
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
            ToolbarItem(placement: .principal) {
                Button {
                    withAnimation(.easeInOut(duration: 0.25)) {
                        showSettings.toggle()
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: showSettings ? "chevron.up" : "chevron.down")
                            .font(.system(size: 9, weight: .semibold))
                        Text("Preview")
                            .font(.system(size: 13, weight: .medium))
                    }
                    .foregroundStyle(showSettings ? theme.primaryText : theme.secondaryText)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                    .background(
                        Capsule()
                            .fill(showSettings ? theme.primaryText.opacity(0.1) : theme.primaryText.opacity(0.05))
                    )
                }
                .buttonStyle(.plain)
            }
        }
        .toolbarColorScheme(.dark, for: .navigationBar)
    }

    @ViewBuilder
    private var modeSection: some View {
        VStack(spacing: 14) {
            ModePillToggle(mode: Binding(
                get: { liveActivityManager.mode },
                set: { newMode in
                    Task { await liveActivityManager.switchMode(to: newMode) }
                }
            ))

            LiveActivityPreview(
                mode: liveActivityManager.mode,
                sessions: sessions.isEmpty ? nil : sessions,
                primarySessionId: primarySessionManager.primarySessionId,
                maxSessions: liveActivityManager.maxOverallSessions
            )

            HStack {
                Text("Max Live Activities")
                    .font(.caption.bold())
                    .foregroundStyle(theme.secondaryText)
                Spacer()
                Picker("Max", selection: Binding(
                    get: { liveActivityManager.maxOverallSessions },
                    set: { newVal in
                        liveActivityManager.maxOverallSessions = newVal
                        Task { await liveActivityManager.savePreference() }
                        liveActivityManager.updateSessions(sessions)
                    }
                )) {
                    ForEach(1...4, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 160)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
}

private struct SessionRowContent: View {
    let session: SessionState
    let isPrimary: Bool
    var depth: Int = 0
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
                                .id(desc)
                                .transition(.push(from: .bottom))
                                .animation(.easeInOut(duration: 0.3), value: desc)
                        }
                        Spacer()
                        Text(relativeTime(session.updatedAt))
                            .font(.system(size: 12, design: .monospaced))
                            .foregroundStyle(theme.tertiaryText)
                    }
                    .clipped()
                }
                if !session.tasks.isEmpty {
                    MiniProgressBar(tasks: session.tasks)
                }
            }

            Button(action: onSetPrimary) {
                Circle()
                    .fill(isPrimary ? theme.primaryText : theme.tertiaryText.opacity(0.3))
                    .frame(width: isPrimary ? 8 : 6, height: isPrimary ? 8 : 6)
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .padding(.vertical, isPrimary ? 4 : 0)
        .background(
            isPrimary
                ? RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(theme.primaryText.opacity(0.06))
                    .padding(.horizontal, 8)
                : nil
        )
        .opacity(depth == 0 ? 1.0 : max(0.65, 1.0 - Double(depth) * 0.1))
        .animation(.spring(duration: 0.5, bounce: 0.15), value: isPrimary)
        .animation(.spring(duration: 0.5, bounce: 0.15), value: depth)
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
