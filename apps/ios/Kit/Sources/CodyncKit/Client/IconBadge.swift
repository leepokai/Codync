import UserNotifications
import os

/// The app icon's badge: unread messages across every computer's roster, as the chats' badges add up.
public enum IconBadge {
    /// More than any chat holds. A paired computer's alert is trusted no further than this: past it a count
    /// would only overflow the sum.
    private static let mostUnread = 99_999
    /// Once per launch: the roster changes often, and what fails once fails every time.
    @MainActor private static var failureReported = false

    /// The extension, on an alert from `computerId`: keeps its total and gives the badge. `nil` from a
    /// computer removed from this phone, which counts no more.
    public static func alerted(_ unread: Int, from computerId: ComputerID, in store: SharedStore.Context) -> Int? {
        let paired = Set(store.computers.map(\.id))
        guard paired.contains(computerId) else { return nil }
        var pushed = store.pushedUnread.filter { paired.contains($0.key) }
        pushed[computerId] = unread
        store.pushedUnread = pushed
        return count(pushed: pushed, snapshot: store.bots)
    }

    /// The app, when the roster changed: alerts' totals stay only for `behind`, the paired computers whose
    /// roster may still trail the host. Gives the badge.
    public static func settled(behind: Set<ComputerID>, in store: SharedStore.Context) -> Int {
        let saved = store.pushedUnread
        let pushed = saved.filter { behind.contains($0.key) }
        if pushed != saved { store.pushedUnread = pushed }
        return count(pushed: pushed, snapshot: store.bots)
    }

    /// The alerts' totals, plus the other computers' unread from the snapshot (which holds no hidden bots).
    private static func count(pushed: [ComputerID: Int], snapshot: [BotSnapshot]) -> Int {
        let counts = pushed.values + snapshot.filter { pushed[$0.computerId] == nil }.map(\.bot.unread)
        return counts.reduce(0) { $0 + min(max($1, 0), mostUnread) }
    }

    /// iOS keeps the count but shows it only once the app may notify (the push switch asks for that).
    public static func show(_ count: Int) {
        UNUserNotificationCenter.current().setBadgeCount(count) { error in
            guard let error else { return }
            Logger(subsystem: "com.pokai.Codync.ios", category: "IconBadge")
                .error("Couldn't set the app icon badge: \(error.localizedDescription)")
            let reason: AnalyticsEvent.IconBadgeFailure = (error as? UNError)?.code == .notificationsNotAllowed ? .notAllowed : .other
            Task { @MainActor in
                guard !failureReported else { return }
                failureReported = true
                Analytics.shared.capture(.iconBadgeFailed(reason))
            }
        }
    }
}
