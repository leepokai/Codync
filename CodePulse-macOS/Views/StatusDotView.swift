import SwiftUI
import CodePulseShared

struct StatusDotView: View {
    let status: SessionStatus
    @State private var isAnimating = false

    private func animates(_ s: SessionStatus) -> Bool {
        switch s {
        case .working, .needsInput, .error: return true
        default: return false
        }
    }

    var body: some View {
        let dotColor = status.color
        let shadowColor = dotColor.opacity(0.5)
        let shadowRadius: CGFloat = animates(status) ? 3 : 0
        let dotOpacity: Double = isAnimating ? 0.4 : 1.0
        let anim: Animation = animates(status)
            ? .easeInOut(duration: 1.0).repeatForever(autoreverses: true)
            : .default
        return Circle()
            .fill(dotColor)
            .frame(width: 8, height: 8)
            .shadow(color: shadowColor, radius: shadowRadius)
            .opacity(dotOpacity)
            .animation(anim, value: isAnimating)
            .onAppear {
                if animates(status) { isAnimating = true }
            }
            .onChange(of: status) { _, newStatus in
                isAnimating = animates(newStatus)
            }
    }
}
