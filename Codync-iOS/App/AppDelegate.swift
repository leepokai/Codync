import UIKit
import CloudKit
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync.ios", category: "AppDelegate")

final class AppDelegate: NSObject, UIApplicationDelegate {
    let receiver = CloudKitReceiver()
    let liveActivityManager = LiveActivityManager()
    let primarySessionManager = PrimarySessionManager()
    private var pollTimer: Timer?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        application.registerForRemoteNotifications()
        NotificationCenter.default.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                await self.receiver.fetch(source: "did-become-active", force: true)
                self.liveActivityManager.userPrimarySessionId = self.primarySessionManager.primarySessionId
                self.liveActivityManager.updateSessions(self.receiver.sessions)
            }
        }
        Task {
            PremiumManager.shared.configure()
            async let startReceiver: () = receiver.start()
            async let loadPins: () = liveActivityManager.loadPinnedSessions()
            async let loadPref: () = liveActivityManager.loadPreference()
            async let loadPrimary: () = primarySessionManager.load()
            _ = await (startReceiver, loadPins, loadPref, loadPrimary)

            liveActivityManager.userPrimarySessionId = primarySessionManager.primarySessionId
            liveActivityManager.updateSessions(receiver.sessions)
            primarySessionManager.autoSelect(from: receiver.sessions)

            startForegroundPolling()
        }
        return true
    }

    // MARK: - Foreground Polling

    func startForegroundPolling() {
        pollTimer?.invalidate()
        pollTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                await self.receiver.fetch(source: "poll", force: true)
                self.liveActivityManager.userPrimarySessionId = self.primarySessionManager.primarySessionId
                self.liveActivityManager.updateSessions(self.receiver.sessions)
                self.primarySessionManager.autoSelect(from: self.receiver.sessions)
            }
        }
        RunLoop.main.add(pollTimer!, forMode: .common)
    }

    func stopForegroundPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        logger.info("APNs device token registered: \(hex.prefix(8))...")
        // Save device token to CloudKit for Pro alert push (session complete notifications)
        Task {
            guard PremiumManager.shared.isPro else { return }
            let recordID = CKRecord.ID(
                recordName: "device-push-token",
                zoneID: CloudKitManager.zoneID
            )
            do {
                let record: CKRecord
                do {
                    record = try await CloudKitManager.shared.database.record(for: recordID)
                } catch {
                    record = CKRecord(recordType: "DeviceToken", recordID: recordID)
                }
                record["token"] = hex as CKRecordValue
                record["updatedAt"] = Date() as CKRecordValue
                _ = try await CloudKitManager.shared.database.save(record)
                logger.info("Saved device push token to CloudKit")
            } catch {
                logger.error("Failed to save device push token: \(error.localizedDescription)")
            }
        }
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
        // Pro users rely on APNs Worker push — skip silent push to isolate testing
        if PremiumManager.shared.isPro {
            logger.info("CloudKit silent push skipped (Pro uses APNs Worker)")
            return .noData
        }

        let notification = CKNotification(fromRemoteNotificationDictionary: userInfo)
        let subID = notification?.subscriptionID
        guard subID == "session-zone-changes" || subID == "session-changes" else {
            return .noData
        }
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        let ts = fmt.string(from: Date())
        logger.info("[\(ts)] CloudKit push received")
        await receiver.fetch(source: "silent-push", force: true)
        let statuses = receiver.sessions.map { "\($0.projectName):\($0.status.rawValue)" }.joined(separator: ", ")
        logger.info("[\(ts)] → \(statuses)")
        liveActivityManager.updateSessions(receiver.sessions)
        primarySessionManager.autoSelect(from: receiver.sessions)
        return .newData
    }
}
