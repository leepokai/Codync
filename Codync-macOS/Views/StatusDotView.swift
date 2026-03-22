import SwiftUI
import CodyncShared

struct SessionStatusView: View {
    let status: SessionStatus
    let completedTasks: Int
    let totalTasks: Int
    var waitingReason: WaitingReason?
    @Environment(\.theme) private var theme

    private var hasTasks: Bool { totalTasks > 0 }

    var body: some View {
        if hasTasks {
            SegmentedRingView(
                completedCount: completedTasks,
                totalCount: totalTasks,
                isWorking: status == .working,
                fg: theme.primaryText,
                lineWidth: 1.5,
                gapDegrees: 24
            )
            .frame(width: 12, height: 12)
        } else if status == .working {
            ClaudeSparkleView()
                .frame(width: 14, height: 14)
        } else {
            MinimalDotView(status: status, waitingReason: waitingReason)
        }
    }
}

// MARK: - Minimal Dot

private struct MinimalDotView: View {
    let status: SessionStatus
    var waitingReason: WaitingReason?
    @Environment(\.theme) private var theme
    @State private var isPulsing = false

    private var needsPulse: Bool { status == .needsInput || status == .error }

    var body: some View {
        ZStack {
            if needsPulse {
                Circle()
                    .fill(dotColor.opacity(0.3))
                    .frame(width: 16, height: 16)
                    .scaleEffect(isPulsing ? 1.4 : 0.8)
                    .opacity(isPulsing ? 0 : 0.6)
            }
            Circle()
                .fill(dotColor)
                .frame(width: 9, height: 9)
                .scaleEffect(isPulsing && needsPulse ? 1.15 : 1.0)
        }
        .frame(width: 14, height: 14)
        .animation(
            needsPulse ? .easeInOut(duration: 1.5).repeatForever(autoreverses: true) : .default,
            value: isPulsing
        )
        .onAppear { if needsPulse { isPulsing = true } }
        .onChange(of: status) { _, _ in withAnimation { isPulsing = needsPulse } }
    }

    private var dotColor: Color {
        switch status {
        case .needsInput, .error:
            return theme.waitingColor(for: waitingReason)
        case .idle: return theme.tertiaryText
        case .completed: return theme.tertiaryText.opacity(0.7)
        default: return theme.tertiaryText
        }
    }
}

// MARK: - Claude Sparkle

struct ClaudeSparkleView: View {
    private static let phases: [String] = ["·", "✢", "✶", "✻", "✽"]
    private static let cycle: [String] = phases + phases.dropFirst().dropLast().reversed()
    @Environment(\.theme) private var theme
    @State private var currentIndex = 0
    private let timer = Timer.publish(every: 0.22, on: .main, in: .common).autoconnect()

    var body: some View {
        Text(Self.cycle[currentIndex])
            .font(.system(size: 12))
            .foregroundStyle(theme.primaryText.opacity(opacity))
            .onReceive(timer) { _ in
                currentIndex = (currentIndex + 1) % Self.cycle.count
            }
    }

    private var opacity: Double {
        let pos = Double(currentIndex) / Double(Self.cycle.count - 1)
        return 0.4 + sin(pos * .pi) * 0.6
    }
}
