import Testing
@testable import CodyncUI

@MainActor @Test func blockCacheReusesCompletedPrefixSlotsDuringTailGrowth() {
    let inlineCache = MarkdownInlineCache(capacity: 64)
    let blockCache = MarkdownBlockCache()

    let base = "# Title\n\nFirst settled paragraph with **bold**.\n\n- bullet one\n\n"
    let firstRender = blockCache.render(base + "Tail one", isStreaming: true, inlineCache: inlineCache)
    #expect(firstRender.count == 4)
    #expect(blockCache.slotMisses == 4)

    blockCache.resetStats()
    let secondRender = blockCache.render(base + "Tail one two", isStreaming: true, inlineCache: inlineCache)
    #expect(secondRender.count == 4)
    #expect(secondRender[0] == firstRender[0])
    #expect(secondRender[1] == firstRender[1])
    #expect(secondRender[2] == firstRender[2])
    #expect(blockCache.slotHits == 3)
    #expect(blockCache.slotMisses == 1)
}

@MainActor @Test func blockCacheReusesClosedCodeFenceCheckpointsWithoutReparsingPrefix() {
    let inlineCache = MarkdownInlineCache(capacity: 64)
    let blockCache = MarkdownBlockCache()

    let fenced = "```swift\nlet x = 1\n```\n"
    let first = blockCache.render(fenced + "After", isStreaming: true, inlineCache: inlineCache)
    #expect(first.count == 2)

    blockCache.resetStats()
    let second = blockCache.render(fenced + "Afterwards more text", isStreaming: true, inlineCache: inlineCache)
    #expect(second.count == 2)
    #expect(second[0] == first[0])
    #expect(blockCache.slotHits == 1)
    #expect(blockCache.slotMisses == 1)
}

@MainActor @Test func staticRepeatedRenderHitsInlineAndSlotCaches() {
    let inlineCache = MarkdownInlineCache(capacity: 64)
    let blockCache = MarkdownBlockCache()
    let doc = "Hello [link](https://example.com) and `code`."

    let a = blockCache.render(doc, isStreaming: false, inlineCache: inlineCache)
    #expect(inlineCache.misses == 1)

    blockCache.resetStats()
    inlineCache.resetStats()
    let b = blockCache.render(doc, isStreaming: false, inlineCache: inlineCache)
    #expect(a == b)
    #expect(blockCache.slotHits == a.count)
    #expect(blockCache.slotMisses == 0)

    // Rendering the same inline fragment in a fresh block cache still hits the shared inline LRU.
    let secondCache = MarkdownBlockCache()
    _ = secondCache.render(doc, isStreaming: false, inlineCache: inlineCache)
    #expect(inlineCache.hits == 1)
    #expect(inlineCache.misses == 0)
}

@MainActor @Test func inlineCacheEvictsLeastRecentlyUsedEntriesWhenOverCapacity() {
    let cache = MarkdownInlineCache(capacity: 3)
    _ = cache.attributedString(for: "a")
    _ = cache.attributedString(for: "b")
    _ = cache.attributedString(for: "c")
    // Touch "a" so "b" becomes least recently used.
    _ = cache.attributedString(for: "a")
    _ = cache.attributedString(for: "d")

    #expect(cache.count == 3)
    #expect(cache.contains("a"))
    #expect(!cache.contains("b"))
    #expect(cache.contains("c"))
    #expect(cache.contains("d"))
}

@MainActor @Test func incrementalBlockCacheMatchesFullMarkdownBlocksParse() {
    let samples = [
        "# H1\n\nParagraph line 1\nline 2\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n---\n\n> quote",
        "Intro\n\n```py\nprint(1)\n```\n\n- one\n  - two\n1. ordered\n\nTail **unclosed",
        "Plain single line",
        "",
    ]

    for sample in samples {
        for streaming in [false, true] {
            let cache = MarkdownBlockCache()
            // Step through incremental checkpoints before reaching full sample.
            if !sample.isEmpty {
                let midpoint = sample.index(sample.startIndex, offsetBy: sample.count / 2)
                _ = cache.render(String(sample[..<midpoint]), isStreaming: streaming)
            }
            let rendered = cache.render(sample, isStreaming: streaming).map(\.block)
            let expected = MarkdownBlocks.parse(sample, isStreaming: streaming)
            #expect(rendered == expected)
        }
    }
}

@MainActor @Test func blockCacheHandlesPrefixEditsAndShrinksCleanly() {
    let blockCache = MarkdownBlockCache()
    _ = blockCache.render("One\n\nTwo\n\nThree", isStreaming: false)
    let shrunk = blockCache.render("Changed\n\nFour", isStreaming: false).map(\.block)
    #expect(shrunk == MarkdownBlocks.parse("Changed\n\nFour", isStreaming: false))
}

@MainActor @Test func crlfLineEndingsPreserveBlocksAndIncrementalCheckpoints() {
    let doc = "# Title\r\n\r\n```swift\r\nlet a = 1\r\nlet b = 2\r\n```\r\n\r\n- item 1\r\n- item 2\r\n\r\n| Key | Val |\r\n| --- | --- |\r\n| x | y |\r\n\r\n---\r\n\r\n> quote"
    let expected = MarkdownBlocks.parse(doc)
    #expect(expected == [
        .heading("Title", level: 1),
        .code("let a = 1\nlet b = 2", language: "swift"),
        .bullet("item 1", marker: "•", depth: 0),
        .bullet("item 2", marker: "•", depth: 0),
        .table(header: ["Key", "Val"], rows: [["x", "y"]]),
        .rule,
        .quote("quote"),
    ])

    let inlineCache = MarkdownInlineCache(capacity: 64)
    let blockCache = MarkdownBlockCache()

    let prefix = "```swift\r\nlet a = 1\r\n```\r\n"
    let first = blockCache.render(prefix + "Tail", isStreaming: true, inlineCache: inlineCache)
    #expect(first.map(\.block) == [.code("let a = 1", language: "swift"), .paragraph("Tail")])

    blockCache.resetStats()
    let second = blockCache.render(prefix + "Tail extended", isStreaming: true, inlineCache: inlineCache)
    #expect(second.map(\.block) == [.code("let a = 1", language: "swift"), .paragraph("Tail extended")])
    #expect(blockCache.slotHits == 1)
    #expect(blockCache.slotMisses == 1)

    let fullCache = MarkdownBlockCache()
    _ = fullCache.render(String(doc.prefix(doc.count / 2)), isStreaming: true, inlineCache: inlineCache)
    let fullRendered = fullCache.render(doc, isStreaming: false, inlineCache: inlineCache).map(\.block)
    #expect(fullRendered == expected)
}
