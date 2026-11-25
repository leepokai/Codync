import CodyncKit
import Foundation
import SwiftUI

/// Splits long plain-text blocks (such as live thinking traces) into ~1 KB newline-aligned pieces
/// so completed prefix chunks render statically while only the trailing piece animates.
enum ThoughtChunking {
    static let pieceLength = 1024

    static func pieces(of source: String, pieceLength: Int = pieceLength) -> [String] {
        guard !source.isEmpty else { return [] }
        let limit = max(64, pieceLength)
        guard source.utf8.count > limit else { return [source] }

        var result: [String] = []
        var pieceStart = source.startIndex
        var cursor = source.startIndex
        var bytesInPiece = 0

        while cursor < source.endIndex {
            let next = source.index(after: cursor)
            let ch = source[cursor]
            bytesInPiece += ch.utf8.count

            if bytesInPiece >= limit {
                if ch == "\n" || bytesInPiece >= limit * 2 {
                    result.append(String(source[pieceStart ..< next]))
                    pieceStart = next
                    cursor = next
                    bytesInPiece = 0
                    continue
                }
            }
            cursor = next
        }

        if pieceStart < source.endIndex {
            result.append(String(source[pieceStart ..< source.endIndex]))
        }
        return result
    }
}

/// Pure, deterministic state machine for trailing token fade-in sweeps during streaming text reveal.
/// Tracks character-level opacity batches across frames with bounded total delay (`0.36 s`) and
/// bounded active batch count (`32`) without mutating plain text or inserting invisible characters.
struct TailFadeSweepState: Equatable, Sendable {
    static let duration: TimeInterval = 0.715
    static let staggerInterval: TimeInterval = 0.028
    static let maxTotalDelay: TimeInterval = 0.36
    static let maxBatches = 32
    static let coarseGap: TimeInterval = 0.13
    static let maxWaveUnits = 12

    struct Batch: Equatable, Sendable {
        var startOffset: Int
        var endOffset: Int
        var startTime: TimeInterval
        var duration: TimeInterval

        var endTime: TimeInterval { startTime + duration }
    }

    struct Slice: Equatable, Sendable {
        var startOffset: Int
        var endOffset: Int
        var opacity: Double
    }

    private(set) var renderedText: String
    private(set) var totalCount: Int
    private(set) var activeOffset: Int
    private(set) var batches: [Batch]
    private(set) var isStreaming: Bool
    private(set) var isHidden: Bool
    private(set) var latestBatchStart: TimeInterval

    var isAnimating: Bool { !batches.isEmpty }
    var latestBatchEndTime: TimeInterval? { batches.map(\.endTime).max() }

    init(
        text: String = "",
        isStreaming: Bool = false,
        hidden: Bool = false,
        now: TimeInterval = 0,
        animateInitial: Bool = false
    ) {
        self.renderedText = ""
        self.totalCount = 0
        self.activeOffset = 0
        self.batches = []
        self.isStreaming = isStreaming
        self.isHidden = hidden
        self.latestBatchStart = now

        if animateInitial {
            sync(text: text, isStreaming: isStreaming, hidden: hidden, now: now)
        } else {
            self.renderedText = text
            self.totalCount = text.count
            self.activeOffset = self.totalCount
        }
    }

    mutating func converge() {
        batches.removeAll(keepingCapacity: false)
        activeOffset = totalCount
    }

    mutating func settle(at now: TimeInterval = 0, hidden: Bool = false) {
        isHidden = hidden
        isStreaming = false
        latestBatchStart = now
        converge()
    }

    mutating func prune(now: TimeInterval) {
        guard !batches.isEmpty else { return }
        var firstActive = 0
        while firstActive < batches.count, now >= batches[firstActive].endTime {
            activeOffset = max(activeOffset, batches[firstActive].endOffset)
            firstActive += 1
        }
        if firstActive > 0 {
            batches.removeFirst(firstActive)
        }
        if batches.isEmpty {
            activeOffset = totalCount
        }
    }

    @discardableResult
    mutating func pruneAndCheckActive(now: TimeInterval) -> Bool {
        prune(now: now)
        return isAnimating
    }

    mutating func sync(text nextText: String, isStreaming nextStreaming: Bool, hidden nextHidden: Bool = false, now: TimeInterval) {
        isStreaming = nextStreaming
        isHidden = nextHidden

        guard nextStreaming, !nextHidden else {
            renderedText = nextText
            totalCount = nextText.count
            latestBatchStart = now
            converge()
            return
        }

        prune(now: now)

        if nextText == renderedText {
            return
        }

        let common = Self.commonPrefixCount(renderedText, nextText)
        let nextTotal = nextText.count

        if common < activeOffset {
            activeOffset = common
        }
        if !batches.isEmpty {
            var trimmed: [Batch] = []
            trimmed.reserveCapacity(batches.count)
            for batch in batches where batch.startOffset < common {
                var copy = batch
                if copy.endOffset > common {
                    copy.endOffset = common
                }
                if copy.endOffset > copy.startOffset {
                    trimmed.append(copy)
                }
            }
            batches = trimmed
        }

        renderedText = nextText
        totalCount = nextTotal

        guard common < nextTotal else {
            if batches.isEmpty {
                activeOffset = totalCount
            }
            return
        }

        let startIndex = nextText.index(nextText.startIndex, offsetBy: common)
        let appended = String(nextText[startIndex...])
        let unitLengths = Self.waveUnitLengths(for: appended, maxUnits: Self.maxWaveUnits)
        guard !unitLengths.isEmpty else {
            if batches.isEmpty {
                activeOffset = totalCount
            }
            return
        }

        let baseStart: TimeInterval
        if batches.isEmpty {
            baseStart = now
        } else {
            baseStart = max(now, min(latestBatchStart + Self.staggerInterval, now + Self.maxTotalDelay))
        }

        let remainingDelayBudget = max(0, (now + Self.maxTotalDelay) - baseStart)
        let intervals = max(0, unitLengths.count - 1)
        let step: TimeInterval
        if intervals == 0 {
            step = 0
        } else {
            step = min(Self.staggerInterval, remainingDelayBudget / Double(intervals))
        }

        var cursorOffset = common
        for (i, length) in unitLengths.enumerated() where length > 0 {
            let startTime = baseStart + Double(i) * step
            let batch = Batch(
                startOffset: cursorOffset,
                endOffset: cursorOffset + length,
                startTime: startTime,
                duration: Self.duration
            )
            batches.append(batch)
            latestBatchStart = startTime
            cursorOffset += length
        }

        if batches.count > Self.maxBatches {
            let overflow = batches.count - Self.maxBatches
            for index in 0 ..< overflow {
                activeOffset = max(activeOffset, batches[index].endOffset)
            }
            batches.removeFirst(overflow)
        }
    }

    func opacity(forCharacterAt offset: Int, at now: TimeInterval) -> Double {
        if !isStreaming || isHidden || offset < activeOffset || offset >= totalCount {
            return 1
        }
        for batch in batches {
            if offset < batch.startOffset {
                return 1
            }
            if offset < batch.endOffset {
                let raw = (now - batch.startTime) / max(0.001, batch.duration)
                return min(1, max(0, raw))
            }
        }
        return 1
    }

    func slices(at now: TimeInterval) -> [Slice] {
        guard totalCount > 0 else { return [] }
        if !isStreaming || isHidden || batches.isEmpty {
            return [Slice(startOffset: 0, endOffset: totalCount, opacity: 1)]
        }

        var result: [Slice] = []
        result.reserveCapacity(batches.count + 2)
        var cursor = 0

        for batch in batches {
            let clampedStart = max(cursor, min(totalCount, batch.startOffset))
            let clampedEnd = max(clampedStart, min(totalCount, batch.endOffset))
            if clampedStart > cursor {
                result.append(Slice(startOffset: cursor, endOffset: clampedStart, opacity: 1))
            }
            if clampedEnd > clampedStart {
                let raw = (now - batch.startTime) / max(0.001, batch.duration)
                let alpha = min(1, max(0, raw))
                result.append(Slice(startOffset: clampedStart, endOffset: clampedEnd, opacity: alpha))
                cursor = clampedEnd
            }
        }

        if cursor < totalCount {
            result.append(Slice(startOffset: cursor, endOffset: totalCount, opacity: 1))
        }
        return result
    }

    /// Applies per-batch foreground/code-tint opacity directly onto an `AttributedString`
    /// without touching its characters, preserving inline links and verbatim text selection.
    static func applyFade(
        to attributed: inout AttributedString,
        state: TailFadeSweepState,
        at now: TimeInterval,
        baseColor: Color
    ) {
        guard state.isStreaming, !state.isHidden, !state.batches.isEmpty else { return }
        let charCount = attributed.characters.count
        guard charCount > 0 else { return }

        var offset = 0
        var charIndex = attributed.startIndex
        for slice in state.slices(at: now) {
            let clampedEnd = min(charCount, slice.endOffset)
            let clampedStart = min(clampedEnd, slice.startOffset)
            guard clampedEnd > clampedStart, slice.opacity < 0.999 else { continue }

            if offset < clampedStart {
                charIndex = attributed.characters.index(charIndex, offsetBy: clampedStart - offset)
                offset = clampedStart
            }
            let endCharIndex = attributed.characters.index(charIndex, offsetBy: clampedEnd - offset)
            let range = charIndex ..< endCharIndex

            let tintedText = baseColor.opacity(slice.opacity)
            let tintedCodeBg = Palette.text.opacity(0.08 * slice.opacity)
            let runs = Array(attributed[range].runs)
            if runs.isEmpty {
                attributed[range].foregroundColor = tintedText
            } else {
                for run in runs {
                    let subrange = run.range
                    attributed[subrange].foregroundColor = tintedText
                    if run.inlinePresentationIntent?.contains(.code) == true {
                        attributed[subrange].backgroundColor = tintedCodeBg
                    }
                }
            }

            charIndex = endCharIndex
            offset = clampedEnd
        }
    }

    /// Splits appended text into natural reveal units (words + attached whitespace, or single CJK glyphs)
    /// and coalesces excess units so one burst creates at most `maxUnits` staggered batches.
    private static func waveUnitLengths(for text: String, maxUnits: Int) -> [Int] {
        guard !text.isEmpty else { return [] }
        var lengths: [Int] = []
        var cursor = text.startIndex

        while cursor < text.endIndex {
            let unitStart = cursor
            let first = text[cursor]
            if first.isWhitespace {
                while cursor < text.endIndex, text[cursor].isWhitespace {
                    cursor = text.index(after: cursor)
                }
            } else if isWordCharacter(first) {
                while cursor < text.endIndex, isWordCharacter(text[cursor]) {
                    cursor = text.index(after: cursor)
                }
                while cursor < text.endIndex, text[cursor] == " " || text[cursor] == "\t" {
                    cursor = text.index(after: cursor)
                }
            } else {
                cursor = text.index(after: cursor)
                while cursor < text.endIndex, text[cursor] == " " || text[cursor] == "\t" {
                    cursor = text.index(after: cursor)
                }
            }
            lengths.append(text.distance(from: unitStart, to: cursor))
        }

        let cap = max(1, maxUnits)
        guard lengths.count > cap else { return lengths }

        var grouped: [Int] = []
        grouped.reserveCapacity(cap)
        let totalUnits = lengths.count
        for bucket in 0 ..< cap {
            let start = (bucket * totalUnits) / cap
            let end = ((bucket + 1) * totalUnits) / cap
            let sum = lengths[start ..< end].reduce(0, +)
            if sum > 0 {
                grouped.append(sum)
            }
        }
        return grouped
    }

    private static func isWordCharacter(_ character: Character) -> Bool {
        guard let scalar = character.unicodeScalars.first, character.unicodeScalars.count == 1 else {
            return false
        }
        return scalar.isASCII && (character.isLetter || character.isNumber || character == "_" || character == "-")
    }

    private static func commonPrefixCount(_ a: String, _ b: String) -> Int {
        var count = 0
        for (ca, cb) in zip(a, b) {
            if ca != cb { break }
            count += 1
        }
        return count
    }
}

/// Selects between chunked fade rendering during active streaming and a single unified `Text`
/// once finalized or faded so long thoughts support whole-text selection without artificial breaks.
enum ThoughtDisplaySegments: Equatable, Sendable {
    case unified(String)
    case chunked([String])

    static func build(text: String, pieces: [String]? = nil, isActivelyFading: Bool) -> Self {
        guard isActivelyFading else { return .unified(text) }
        let resolved = pieces ?? ThoughtChunking.pieces(of: text)
        guard resolved.count > 1 else { return .unified(text) }
        return .chunked(resolved)
    }
}

/// Manages per-block `TailFadeSweepState` instances across streaming renders of a Markdown message.
@MainActor
final class MarkdownFadeCoordinator {
    enum Key: Hashable {
        case block(Int)
        case tableCell(block: Int, row: Int, column: Int)
    }

    private var states: [Key: TailFadeSweepState] = [:]
    private var currentKey: String?

    var hasActiveAnimations: Bool {
        states.values.contains(where: \.isAnimating)
    }

    var latestEndTime: TimeInterval? {
        states.values.compactMap(\.latestBatchEndTime).max()
    }

    func prepare(forKey fadeKey: String?) {
        guard currentKey != fadeKey else { return }
        currentKey = fadeKey
        states.removeAll(keepingCapacity: true)
    }

    func settleAll(at now: TimeInterval = ProcessInfo.processInfo.systemUptime, hidden: Bool = true) {
        for key in Array(states.keys) {
            states[key]?.settle(at: now, hidden: hidden)
        }
    }

    func pruneAndCheckActive(at now: TimeInterval) -> Bool {
        var anyActive = false
        for key in Array(states.keys) {
            states[key]?.prune(now: now)
            if states[key]?.isAnimating == true {
                anyActive = true
            }
        }
        return anyActive
    }

    func sync(
        blocks: [RenderedMarkdownBlock],
        isStreaming: Bool,
        hidden: Bool,
        now: TimeInterval = ProcessInfo.processInfo.systemUptime
    ) {
        guard isStreaming, !hidden else {
            if !states.isEmpty {
                settleAll(at: now, hidden: hidden)
            }
            return
        }

        var activeKeys = Set<Key>()
        for (blockIndex, rendered) in blocks.enumerated() {
            let isTrailingBlock = blockIndex == blocks.count - 1
            switch rendered.content {
            case let .paragraph(_, plain), let .heading(_, plain, _), let .bullet(_, plain, _, _), let .quote(_, plain), let .code(plain, _):
                let key = Key.block(blockIndex)
                activeKeys.insert(key)
                if var existing = states[key] {
                    existing.sync(text: plain, isStreaming: true, hidden: false, now: now)
                    states[key] = existing
                } else {
                    states[key] = TailFadeSweepState(
                        text: plain,
                        isStreaming: true,
                        hidden: false,
                        now: now,
                        animateInitial: isTrailingBlock
                    )
                }
            case let .table(_, headerPlain, _, rowsPlain):
                for (col, text) in headerPlain.enumerated() {
                    let key = Key.tableCell(block: blockIndex, row: -1, column: col)
                    activeKeys.insert(key)
                    if var existing = states[key] {
                        existing.sync(text: text, isStreaming: true, hidden: false, now: now)
                        states[key] = existing
                    } else {
                        states[key] = TailFadeSweepState(
                            text: text,
                            isStreaming: true,
                            hidden: false,
                            now: now,
                            animateInitial: isTrailingBlock && rowsPlain.isEmpty
                        )
                    }
                }
                for (row, cells) in rowsPlain.enumerated() {
                    let isLastRow = row == rowsPlain.count - 1
                    for (col, text) in cells.enumerated() {
                        let key = Key.tableCell(block: blockIndex, row: row, column: col)
                        activeKeys.insert(key)
                        if var existing = states[key] {
                            existing.sync(text: text, isStreaming: true, hidden: false, now: now)
                            states[key] = existing
                        } else {
                            states[key] = TailFadeSweepState(
                                text: text,
                                isStreaming: true,
                                hidden: false,
                                now: now,
                                animateInitial: isTrailingBlock && isLastRow
                            )
                        }
                    }
                }
            case .rule:
                break
            }
        }
        states = states.filter { activeKeys.contains($0.key) }
    }

    func state(for key: Key) -> TailFadeSweepState? {
        states[key]
    }

    struct TableCellKey: Hashable, Equatable, Sendable {
        let row: Int
        let column: Int
    }

    func tableCellStates(forBlock block: Int) -> [TableCellKey: TailFadeSweepState] {
        var result: [TableCellKey: TailFadeSweepState] = [:]
        for (key, state) in states where state.isAnimating {
            if case let .tableCell(b, row, col) = key, b == block {
                result[TableCellKey(row: row, column: col)] = state
            }
        }
        return result
    }
}
