import SwiftUI

/// Shared session row used in both Live Activity widget and in-app preview.
/// Renders: dot + project name + model tag + spacer + current task
public struct OverallSessionRow: View {
    public let projectName: String
    public let model: String
    public let currentTask: String?
    public let status: SessionStatus
    public let isPrimary: Bool
    public let fg: Color
    public var completedCount: Int = 0
    public var totalCount: Int = 0

    public init(
        projectName: String, model: String, currentTask: String?,
        status: SessionStatus, isPrimary: Bool, fg: Color,
        completedCount: Int = 0, totalCount: Int = 0
    ) {
        self.projectName = projectName
        self.model = model
        self.currentTask = currentTask
        self.status = status
        self.isPrimary = isPrimary
        self.fg = fg
        self.completedCount = completedCount
        self.totalCount = totalCount
    }

    public var body: some View {
        HStack(spacing: 8) {
            if totalCount > 0 {
                SegmentedRingView(
                    completedCount: completedCount,
                    totalCount: totalCount,
                    isWorking: status == .working,
                    fg: fg,
                    lineWidth: 1.5,
                    gapDegrees: 24
                )
                .frame(width: 12, height: 12)
            } else {
                Circle()
                    .fill(dotColor)
                    .frame(width: 7, height: 7)
            }

            Text(projectName)
                .font(.system(size: 13, weight: isPrimary ? .semibold : .medium))
                .foregroundStyle(fg)
                .lineLimit(1)
                .layoutPriority(1)

            Text(modelDisplayLabel(model))
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(fg.opacity(0.5))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(fg.opacity(0.08), in: Capsule())

            Spacer(minLength: 0)

            if let task = currentTask, !task.isEmpty {
                Text(task)
                    .font(.system(size: 11))
                    .foregroundStyle(fg.opacity(0.4))
                    .lineLimit(1)
                    .id(task)
                    .transition(.push(from: .bottom))
            }
        }
    }

    private var dotColor: Color {
        if isPrimary { return fg }
        switch status {
        case .working: return fg
        case .idle: return fg.opacity(0.3)
        default: return fg.opacity(0.5)
        }
    }
}
