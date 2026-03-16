import SwiftUI
import CodePulseShared

struct SessionDetailView: View {
    let session: SessionState
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button(action: onBack) {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.left")
                        Text("Sessions")
                    }
                    .font(.system(size: 12))
                    .foregroundStyle(.cyan)
                }
                .buttonStyle(.plain)
                Spacer()
                StatusDotView(status: session.status)
                Text(session.status.label)
                    .font(.system(size: 11))
                    .foregroundStyle(session.status.color)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(Color.black.opacity(0.1))

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(session.summary)
                            .font(.system(size: 14, weight: .semibold))
                        HStack(spacing: 4) {
                            Text(session.projectName)
                            Text("·")
                            Text(session.gitBranch)
                            Text("·")
                            Text(session.model)
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 8)

                    if !session.tasks.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text("TASKS")
                                    .font(.system(size: 10, weight: .medium))
                                    .foregroundStyle(.secondary)
                                Spacer()
                                Text("\(session.completedTaskCount) of \(session.totalTaskCount)")
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }

                            ProgressBarView(tasks: session.tasks)

                            ForEach(session.tasks) { task in
                                HStack(spacing: 6) {
                                    taskIcon(task.status)
                                    Text(task.content)
                                        .font(.system(size: 11))
                                        .foregroundStyle(taskColor(task.status))
                                    if task.status == .inProgress, let form = task.activeForm {
                                        Text("— \(form)")
                                            .font(.system(size: 11))
                                            .foregroundStyle(.quaternary)
                                    }
                                }
                            }
                        }
                        .padding(.horizontal, 12)
                    }

                    HStack(spacing: 6) {
                        statCard("CONTEXT", "\(session.contextPct)%", .orange)
                        statCard("COST", String(format: "$%.2f", session.costUSD), .yellow)
                        statCard("DURATION", formatDuration(session.durationSec), .pink)
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
                }
            }
        }
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed: Text("✓").foregroundStyle(.green)
            case .inProgress: Text("◼").foregroundStyle(.cyan)
            case .pending: Text("◻").foregroundStyle(.quaternary)
            }
        }
        .font(.system(size: 11))
    }

    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.tertiaryLabelColor)
        }
    }

    private func statCard(_ label: String, _ value: String, _ color: Color) -> some View {
        VStack(spacing: 2) {
            Text(label).font(.system(size: 9, weight: .medium)).foregroundStyle(.secondary)
            Text(value).font(.system(size: 12, weight: .semibold)).foregroundStyle(color)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.black.opacity(0.15)))
    }

    private func formatDuration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        return "\(seconds / 3600)h \((seconds % 3600) / 60)m"
    }
}
