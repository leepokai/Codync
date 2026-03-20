import UIKit
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "AppDelegate")

final class AppDelegate: NSObject, UIApplicationDelegate {
    // AppDelegate OWNS these objects so they exist before any push can arrive.
    // Previously they were weak refs assigned from .task{}, creating a race condition
    // where push could arrive before wiring completed.
    let receiver = CloudKitReceiver()
    let liveActivityManager = LiveActivityManager()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        application.registerForRemoteNotifications()
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
        return .newData
    }
}
