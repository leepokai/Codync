import SwiftUI
import CodePulseShared

struct IOSSessionListView: View {
    let sessions: [SessionState]

    var body: some View {
        List(sessions) { session in
            NavigationLink(destination: IOSSessionDetailView(session: session)) {
                HStack(spacing: 10) {
                    Circle()
                        .fill(session.status.color)
                        .frame(width: 10, height: 10)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(session.summary)
                            .font(.body.weight(.medium))
                            .lineLimit(1)
                        Text("\(session.projectName) · \(session.completedTaskCount)/\(session.totalTaskCount) tasks · \(session.status.label)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        if !session.tasks.isEmpty {
                            IOSMiniProgressBar(tasks: session.tasks)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .navigationTitle("CodePulse")
    }
}

struct IOSMiniProgressBar: View {
    let tasks: [TaskItem]
    var body: some View {
        HStack(spacing: 2) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 2)
                    .fill(color(for: task.status))
                    .frame(height: 3)
            }
        }
        .padding(.top, 2)
    }
    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.systemGray4)
        }
    }
}
