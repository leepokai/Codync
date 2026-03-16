import SwiftUI
import CodePulseShared

struct StatusDotView: View {
    let status: SessionStatus
    @State private var animating = false

    var body: some View {
        Circle()
            .fill(status.color)
            .frame(width: 8, height: 8)
            .shadow(color: status.color.opacity(shouldPulse ? 0.6 : 0), radius: shouldPulse ? 4 : 0)
            .scaleEffect(animating ? 1.15 : 1.0)
            .opacity(animating ? 0.5 : 1.0)
            .onAppear { if shouldPulse { animating = true } }
            .onChange(of: status) { _, s in animating = shouldPulse(s) }
            .animation(shouldPulse ? .easeInOut(duration: 1.2).repeatForever(autoreverses: true) : .default, value: animating)
    }

    private var shouldPulse: Bool { shouldPulse(status) }
    private func shouldPulse(_ s: SessionStatus) -> Bool {
        s == .working || s == .needsInput || s == .error
    }
}
