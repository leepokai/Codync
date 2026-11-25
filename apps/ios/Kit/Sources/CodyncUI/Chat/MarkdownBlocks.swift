import Foundation

/// Block-level Markdown as replies use it: paragraphs, headings, nested lists, quotes, rules,
/// tables and fenced code. Pure parsing, apart from the views, so it's tested on its own.
/// `MarkdownText` renders these (the desktop app ports the same parser).
enum MarkdownBlocks {
    enum Block: Hashable, Sendable {
        case paragraph(String)
        case heading(String, level: Int)
        case bullet(String, marker: String, depth: Int = 0)
        case quote(String)
        case code(String, language: String)
        case table(header: [String], rows: [[String]])
        case rule
    }

    static func parse(_ source: String, isStreaming: Bool) -> [Block] {
        guard isStreaming else { return parse(source) }
        return parse(StreamingTailRepair.repair(source))
    }

    static func parse(_ source: String) -> [Block] {
        parseBlocks(source)
    }

    static func scanLines(_ source: String, body: (_ raw: String, _ hasNewline: Bool, _ utf8End: Int) -> Void) {
        guard !source.isEmpty else { return }
        let utf8 = source.utf8
        var cursor = utf8.startIndex
        var utf8Offset = 0

        while true {
            var lineEnd = cursor
            var lineBytes = 0
            while lineEnd < utf8.endIndex {
                let byte = utf8[lineEnd]
                if byte == 0x0A || byte == 0x0D { break }
                lineEnd = utf8.index(after: lineEnd)
                lineBytes += 1
            }

            let raw = String(decoding: utf8[cursor ..< lineEnd], as: UTF8.self)
            let hasNewline = lineEnd < utf8.endIndex
            let nextCursor: String.UTF8View.Index
            let newlineBytes: Int

            if hasNewline {
                let first = utf8[lineEnd]
                let afterFirst = utf8.index(after: lineEnd)
                if first == 0x0D, afterFirst < utf8.endIndex, utf8[afterFirst] == 0x0A {
                    nextCursor = utf8.index(after: afterFirst)
                    newlineBytes = 2
                } else {
                    nextCursor = afterFirst
                    newlineBytes = 1
                }
            } else {
                nextCursor = lineEnd
                newlineBytes = 0
            }

            utf8Offset += lineBytes + newlineBytes
            body(raw, hasNewline, utf8Offset)
            if !hasNewline { break }
            cursor = nextCursor
        }
    }

    static func parseBlocks(
        _ source: String,
        onCheckpoint: ((_ utf8End: Int, _ blockCount: Int) -> Void)? = nil
    ) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var table: [String] = []
        var code: [String]?
        var language = ""

        func flush() {
            if !paragraph.isEmpty {
                blocks.append(.paragraph(paragraph.joined(separator: "\n")))
                paragraph = []
            }
            if !table.isEmpty {
                // A header and its `|---|` line make a table; anything less stays text.
                if table.count >= 2, isTableRule(table[1]) {
                    blocks.append(.table(header: cells(table[0]), rows: table.dropFirst(2).map(cells)))
                } else {
                    blocks.append(.paragraph(table.joined(separator: "\n")))
                }
                table = []
            }
        }

        scanLines(source) { raw, hasNewline, utf8End in
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("```") {
                if let c = code {
                    blocks.append(.code(c.joined(separator: "\n"), language: language))
                    code = nil
                    if hasNewline {
                        onCheckpoint?(utf8End, blocks.count)
                    }
                } else {
                    flush()
                    code = []
                    language = String(line.dropFirst(3))
                }
                return
            }
            if code != nil {
                code?.append(raw)
                return
            }
            if line.hasPrefix("|") {
                if !paragraph.isEmpty { flush() }
                table.append(line)
                return
            }
            if !table.isEmpty { flush() }
            let depth = min(raw.prefix { $0 == " " || $0 == "\t" }.count / 2, 3)
            if line.isEmpty {
                flush()
                if hasNewline, !blocks.isEmpty {
                    onCheckpoint?(utf8End, blocks.count)
                }
            } else if line == "---" || line == "***" || line == "___" {
                flush()
                blocks.append(.rule)
            } else if let hashes = line.firstIndex(where: { $0 != "#" }), line.hasPrefix("#"), line[hashes] == " " {
                flush()
                let level = line.distance(from: line.startIndex, to: hashes)
                blocks.append(.heading(String(line[hashes...]).trimmingCharacters(in: .whitespaces), level: level))
            } else if line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("+ ") || line.hasPrefix("• ") {
                flush()
                blocks.append(.bullet(String(line.dropFirst(2)), marker: depth > 0 ? "◦" : "•", depth: depth))
            } else if let dot = line.firstIndex(of: "."), line[..<dot].allSatisfy(\.isNumber), !line[..<dot].isEmpty,
                      line[line.index(after: dot)...].hasPrefix(" ") {
                flush()
                blocks.append(.bullet(String(line[line.index(dot, offsetBy: 2)...]), marker: String(line[...dot]), depth: depth))
            } else if line.hasPrefix(">") {
                flush()
                blocks.append(.quote(String(line.dropFirst()).trimmingCharacters(in: .whitespaces)))
            } else {
                paragraph.append(raw)
            }
        }
        if let c = code { blocks.append(.code(c.joined(separator: "\n"), language: language)) }
        flush()
        return blocks
    }

    private static func isTableRule(_ line: String) -> Bool {
        line.contains("-") && line.allSatisfy { "|-: ".contains($0) }
    }

    private static func cells(_ line: String) -> [String] {
        var row = line
        if row.hasPrefix("|") { row.removeFirst() }
        if row.hasSuffix("|") { row.removeLast() }
        return row.split(separator: "|", omittingEmptySubsequences: false).map { $0.trimmingCharacters(in: .whitespaces) }
    }
}
