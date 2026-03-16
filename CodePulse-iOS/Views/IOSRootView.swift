import SwiftUI
import CodePulseShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver

    var body: some View {
        NavigationStack {
            if receiver.sessions.isEmpty {
                IOSOnboardingView()
            } else {
                IOSSessionListView(sessions: receiver.sessions)
            }
        }
    }
}
