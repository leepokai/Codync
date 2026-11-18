import Foundation
import UserNotifications
import os

/// The app icon's badge: unread messages across every computer's roster, as the chats' badges add up.
/// Off until the user turns it on in Settings; while off, the icon shows no number.
public enum IconBadge {
    /// The Settings switch, in the App Group so the notification extension reads it too.
    public static let enabledKey = "iconBadgeEnabled"
    public static var isEnabled: Bool { SharedStore.defaults.bool(forKey: enabledKey) }

    /// What the icon shows for `count` unread: nothing while the switch is off.
    public static func shown(_ count: Int, enabled: Bool = isEnabled) -> Int { enabled ? count : 0 }

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
        store.setPushedUnread(unread, for: computerId)
        return count(pushed: pushed(keeping: paired.contains, in: store), snapshot: store.bots)
    }

    /// The app, when the roster changed: alerts' totals stay only for `behind`, the paired computers whose
    /// roster may still trail the host. Gives the badge.
    public static func settled(behind: Set<ComputerID>, in store: SharedStore.Context) -> Int {
        count(pushed: pushed(keeping: behind.contains, in: store), snapshot: store.bots)
    }

    /// The alerts' totals of the computers `keep` accepts; the others' are dropped from `store`.
    private static func pushed(keeping keep: (ComputerID) -> Bool, in store: SharedStore.Context) -> [ComputerID: Int] {
        let pushed = store.pushedUnread
        for computerId in pushed.keys where !keep(computerId) { store.setPushedUnread(nil, for: computerId) }
        return pushed.filter { keep($0.key) }
    }

    /// The alerts' totals, plus the other computers' unread from the snapshot (which holds no hidden bots).
    private static func count(pushed: [ComputerID: Int], snapshot: [BotSnapshot]) -> Int {
        let counts = pushed.values + snapshot.filter { pushed[$0.computerId] == nil }.map(\.bot.unread)
        return counts.reduce(0) { $0 + min(max($1, 0), mostUnread) }
    }

    /// iOS keeps the count but shows it only once the app may notify (the push switch asks for that).
    @MainActor public static func show(_ count: Int) {
        let count = shown(count)
        Task {
            do {
                try await UNUserNotificationCenter.current().setBadgeCount(count)
            } catch {
                Logger(subsystem: "com.pokai.Codync.ios", category: "IconBadge")
                    .error("Couldn't set the app icon badge: \(error.localizedDescription)")
                guard !failureReported else { return }
                failureReported = true
                let notAllowed = (error as? UNError)?.code == .notificationsNotAllowed
                Analytics.shared.capture(.iconBadgeFailed(notAllowed ? .notAllowed : .other))
            }
        }
    }
}
