import Testing
@testable import CodyncUI

private struct CatalogItem: Identifiable { let id: Int }

@Test func catalogPagesDeduplicate() {
    var page = MarketplacePage<CatalogItem>()
    page.replace(with: (0..<12).map { CatalogItem(id: $0) })
    page.append((10..<15).map { CatalogItem(id: $0) })
    #expect(page.items.map(\.id) == Array(0..<15))
    page.replace(with: [])
    #expect(page.isEmpty)
}

private struct Offline: Error {}

@Test @MainActor func moreConnectorsReadOnPastEmptyPagesAndStopAtAFailure() async {
    let catalog = ConnectorCatalog<CatalogItem>()
    var asked: [String?] = []
    let rows = { (range: Range<Int>) in range.map { CatalogItem(id: $0) } }
    await catalog.load(search: "") { _, cursor in asked.append(cursor); return (rows(0..<15), "a") }
    #expect(catalog.items.items.count == 15 && !catalog.loading)

    // The next page has nothing to install but moves the cursor on: the one after it brings the rows.
    await catalog.loadMore { _, cursor in asked.append(cursor); return ([], "b") }
    #expect(catalog.items.items.count == 15 && catalog.cursor == "b")
    await catalog.loadMore { _, cursor in asked.append(cursor); return (rows(15..<25), "c") }
    #expect(asked == [nil, "a", "b"])
    #expect(catalog.items.items.count == 25 && catalog.cursor == "c" && !catalog.loadingMore)

    await catalog.loadMore { _, _ in throw Offline() }
    #expect(catalog.items.items.count == 25 && catalog.cursor == "c" && catalog.error != nil && !catalog.loadingMore)

    // The cursor ends: nothing more to ask for.
    await catalog.loadMore { _, _ in (rows(25..<26), nil) }
    #expect(catalog.items.items.count == 26 && !catalog.hasMore && catalog.error == nil)
}

@Test @MainActor func anEmptyFirstPageThatContinuesKeepsItsCursor() async {
    let catalog = ConnectorCatalog<CatalogItem>()
    await catalog.load(search: "search") { _, cursor in cursor == nil ? ([], "a") : ([CatalogItem(id: 1)], nil) }
    #expect(catalog.items.isEmpty && catalog.cursor == "a")
    await catalog.loadMore { _, _ in ([CatalogItem(id: 1)], nil) }
    #expect(catalog.items.items.count == 1 && !catalog.hasMore)
}
