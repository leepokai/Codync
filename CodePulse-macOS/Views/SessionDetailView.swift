import SwiftUI
import CodePulseShared

struct SessionDetailView: View {
    let session: SessionState
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            // Navigation header
            HStack(spacing: 6) {
                Button(action: onBack) {
                    HStack(spacing: 2) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 11, weight: .semibold))
                        Text("Back")
                            .font(.system(size: 12))
                    }
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)

                Spacer()

                StatusDotView(status: session.status)
                Text(session.status.label)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(session.status.color)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            Divider().padding(.horizontal, 8)

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {

                    // Title section
                    VStack(alignment: .leading, spacing: 4) {
                        Text(session.summary)
                            .font(.system(size: 14, weight: .semibold))
                            .lineLimit(2)

                        HStack(spacing: 6) {
                            Label(session.projectName, systemImage: "folder")
                            Text("·")
                            Text(session.gitBranch)
                            Text("·")
                            Text(session.model)
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    }

                    // Stats row
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
                            .fill(.quaternary.opacity(0.3))
                    )

                    // Tasks section
                    if !session.tasks.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("Tasks")
                                    .font(.system(size: 12, weight: .medium))
                                Spacer()
                                Text("\(session.completedTaskCount)/\(session.totalTaskCount)")
                                    .font(.system(size: 11, design: .rounded))
                                    .foregroundStyle(.secondary)
                            }

                            ProgressBarView(tasks: session.tasks)

                            VStack(alignment: .leading, spacing: 4) {
                                ForEach(session.tasks) { task in
                                    HStack(spacing: 6) {
                                        taskIcon(task.status)
                                        Text(task.content)
                                            .font(.system(size: 12))
                                            .foregroundStyle(task.status == .pending ? .tertiary : .primary)
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

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed:
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            case .inProgress:
                Image(systemName: "circlebadge.fill")
                    .foregroundStyle(.cyan)
            case .pending:
                Image(systemName: "circle")
                    .foregroundStyle(.tertiary)
            }
        }
        .font(.system(size: 11))
    }

    private func statItem(_ value: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
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
