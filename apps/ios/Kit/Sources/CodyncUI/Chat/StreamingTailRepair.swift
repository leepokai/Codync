import Foundation

/// Repairs incomplete Markdown tokens at the trailing edge of a streaming message
/// so partial fences, tables, list markers, emphasis, and images render cleanly
/// without flashing raw syntax or reflowing earlier blocks.
enum StreamingTailRepair {
    struct Result: Equatable, Sendable {
        /// Number of leading UTF-8 bytes kept verbatim from the input.
        let keptLength: Int
        /// Temporary closing syntax appended after the kept prefix.
        let suffix: String

        func applied(to markdown: String) -> String {
            let utf8 = markdown.utf8
            guard keptLength < utf8.count || !suffix.isEmpty else { return markdown }
            let endIndex = utf8.index(utf8.startIndex, offsetBy: keptLength)
            return String(markdown[..<endIndex]) + suffix
        }
    }

    static func repair(_ markdown: String) -> String {
        guard !markdown.isEmpty else { return markdown }
        return compute(markdown).applied(to: markdown)
    }

    static func compute(_ markdown: String) -> Result {
        guard !markdown.isEmpty else { return Result(keptLength: 0, suffix: "") }
        var source = markdown
        let raw = source.withUTF8 { buffer -> Result in
            TailScanner(bytes: buffer).repair()
        }
        let clamped = clampToCharacterBoundary(in: markdown, utf8Count: raw.keptLength)
        return Result(keptLength: clamped, suffix: raw.suffix)
    }

    /// Snaps a UTF-8 byte offset down to a Swift `Character` (Extended Grapheme Cluster) boundary
    /// so multi-byte scalars and ZWJ emoji sequences are never split.
    private static func clampToCharacterBoundary(in markdown: String, utf8Count: Int) -> Int {
        let utf8 = markdown.utf8
        if utf8Count <= 0 { return 0 }
        if utf8Count >= utf8.count { return utf8.count }
        let utf8Index = utf8.index(utf8.startIndex, offsetBy: utf8Count)
        if let exact = String.Index(utf8Index, within: markdown) {
            let range = markdown.rangeOfComposedCharacterSequence(at: exact)
            if range.lowerBound == exact {
                return utf8.distance(from: utf8.startIndex, to: exact)
            }
            return utf8.distance(from: utf8.startIndex, to: range.lowerBound)
        }
        var cursor = utf8Index
        while cursor > utf8.startIndex {
            cursor = utf8.index(before: cursor)
            if let aligned = String.Index(cursor, within: markdown) {
                let range = markdown.rangeOfComposedCharacterSequence(at: aligned)
                return utf8.distance(from: utf8.startIndex, to: range.lowerBound)
            }
        }
        return 0
    }
}

private struct TailScanner {
    let bytes: UnsafeBufferPointer<UInt8>
    var count: Int { bytes.count }

    private var unchanged: StreamingTailRepair.Result {
        StreamingTailRepair.Result(keptLength: count, suffix: "")
    }

    func repair() -> StreamingTailRepair.Result {
        guard count > 0 else { return StreamingTailRepair.Result(keptLength: 0, suffix: "") }
        let blocks = scanBlocks()
        let endsWithNewline = bytes[count - 1] == .newline

        if let fence = blocks.openFence {
            return repairInsideFence(fence, lastLine: blocks.lastLine, endsWithNewline: endsWithNewline)
        }
        if blocks.doubleDollarCount % 2 == 1 {
            return unchanged
        }

        var tailLines = blocks.tailLines
        if tailLines.isEmpty || blocks.tailIsIndentedCode {
            return unchanged
        }

        if let table = repairTableHeader(tailLines, endsWithNewline: endsWithNewline) {
            return table
        }

        // A newline-terminated line is complete; dangling tokens on it are treated as literal.
        guard !endsWithNewline else { return unchanged }

        var keep = count
        if isMarkerOnly(tailLines[tailLines.count - 1]) {
            keep = tailLines[tailLines.count - 1].lowerBound
            tailLines.removeLast()
            if tailLines.isEmpty {
                return StreamingTailRepair.Result(keptLength: keep, suffix: "")
            }
        }

        var scopeStart = tailLines[0].lowerBound
        for line in tailLines.reversed() where startsBlock(line) {
            scopeStart = line.lowerBound
            break
        }

        var scopeEnd = keep
        while scopeEnd > scopeStart, bytes[scopeEnd - 1].isWhitespace {
            scopeEnd -= 1
        }
        // Keep trailing spaces after an unclosed inline code opener so the closing backtick preserves them.
        let inlineEnd = max(scopeEnd, keep)
        let lastLine = tailLines[tailLines.count - 1]
        if let inline = InlineScanner(
            bytes: bytes,
            scope: scopeStart ..< inlineEnd,
            lastLineStart: lastLine.lowerBound,
            isTableRow: isPipeRow(lastLine)
        ).repair() {
            return StreamingTailRepair.Result(keptLength: min(inline.keep, count), suffix: inline.suffix)
        }
        return StreamingTailRepair.Result(keptLength: keep, suffix: "")
    }

    // MARK: - Block scanning

    private struct Fence {
        let character: UInt8
        let length: Int
        let lineStart: Int
    }

    private struct BlockScan {
        var openFence: Fence?
        var doubleDollarCount = 0
        var tailLines: [Range<Int>] = []
        var tailIsIndentedCode = false
        var lastLine: Range<Int> = 0 ..< 0
    }

    private func scanBlocks() -> BlockScan {
        var scan = BlockScan()
        var inList = false
        var lineStart = 0
        while lineStart < count {
            var lineEnd = lineStart
            while lineEnd < count, bytes[lineEnd] != .newline {
                lineEnd += 1
            }
            let line = lineStart ..< lineEnd
            scan.lastLine = line
            lineStart = lineEnd + 1

            if let fence = scan.openFence {
                if isClosingFence(line, of: fence) {
                    scan.openFence = nil
                    scan.tailLines.removeAll(keepingCapacity: true)
                }
                continue
            }
            if isBlank(line) {
                scan.tailLines.removeAll(keepingCapacity: true)
                continue
            }
            let isListItem = startsListItem(line)
            if isListItem {
                inList = true
            } else if leadingColumns(line) < 2, scan.tailLines.isEmpty {
                inList = false
            }
            let content = contentStart(line)
            if inList || leadingColumns(line) < 4, let fence = openingFence(line, content: content) {
                scan.openFence = fence
                scan.tailLines.removeAll(keepingCapacity: true)
                continue
            }
            if scan.tailLines.isEmpty {
                scan.tailIsIndentedCode = !inList && leadingColumns(line) >= 4
            }
            scan.doubleDollarCount += doubleDollars(in: line)
            scan.tailLines.append(line)
        }
        return scan
    }

    private func repairInsideFence(
        _ fence: Fence,
        lastLine: Range<Int>,
        endsWithNewline: Bool
    ) -> StreamingTailRepair.Result {
        guard !endsWithNewline else { return unchanged }
        if fence.lineStart == lastLine.lowerBound {
            return StreamingTailRepair.Result(keptLength: lastLine.lowerBound, suffix: "")
        }
        let content = contentStart(lastLine)
        let run = runLength(of: fence.character, from: content, before: lastLine.upperBound)
        if run > 0, content + run == lastLine.upperBound {
            return StreamingTailRepair.Result(keptLength: lastLine.lowerBound, suffix: "")
        }
        return unchanged
    }

    private func openingFence(_ line: Range<Int>, content: Int) -> Fence? {
        guard content < line.upperBound else { return nil }
        let character = bytes[content]
        guard character == .backtick || character == .tilde else { return nil }
        let run = runLength(of: character, from: content, before: line.upperBound)
        guard run >= 3 else { return nil }
        if character == .backtick {
            for index in content + run ..< line.upperBound where bytes[index] == .backtick {
                return nil
            }
        }
        return Fence(character: character, length: run, lineStart: line.lowerBound)
    }

    private func isClosingFence(_ line: Range<Int>, of fence: Fence) -> Bool {
        let content = contentStart(line)
        let run = runLength(of: fence.character, from: content, before: line.upperBound)
        return run >= fence.length && isBlank(content + run ..< line.upperBound)
    }

    // MARK: - Rule B: Table headers

    private func repairTableHeader(
        _ tailLines: [Range<Int>],
        endsWithNewline: Bool
    ) -> StreamingTailRepair.Result? {
        if endsWithNewline {
            let headerIndex = tailLines.count - 1
            guard headerIndex >= 0 else { return nil }
            guard headerIndex == 0 || (headerIndex == 1 && isATXHeading(tailLines[0])) else { return nil }
            let header = tailLines[headerIndex]
            guard isPipeRow(header), !isDelimiterRow(header) else { return nil }
            let stats = pipeStats(header)
            return StreamingTailRepair.Result(keptLength: count, suffix: "|" + String(repeating: " --- |", count: stats.cells))
        }

        if tailLines.count >= 2 {
            let headerIndex = tailLines.count - 2
            if headerIndex == 0 || (headerIndex == 1 && isATXHeading(tailLines[0])) {
                let header = tailLines[headerIndex]
                let last = tailLines[tailLines.count - 1]
                if isPipeRow(header), !isDelimiterRow(header), isDelimiterRow(last) {
                    let stats = pipeStats(header)
                    return StreamingTailRepair.Result(
                        keptLength: last.lowerBound,
                        suffix: "|" + String(repeating: " --- |", count: stats.cells)
                    )
                }
            }
        }

        let singleIndex = tailLines.count - 1
        if singleIndex == 0 || (singleIndex == 1 && isATXHeading(tailLines[0])) {
            let header = tailLines[singleIndex]
            if isPipeRow(header), !isDelimiterRow(header) {
                let stats = pipeStats(header)
                if stats.hasTrailingPipe, stats.cells > 0 {
                    return StreamingTailRepair.Result(
                        keptLength: count,
                        suffix: "\n|" + String(repeating: " --- |", count: stats.cells)
                    )
                }
                if stats.pipes >= 2, stats.hasContentAfterLastPipe, stats.cells > 0 {
                    return StreamingTailRepair.Result(
                        keptLength: count,
                        suffix: " |\n|" + String(repeating: " --- |", count: stats.cells)
                    )
                }
            }
        }
        return nil
    }

    private func pipeStats(_ line: Range<Int>) -> (pipes: Int, cells: Int, hasTrailingPipe: Bool, hasContentAfterLastPipe: Bool) {
        var pipes = 0
        var lastPipe = -1
        var index = line.lowerBound
        while index < line.upperBound {
            if bytes[index] == .backslash {
                index += 2
                continue
            }
            if bytes[index] == .pipe {
                pipes += 1
                lastPipe = index
            }
            index += 1
        }
        var end = line.upperBound
        while end > line.lowerBound, bytes[end - 1].isSpaceOrTab || bytes[end - 1] == .carriageReturn {
            end -= 1
        }
        let hasTrailingPipe = lastPipe != -1 && end == lastPipe + 1
        let hasContentAfterLastPipe = lastPipe != -1 && end > lastPipe + 1
        let cells = hasTrailingPipe ? max(0, pipes - 1) : pipes
        return (pipes, cells, hasTrailingPipe, hasContentAfterLastPipe)
    }

    private func isPipeRow(_ line: Range<Int>) -> Bool {
        var index = line.lowerBound
        while index < line.upperBound, bytes[index] == .space {
            index += 1
        }
        return index < line.upperBound && bytes[index] == .pipe && pipeStats(line).cells > 0
    }

    private func isDelimiterRow(_ line: Range<Int>) -> Bool {
        var sawDash = false
        var sawPipe = false
        for index in line {
            switch bytes[index] {
            case .dash: sawDash = true
            case .pipe: sawPipe = true
            case .colon, .space, .tab, .carriageReturn: continue
            default: return false
            }
        }
        return sawDash || sawPipe
    }

    // MARK: - Line classification

    private func isBlank(_ range: Range<Int>) -> Bool {
        bytes[range].allSatisfy { $0.isSpaceOrTab || $0 == .carriageReturn }
    }

    private func leadingColumns(_ line: Range<Int>) -> Int {
        var columns = 0
        for index in line {
            switch bytes[index] {
            case .space: columns += 1
            case .tab: columns += 4 - columns % 4
            default: return columns
            }
        }
        return columns
    }

    private func leadingSpaces(_ line: Range<Int>) -> Int {
        var index = line.lowerBound
        while index < line.upperBound, bytes[index].isSpaceOrTab {
            index += 1
        }
        return index - line.lowerBound
    }

    private func contentStart(_ line: Range<Int>) -> Int {
        var index = line.lowerBound
        while index < line.upperBound {
            let byte = bytes[index]
            if byte.isSpaceOrTab || byte == .greaterThan {
                index += 1
                continue
            }
            if let next = listMarkerEnd(at: index, in: line) {
                index = next
                continue
            }
            break
        }
        return index
    }

    private func startsListItem(_ line: Range<Int>) -> Bool {
        var index = line.lowerBound
        while index < line.upperBound, bytes[index].isSpaceOrTab || bytes[index] == .greaterThan {
            index += 1
        }
        return index < line.upperBound && listMarkerEnd(at: index, in: line) != nil
    }

    private func listMarkerEnd(at index: Int, in line: Range<Int>) -> Int? {
        let byte = bytes[index]
        if byte == .dash || byte == .plus || byte == .asterisk {
            guard index + 1 < line.upperBound, bytes[index + 1].isSpaceOrTab else { return nil }
            return index + 2
        }
        guard byte.isDigit else { return nil }
        var end = index
        while end < line.upperBound, bytes[end].isDigit, end - index < 9 {
            end += 1
        }
        guard end + 1 < line.upperBound,
              bytes[end] == .period || bytes[end] == .closeParen,
              bytes[end + 1].isSpaceOrTab
        else { return nil }
        return end + 2
    }

    private func isATXHeading(_ line: Range<Int>) -> Bool {
        var index = line.lowerBound
        while index < line.upperBound, bytes[index] == .space {
            index += 1
        }
        let run = runLength(of: .hash, from: index, before: line.upperBound)
        guard (1 ... 6).contains(run) else { return false }
        return index + run == line.upperBound || bytes[index + run].isSpaceOrTab || bytes[index + run] == .carriageReturn
    }

    private func startsBlock(_ line: Range<Int>) -> Bool {
        if contentStart(line) != line.lowerBound + leadingSpaces(line) {
            return true
        }
        return isATXHeading(line) || isPipeRow(line)
    }

    private func isMarkerOnly(_ line: Range<Int>) -> Bool {
        guard !isBlank(line) else { return false }
        var index = line.lowerBound
        var sawMarker = false
        let lead = line.lowerBound + leadingSpaces(line)
        while index < line.upperBound {
            let byte = bytes[index]
            if byte.isSpaceOrTab || byte == .carriageReturn {
                index += 1
                continue
            }
            if Self.markerBytes.contains(byte) {
                sawMarker = true
                index += 1
                continue
            }
            if byte == .lowercaseX || byte == .uppercaseX, index > line.lowerBound, bytes[index - 1] == .openBracket {
                index += 1
                continue
            }
            if byte.isDigit {
                var end = index
                while end < line.upperBound, bytes[end].isDigit {
                    end += 1
                }
                guard end - index <= 9 else { return false }
                if end == line.upperBound {
                    guard index == lead else { return false }
                } else if bytes[end] != .period, bytes[end] != .closeParen {
                    return false
                }
                sawMarker = true
                index = min(end + 1, line.upperBound)
                continue
            }
            return false
        }
        return sawMarker
    }

    private static let markerBytes: Set<UInt8> = Set("#-=*+_>|`~:[]".utf8)

    private func doubleDollars(in line: Range<Int>) -> Int {
        var found = 0
        var index = line.lowerBound
        while index + 1 < line.upperBound {
            if bytes[index] == .backslash {
                index += 2
            } else if bytes[index] == .dollar, bytes[index + 1] == .dollar {
                found += 1
                index += 2
            } else {
                index += 1
            }
        }
        return found
    }

    private func runLength(of byte: UInt8, from start: Int, before end: Int) -> Int {
        var index = start
        while index < end, bytes[index] == byte {
            index += 1
        }
        return index - start
    }
}

// MARK: - Inline scanner

private struct InlineScanner {
    let bytes: UnsafeBufferPointer<UInt8>
    let scope: Range<Int>
    let lastLineStart: Int
    let isTableRow: Bool

    fileprivate enum Opener {
        case delimiter(character: UInt8, count: Int, position: Int)
        case bracket(position: Int, isImage: Bool)
    }

    func repair() -> (keep: Int, suffix: String)? {
        var trimmedEnd = scope.upperBound
        while trimmedEnd > scope.lowerBound, bytes[trimmedEnd - 1].isSpaceOrTab || bytes[trimmedEnd - 1] == .carriageReturn {
            trimmedEnd -= 1
        }
        var stack: [Opener] = []
        var keep = trimmedEnd
        var closer = ""
        var heldImage = false
        var index = scope.lowerBound

        scan: while index < trimmedEnd {
            let byte = bytes[index]
            switch byte {
            case .pipe where isTableRow:
                stack.removeAll()
                index += 1

            case .backslash:
                guard index + 1 < trimmedEnd else {
                    keep = index
                    break scan
                }
                var next = index + 1
                if bytes[next] == .backslash, next + 1 < trimmedEnd,
                   bytes[next + 1] == .openParen || bytes[next + 1] == .openBracket
                {
                    next += 1
                }
                let opened = bytes[next]
                if opened == .openParen || opened == .openBracket {
                    let closing: UInt8 = opened == .openParen ? .closeParen : .closeBracket
                    guard let close = find(.backslash, then: closing, from: next + 1, before: trimmedEnd) else {
                        break scan
                    }
                    index = close + 2
                    continue
                }
                index += 2

            case .dollar:
                if index + 1 < trimmedEnd, bytes[index + 1] == .dollar {
                    guard let close = find(.dollar, then: .dollar, from: index + 2, before: trimmedEnd) else {
                        break scan
                    }
                    index = close + 2
                    continue
                }
                guard opensInlineMath(at: index, end: trimmedEnd) else {
                    index += 1
                    continue
                }
                guard let close = inlineMathClose(after: index, end: trimmedEnd) else {
                    break scan
                }
                index = close + 1

            case .backtick:
                let run = runLength(of: byte, from: index, before: scope.upperBound)
                if let close = findRun(of: byte, length: run, from: index + run, before: scope.upperBound) {
                    index = close + run
                    continue
                }
                if index + run == trimmedEnd, index >= lastLineStart {
                    keep = index
                    break scan
                }
                if index >= lastLineStart, index + run < scope.upperBound, !bytes[index + run].isWhitespace {
                    keep = scope.upperBound
                    closer = String(repeating: "`", count: run)
                    break scan
                }
                index += run

            case .asterisk, .underscore, .tilde:
                let run = runLength(of: byte, from: index, before: trimmedEnd)
                let start = index
                index += run
                let before = characterClass(before: start)
                guard index < trimmedEnd else {
                    if before != .whitespace, closeDelimiter(byte, run: run, in: &stack) {
                        continue
                    }
                    // Conservative guard: leave a lone trailing `*`, `_`, or `~` after whitespace verbatim (e.g. `x = 3 *`).
                    if before == .whitespace, run == 1 {
                        continue
                    }
                    if before != .other, start >= lastLineStart {
                        keep = start
                        break scan
                    }
                    continue
                }
                if byte == .tilde, run != 2 {
                    continue
                }
                let after = characterClass(at: index)
                let flanking = Flanking(before: before, after: after)
                var canOpen = flanking.isLeft
                var canClose = flanking.isRight
                if byte == .underscore {
                    canOpen = flanking.isLeft && (!flanking.isRight || before == .punctuation)
                    canClose = flanking.isRight && (!flanking.isLeft || after == .punctuation)
                }
                if canClose, closeDelimiter(byte, run: run, in: &stack) {
                    continue
                }
                guard canOpen, start >= lastLineStart, speculates(after: index, run: start ..< index) else {
                    continue
                }
                stack.append(.delimiter(character: byte, count: min(run, 3), position: start))

            case .exclamation where index + 1 < trimmedEnd && bytes[index + 1] == .openBracket:
                stack.append(.bracket(position: index, isImage: true))
                index += 2

            case .openBracket:
                stack.append(.bracket(position: index, isImage: false))
                index += 1

            case .closeBracket:
                guard let opener = stack.lastIndex(where: \.isBracket),
                      case let .bracket(start, isImage) = stack[opener]
                else {
                    index += 1
                    continue
                }
                if isImage, index + 1 == trimmedEnd, start >= lastLineStart {
                    keep = start
                    heldImage = true
                    break scan
                }
                guard index + 1 < trimmedEnd, bytes[index + 1] == .openParen else {
                    stack.remove(at: opener)
                    index += 1
                    continue
                }
                stack.removeSubrange(opener ..< stack.endIndex)
                if let close = destinationEnd(from: index + 1, before: trimmedEnd) {
                    index = close + 1
                    continue
                }
                if isImage, start >= lastLineStart {
                    keep = start
                    heldImage = true
                    break scan
                }
                closer = destinationCloser(from: index + 1, before: trimmedEnd)
                break scan

            case .lessThan:
                if heldBackTag(at: index, end: trimmedEnd) {
                    keep = index
                    break scan
                }
                index += 1

            case .ampersand:
                if isPartialEntity(at: index, end: trimmedEnd) {
                    keep = index
                    break scan
                }
                index += 1

            default:
                index += 1
            }
        }

        let live = stack.filter { opener in
            switch opener {
            case let .delimiter(_, _, position): position < keep && position >= lastLineStart
            case let .bracket(position, isImage): isImage && position < keep && position >= lastLineStart
            }
        }

        var outKeep = keep
        var outSuffix = closer
        for opener in live.reversed() {
            switch opener {
            case let .delimiter(character, count, position):
                var contentEnd = outKeep
                while contentEnd > position + count, bytes[contentEnd - 1].isSpaceOrTab {
                    contentEnd -= 1
                }
                if outSuffix.isEmpty {
                    if contentEnd <= position + count {
                        outKeep = position
                        continue
                    }
                    outKeep = contentEnd
                }
                outSuffix += String(repeating: String(UnicodeScalar(character)), count: count)
            case let .bracket(position, _):
                outKeep = position
                outSuffix = ""
                heldImage = true
            }
        }

        if outSuffix.isEmpty, !heldImage {
            while outKeep > scope.lowerBound, bytes[outKeep - 1].isSpaceOrTab {
                outKeep -= 1
            }
        }

        if outKeep >= scope.upperBound, outSuffix.isEmpty { return nil }
        if outSuffix.isEmpty, outKeep == trimmedEnd { return nil }
        return (outKeep, outSuffix)
    }

    private func closeDelimiter(_ byte: UInt8, run: Int, in stack: inout [Opener]) -> Bool {
        var remaining = run
        while remaining > 0 {
            guard let openerIndex = stack.lastIndex(where: { $0.isDelimiter(byte) }),
                  case let .delimiter(_, count, position) = stack[openerIndex]
            else {
                return remaining < run
            }
            if count <= remaining {
                remaining -= count
                stack.remove(at: openerIndex)
            } else {
                stack[openerIndex] = .delimiter(character: byte, count: count - remaining, position: position)
                remaining = 0
            }
        }
        return true
    }

    private func speculates(after index: Int, run: Range<Int>) -> Bool {
        let next = bytes[index]
        if next.isASCIIAlphanumeric { return true }
        if next == .backtick || next == .openBracket || next == .openParen || next == .quote || next == .apostrophe {
            return true
        }
        if next >= 0x80, characterClass(at: index) == .other {
            return true
        }
        let opener = bytes[run.lowerBound]
        var cursor = index
        while cursor < scope.upperBound, bytes[cursor] == opener {
            cursor += 1
        }
        return cursor < scope.upperBound && !bytes[cursor].isWhitespace
    }

    private func runLength(of byte: UInt8, from start: Int, before end: Int) -> Int {
        var index = start
        while index < end, bytes[index] == byte {
            index += 1
        }
        return index - start
    }

    private func findRun(of byte: UInt8, length: Int, from start: Int, before end: Int) -> Int? {
        var index = start
        while index < end {
            guard bytes[index] == byte else {
                index += 1
                continue
            }
            let run = runLength(of: byte, from: index, before: end)
            if run == length { return index }
            index += run
        }
        return nil
    }

    private func find(_ first: UInt8, then second: UInt8, from start: Int, before end: Int) -> Int? {
        var index = start
        while index + 1 < end {
            if bytes[index] == .backslash, first != .backslash {
                index += 2
                continue
            }
            if bytes[index] == first, bytes[index + 1] == second {
                return index
            }
            index += 1
        }
        return nil
    }

    private func opensInlineMath(at index: Int, end: Int) -> Bool {
        guard index + 1 < end else { return false }
        let next = bytes[index + 1]
        if next.isDigit || next.isWhitespace { return false }
        return index == scope.lowerBound || !bytes[index - 1].isDigit
    }

    private func inlineMathClose(after open: Int, end: Int) -> Int? {
        var index = open + 2
        while index < end {
            let byte = bytes[index]
            if byte == .newline { return nil }
            if byte == .dollar, !bytes[index - 1].isWhitespace,
               index + 1 == end || !bytes[index + 1].isDigit
            {
                return index
            }
            index += 1
        }
        return nil
    }

    private func destinationEnd(from open: Int, before end: Int) -> Int? {
        var depth = 0
        var index = open
        while index < end {
            switch bytes[index] {
            case .backslash: index += 1
            case .openParen: depth += 1
            case .closeParen:
                depth -= 1
                if depth == 0 { return index }
            default: break
            }
            index += 1
        }
        return nil
    }

    private func destinationCloser(from open: Int, before end: Int) -> String {
        var depth = 0
        var quotes = 0
        var angled = open + 1 < end && bytes[open + 1] == .lessThan
        var index = open
        while index < end {
            switch bytes[index] {
            case .backslash: index += 1
            case .openParen where quotes % 2 == 0: depth += 1
            case .closeParen where quotes % 2 == 0: depth -= 1
            case .quote: quotes += 1
            case .greaterThan: angled = false
            default: break
            }
            index += 1
        }
        var closer = ""
        if quotes % 2 == 1 { closer += "\"" }
        if angled { closer += ">" }
        return closer + String(repeating: ")", count: max(depth, 1))
    }

    private func heldBackTag(at index: Int, end: Int) -> Bool {
        guard index >= lastLineStart else { return false }
        guard index + 1 < end else { return true }
        var cursor = index + 1
        if bytes[cursor] == .slash { cursor += 1 }
        let nameStart = cursor
        while cursor < end, bytes[cursor].isASCIIAlphanumeric || bytes[cursor] == .dash {
            cursor += 1
        }
        guard cursor > nameStart, bytes[nameStart].isASCIILetter else { return false }
        if cursor == end { return true }
        var rest = cursor
        var sawEquals = false
        var sawSpace = false
        while rest < end {
            switch bytes[rest] {
            case .greaterThan: return false
            case .equals: sawEquals = true
            case .space, .tab: sawSpace = true
            default: break
            }
            rest += 1
        }
        if bytes[cursor] == .colon {
            return !sawSpace
        }
        return sawEquals && bytes[cursor].isSpaceOrTab
    }

    private func isPartialEntity(at index: Int, end: Int) -> Bool {
        guard index >= lastLineStart, index + 1 < end else { return false }
        var cursor = index + 1
        if bytes[cursor] == .hash {
            cursor += 1
            guard cursor < end else { return false }
            if bytes[cursor] == .lowercaseX || bytes[cursor] == .uppercaseX {
                cursor += 1
                let start = cursor
                while cursor < end, bytes[cursor].isASCIIHexDigit {
                    cursor += 1
                }
                return cursor == end && cursor > start
            }
            let start = cursor
            while cursor < end, bytes[cursor].isDigit {
                cursor += 1
            }
            return cursor == end && cursor > start
        }
        let start = cursor
        while cursor < end, bytes[cursor].isASCIIAlphanumeric {
            cursor += 1
        }
        return cursor == end && cursor > start && bytes[start].isASCIILetter
    }

    // MARK: - Character classes

    private enum CharacterClass {
        case whitespace
        case punctuation
        case other
    }

    private struct Flanking {
        let isLeft: Bool
        let isRight: Bool

        init(before: CharacterClass, after: CharacterClass) {
            isLeft = after != .whitespace && (after != .punctuation || before != .other)
            isRight = before != .whitespace && (before != .punctuation || after != .other)
        }
    }

    private func characterClass(before index: Int) -> CharacterClass {
        guard index > scope.lowerBound else { return .whitespace }
        var start = index - 1
        while start > scope.lowerBound, bytes[start] & 0xC0 == 0x80 {
            start -= 1
        }
        return characterClass(at: start)
    }

    private func characterClass(at index: Int) -> CharacterClass {
        let byte = bytes[index]
        if byte < 0x80 {
            if byte.isWhitespace { return .whitespace }
            return byte.isASCIIAlphanumeric ? .other : .punctuation
        }
        guard let scalar = scalar(at: index) else { return .other }
        if scalar.properties.isWhitespace { return .whitespace }
        switch scalar.properties.generalCategory {
        case .connectorPunctuation, .dashPunctuation, .openPunctuation, .closePunctuation,
             .initialPunctuation, .finalPunctuation, .otherPunctuation:
            return .punctuation
        default:
            return .other
        }
    }

    private func scalar(at index: Int) -> Unicode.Scalar? {
        let lead = bytes[index]
        let length = lead >= 0xF0 ? 4 : lead >= 0xE0 ? 3 : lead >= 0xC0 ? 2 : 1
        guard index + length <= bytes.count else { return nil }
        var iterator = bytes[index ..< index + length].makeIterator()
        var decoder = UTF8()
        guard case let .scalarValue(scalar) = decoder.decode(&iterator) else { return nil }
        return scalar
    }
}

private extension InlineScanner.Opener {
    var isBracket: Bool {
        if case .bracket = self { true } else { false }
    }

    func isDelimiter(_ byte: UInt8) -> Bool {
        if case let .delimiter(character, _, _) = self { character == byte } else { false }
    }
}

private extension UInt8 {
    static let tab = UInt8(ascii: "\t")
    static let newline = UInt8(ascii: "\n")
    static let carriageReturn = UInt8(ascii: "\r")
    static let space = UInt8(ascii: " ")
    static let exclamation = UInt8(ascii: "!")
    static let quote = UInt8(ascii: "\"")
    static let apostrophe = UInt8(ascii: "'")
    static let hash = UInt8(ascii: "#")
    static let dollar = UInt8(ascii: "$")
    static let ampersand = UInt8(ascii: "&")
    static let openParen = UInt8(ascii: "(")
    static let closeParen = UInt8(ascii: ")")
    static let asterisk = UInt8(ascii: "*")
    static let plus = UInt8(ascii: "+")
    static let dash = UInt8(ascii: "-")
    static let period = UInt8(ascii: ".")
    static let slash = UInt8(ascii: "/")
    static let colon = UInt8(ascii: ":")
    static let lessThan = UInt8(ascii: "<")
    static let equals = UInt8(ascii: "=")
    static let greaterThan = UInt8(ascii: ">")
    static let openBracket = UInt8(ascii: "[")
    static let backslash = UInt8(ascii: "\\")
    static let closeBracket = UInt8(ascii: "]")
    static let underscore = UInt8(ascii: "_")
    static let backtick = UInt8(ascii: "`")
    static let pipe = UInt8(ascii: "|")
    static let tilde = UInt8(ascii: "~")
    static let lowercaseX = UInt8(ascii: "x")
    static let uppercaseX = UInt8(ascii: "X")

    var isSpaceOrTab: Bool { self == .space || self == .tab }
    var isWhitespace: Bool { isSpaceOrTab || self == .newline || self == .carriageReturn }
    var isDigit: Bool { self >= UInt8(ascii: "0") && self <= UInt8(ascii: "9") }
    var isASCIILetter: Bool { (self | 0x20) >= UInt8(ascii: "a") && (self | 0x20) <= UInt8(ascii: "z") }
    var isASCIIAlphanumeric: Bool { isDigit || isASCIILetter }
    var isASCIIHexDigit: Bool {
        isDigit || ((self | 0x20) >= UInt8(ascii: "a") && (self | 0x20) <= UInt8(ascii: "f"))
    }
}
