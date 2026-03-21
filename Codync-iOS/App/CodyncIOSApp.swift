import SwiftUI
import CodyncShared

@main
struct CodyncIOSApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            IOSRootView(
                receiver: appDelegate.receiver,
                liveActivityManager: appDelegate.liveActivityManager,
                primarySessionManager: appDelegate.primarySessionManager
            )
        }
        .onChange(of: scenePhase) { phase in
            if phase == .active {
                Task {
                    await appDelegate.receiver.fetch(source: "foreground-return")
                    appDelegate.liveActivityManager.updateSessions(appDelegate.receiver.sessions)
                    appDelegate.primarySessionManager.autoSelect(from: appDelegate.receiver.sessions)
                }
            }
        }
    }
}
