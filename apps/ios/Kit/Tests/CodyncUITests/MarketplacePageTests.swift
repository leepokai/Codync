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

@Test func catalogPagingClampsAndFetchesOnlyPastLoadedRows() {
    let items = Array(0..<47)
    #expect(CatalogPaging.pageCount(47) == 5)
    #expect(CatalogPaging.pageCount(19) == 2)
    #expect(CatalogPaging.pageCount(0) == 1)
    #expect(CatalogPaging.pageCount(72, hasMore: true) == 9)
    #expect(Array(CatalogPaging.slice(items, page: 0)) == Array(0..<10))
    #expect(Array(CatalogPaging.slice(items, page: 4)) == Array(40..<47))
    #expect(Array(CatalogPaging.slice(items, page: 9)).isEmpty)
    #expect(CatalogPaging.clamp(page: 9, count: 47) == 4)
    #expect(CatalogPaging.clamp(page: 9, count: 72, hasMore: true) == 8)
    #expect(CatalogPaging.clamp(page: -1, count: 20) == 0)
    #expect(CatalogPaging.clamp(page: 4, count: 0) == 0)
    #expect(CatalogPaging.canForward(count: 10, page: 0, hasMore: true))
    #expect(CatalogPaging.needsFetch(count: 10, page: 1))
    #expect(CatalogPaging.canForward(count: 20, page: 0, hasMore: false))
    #expect(!CatalogPaging.needsFetch(count: 20, page: 1))
    #expect(!CatalogPaging.canForward(count: 8, page: 0, hasMore: false))
    #expect(!CatalogPaging.needsFetch(count: 60, page: 5))
    #expect(CatalogPaging.needsFetch(count: 60, page: 6))
    #expect(CatalogPaging.needsFetch(count: 72, page: 7))
    #expect(!CatalogPaging.needsFetch(count: 80, page: 7))
    #expect(!CatalogPaging.needsFetch(count: 0, page: 0))
    #expect(!CatalogPaging.needsFetch(count: 3, page: 0))
    #expect(CatalogPaging.needsFetch(count: 0, page: 1))
}

private struct Offline: Error {}

@Test @MainActor func aConnectorPageFillsUntilFullAndFallsBackWhenItCannot() async {
    let catalog = ConnectorCatalog<CatalogItem>()
    var asked: [String?] = []
    let rows = { (range: Range<Int>) in range.map { CatalogItem(id: $0) } }
    await catalog.load(search: "") { _, cursor in asked.append(cursor); return (rows(0..<15), "a") }
    #expect(catalog.items.items.count == 15 && catalog.page == 0 && !catalog.loading)

    // Page 2 needs rows 10-19: two registry pages fill it.
    catalog.show(1, size: 10)
    await catalog.fill(size: 10) { _, cursor in
        asked.append(cursor)
        return cursor == "a" ? (rows(15..<18), "b") : (rows(18..<25), "c")
    }
    #expect(asked == [nil, "a", "b"])
    #expect(catalog.page == 1 && catalog.items.items.count == 25 && catalog.cursor == "c" && !catalog.loadingMore)

    // The page past the loaded rows fails: back to the last page with rows.
    catalog.show(3, size: 10)
    #expect(catalog.page == 3)
    await catalog.fill(size: 10) { _, _ in throw Offline() }
    #expect(catalog.page == 2 && catalog.error != nil)
}
