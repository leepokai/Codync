import Foundation

/// Device-local StoreKit policy. Requests are attempts, never evidence of a rating.
struct ReviewHistory: Codable {
    struct Request: Codable {
        let date: Date
        let version: String
        let prompts: Int
    }

    static let day: TimeInterval = 86_400
    /// The prompt that brings the first request, and how many more each later one needs.
    static let promptsPerRequest = 5
    var prompts = 0
    var requests: [Request] = []
    var requestedVersions: Set<String> = []

    func eligible(at now: Date, version: String) -> Bool {
        guard prompts >= Self.promptsPerRequest,
              !requestedVersions.contains(version),
              requests.filter({ now.timeIntervalSince($0.date) < 365 * Self.day }).count < 2 else { return false }
        guard let last = requests.last else { return true }
        return now.timeIntervalSince(last.date) >= 120 * Self.day && prompts - last.prompts >= Self.promptsPerRequest
    }

    mutating func recordRequest(at now: Date, version: String) {
        requestedVersions.insert(version)
        requests.removeAll { now.timeIntervalSince($0.date) >= 365 * Self.day }
        requests.append(Request(date: now, version: version, prompts: prompts))
    }
}

/// A history saved before 2.39.1 counted results, not prompts: its requests still hold, and prompts start at zero.
extension ReviewHistory {
    init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        prompts = try values.decodeIfPresent(Int.self, forKey: .prompts) ?? 0
        requests = try values.decode([Request].self, forKey: .requests)
        requestedVersions = try values.decode(Set<String>.self, forKey: .requestedVersions)
    }
}

extension ReviewHistory.Request {
    init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        date = try values.decode(Date.self, forKey: .date)
        version = try values.decode(String.self, forKey: .version)
        prompts = try values.decodeIfPresent(Int.self, forKey: .prompts) ?? 0
    }
}
