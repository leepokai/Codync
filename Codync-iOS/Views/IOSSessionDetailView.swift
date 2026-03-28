import SwiftUI
import CodyncShared

// MARK: - Session Detail

struct IOSSessionDetailView: View {
    let session: SessionState
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                headerSection
                statsCard
                if !session.tasks.isEmpty {
                    tasksSection
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
        .background(theme.background)
        .navigationTitle(session.summary)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarColorScheme(.dark, for: .navigationBar)
    }

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                SessionStatusView(
                    status: session.status,
                    completedTasks: session.completedTaskCount,
                    totalTasks: session.totalTaskCount,
                    waitingReason: session.waitingReason
                )
                Text(statusLabel)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(statusLabelColor)
                Spacer()
                if !session.model.isEmpty {
                    SessionTagView(tag: session.model)
                }
            }
            HStack(spacing: 6) {
                Label(session.projectName, systemImage: "folder")
                if !session.gitBranch.isEmpty && session.gitBranch != "unknown" {
                    Text("·")
                    Text(session.gitBranch)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(theme.secondaryText)
        }
    }

    private var statsCard: some View {
        HStack(spacing: 0) {
            statItem("\(session.contextPct)%", "Context")
            Divider().frame(height: 28).overlay(theme.separator)
            statItem(String(format: "$%.2f", session.costUSD), "Cost")
            Divider().frame(height: 28).overlay(theme.separator)
            statItem(formatDuration(session.durationSec), "Duration")
        }
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(theme.cardBackground)
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .stroke(theme.border, lineWidth: 1)
                )
        )
    }

    private var tasksSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                DetailProgressRing(
                    completed: session.completedTaskCount,
                    total: session.totalTaskCount
                )
                VStack(alignment: .leading, spacing: 2) {
                    Text("Tasks")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                    Text("\(session.completedTaskCount) of \(session.totalTaskCount) completed")
                        .font(.system(size: 13))
                        .foregroundStyle(theme.secondaryText)
                }
                Spacer()
            }

            VStack(alignment: .leading, spacing: 6) {
                ForEach(session.tasks) { task in
                    HStack(spacing: 8) {
                        taskIcon(task.status)
                        Text(task.content)
                            .font(.system(size: 14))
                            .foregroundStyle(task.status == .pending ? theme.secondaryText : theme.primaryText)
                            .lineLimit(2)
                    }
                }
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
                Image(systemName: "circlebadge.fill").foregroundStyle(theme.accent)
            case .pending:
                Image(systemName: "circle").foregroundStyle(theme.tertiaryText)
            }
        }
        .font(.system(size: 14))
    }

    private func statItem(_ value: String, _ label: String) -> some View {
        VStack(spacing: 3) {
            Text(value)
                .font(.system(size: 16, weight: .semibold, design: .monospaced))
                .foregroundStyle(theme.primaryText)
            Text(label)
                .font(.system(size: 11))
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
            Circle().stroke(theme.secondaryText.opacity(0.15), lineWidth: 3.5)
            Circle()
                .trim(from: 0, to: animatedProgress)
                .stroke(theme.primaryText.opacity(0.7), style: StrokeStyle(lineWidth: 3.5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int(progress * 100))%")
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                .foregroundStyle(theme.primaryText)
        }
        .frame(width: 44, height: 44)
        .onAppear { withAnimation(.easeOut(duration: 0.6)) { animatedProgress = progress } }
        .onChange(of: progress) { _, newValue in
            withAnimation(.easeOut(duration: 0.4)) { animatedProgress = newValue }
        }
    }
}
