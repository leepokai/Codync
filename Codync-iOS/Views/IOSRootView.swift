import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @AppStorage("codync_darkMode") private var isDarkMode = true

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
        .environment(\.theme, CodyncTheme(isDark: isDarkMode))
        .preferredColorScheme(isDarkMode ? .dark : .light)
        .onChange(of: receiver.sessions) { _, sessions in
            if !sessions.isEmpty {
                onboardingComplete = true
            }
            liveActivityManager.updateSessions(sessions)
        }
    }
}
