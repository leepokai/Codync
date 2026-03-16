import SwiftUI
import CodePulseShared

struct ProgressBarView: View {
    let tasks: [TaskItem]

    var body: some View {
        HStack(spacing: 2) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 2, style: .continuous)
                    .fill(color(for: task.status))
                    .frame(height: 4)
            }
        }
    }

    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return .primary.opacity(0.1)
        }
    }
}
