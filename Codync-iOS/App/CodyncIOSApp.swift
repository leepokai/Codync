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
        .onChange(of: scenePhase) { _, phase in
            if phase == .active {
                appDelegate.liveActivityManager.ensureTimerRunning()
                // Force push current state immediately (isDark, duration, etc.)
                appDelegate.liveActivityManager.invalidateCache()
                appDelegate.liveActivityManager.userPrimarySessionId = appDelegate.primarySessionManager.primarySessionId
                appDelegate.liveActivityManager.updateSessions(appDelegate.receiver.sessions)
                // Then fetch fresh data from CloudKit
                Task {
                    await appDelegate.receiver.fetch(source: "foreground-return", force: true)
                    appDelegate.liveActivityManager.updateSessions(appDelegate.receiver.sessions)
                    appDelegate.primarySessionManager.autoSelect(from: appDelegate.receiver.sessions)
                }
            }
        }
    }
}
