import CodyncKit
import Foundation
import SwiftUI

/// Bounded LRU cache for inline Markdown `AttributedString` values so repeated renders
/// and unchanged prefix blocks avoid redundant `AttributedString(markdown:)` parsing.
@MainActor
final class MarkdownInlineCache {
    static let shared = MarkdownInlineCache(capacity: 512)

    private final class Node {
        let key: String
        var value: AttributedString
        var prev: Node?
        var next: Node?

        init(key: String, value: AttributedString) {
            self.key = key
            self.value = value
        }
    }

    let capacity: Int
    private var map: [String: Node] = [:]
    private var head: Node?
    private var tail: Node?

    private(set) var hits = 0
    private(set) var misses = 0
    var count: Int { map.count }

    init(capacity: Int = 512) {
        self.capacity = max(1, capacity)
    }

    func resetStats() {
        hits = 0
        misses = 0
    }

    func removeAll() {
        map.removeAll()
        head = nil
        tail = nil
        resetStats()
    }

    func contains(_ key: String) -> Bool {
        map[key] != nil
    }

    func attributedString(for raw: String) -> AttributedString {
        if let node = map[raw] {
            hits += 1
            moveToFront(node)
            return node.value
        }
        misses += 1
        let built = Self.buildInline(raw)
        insert(raw, value: built)
        return built
    }

    static func buildInline(_ s: String) -> AttributedString {
        guard var text = try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))
        else { return AttributedString(s) }
        for run in text.runs where run.link != nil {
            text[run.range].underlineStyle = .single
        }
        for run in text.runs where run.inlinePresentationIntent?.contains(.code) == true {
            text[run.range].backgroundColor = Palette.text.opacity(0.08)
        }
        return text
    }

    private func insert(_ key: String, value: AttributedString) {
        if let existing = map[key] {
            existing.value = value
            moveToFront(existing)
            return
        }
        let node = Node(key: key, value: value)
        map[key] = node
        attachToFront(node)
        if map.count > capacity, let evicted = tail {
            detach(evicted)
            map.removeValue(forKey: evicted.key)
        }
    }

    private func moveToFront(_ node: Node) {
        guard head !== node else { return }
        detach(node)
        attachToFront(node)
    }

    private func attachToFront(_ node: Node) {
        node.prev = nil
        node.next = head
        head?.prev = node
        head = node
        if tail == nil { tail = node }
    }

    private func detach(_ node: Node) {
        let prev = node.prev
        let next = node.next
        prev?.next = next
        next?.prev = prev
        if head === node { head = next }
        if tail === node { tail = prev }
        node.prev = nil
        node.next = nil
    }
}

/// Pre-rendered representation of a single `MarkdownBlocks.Block` so SwiftUI rows can
/// compare equality and skip rebuilding `AttributedString` for unchanged prefix blocks.
struct RenderedMarkdownBlock: Equatable {
    enum Content: Equatable {
        case paragraph(AttributedString, plain: String)
        case heading(AttributedString, plain: String, level: Int)
        case bullet(AttributedString, plain: String, marker: String, depth: Int)
        case quote(AttributedString, plain: String)
        case code(String, language: String)
        case table(header: [AttributedString], headerPlain: [String], rows: [[AttributedString]], rowsPlain: [[String]])
        case rule
    }

    let block: MarkdownBlocks.Block
    let content: Content

    @MainActor
    static func build(from block: MarkdownBlocks.Block, inlineCache: MarkdownInlineCache) -> Self {
        switch block {
        case let .paragraph(text):
            let attr = inlineCache.attributedString(for: text)
            return Self(block: block, content: .paragraph(attr, plain: String(attr.characters)))
        case let .heading(text, level):
            let attr = inlineCache.attributedString(for: text)
            return Self(block: block, content: .heading(attr, plain: String(attr.characters), level: level))
        case let .bullet(text, marker, depth):
            let attr = inlineCache.attributedString(for: text)
            return Self(block: block, content: .bullet(attr, plain: String(attr.characters), marker: marker, depth: depth))
        case let .quote(text):
            let attr = inlineCache.attributedString(for: text)
            return Self(block: block, content: .quote(attr, plain: String(attr.characters)))
        case let .code(text, language):
            return Self(block: block, content: .code(text, language: language))
        case let .table(header, rows):
            let columns = max(header.count, rows.map(\.count).max() ?? 0)
            let headerAttr = (0 ..< columns).map { i in
                inlineCache.attributedString(for: i < header.count ? header[i] : "")
            }
            let headerPlain = headerAttr.map { String($0.characters) }
            let rowsAttr = rows.map { row in
                (0 ..< columns).map { i in
                    inlineCache.attributedString(for: i < row.count ? row[i] : "")
                }
            }
            let rowsPlain = rowsAttr.map { row in row.map { String($0.characters) } }
            return Self(block: block, content: .table(header: headerAttr, headerPlain: headerPlain, rows: rowsAttr, rowsPlain: rowsPlain))
        case .rule:
            return Self(block: block, content: .rule)
        }
    }
}

/// Incremental block-level parse and `AttributedString` cache for streaming Markdown documents.
@MainActor
final class MarkdownBlockCache {
    private struct Checkpoint {
        let utf8End: Int
        let blockCount: Int
    }

    private var lastEffectiveSource = ""
    private var slots: [RenderedMarkdownBlock] = []
    private var checkpoints: [Checkpoint] = []

    private(set) var slotHits = 0
    private(set) var slotMisses = 0

    func resetStats() {
        slotHits = 0
        slotMisses = 0
    }

    func render(
        _ source: String,
        isStreaming: Bool,
        inlineCache: MarkdownInlineCache = .shared
    ) -> [RenderedMarkdownBlock] {
        let effective = isStreaming ? StreamingTailRepair.repair(source) : source
        if effective == lastEffectiveSource, !slots.isEmpty || effective.isEmpty {
            slotHits += slots.count
            return slots
        }

        let (blocks, nextCheckpoints) = parseIncremental(effective)
        var nextSlots: [RenderedMarkdownBlock] = []
        nextSlots.reserveCapacity(blocks.count)

        for (index, block) in blocks.enumerated() {
            if slots.indices.contains(index), slots[index].block == block {
                slotHits += 1
                nextSlots.append(slots[index])
            } else {
                slotMisses += 1
                nextSlots.append(RenderedMarkdownBlock.build(from: block, inlineCache: inlineCache))
            }
        }

        lastEffectiveSource = effective
        slots = nextSlots
        checkpoints = nextCheckpoints
        return nextSlots
    }

    private func parseIncremental(_ effective: String) -> ([MarkdownBlocks.Block], [Checkpoint]) {
        let utf8 = effective.utf8
        let prevUtf8 = lastEffectiveSource.utf8

        // Reuse any prefix up to a clean block boundary (blank line or closed code fence)
        // whose bytes match verbatim, so only the trailing block(s) are parsed.
        var reusedEnd = 0
        var reusedBlocks: [MarkdownBlocks.Block] = []
        var reusedCheckpoints: [Checkpoint] = []

        for cp in checkpoints.reversed() {
            guard cp.utf8End <= utf8.count, cp.utf8End <= prevUtf8.count, cp.blockCount <= slots.count else { continue }
            let curEnd = utf8.index(utf8.startIndex, offsetBy: cp.utf8End)
            let prevEnd = prevUtf8.index(prevUtf8.startIndex, offsetBy: cp.utf8End)
            if utf8[..<curEnd].elementsEqual(prevUtf8[..<prevEnd]) {
                reusedEnd = cp.utf8End
                reusedBlocks = slots.prefix(cp.blockCount).map(\.block)
                reusedCheckpoints = checkpoints.filter { $0.utf8End <= cp.utf8End }
                break
            }
        }

        let suffixStart = utf8.index(utf8.startIndex, offsetBy: reusedEnd)
        let suffix = String(effective[suffixStart...])
        let (tailBlocks, tailCheckpoints) = Self.parseWithCheckpoints(suffix, baseUtf8Offset: reusedEnd, baseBlockCount: reusedBlocks.count)
        return (reusedBlocks + tailBlocks, reusedCheckpoints + tailCheckpoints)
    }

    /// Mirrors `MarkdownBlocks.parse(_:)` while recording parser-clean boundaries after blank lines
    /// and closing fences so subsequent streaming frames skip earlier blocks.
    private static func parseWithCheckpoints(
        _ source: String,
        baseUtf8Offset: Int,
        baseBlockCount: Int
    ) -> ([MarkdownBlocks.Block], [Checkpoint]) {
        guard !source.isEmpty else { return ([], []) }
        var checkpoints: [Checkpoint] = []
        let blocks = MarkdownBlocks.parseBlocks(source) { utf8End, blockCount in
            checkpoints.append(Checkpoint(utf8End: baseUtf8Offset + utf8End, blockCount: baseBlockCount + blockCount))
        }
        return (blocks, checkpoints)
    }
}
