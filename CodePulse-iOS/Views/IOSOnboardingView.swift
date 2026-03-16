import SwiftUI

struct IOSOnboardingView: View {
    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "waveform.path")
                .font(.system(size: 64))
                .foregroundStyle(.cyan)
            Text("CodePulse")
                .font(.title.bold())
            Text("Install CodePulse on your Mac\nto start monitoring Claude Code sessions")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            HStack(spacing: 8) {
                Circle().fill(.green).frame(width: 8, height: 8)
                Text("Waiting for connection...")
                    .foregroundStyle(.secondary)
                    .font(.callout)
            }
            .padding(.top, 20)
        }
        .padding()
    }
}
