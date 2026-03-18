import ActivityKit
import WidgetKit
import SwiftUI
import CodyncShared

struct CodyncLiveActivityWidget: Widget {
    let kind: String = "CodyncLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodyncAttributes.self) { context in
            lockScreenBanner(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Circle()
                        .fill(islandStatusColor(context.state))
                        .frame(width: 8, height: 8)
                        .padding(.top, 6)
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(context.attributes.projectName)
                            .font(.headline)
                        Text(context.state.currentTask ?? statusLabel(context.state.status))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if context.state.status != "completed" {
                        Text("\(context.state.completedCount)/\(context.state.totalCount)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    let completed = context.state.tasks.filter { $0.status == .completed }
                    if let prev = completed.last, context.state.status != "completed" {
                        HStack(spacing: 6) {
                            Image(systemName: "checkmark")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.green)
                            Text(prev.truncatedContent)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
            } compactLeading: {
                Circle()
                    .fill(islandStatusColor(context.state))
                    .frame(width: 6, height: 6)
            } compactTrailing: {
                if context.state.status == "completed" {
                    Text("Done")
                        .font(.caption2)
                        .foregroundStyle(.green)
                } else {
                    Text(context.state.currentTask ?? "Working")
                        .font(.caption2)
                        .lineLimit(1)
                        .frame(maxWidth: 64)
                }
            } minimal: {
                Circle()
                    .fill(islandStatusColor(context.state))
                    .frame(width: 6, height: 6)
            }
        }
    }

    // MARK: - Lock Screen Banner

    @ViewBuilder
    private func lockScreenBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state
        let isCompleted = state.status == "completed"

        let completedTasks = state.tasks.filter { $0.status == .completed }
        let cards: [String] = {
            var result: [String] = []
            if completedTasks.count >= 2 {
                result.append(completedTasks[completedTasks.count - 2].truncatedContent)
            }
            if let last = completedTasks.last {
                result.append(last.truncatedContent)
            }
            return result
        }()
        let frontCard = completedTasks.last?.truncatedContent
        let isWaiting = cards.isEmpty && !isCompleted

        let primaryFg = Color(red: 47/255, green: 59/255, blue: 84/255)

        VStack(alignment: .leading, spacing: 4) {
            // MARK: Header
            HStack(spacing: 10) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(primaryFg.opacity(0.6))
                        .frame(width: 21, height: 21)
                        .background(primaryFg.opacity(0.08), in: Circle())

                    Text(context.attributes.projectName)
                        .font(.callout.bold())
                        .opacity(0.72)
                        .lineLimit(1)
                }

                Spacer()

                if state.costUSD > 0 {
                    let costStr = String(format: "$%.2f", state.costUSD)
                    let alert = state.costUSD >= 1.0
                    Text(costStr)
                        .font(.subheadline)
                        .fontWeight(.regular)
                        .padding(.vertical, 4)
                        .padding(.horizontal, 8)
                        .overlay {
                            Capsule()
                                .stroke(primaryFg.opacity(alert ? 0.06 : 0.12))
                        }
                        .background(primaryFg.opacity(alert ? 0.12 : 0), in: Capsule())
                        .monospacedDigit()
                } else {
                    HStack(spacing: 5) {
                        Circle()
                            .fill(statusColor(state.status))
                            .frame(width: 5, height: 5)
                        Text(statusLabel(state.status))
                            .opacity(0.48)
                    }
                    .transition(.blurReplace)
                }
            }
            .font(.subheadline.bold())
            .frame(height: 28)
            .padding(.horizontal, 6)
            .foregroundStyle(primaryFg)

            // MARK: Cards area
            ZStack {
                if isCompleted {
                    // Completion view
                    VStack(alignment: .center, spacing: 8) {
                        Image(systemName: "checkmark")
                            .font(.system(size: 16, weight: .bold))
                            .foregroundStyle(.white)
                            .frame(width: 28, height: 28)
                            .background(Color.green, in: Circle())

                        VStack(spacing: 2) {
                            Text("Session Complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(primaryFg)
                            Text("\(state.completedCount)/\(state.totalCount) tasks · $\(String(format: "%.2f", state.costUSD))")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 12)
                } else if isWaiting {
                    // Chat bubble — waiting for first task to complete
                    Text(state.currentTask ?? "Analyzing code…")
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .foregroundStyle(.blue)
                        .padding(12)
                        .frame(minWidth: 52)
                        .background(
                            Color.blue.opacity(0.12),
                            in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                        )
                        .padding(.leading, 48)
                        .padding(.trailing, 8)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .transition(.blurReplace)
                } else {
                    // Stacked task cards
                    ZStack {
                        ForEach(cards, id: \.self) { card in
                            let isBehind = card != frontCard
                            TaskCard(text: card, isBehind: isBehind)
                        }
                    }
                    .compositingGroup()
                    .transition(.blurReplace)
                }
            }
            .frame(height: 80)
            .padding(.bottom, 8)
            .frame(maxHeight: .infinity)
            .zIndex(10)

            // MARK: Footer
            HStack(spacing: 6) {
                if isCompleted {
                    Text("^[\(state.totalCount) task](inflect: true) completed")
                        .transition(.blurReplace)
                        .padding(.leading, 8)
                } else {
                    HStack(spacing: 2) {
                        Image(systemName: statusIcon(state.status))
                            .frame(width: 24, height: 18)
                        Text(isWaiting ? "Starting…" : (state.currentTask ?? statusLabel(state.status)))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .multilineTextAlignment(.leading)
                    }
                    .id(state.currentTask)
                    .transition(.blurReplace)
                }

                Spacer()

                if !isWaiting {
                    Text("00:00")
                        .opacity(0)
                        .overlay(alignment: .trailing) {
                            Text(state.sessionStartDate, style: .timer)
                                .contentTransition(.numericText(countsDown: false))
                                .opacity(0.5)
                        }
                        .font(.footnote.bold())
                        .monospacedDigit()
                        .multilineTextAlignment(.trailing)
                        .layoutPriority(1)
                }
            }
            .foregroundStyle(primaryFg)
            .padding(.leading, 4)
            .padding(.trailing, 12)
            .font(.footnote.bold())
            .opacity(isWaiting || isCompleted ? 0.36 : 1)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(isWaiting || isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Helpers

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "working": .blue
        case "idle": .secondary
        case "needsInput": .orange
        case "compacting": .purple
        case "error": .red
        case "completed": .green
        default: .gray
        }
    }

    private func islandStatusColor(_ state: CodyncAttributes.ContentState) -> Color {
        state.status == "completed" ? .green : .blue
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "working": "Working"
        case "idle": "Idle"
        case "needsInput": "Needs Input"
        case "compacting": "Compacting"
        case "error": "Error"
        case "completed": "Complete"
        default: "Working"
        }
    }

    private func statusIcon(_ status: String) -> String {
        switch status {
        case "working": "arrow.turn.down.right"
        case "idle": "pause.circle"
        case "needsInput": "exclamationmark.bubble"
        case "compacting": "arrow.triangle.2.circlepath"
        case "error": "exclamationmark.triangle"
        default: "arrow.turn.down.right"
        }
    }
}

// MARK: - Task Card

struct TaskCard: View {
    let text: String
    let isBehind: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "checkmark")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.black.opacity(0.5))
                .frame(width: 21, height: 21)
                .background(
                    Circle()
                        .foregroundStyle(.black.opacity(0.12))
                )

            Text(text)
                .font(.callout)
                .foregroundStyle(.black)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 60)
        .background(Color.white, in: RoundedRectangle(cornerRadius: isBehind ? 10 : 16, style: .continuous))
        .scaleEffect(isBehind ? 0.9 : 1)
        .offset(y: isBehind ? 10 : 0)
        .opacity(isBehind ? 0.72 : 1)
        .zIndex(isBehind ? 0 : 1)
        .transition(.asymmetric(
            insertion: .offset(y: 120),
            removal: .opacity
        ))
    }
}
