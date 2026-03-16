import SwiftUI
import CodePulseShared

struct ProgressBarView: View {
    let tasks: [TaskItem]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 3)
                    .fill(color(for: task.status))
                    .frame(height: 6)
            }
        }
    }

    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.separatorColor)
        }
    }
}
