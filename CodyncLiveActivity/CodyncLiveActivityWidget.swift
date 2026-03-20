import ActivityKit
import WidgetKit
import SwiftUI
import CodyncShared

/// Monochrome palette — dark navy on light Lock Screen background
private let widgetFg = Color(red: 47/255, green: 59/255, blue: 84/255)

struct CodyncLiveActivityWidget: Widget {
    let kind: String = "CodyncLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodyncAttributes.self) { context in
            lockScreenBanner(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    if context.state.isBusy {
                        IslandSparkle(durationSec: context.state.durationSec, size: 12)
                            .padding(.top, 4)
                    } else {
                        Circle()
                            .fill(.white)
                            .frame(width: 8, height: 8)
                            .opacity(context.state.isCompleted ? 0.5 : 1)
                            .padding(.top, 6)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(context.attributes.projectName)
                            .font(.headline)
                        Text(nonEmpty(context.state.currentTask) ?? statusLabel(context.state.status))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                            .id(context.state.currentTask ?? context.state.status)
                            .transition(.blurReplace)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if context.state.totalCount > 0 {
                        ProgressRing(
                            progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                            size: 18,
                            lineWidth: 2.5
                        )
                        .padding(.top, 4)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    if context.state.totalCount > 0 {
                        HStack(spacing: 2) {
                            ForEach(Array(context.state.tasks.enumerated()), id: \.offset) { _, task in
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(islandTaskColor(task.status))
                                    .frame(height: 4)
                            }
                        }
                    } else {
                        let completed = context.state.tasks.filter { $0.status == .completed }
                        if let prev = completed.last, !context.state.isCompleted {
                            HStack(spacing: 6) {
                                Image(systemName: "checkmark")
                                    .font(.caption2.weight(.semibold))
                                    .foregroundStyle(.white.opacity(0.7))
                                Text(prev.truncatedContent)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                    }
                }
            } compactLeading: {
                if context.state.totalCount > 0 {
                    ProgressRing(
                        progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                        size: 14,
                        lineWidth: 2
                    )
                } else if context.state.isBusy {
                    IslandSparkle(durationSec: context.state.durationSec, size: 10)
                } else {
                    Circle()
                        .fill(.white)
                        .frame(width: 6, height: 6)
                        .opacity(context.state.isCompleted ? 0.5 : 1)
                }
            } compactTrailing: {
                Image(systemName: compactTrailingIcon(context.state.status))
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.white.opacity(compactTrailingOpacity(context.state.status)))
            } minimal: {
                if context.state.totalCount > 0 {
                    ProgressRing(
                        progress: Double(context.state.completedCount) / Double(max(context.state.totalCount, 1)),
                        size: 11,
                        lineWidth: 2
                    )
                } else if context.state.isBusy {
                    IslandSparkle(durationSec: context.state.durationSec, size: 9)
                } else {
                    Circle()
                        .fill(.white)
                        .frame(width: 6, height: 6)
                        .opacity(context.state.isCompleted ? 0.5 : 1)
                }
            }
        }
    }

    // MARK: - Lock Screen Banner

    @ViewBuilder
    private func lockScreenBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        if context.state.totalCount > 0 {
            progressBanner(context: context)
        } else {
            cardBanner(context: context)
        }
    }

    // MARK: - Progress Bar Mode

    @ViewBuilder
    private func progressBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state
        let progress = state.totalCount > 0
            ? Double(state.completedCount) / Double(state.totalCount)
            : 0

        VStack(alignment: .leading, spacing: 0) {
            bannerHeader(context: context)

            VStack(alignment: .leading, spacing: 10) {
                if state.isCompleted {
                    HStack(spacing: 8) {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 20))
                            .foregroundStyle(widgetFg)
                        VStack(alignment: .leading, spacing: 1) {
                            Text("All tasks complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(widgetFg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.caption)
                                .foregroundStyle(widgetFg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 6)
                } else if let task = nonEmpty(state.currentTask) {
                    HStack(alignment: .top, spacing: 6) {
                        Image(systemName: "arrow.turn.down.right")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(widgetFg.opacity(0.4))
                            .frame(width: 16, height: 16)
                            .padding(.top, 1)
                        Text(task)
                            .font(.callout)
                            .foregroundStyle(widgetFg)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, 6)
                }

                VStack(spacing: 6) {
                    HStack(spacing: 2) {
                        ForEach(Array(state.tasks.enumerated()), id: \.offset) { _, task in
                            RoundedRectangle(cornerRadius: 3)
                                .fill(taskColor(task.status))
                                .frame(height: 6)
                        }
                    }
                    .padding(.horizontal, 6)

                    HStack {
                        Text("\(state.completedCount)/\(state.totalCount) tasks")
                            .font(.caption)
                            .foregroundStyle(widgetFg.opacity(0.5))
                        Spacer()
                        Text("\(Int(progress * 100))%")
                            .font(.caption.bold())
                            .foregroundStyle(widgetFg.opacity(0.6))
                    }
                    .padding(.horizontal, 6)
                }
            }
            .frame(maxHeight: .infinity)

            bannerFooter(context: context, isWaiting: false)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Card Stack Mode

    @ViewBuilder
    private func cardBanner(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state

        let completedTasks = state.tasks.filter { $0.status == .completed }
        let currentTool = nonEmpty(state.currentTask)
        let cards: [(text: String, isInProgress: Bool)] = {
            var result: [(String, Bool)] = []
            // Show last completed task behind
            if let last = completedTasks.last {
                result.append((last.truncatedContent, false))
            }
            // Show current tool as front card (in-progress)
            if let tool = currentTool, state.isBusy {
                result.append((tool, true))
            } else if completedTasks.count >= 2 {
                // No active tool — show last 2 completed
                result.insert((completedTasks[completedTasks.count - 2].truncatedContent, false), at: 0)
            }
            return result
        }()
        let isWaiting = cards.isEmpty && !state.isCompleted

        VStack(alignment: .leading, spacing: 4) {
            bannerHeader(context: context)

            ZStack {
                if state.isCompleted {
                    VStack(alignment: .center, spacing: 8) {
                        Image(systemName: "checkmark")
                            .font(.system(size: 16, weight: .bold))
                            .foregroundStyle(.white)
                            .frame(width: 28, height: 28)
                            .background(widgetFg, in: Circle())

                        VStack(spacing: 2) {
                            Text("Session Complete")
                                .font(.subheadline.bold())
                                .foregroundStyle(widgetFg)
                            Text("$\(String(format: "%.2f", state.costUSD))")
                                .font(.footnote)
                                .foregroundStyle(widgetFg.opacity(0.5))
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 12)
                } else if isWaiting {
                    Text(nonEmpty(state.currentTask) ?? "Analyzing code…")
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .foregroundStyle(widgetFg)
                        .padding(12)
                        .frame(minWidth: 52)
                        .background(
                            widgetFg.opacity(0.08),
                            in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                        )
                        .padding(.leading, 48)
                        .padding(.trailing, 8)
                        .font(.callout)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .transition(.blurReplace)
                } else {
                    ZStack {
                        ForEach(Array(cards.enumerated()), id: \.offset) { idx, card in
                            let isBehind = idx < cards.count - 1
                            TaskCard(text: card.text, isBehind: isBehind, isInProgress: card.isInProgress)
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

            bannerFooter(context: context, isWaiting: isWaiting)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 10)
        .frame(height: 160)
        .background(Color.white.opacity(isWaiting || state.isCompleted ? 1 : 0.75))
        .activityBackgroundTint(.clear)
    }

    // MARK: - Shared Components

    @ViewBuilder
    private func bannerHeader(context: ActivityViewContext<CodyncAttributes>) -> some View {
        let state = context.state

        HStack(spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(widgetFg.opacity(0.6))
                    .frame(width: 21, height: 21)
                    .background(widgetFg.opacity(0.08), in: Circle())

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
                            .stroke(widgetFg.opacity(alert ? 0.06 : 0.12))
                    }
                    .background(widgetFg.opacity(alert ? 0.12 : 0), in: Capsule())
                    .monospacedDigit()
            } else {
                HStack(spacing: 5) {
                    Circle()
                        .fill(widgetFg)
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
        .foregroundStyle(widgetFg)
    }

    @ViewBuilder
    private func bannerFooter(context: ActivityViewContext<CodyncAttributes>, isWaiting: Bool) -> some View {
        let state = context.state

        HStack(spacing: 6) {
            if state.isCompleted {
                Text("^[\(state.totalCount) task](inflect: true) completed")
                    .transition(.blurReplace)
                    .padding(.leading, 8)
            } else {
                HStack(spacing: 2) {
                    Image(systemName: "arrow.turn.down.right")
                        .frame(width: 24, height: 18)
                    Text(isWaiting ? "Starting…" : (nonEmpty(state.currentTask) ?? statusLabel(state.status)))
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
        .foregroundStyle(widgetFg)
        .padding(.leading, 4)
        .padding(.trailing, 12)
        .font(.footnote.bold())
        .opacity(isWaiting || state.isCompleted ? 0.36 : 1)
    }

    // MARK: - Helpers

    /// Treat empty strings as nil — CloudKit stores nil optionals as ""
    private func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return value
    }

    private func compactTrailingIcon(_ status: String) -> String {
        switch status {
        case "working", "compacting": "chevron.left.forwardslash.chevron.right"
        case "needsInput": "hand.raised.fill"
        case "completed": "checkmark"
        case "idle": "pause"
        default: "chevron.left.forwardslash.chevron.right"
        }
    }

    private func compactTrailingOpacity(_ status: String) -> Double {
        switch status {
        case "working", "compacting": 0.8
        case "needsInput": 1.0
        case "completed": 0.4
        case "idle": 0.4
        default: 0.6
        }
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

    /// Lock Screen task segments — monochrome navy at different opacities
    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: widgetFg
        case .inProgress: widgetFg.opacity(0.4)
        case .pending: widgetFg.opacity(0.12)
        }
    }

    /// Dynamic Island task segments — monochrome white at different opacities
    private func islandTaskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: .white
        case .inProgress: .white.opacity(0.4)
        case .pending: .white.opacity(0.12)
        }
    }
}

// MARK: - Task Card

struct TaskCard: View {
    let text: String
    let isBehind: Bool
    var isInProgress: Bool = false

    private var cornerRadius: CGFloat { isBehind ? 10 : 16 }

    var body: some View {
        HStack(spacing: 8) {
            if isInProgress {
                ProgressView()
                    .controlSize(.small)
                    .tint(widgetFg.opacity(0.6))
                    .frame(width: 21, height: 21)
            } else {
                Image(systemName: "checkmark")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(widgetFg.opacity(0.5))
                    .frame(width: 21, height: 21)
                    .background(
                        Circle()
                            .foregroundStyle(widgetFg.opacity(0.12))
                    )
            }

            Text(text)
                .font(.callout)
                .foregroundStyle(widgetFg)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: 60)
        .background(Color.white, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
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

// MARK: - Island Sparkle

/// Sparkle indicator for working state in Dynamic Island.
/// Cycles through phases driven by durationSec (updated every second by LiveActivityManager tick timer).
struct IslandSparkle: View {
    let durationSec: Int
    let size: CGFloat

    // Same phases as ClaudeSparkleView: ·✢✶✻✽✻✶✢ (ping-pong)
    private static let cycle: [String] = ["·", "✢", "✶", "✻", "✽", "✻", "✶", "✢"]

    private var phase: Int { abs(durationSec) % Self.cycle.count }

    private var opacity: Double {
        let pos = Double(phase) / Double(Self.cycle.count - 1)
        return 0.5 + sin(pos * .pi) * 0.5
    }

    var body: some View {
        Text(Self.cycle[phase])
            .font(.system(size: size))
            .foregroundStyle(.white.opacity(opacity))
            .contentTransition(.interpolate)
            .animation(.easeInOut(duration: 0.3), value: phase)
    }
}

// MARK: - Progress Ring

struct ProgressRing: View {
    let progress: Double
    let size: CGFloat
    let lineWidth: CGFloat

    var body: some View {
        ZStack {
            Circle()
                .stroke(.white.opacity(0.15), lineWidth: lineWidth)
            Circle()
                .trim(from: 0, to: min(progress, 1.0))
                .stroke(.white, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
    }
}
