import SwiftUI
import CodePulseShared

@main
struct CodePulseIOSApp: App {
    @StateObject private var receiver = CloudKitReceiver()

    var body: some Scene {
        WindowGroup {
            IOSRootView(receiver: receiver)
                .task { await receiver.start() }
                .task {
                    let center = UNUserNotificationCenter.current()
                    try? await center.requestAuthorization(options: [.alert, .sound, .badge])
                }
        }
    }
}
