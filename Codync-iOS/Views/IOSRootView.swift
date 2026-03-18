import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false

    private let theme = CodyncTheme()

    var body: some View {
        NavigationStack {
            Group {
                if !onboardingComplete {
                    IOSOnboardingView()
                } else {
                    IOSSessionListView(
                        sessions: receiver.sessions,
                        liveActivityManager: liveActivityManager
                    )
                }
            }
        }
        .environment(\.theme, theme)
        .preferredColorScheme(.dark)
        .onChange(of: receiver.sessions) { _, sessions in
            if !sessions.isEmpty {
                onboardingComplete = true
            }
            liveActivityManager.updateSessions(sessions)
        }
    }
}
