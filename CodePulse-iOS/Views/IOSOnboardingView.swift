import SwiftUI

struct IOSOnboardingView: View {
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            Image(systemName: "waveform.path")
                .font(.system(size: 56, weight: .light))
                .foregroundStyle(theme.secondaryText.opacity(0.6))

            VStack(spacing: 8) {
                Text("CodePulse")
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                    .foregroundStyle(theme.primaryText)
                Text("Install CodePulse on your Mac\nto start monitoring Claude Code sessions")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 15))
                    .foregroundStyle(theme.secondaryText)
            }

            HStack(spacing: 6) {
                ClaudeSparkleView()
                    .frame(width: 14, height: 14)
                Text("Waiting for sessions…")
                    .font(.system(size: 14))
                    .foregroundStyle(theme.secondaryText)
            }
            .padding(.top, 12)

            Spacer()
        }
        .padding()
    }
}
