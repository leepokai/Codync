import SwiftUI
import CodePulseShared

struct SessionStatusView: View {
    let status: SessionStatus
    let completedTasks: Int
    let totalTasks: Int
    var waitingReason: WaitingReason?
    @Environment(\.theme) private var theme

    private var hasTasks: Bool { totalTasks > 0 }
    private var progress: Double {
        totalTasks > 0 ? Double(completedTasks) / Double(totalTasks) : 0
    }

    var body: some View {
        if hasTasks {
            ProgressRingView(progress: progress, status: status, waitingReason: waitingReason)
                .frame(width: 22, height: 22)
        } else if status == .working {
            ClaudeSparkleView()
                .frame(width: 18, height: 18)
        } else {
            MinimalDotView(status: status, waitingReason: waitingReason)
        }
    }
}

private struct ProgressRingView: View {
    let progress: Double
    let status: SessionStatus
    var waitingReason: WaitingReason?
    @Environment(\.theme) private var theme
    @State private var isPulsing = false

    private var isWorking: Bool { status == .working }
    private var needsAttention: Bool { status == .needsInput || status == .error }

    var body: some View {
        ZStack {
            Circle()
                .stroke(theme.secondaryText.opacity(0.15), lineWidth: 2.5)
            Circle()
                .trim(from: 0, to: progress)
                .stroke(theme.primaryText.opacity(0.7), style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            if isWorking {
                Circle()
                    .trim(from: max(0, progress - 0.08), to: progress)
                    .stroke(theme.primaryText.opacity(0.4), style: StrokeStyle(lineWidth: 3.5, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .blur(radius: 1.5)
                    .opacity(isPulsing ? 1.0 : 0.3)
            }
            if needsAttention {
                Circle()
                    .fill(theme.waitingColor(for: waitingReason))
                    .frame(width: 6, height: 6)
                    .offset(x: 7, y: -7)
            }
        }
        .animation(
            isWorking ? .easeInOut(duration: 1.2).repeatForever(autoreverses: true) : .default,
            value: isPulsing
        )
        .onAppear { if isWorking { isPulsing = true } }
        .onChange(of: status) { _, _ in isPulsing = (status == .working) }
    }
}

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
                    .frame(width: 20, height: 20)
                    .scaleEffect(isPulsing ? 1.4 : 0.8)
                    .opacity(isPulsing ? 0 : 0.6)
            }
            Circle()
                .fill(dotColor)
                .frame(width: 10, height: 10)
                .scaleEffect(isPulsing && needsPulse ? 1.15 : 1.0)
        }
        .frame(width: 18, height: 18)
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
        case .compacting: return theme.accent
        case .idle: return theme.tertiaryText
        case .completed: return theme.tertiaryText.opacity(0.7)
        default: return theme.tertiaryText
        }
    }
}

struct ClaudeSparkleView: View {
    private static let phases: [String] = ["·", "✢", "✳", "✶", "✻", "✽"]
    private static let cycle: [String] = phases + phases.dropFirst().dropLast().reversed()
    @Environment(\.theme) private var theme
    @State private var currentIndex = 0
    private let timer = Timer.publish(every: 0.22, on: .main, in: .common).autoconnect()

    var body: some View {
        Text(Self.cycle[currentIndex])
            .font(.system(size: 14))
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
