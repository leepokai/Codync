import Foundation

/// The choices of how long a computer keeps ended threads (`Hello.threadKeepDays`), in days, 0 never.
public enum ThreadKeep {
    public static let choices = [0, 7, 30, 90]

    /// The choices, with `current` among them when another device picked another number.
    public static func choices(with current: Int) -> [Int] {
        (choices.contains(current) ? choices : choices + [current]).sorted()
    }

    /// A choice in words (a phrase to look up).
    public static func label(_ days: Int) -> String { days == 0 ? "Keep forever" : "After \(days) days" }
}
