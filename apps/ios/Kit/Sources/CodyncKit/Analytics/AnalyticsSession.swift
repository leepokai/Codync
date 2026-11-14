import Foundation

/// `$session_id`: one id for a stretch of use, so an error's page in PostHog shows the events
/// that led up to it. A new one after 30 minutes without events or 24 hours in all, as PostHog's
/// own SDKs do (the host's `analytics/session.rs`).
struct AnalyticsSession {
    private static let idle: TimeInterval = 30 * 60
    private static let longest: TimeInterval = 24 * 60 * 60
    private var current: (id: String, started: Date, last: Date)?

    /// The id for an event at `now`; the first event after the last session ran out starts a new one.
    mutating func id(at now: Date) -> String {
        if let s = current, now.timeIntervalSince(s.last) <= Self.idle, now.timeIntervalSince(s.started) <= Self.longest {
            current?.last = now
            return s.id
        }
        let id = Self.uuidV7(now).uuidString.lowercased()
        current = (id, now, now)
        return id
    }

    /// PostHog reads a session's start from its UUIDv7: milliseconds since 1970 in the first 48 bits.
    private static func uuidV7(_ date: Date) -> UUID {
        let ms = UInt64(date.timeIntervalSince1970 * 1000)
        var bytes = UUID().uuid
        withUnsafeMutableBytes(of: &bytes) { b in
            for i in 0..<6 { b[i] = UInt8(truncatingIfNeeded: ms >> (40 - 8 * i)) }
            b[6] = 0x70 | (b[6] & 0x0F)
            b[8] = 0x80 | (b[8] & 0x3F)
        }
        return UUID(uuid: bytes)
    }
}
