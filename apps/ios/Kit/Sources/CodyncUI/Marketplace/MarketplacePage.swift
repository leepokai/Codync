import SwiftUI

/// Keep fetched catalog metadata in memory. The Extensions tabs scroll through it.
struct MarketplacePage<Item: Identifiable> {
    private(set) var items: [Item] = []

    var isEmpty: Bool { items.isEmpty }

    mutating func replace(with items: [Item]) {
        self.items = []
        append(items)
    }

    mutating func append(_ page: [Item]) {
        var known = Set(items.map(\.id))
        items += page.filter { known.insert($0.id).inserted }
    }
}

/// The connector list Extensions scrolls through: the rows loaded so far and the registry cursor.
/// The end of the list coming into view asks for more (`loadMore`).
@MainActor @Observable
final class ConnectorCatalog<Item: Identifiable> {
    typealias Fetch = @MainActor (_ search: String, _ cursor: String?) async throws -> (items: [Item], next: String?)

    private(set) var items = MarketplacePage<Item>()
    /// The registry page after the ones loaded; nil at the end.
    private(set) var cursor: String?
    private(set) var loading = true
    private(set) var loadingMore = false
    private(set) var error: String?
    private var request = UUID()
    private var query = ""

    var hasMore: Bool { cursor != nil }

    /// The first registry page for `search`; answers to an older search are dropped.
    func load(search: String, fetch: Fetch) async {
        let request = UUID()
        self.request = request
        query = search
        loading = true
        loadingMore = false
        defer { if self.request == request { loading = false } }
        do {
            let first = try await fetch(search, nil)
            guard !Task.isCancelled, self.request == request else { return }
            items.replace(with: first.items)
            cursor = first.next
            error = nil
        } catch {
            guard self.request == request, !(error is CancellationError), !Task.isCancelled else { return }
            items.replace(with: [])
            cursor = nil
            self.error = error.localizedDescription
        }
    }

    /// The registry page at the cursor. It can bring nothing Codync can install; the cursor still moves
    /// on, so `LoadMoreRow` asks again. A failure keeps the cursor and shows under the list.
    func loadMore(fetch: Fetch) async {
        guard !loading, !loadingMore, let cursor else { return }
        let request = self.request
        loadingMore = true
        error = nil
        defer { if self.request == request { loadingMore = false } }
        do {
            let more = try await fetch(query, cursor)
            guard self.request == request else { return }
            items.append(more.items)
            self.cursor = more.next
        } catch {
            guard self.request == request, !(error is CancellationError), !Task.isCancelled else { return }
            self.error = error.localizedDescription
        }
    }
}
