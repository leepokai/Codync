import SwiftUI
import CodyncShared

@main
struct CodyncIOSApp: App {
    @StateObject private var receiver = CloudKitReceiver()
    @StateObject private var liveActivityManager = LiveActivityManager()

    var body: some Scene {
        WindowGroup {
            IOSRootView(receiver: receiver, liveActivityManager: liveActivityManager)
                .task { await receiver.start() }
                .task {
                    let center = UNUserNotificationCenter.current()
                    try? await center.requestAuthorization(options: [.alert, .sound, .badge])
                }
        }
    }
}
