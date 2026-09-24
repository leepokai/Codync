import CodyncKit
import SwiftUI

/// Small block-level Markdown renderer: paragraphs, headings, lists, quotes and
/// fenced code; inline styling comes from `AttributedString(markdown:)`.
public struct MarkdownText: View {
    let blocks: [Block]

    public init(_ source: String) {
        blocks = Self.parse(source)
    }

    enum Block: Hashable {
        case paragraph(String)
        case heading(String, level: Int)
        case bullet(String, marker: String)
        case quote(String)
        case code(String, language: String)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .textSelection(.enabled)
    }

    @ViewBuilder private func view(for block: Block) -> some View {
        switch block {
        case let .paragraph(t):
            Text(Self.inline(t)).font(.body).foregroundStyle(Palette.text)
        case let .heading(t, level):
            Text(Self.inline(t))
                .font(level == 1 ? .title3.bold() : level == 2 ? .headline : .subheadline.bold())
                .foregroundStyle(Palette.text)
        case let .bullet(t, marker):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(marker).foregroundStyle(Palette.secondary).monospacedDigit()
                Text(Self.inline(t)).foregroundStyle(Palette.text)
            }
            .font(.body)
        case let .quote(t):
            Text(Self.inline(t))
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .padding(.leading, 10)
                .overlay(alignment: .leading) { Rectangle().fill(Palette.border).frame(width: 3) }
        case let .code(t, _):
            ScrollView(.horizontal, showsIndicators: false) {
                Text(t)
                    .font(.system(.footnote, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .padding(10)
            }
            .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.border))
        }
    }

    static func inline(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(s)
    }

    static func parse(_ source: String) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var code: [String]?
        var language = ""

        func flush() {
            if !paragraph.isEmpty {
                blocks.append(.paragraph(paragraph.joined(separator: "\n")))
                paragraph = []
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
            if line.isEmpty {
                flush()
            } else if let hashes = line.firstIndex(where: { $0 != "#" }), line.hasPrefix("#"), line[hashes] == " " {
                flush()
                let level = line.distance(from: line.startIndex, to: hashes)
                blocks.append(.heading(String(line[hashes...]).trimmingCharacters(in: .whitespaces), level: level))
            } else if line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("• ") {
                flush()
                blocks.append(.bullet(String(line.dropFirst(2)), marker: "•"))
            } else if let dot = line.firstIndex(of: "."), line[..<dot].allSatisfy(\.isNumber), !line[..<dot].isEmpty,
                      line[line.index(after: dot)...].hasPrefix(" ") {
                flush()
                blocks.append(.bullet(String(line[line.index(dot, offsetBy: 2)...]), marker: String(line[...dot])))
            } else if line.hasPrefix("> ") {
                flush()
                blocks.append(.quote(String(line.dropFirst(2))))
            } else {
                paragraph.append(raw)
            }
        }
        if let c = code { blocks.append(.code(c.joined(separator: "\n"), language: language)) }
        flush()
        return blocks
    }
}
