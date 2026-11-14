import Foundation

/// The IANA zones a routine can run in, and the search over them.
public enum TimeZones {
    /// "GMT+8", "GMT-4", "GMT+5:30" right now; empty for an unknown zone.
    public static func offset(_ zone: String) -> String {
        guard let timeZone = TimeZone(identifier: zone) else { return "" }
        return Date.now.formatted(Date.FormatStyle(timeZone: timeZone).timeZone(.localizedGMT(.short)))
    }

    /// Zones matching the query, best first: the name starts with it, then each word starts a word,
    /// then each word appears somewhere, then its letters appear in order. "/", "_" and "-" count as spaces.
    public static func search(_ query: String, in zones: [String] = TimeZone.knownTimeZoneIdentifiers) -> [String] {
        let wanted = words(query)
        guard !wanted.isEmpty else { return zones }
        return zones.compactMap { zone in rank(words(zone), wanted).map { (rank: $0, zone: zone) } }
            .sorted { ($0.rank, $0.zone) < ($1.rank, $1.zone) }
            .map(\.zone)
    }

    private static func words(_ text: String) -> [String] {
        text.lowercased().split { " /_-".contains($0) }.map(String.init)
    }

    private static func rank(_ name: [String], _ query: [String]) -> Int? {
        let spaced = name.joined(separator: " ")
        if spaced.hasPrefix(query.joined(separator: " ")) { return 0 }
        if query.allSatisfy({ word in name.contains { $0.hasPrefix(word) } }) { return 1 }
        if query.allSatisfy({ spaced.contains($0) }) { return 2 }
        var rest = Substring(name.joined())
        for letter in query.joined() {
            guard let found = rest.firstIndex(of: letter) else { return nil }
            rest = rest[rest.index(after: found)...]
        }
        return 3
    }
}
