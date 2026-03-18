import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager

    private let theme = CodyncTheme()

    var body: some View {
        NavigationStack {
            Group {
                if receiver.sessions.isEmpty {
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
            liveActivityManager.updateSessions(sessions)
        }
    }
}
