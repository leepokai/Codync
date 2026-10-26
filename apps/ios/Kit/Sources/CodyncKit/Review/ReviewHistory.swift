import Foundation

/// Device-local StoreKit policy. Requests are attempts, never evidence of a rating.
struct ReviewHistory: Codable {
    struct Request: Codable {
        let date: Date
        let version: String
        let completed: Int
    }

    static let day: TimeInterval = 86_400
    var firstUse: Date
    var activeDays: Set<String> = []
    var completed = 0
    var requests: [Request] = []
    var requestedVersions: Set<String> = []
    var sent: [String: Date] = [:]

    mutating func recordActivity(at now: Date) {
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: now)
        activeDays.insert("\(parts.year ?? 0)-\(parts.month ?? 0)-\(parts.day ?? 0)")
        // Only the three-day milestone matters; retain a bounded set after reaching it.
        if activeDays.count > 3 { activeDays = Set(activeDays.sorted().suffix(3)) }
    }

    func eligible(at now: Date, version: String) -> Bool {
        guard now.timeIntervalSince(firstUse) >= 7 * Self.day,
              activeDays.count >= 3, completed >= 5,
              !requestedVersions.contains(version),
              requests.filter({ now.timeIntervalSince($0.date) < 365 * Self.day }).count < 2 else { return false }
        guard let last = requests.last else { return true }
        return now.timeIntervalSince(last.date) >= 120 * Self.day && completed - last.completed >= 5
    }

    mutating func recordRequest(at now: Date, version: String) {
        requestedVersions.insert(version)
        requests.removeAll { now.timeIntervalSince($0.date) >= 365 * Self.day }
        requests.append(Request(date: now, version: version, completed: completed))
    }
}
