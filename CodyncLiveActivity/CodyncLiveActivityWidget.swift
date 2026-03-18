import ActivityKit
import WidgetKit
import SwiftUI
import CodyncShared

struct CodyncLiveActivityWidget: Widget {
    let kind: String = "CodyncLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodyncAttributes.self) { context in
            // Lock Screen view
            VStack(spacing: 6) {
                HStack {
                    HStack(spacing: 4) {
                        Image(systemName: "waveform.path")
                            .font(.caption2)
                        Text(context.attributes.projectName)
                            .font(.callout.weight(.medium))
                    }
                    Spacer()
                    HStack(spacing: 4) {
                        Circle()
                            .fill(statusColor(context.state.status))
                            .frame(width: 6, height: 6)
                        Text(statusLabel(context.state.status))
                            .font(.caption)
                            .foregroundStyle(statusColor(context.state.status))
                    }
                }
                HStack(spacing: 2) {
                    ForEach(Array(context.state.tasks.prefix(10).enumerated()), id: \.offset) { _, task in
                        RoundedRectangle(cornerRadius: 3)
                            .fill(taskColor(task.status))
                            .frame(height: 6)
                    }
                }
                HStack {
                    if let task = context.state.currentTask {
                        Text("\u{25FC} \(task)")
                            .font(.caption)
                            .foregroundStyle(.blue)
                            .lineLimit(1)
                    }
                    Spacer()
                    Text("\(context.state.completedCount)/\(context.state.totalCount) tasks")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(12)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 4) {
                        Circle()
                            .fill(statusColor(context.state.status))
                            .frame(width: 6, height: 6)
                        Text(context.attributes.projectName)
                            .font(.caption2.weight(.semibold))
                            .lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.model)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 4) {
                        HStack(spacing: 2) {
                            ForEach(Array(context.state.tasks.prefix(10).enumerated()), id: \.offset) { _, task in
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(taskColor(task.status))
                                    .frame(height: 4)
                            }
                        }
                        HStack {
                            if let task = context.state.currentTask {
                                Text("\u{25FC} \(task)")
                                    .font(.caption2)
                                    .foregroundStyle(.blue)
                                    .lineLimit(1)
                            }
                            Spacer()
                            Text("\(context.state.completedCount)/\(context.state.totalCount) · $\(String(format: "%.2f", context.state.costUSD))")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            } compactLeading: {
                HStack(spacing: 3) {
                    Circle()
                        .fill(statusColor(context.state.status))
                        .frame(width: 6, height: 6)
                    HStack(spacing: 2) {
                        ForEach(Array(context.state.tasks.prefix(10).enumerated()), id: \.offset) { _, task in
                            RoundedRectangle(cornerRadius: 2)
                                .fill(taskColor(task.status))
                                .frame(width: 10, height: 4)
                        }
                    }
                }
            } compactTrailing: {
                Text("\(context.state.completedCount)/\(context.state.totalCount)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } minimal: {
                Circle()
                    .fill(statusColor(context.state.status))
                    .frame(width: 6, height: 6)
            }
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "working": return .blue
        case "idle": return .secondary
        case "needsInput": return .orange
        case "compacting": return .purple
        case "error": return .orange
        default: return .gray
        }
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "working": return "Working"
        case "idle": return "Idle"
        case "needsInput": return "Needs Input"
        case "compacting": return "Compacting"
        case "error": return "Error"
        default: return "Done"
        }
    }

    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: return .blue
        case .inProgress: return .blue.opacity(0.4)
        case .pending: return Color(.systemGray4)
        }
    }
}
