import SwiftUI
import CodePulseShared

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
    }

    private var headerSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                SessionStatusView(
                    status: session.status,
                    completedTasks: session.completedTaskCount,
                    totalTasks: session.totalTaskCount
                )
                Text(session.status.label)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(session.status == .needsInput ? theme.warning : theme.secondaryText)
                Spacer()
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
            .font(.system(size: 13))
            .foregroundStyle(theme.secondaryText)
        }
    }

    private var statsCard: some View {
        HStack(spacing: 0) {
            statItem("\(session.contextPct)%", "Context")
            Divider().frame(height: 28)
            statItem(String(format: "$%.2f", session.costUSD), "Cost")
            Divider().frame(height: 28)
            statItem(formatDuration(session.durationSec), "Duration")
        }
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(theme.cardBackground)
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

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed:
                Image(systemName: "checkmark.circle.fill").foregroundStyle(.blue)
            case .inProgress:
                Image(systemName: "circlebadge.fill").foregroundStyle(.blue)
            case .pending:
                Image(systemName: "circle").foregroundStyle(theme.secondaryText)
            }
        }
        .font(.system(size: 14))
    }

    private func statItem(_ value: String, _ label: String) -> some View {
        VStack(spacing: 3) {
            Text(value)
                .font(.system(size: 16, weight: .semibold, design: .rounded))
                .foregroundStyle(theme.primaryText)
            Text(label)
                .font(.system(size: 11))
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

private struct DetailProgressRing: View {
    let completed: Int
    let total: Int
    @State private var animatedProgress: Double = 0

    private var progress: Double {
        total > 0 ? Double(completed) / Double(total) : 0
    }

    var body: some View {
        ZStack {
            Circle().stroke(Color.primary.opacity(0.06), lineWidth: 3.5)
            Circle()
                .trim(from: 0, to: animatedProgress)
                .stroke(Color.blue, style: StrokeStyle(lineWidth: 3.5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Text("\(Int(progress * 100))%")
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .foregroundStyle(.primary)
        }
        .frame(width: 44, height: 44)
        .onAppear { withAnimation(.easeOut(duration: 0.6)) { animatedProgress = progress } }
        .onChange(of: progress) { _, newValue in
            withAnimation(.easeOut(duration: 0.4)) { animatedProgress = newValue }
        }
    }
}
