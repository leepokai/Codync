import CodyncKit
import SwiftUI

/// The iPhone's Markdown renderer for messages (`MarkdownBlocks`: tables, rules, nested lists
/// too); inline styling comes from `AttributedString(markdown:)`.
/// Equatable so a reply that didn't change skips its body.
public struct MarkdownText: View, Equatable {
    let source: String

    public init(_ source: String) {
        self.source = source
    }

    public var body: some View {
        let blocks = MarkdownBlocks.parse(source)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                MarkdownBlock(block: block)
            }
        }
        .textSelection(.enabled)
    }

    /// Underlines keep links distinct in the monochrome theme; inline code gets a soft tint.
    static func inline(_ s: String) -> AttributedString {
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
}

/// One block of a reply.
private struct MarkdownBlock: View {
    let block: MarkdownBlocks.Block

    @ViewBuilder var body: some View {
        switch block {
        case let .paragraph(t):
            Text(MarkdownText.inline(t))
                .font(.body)
                .foregroundStyle(Palette.text)
        case let .heading(t, level):
            Text(MarkdownText.inline(t))
                .font(level == 1 ? .title3.bold() : level == 2 ? .headline : .subheadline.bold())
                .foregroundStyle(Palette.text)
        case let .bullet(t, marker, depth):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(marker).foregroundStyle(Palette.secondary).monospacedDigit()
                Text(MarkdownText.inline(t)).foregroundStyle(Palette.text)
            }
            .font(.body)
            .padding(.leading, CGFloat(depth) * 16)
        case .rule:
            Rectangle().fill(Palette.border).frame(height: 1).padding(.vertical, 4)
        case let .table(header, rows):
            MarkdownTable(header: header, rows: rows)
        case let .quote(t):
            Text(MarkdownText.inline(t))
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
        }
    }
}

/// A Markdown table: a bold header and hairlines between rows, scrolling sideways when wide.
private struct MarkdownTable: View {
    let header: [String]
    let rows: [[String]]

    var body: some View {
        let columns = max(header.count, rows.map(\.count).max() ?? 0)
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 7) {
                GridRow {
                    ForEach(0 ..< columns, id: \.self) { i in
                        Text(MarkdownText.inline(i < header.count ? header[i] : "")).fontWeight(.semibold)
                    }
                }
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    Rectangle().fill(Palette.border).frame(height: 1).gridCellUnsizedAxes(.horizontal)
                    GridRow {
                        ForEach(0 ..< columns, id: \.self) { i in
                            Text(MarkdownText.inline(i < row.count ? row[i] : ""))
                        }
                    }
                }
            }
            .font(.body)
            .foregroundStyle(Palette.text)
            .fixedSize()
            .padding(.vertical, 2)
        }
    }
}
