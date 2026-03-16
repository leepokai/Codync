import SwiftUI
import CodePulseShared

struct IOSSessionDetailView: View {
    let session: SessionState

    var body: some View {
        List {
            Section {
                HStack {
                    Circle().fill(session.status.color).frame(width: 10, height: 10)
                    Text(session.status.label)
                    Spacer()
                    Text(session.model).foregroundStyle(.secondary)
                }
                HStack {
                    Text(session.projectName)
                    Spacer()
                    Text(session.gitBranch).foregroundStyle(.secondary)
                }
            }

            if !session.tasks.isEmpty {
                Section("Tasks — \(session.completedTaskCount) of \(session.totalTaskCount)") {
                    IOSMiniProgressBar(tasks: session.tasks)
                        .padding(.vertical, 4)
                    ForEach(session.tasks) { task in
                        HStack(spacing: 8) {
                            taskIcon(task.status)
                            Text(task.content)
                                .foregroundStyle(task.status == .pending ? .secondary : .primary)
                        }
                    }
                }
            }

            Section {
                HStack {
                    statItem("Context", "\(session.contextPct)%")
                    Divider()
                    statItem("Cost", String(format: "$%.2f", session.costUSD))
                    Divider()
                    statItem("Time", formatDuration(session.durationSec))
                }
                .padding(.vertical, 4)
            }
        }
        .navigationTitle(session.summary)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            case .inProgress: Image(systemName: "circle.dotted.circle").foregroundStyle(.cyan)
            case .pending: Image(systemName: "circle").foregroundStyle(.secondary)
            }
        }
        .font(.system(size: 14))
    }

    private func statItem(_ label: String, _ value: String) -> some View {
        VStack(spacing: 2) {
            Text(label).font(.caption2).foregroundStyle(.secondary)
            Text(value).font(.callout.weight(.semibold))
        }
        .frame(maxWidth: .infinity)
    }

    private func formatDuration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        return "\(seconds / 3600)h \((seconds % 3600) / 60)m"
    }
}
