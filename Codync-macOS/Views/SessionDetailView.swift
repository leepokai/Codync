import SwiftUI
import CodyncShared

struct SessionDetailView: View {
    let session: SessionState
    let onBack: () -> Void
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 0) {
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
                    totalTasks: session.totalTaskCount,
                    waitingReason: session.waitingReason
                )
                Text(statusLabel)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(statusLabelColor)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider().padding(.horizontal, 8)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 4) {
                            Text(session.projectName)
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundStyle(theme.primaryText)
                            if !session.model.isEmpty {
                                SessionTagView(tag: session.model)
                            }
                        }
                        if session.summary != session.projectName {
                            Text(session.summary)
                                .font(.system(size: 12))
                                .foregroundStyle(theme.secondaryText)
                                .lineLimit(2)
                        }
                        HStack(spacing: 6) {
                            Label(session.gitBranch.isEmpty ? session.projectName : session.gitBranch, systemImage: "arrow.triangle.branch")
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(theme.tertiaryText)
                    }

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

                    if !session.tasks.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack(spacing: 10) {
                                DetailProgressRing(completed: session.completedTaskCount, total: session.totalTaskCount)
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
                                            .foregroundStyle(task.status == .pending ? theme.secondaryText : theme.primaryText)
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

    private var statusLabel: String {
        guard session.status == .needsInput else { return session.status.label }
        return session.waitingReason?.label ?? "Needs Input"
    }

    private var statusLabelColor: Color {
        guard session.status == .needsInput else { return theme.secondaryText }
        return theme.waitingColor(for: session.waitingReason)
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed:
                Image(systemName: "checkmark.circle.fill").foregroundStyle(theme.primaryText.opacity(0.7))
            case .inProgress:
                Image(systemName: "circlebadge.fill").foregroundStyle(theme.primaryText.opacity(0.7))
            case .pending:
                Image(systemName: "circle").foregroundStyle(theme.tertiaryText)
            }
        }
        .font(.system(size: 11))
    }

    private func statItem(_ value: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .monospaced))
                .foregroundStyle(theme.primaryText)
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(theme.tertiaryText)
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

private struct DetailProgressRing: View {
    let completed: Int
    let total: Int
    @Environment(\.theme) private var theme
    @State private var animatedProgress: Double = 0

    private var progress: Double {
        total > 0 ? Double(completed) / Double(total) : 0
    }

    var body: some View {
        ZStack {
            Circle().stroke(theme.secondaryText.opacity(0.15), lineWidth: 3)
            Circle()
                .trim(from: 0, to: animatedProgress)
                .stroke(theme.primaryText.opacity(0.7), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int(progress * 100))%")
                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                .foregroundStyle(theme.primaryText)
        }
        .frame(width: 36, height: 36)
        .onAppear { withAnimation(.easeOut(duration: 0.6)) { animatedProgress = progress } }
        .onChange(of: progress) { _, newValue in
            withAnimation(.easeOut(duration: 0.4)) { animatedProgress = newValue }
        }
    }
}
