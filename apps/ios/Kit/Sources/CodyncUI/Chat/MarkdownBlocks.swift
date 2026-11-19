import Foundation

/// Block-level Markdown as replies use it: paragraphs, headings, nested lists, quotes, rules,
/// tables and fenced code. Pure parsing, apart from the views, so it's tested on its own.
/// `MarkdownText` renders these (the desktop app ports the same parser).
enum MarkdownBlocks {
    enum Block: Hashable {
        case paragraph(String)
        case heading(String, level: Int)
        case bullet(String, marker: String, depth: Int = 0)
        case quote(String)
        case code(String, language: String)
        case table(header: [String], rows: [[String]])
        case rule
    }

    static func parse(_ source: String) -> [Block] {
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

        for raw in source.components(separatedBy: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("```") {
                if let c = code {
                    blocks.append(.code(c.joined(separator: "\n"), language: language))
                    code = nil
                } else {
                    flush()
                    code = []
                    language = String(line.dropFirst(3))
                }
                continue
            }
            if code != nil {
                code?.append(raw)
                continue
            }
            if line.hasPrefix("|") {
                if !paragraph.isEmpty { flush() }
                table.append(line)
                continue
            }
            if !table.isEmpty { flush() }
            let depth = min(raw.prefix { $0 == " " || $0 == "\t" }.count / 2, 3)
            if line.isEmpty {
                flush()
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
