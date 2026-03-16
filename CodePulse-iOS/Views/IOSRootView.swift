import SwiftUI
import CodePulseShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager

    var body: some View {
        NavigationStack {
            if receiver.sessions.isEmpty {
                IOSOnboardingView()
            } else {
                IOSSessionListView(sessions: receiver.sessions)
            }
        }
        .onChange(of: receiver.sessions) { _, sessions in
            liveActivityManager.update(sessions: sessions)
        }
    }
}
