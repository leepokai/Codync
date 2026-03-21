import UIKit
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "AppDelegate")

final class AppDelegate: NSObject, UIApplicationDelegate {
    let receiver = CloudKitReceiver()
    let liveActivityManager = LiveActivityManager()
    let primarySessionManager = PrimarySessionManager()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        application.registerForRemoteNotifications()
        Task {
            async let startReceiver: () = receiver.start()
            async let loadPins: () = liveActivityManager.loadPinnedSessions()
            async let loadPref: () = liveActivityManager.loadPreference()
            async let loadPrimary: () = primarySessionManager.load()
            _ = await (startReceiver, loadPins, loadPref, loadPrimary)

            liveActivityManager.updateSessions(receiver.sessions)
            primarySessionManager.autoSelect(from: receiver.sessions)

            let center = UNUserNotificationCenter.current()
            try? await center.requestAuthorization(options: [.alert, .sound, .badge])
        }
        return true
    }

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        let hex = deviceToken.prefix(4).map { String(format: "%02x", $0) }.joined()
        logger.info("APNs device token registered: \(hex)...")
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        logger.error("APNs registration failed: \(error.localizedDescription)")
    }

    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        let notification = CKNotification(fromRemoteNotificationDictionary: userInfo)
        let subID = notification?.subscriptionID
        guard subID == "session-zone-changes" || subID == "session-changes" else {
            return .noData
        }
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        let ts = fmt.string(from: Date())
        logger.info("[\(ts)] CloudKit push received")
        await receiver.fetch(source: "silent-push")
        let statuses = receiver.sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
        logger.info("[\(ts)] → \(statuses)")
        liveActivityManager.updateSessions(receiver.sessions)
        primarySessionManager.autoSelect(from: receiver.sessions)
        return .newData
    }
}
