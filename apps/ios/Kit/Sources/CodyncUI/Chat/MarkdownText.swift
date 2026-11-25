import CodyncKit
import SwiftUI

/// The iPhone's Markdown renderer for messages (`MarkdownBlocks`: tables, rules, nested lists
/// too); inline styling comes from `AttributedString(markdown:)`.
/// Equatable so a reply that didn't change skips its body, and incremental block caching skips
/// rebuilding unchanged prefix blocks while streaming.
public struct MarkdownText: View, Equatable {
    let source: String
    let isStreaming: Bool
    let fadeKey: String?

    @State private var blockCache = MarkdownBlockCache()
    @State private var fadeCoordinator = MarkdownFadeCoordinator()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase

    public init(_ source: String) {
        self.source = source
        self.isStreaming = false
        self.fadeKey = nil
    }

    init(_ source: String, isStreaming: Bool, fadeKey: String? = nil) {
        self.source = source
        self.isStreaming = isStreaming
        self.fadeKey = fadeKey
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.source == rhs.source && lhs.isStreaming == rhs.isStreaming && lhs.fadeKey == rhs.fadeKey
    }

    public var body: some View {
        let shouldAnimate = isStreaming && !reduceMotion && scenePhase == .active
        let blocks = blockCache.render(source, isStreaming: isStreaming)
        let now = ProcessInfo.processInfo.systemUptime
        let _ = fadeCoordinator.prepare(forKey: fadeKey)
        let _ = fadeCoordinator.sync(blocks: blocks, isStreaming: isStreaming, hidden: !shouldAnimate, now: now)

        Group {
            if shouldAnimate, fadeCoordinator.hasActiveAnimations {
                TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: false)) { _ in
                    let frameNow = ProcessInfo.processInfo.systemUptime
                    let _ = fadeCoordinator.pruneAndCheckActive(at: frameNow)
                    blocksStack(blocks, now: frameNow, animate: true)
                }
            } else {
                blocksStack(blocks, now: now, animate: false)
            }
        }
        .textSelection(.enabled)
        .onChange(of: shouldAnimate) { _, active in
            if !active {
                fadeCoordinator.settleAll(hidden: true)
            }
        }
        .onDisappear {
            fadeCoordinator.settleAll(hidden: true)
        }
    }

    @ViewBuilder
    private func blocksStack(_ blocks: [RenderedMarkdownBlock], now: TimeInterval, animate: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, rendered in
                let blockFade = animate ? activeState(fadeCoordinator.state(for: .block(index))) : nil
                let tableFade = animate ? fadeCoordinator.tableCellStates(forBlock: index) : [:]
                MarkdownBlockView(rendered: rendered, fadeState: blockFade, tableFadeStates: tableFade, now: now)
                    .equatable()
            }
        }
    }

    private func activeState(_ state: TailFadeSweepState?) -> TailFadeSweepState? {
        guard let state, state.isAnimating else { return nil }
        return state
    }

    /// Underlines keep links distinct in the monochrome theme; inline code gets a soft tint.
    static func inline(_ s: String) -> AttributedString {
        MarkdownInlineCache.shared.attributedString(for: s)
    }
}

/// One block of a reply. Equatable so completed prefix blocks skip body evaluation while the tail streams.
private struct MarkdownBlockView: View, Equatable {
    let rendered: RenderedMarkdownBlock
    let fadeState: TailFadeSweepState?
    let tableFadeStates: [MarkdownFadeCoordinator.TableCellKey: TailFadeSweepState]
    let now: TimeInterval

    private var isAnimating: Bool {
        fadeState?.isAnimating == true || !tableFadeStates.isEmpty
    }

    static func == (lhs: Self, rhs: Self) -> Bool {
        guard lhs.rendered == rhs.rendered,
              lhs.fadeState == rhs.fadeState,
              lhs.tableFadeStates == rhs.tableFadeStates else {
            return false
        }
        return lhs.isAnimating ? lhs.now == rhs.now : true
    }

    @ViewBuilder var body: some View {
        switch rendered.content {
        case let .paragraph(attr, _):
            Text(faded(attr, baseColor: Palette.text))
                .font(.body)
                .foregroundStyle(Palette.text)
        case let .heading(attr, _, level):
            Text(faded(attr, baseColor: Palette.text))
                .font(level == 1 ? .title3.bold() : level == 2 ? .headline : .subheadline.bold())
                .foregroundStyle(Palette.text)
        case let .bullet(attr, _, marker, depth):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(marker).foregroundStyle(Palette.secondary).monospacedDigit()
                Text(faded(attr, baseColor: Palette.text)).foregroundStyle(Palette.text)
            }
            .font(.body)
            .padding(.leading, CGFloat(depth) * 16)
        case .rule:
            Rectangle().fill(Palette.border).frame(height: 1).padding(.vertical, 4)
        case let .table(header, _, rows, _):
            MarkdownTable(header: header, rows: rows, tableFadeStates: tableFadeStates, now: now)
                .equatable()
        case let .quote(attr, _):
            Text(faded(attr, baseColor: Palette.secondary))
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .padding(.leading, 10)
                .overlay(alignment: .leading) { Rectangle().fill(Palette.border).frame(width: 3) }
        case let .code(text, _):
            ScrollView(.horizontal, showsIndicators: false) {
                codeText(text)
                    .font(.system(.footnote, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .padding(10)
            }
            .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
        }
    }

    private func faded(_ base: AttributedString, baseColor: Color) -> AttributedString {
        guard let fadeState, fadeState.isAnimating else { return base }
        var copy = base
        TailFadeSweepState.applyFade(to: &copy, state: fadeState, at: now, baseColor: baseColor)
        return copy
    }

    private func codeText(_ text: String) -> Text {
        guard let fadeState, fadeState.isAnimating else { return Text(text) }
        var attributed = AttributedString(text)
        TailFadeSweepState.applyFade(to: &attributed, state: fadeState, at: now, baseColor: Palette.text)
        return Text(attributed)
    }
}

/// A Markdown table: a bold header and hairlines between rows, scrolling sideways when wide.
private struct MarkdownTable: View, Equatable {
    let header: [AttributedString]
    let rows: [[AttributedString]]
    let tableFadeStates: [MarkdownFadeCoordinator.TableCellKey: TailFadeSweepState]
    let now: TimeInterval

    static func == (lhs: Self, rhs: Self) -> Bool {
        guard lhs.header == rhs.header,
              lhs.rows == rhs.rows,
              lhs.tableFadeStates == rhs.tableFadeStates else {
            return false
        }
        return !lhs.tableFadeStates.isEmpty ? lhs.now == rhs.now : true
    }

    var body: some View {
        let columns = max(header.count, rows.map(\.count).max() ?? 0)
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 7) {
                GridRow {
                    ForEach(0 ..< columns, id: \.self) { col in
                        let base = col < header.count ? header[col] : AttributedString("")
                        Text(fadedCell(base, row: -1, column: col)).fontWeight(.semibold)
                    }
                }
                ForEach(Array(rows.enumerated()), id: \.offset) { rowIndex, row in
                    Rectangle().fill(Palette.border).frame(height: 1).gridCellUnsizedAxes(.horizontal)
                    GridRow {
                        ForEach(0 ..< columns, id: \.self) { col in
                            let base = col < row.count ? row[col] : AttributedString("")
                            Text(fadedCell(base, row: rowIndex, column: col))
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

    private func fadedCell(_ base: AttributedString, row: Int, column: Int) -> AttributedString {
        guard let fadeState = tableFadeStates[MarkdownFadeCoordinator.TableCellKey(row: row, column: column)],
              fadeState.isAnimating else {
            return base
        }
        var copy = base
        TailFadeSweepState.applyFade(to: &copy, state: fadeState, at: now, baseColor: Palette.text)
        return copy
    }
}
