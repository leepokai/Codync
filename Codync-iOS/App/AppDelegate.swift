import UIKit
import CloudKit
import CodyncShared

final class AppDelegate: NSObject, UIApplicationDelegate {
    weak var receiver: CloudKitReceiver?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        application.registerForRemoteNotifications()
        return true
    }

    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        let notification = CKNotification(fromRemoteNotificationDictionary: userInfo)
        guard notification?.subscriptionID == "session-changes" else {
            return .noData
        }
        await receiver?.fetch()
        return .newData
    }
}
