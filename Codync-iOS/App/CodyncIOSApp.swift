import SwiftUI
import CodyncShared

@main
struct CodyncIOSApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            // AppDelegate owns receiver & liveActivityManager — no race condition
            IOSRootView(receiver: appDelegate.receiver, liveActivityManager: appDelegate.liveActivityManager)
                .task {
                    await appDelegate.receiver.start()
                }
                .task {
                    let center = UNUserNotificationCenter.current()
                    try? await center.requestAuthorization(options: [.alert, .sound, .badge])
                }
        }
    }
}
