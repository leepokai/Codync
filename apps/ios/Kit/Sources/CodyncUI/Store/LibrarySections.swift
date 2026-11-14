import CodyncKit
import Foundation

extension ChatLibraryItem {
    enum Period: String, CaseIterable {
        case today = "Today", yesterday = "Yesterday", thisWeek = "This week", earlier = "Earlier"
    }

    /// The Library's groups by when each item was published or shared, newest first; empty groups left out.
    static func sections(_ items: [ChatLibraryItem], now: Date = .now, calendar: Calendar = .current)
        -> [(period: Period, items: [ChatLibraryItem])] {
        let grouped = Dictionary(grouping: items) { item -> Period in
            if calendar.isDate(item.date, inSameDayAs: now) { return .today }
            if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(item.date, inSameDayAs: yesterday) {
                return .yesterday
            }
            // The last seven days, as on the other clients (not the calendar week).
            let weekAgo = calendar.date(byAdding: .day, value: -6, to: calendar.startOfDay(for: now)) ?? now
            return item.date >= weekAgo ? .thisWeek : .earlier
        }
        return Period.allCases.compactMap { period in
            grouped[period].map { (period, $0.sorted { $0.at > $1.at }) }
        }
    }
}
