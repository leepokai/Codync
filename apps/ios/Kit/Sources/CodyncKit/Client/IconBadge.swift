import UserNotifications
import os

/// The app icon's badge: unread messages across every computer's roster, as the chats' badges add up.
public enum IconBadge {
    /// Unread on a roster (its bots, hidden ones already left out).
    public static func count(_ roster: some Sequence<Bot>) -> Int {
        roster.reduce(0) { $0 + $1.unread }
    }

    /// A push's count for its computer, plus the other computers' from the last roster the app saw
    /// (the App Group snapshot, which holds no hidden bots).
    public static func count(pushed: Int, computerId: ComputerID, snapshot: [BotSnapshot]) -> Int {
        pushed + count(snapshot.filter { $0.computerId != computerId }.map(\.bot))
    }

    /// iOS keeps the count but shows it only once the app may notify (the push switch asks for that).
    public static func show(_ count: Int) {
        UNUserNotificationCenter.current().setBadgeCount(count) { error in
            if let error {
                Logger(subsystem: "com.pokai.Codync.ios", category: "IconBadge")
                    .error("Couldn't set the app icon badge: \(error.localizedDescription)")
            }
        }
    }
}
