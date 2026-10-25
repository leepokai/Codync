import SwiftUI

/// Keep fetched catalog metadata in memory. The Extensions tabs page through it.
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

/// How many catalog rows one Extensions page shows, and which page a click lands on.
enum CatalogPaging {
    static let pageSize = 10
    /// Two agent tiles across, three rows: a page has no leftover card.
    static let phonePageSize = 6
    static let pageSizes = [6, 10, 20, 50]

    /// Loaded rows split into pages. While the registry still has a cursor, one more page is offered so the bar is never 1 / 1 for a list that continues.
    static func pageCount(_ count: Int, size: Int = pageSize, hasMore: Bool = false) -> Int {
        let loaded = count <= 0 ? 1 : (count + size - 1) / size
        return hasMore ? loaded + 1 : loaded
    }

    static func clamp(page: Int, count: Int, size: Int = pageSize, hasMore: Bool = false) -> Int {
        min(max(0, page), pageCount(count, size: size, hasMore: hasMore) - 1)
    }

    static func slice<T>(_ items: [T], page: Int, size: Int = pageSize) -> ArraySlice<T> {
        let start = max(0, page) * size
        return items.dropFirst(start).prefix(size)
    }

    /// Next stays available through the last shown page, including the page that still has to be fetched.
    static func canForward(count: Int, page: Int, hasMore: Bool, size: Int = pageSize) -> Bool {
        page < pageCount(count, size: size, hasMore: hasMore) - 1
    }

    /// A later page that isn't full yet, so the registry has to be asked. The first page is whatever the first answer held.
    static func needsFetch(count: Int, page: Int, size: Int = pageSize) -> Bool {
        let start = max(0, page) * size
        return start > 0 && start + size > count
    }
}

/// The connector list Extensions pages through: the rows loaded so far, the registry cursor and the
/// page on screen. A page that isn't full asks the registry until it is, or until the cursor ends.
@MainActor @Observable
final class ConnectorCatalog<Item: Identifiable> {
    typealias Fetch = @MainActor (_ search: String, _ cursor: String?) async throws -> (items: [Item], next: String?)

    private(set) var items = MarketplacePage<Item>()
    /// The registry page after the ones loaded; nil at the end.
    private(set) var cursor: String?
    /// The page on screen. While the registry fills it, it can be the page past the loaded rows.
    private(set) var page = 0
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
            page = 0
            error = nil
        } catch {
            guard self.request == request, !(error is CancellationError), !Task.isCancelled else { return }
            items.replace(with: [])
            cursor = nil
            self.error = error.localizedDescription
        }
    }

    /// Turns to `next` at once, so a swipe stays where it landed; `fill` then loads its rows.
    func show(_ next: Int, size: Int) {
        guard !loading else { return }
        page = CatalogPaging.clamp(page: next, count: items.items.count, size: size, hasMore: hasMore)
    }

    /// Asks the registry until the page on screen is full or the cursor ends. A page turned meanwhile is
    /// filled by the same loop. If a page fails, the list goes back to the last page with rows.
    func fill(size: Int, animation: Animation? = nil, fetch: Fetch) async {
        guard !loading, !loadingMore, hasMore, CatalogPaging.needsFetch(count: items.items.count, page: page, size: size) else { return }
        let request = self.request
        loadingMore = true
        defer { if self.request == request { loadingMore = false } }
        do {
            while let cursor, CatalogPaging.needsFetch(count: items.items.count, page: page, size: size) {
                let more = try await fetch(query, cursor)
                guard !Task.isCancelled, self.request == request else { return }
                items.append(more.items)
                self.cursor = more.next
            }
            page = CatalogPaging.clamp(page: page, count: items.items.count, size: size, hasMore: hasMore)
            error = nil
        } catch {
            guard self.request == request, !(error is CancellationError), !Task.isCancelled else { return }
            self.error = error.localizedDescription
            withAnimation(animation) { page = CatalogPaging.clamp(page: page, count: items.items.count, size: size) }
        }
    }
}
