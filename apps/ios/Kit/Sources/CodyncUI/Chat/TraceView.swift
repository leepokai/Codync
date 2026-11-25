import CodyncKit
import SwiftUI

/// "Full conversation": everything the agent did — narration, thinking, tool
/// calls with output and diffs, plans — grouped by turn.
public struct TraceView: View {
    let botId: String

    public init(botId: String) { self.botId = botId }
    @Environment(BotStore.self) private var model

    public var body: some View {
        let all = model.chat(botId)
        let turns = Dictionary(grouping: all, by: \.turn)
            .sorted { $0.key < $1.key }
        // The running turn's thoughts start unfolded.
        let running = Entry.runningTurn(all, working: model.bots[botId]?.isWorking(in: botId) == true)
        VStack(spacing: 0) {
            ModalHeader("Full conversation")
            ScrollView {
            LazyVStack(alignment: .leading, spacing: 22) {
                if turns.isEmpty {
                    Text(L10n.tr("Nothing yet.")).foregroundStyle(Palette.tertiary)
                }
                ForEach(turns, id: \.key) { turn, entries in
                    VStack(alignment: .leading, spacing: 8) {
                        // A plain one-line turn header (CardSection's own title wraps).
                        Text(entries.first { $0.kind == "user" }?.data.text ?? L10n.tr("Turn {turn}").replacingOccurrences(of: "{turn}", with: String(turn)))
                            .lineLimit(1)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(Palette.text)
                            .padding(.leading, 4)
                        CardSection {
                            ForEach(Entry.nested(entries), id: \.entry.id) { TraceRow(entry: $0.entry, unfolded: turn == running).nested($0.depth) }
                        }
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
            }
            .defaultScrollAnchor(.bottom)
        }
        .background(Palette.background)
        .tracked(.screenViewed(.trace))
    }
}

extension View {
    /// A step a subagent took, indented under the call that launched it. ponytail: one indent for any depth; Claude's
    /// subagents launch none of their own.
    func nested(_ depth: Int) -> some View {
        padding(.leading, depth > 0 ? 24 : 0)
    }
}

struct TraceRow: View {
    let entry: Entry
    private let allowsLiveReveal: Bool
    @State private var expanded: Bool

    /// The running turn's thoughts start unfolded, so its reasoning reads as it comes in.
    init(entry: Entry, unfolded: Bool) {
        self.entry = entry
        self.allowsLiveReveal = unfolded
        _expanded = State(initialValue: unfolded)
    }

    var body: some View {
        let d = entry.data
        switch entry.kind {
        case "user":
            Label { Text(d.text ?? "").foregroundStyle(Palette.text) } icon: { Image(systemName: "person.fill").foregroundStyle(Palette.secondary) }
                .font(.subheadline)
        case "agent":
            VStack(alignment: .leading, spacing: 4) {
                Text(L10n.tr(d.final == true ? "Reply" : "Said")).font(.caption2.bold()).foregroundStyle(Palette.tertiary)
                StreamingMarkdownBody(entry: entry)
            }
        case "thought":
            Disclosure(isExpanded: $expanded) {
                StreamingThoughtBody(entry: entry, allowsLiveReveal: allowsLiveReveal, expanded: expanded)
            } label: {
                Label { Text(L10n.tr("Thinking")) } icon: {
                    ThinkingOrb(size: 16, color: Palette.secondary, animated: false)
                }.font(.subheadline).foregroundStyle(Palette.secondary)
            }
        case "tool":
            ToolRow(data: d)
        case "plan":
            VStack(alignment: .leading, spacing: 6) {
                Label(L10n.tr("Plan"), systemImage: "checklist").font(.subheadline.bold())
                ForEach(Array((d.entries ?? []).enumerated()), id: \.offset) { _, item in
                    Label {
                        Text(item.content).strikethrough(item.status == "completed").foregroundStyle(Palette.text)
                    } icon: {
                        Image(systemName: item.status == "completed" ? "checkmark.circle.fill" : item.status == "in_progress" ? "circle.dotted" : "circle")
                            .foregroundStyle(item.status == "completed" ? Palette.accent : Palette.tertiary)
                    }
                    .font(.footnote)
                }
            }
        case "permission":
            Label(d.title ?? "Approval", systemImage: "hand.raised")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
        default:
            Text(L10n.tr(d.text ?? "")).font(.footnote).foregroundStyle(Palette.tertiary)
        }
    }
}

@MainActor
private final class ThoughtTailFadeHolder {
    var chunkCount = 0
    var state = TailFadeSweepState()

    func sync(pieces: [String], isStreaming: Bool, hidden: Bool, now: TimeInterval) {
        guard isStreaming, !hidden, let tail = pieces.last else {
            if state.isAnimating {
                state.settle(at: now, hidden: hidden)
            }
            return
        }
        if pieces.count != chunkCount {
            chunkCount = pieces.count
            state = TailFadeSweepState(
                text: tail,
                isStreaming: true,
                hidden: false,
                now: now,
                animateInitial: true
            )
        } else {
            state.sync(text: tail, isStreaming: true, hidden: false, now: now)
        }
    }

    func settle(hidden: Bool = true, now: TimeInterval = ProcessInfo.processInfo.systemUptime) {
        if state.isAnimating {
            state.settle(at: now, hidden: hidden)
        }
    }
}

private struct StreamingThoughtBody: View {
    let entry: Entry
    let allowsLiveReveal: Bool
    let expanded: Bool

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @State private var pacer: StreamTextController?
    @State private var fadeHolder = ThoughtTailFadeHolder()

    private var rawText: String { entry.data.text ?? "" }
    private var isBotWorking: Bool { model.bots[entry.botId]?.isWorking(in: entry.botId) == true }

    var body: some View {
        let eligible = allowsLiveReveal && expanded && !reduceMotion && scenePhase == .active && model.shouldRevealLiveText(for: entry)
        let displayedText: String
        let isAnimating: Bool
        if let pacer {
            displayedText = (eligible || pacer.needsTick) ? pacer.displayedText : rawText
            isAnimating = eligible && pacer.isStreamingActive
        } else if eligible {
            let initial = min(max(0, model.initialRevealedCount(for: entry) ?? 0), rawText.count)
            let end = rawText.index(rawText.startIndex, offsetBy: initial)
            displayedText = String(rawText[..<end])
            isAnimating = true
        } else {
            displayedText = rawText
            isAnimating = false
        }

        let pieces = ThoughtChunking.pieces(of: displayedText)
        let now = ProcessInfo.processInfo.systemUptime
        let _ = fadeHolder.sync(pieces: pieces, isStreaming: isAnimating, hidden: !eligible, now: now)

        Group {
            if eligible, isAnimating, fadeHolder.state.isAnimating {
                TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: false)) { _ in
                    let frameNow = ProcessInfo.processInfo.systemUptime
                    let _ = fadeHolder.state.prune(now: frameNow)
                    piecesStack(pieces, fadeState: fadeHolder.state, now: frameNow)
                }
            } else {
                piecesStack(pieces, fadeState: nil, now: now)
            }
        }
        .font(.footnote)
        .foregroundStyle(Palette.secondary)
        .textSelection(.enabled)
        .onAppear {
            reconcile()
        }
        .onChange(of: rawText) { _, _ in
            reconcile()
        }
        .onChange(of: expanded) { _, _ in
            reconcile()
        }
        .onChange(of: isBotWorking) { _, _ in
            reconcile()
        }
        .onChange(of: reduceMotion) { _, _ in
            reconcile()
        }
        .onChange(of: scenePhase) { _, _ in
            reconcile()
        }
        .onDisappear {
            pacer?.stop()
            fadeHolder.settle(hidden: true)
        }
    }

    @ViewBuilder
    private func piecesStack(_ pieces: [String], fadeState: TailFadeSweepState?, now: TimeInterval) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(pieces.enumerated()), id: \.offset) { index, piece in
                let isTail = index == pieces.count - 1
                let displayPiece = !isTail && piece.hasSuffix("\n") ? String(piece.dropLast()) : piece
                if isTail, let fadeState, fadeState.isAnimating {
                    Text(faded(displayPiece, state: fadeState, now: now))
                } else {
                    Text(displayPiece)
                }
            }
        }
    }

    private func faded(_ text: String, state: TailFadeSweepState, now: TimeInterval) -> AttributedString {
        var attributed = AttributedString(text)
        TailFadeSweepState.applyFade(to: &attributed, state: state, at: now, baseColor: Palette.secondary)
        return attributed
    }

    private func reconcile() {
        let hidden = reduceMotion || scenePhase != .active || !expanded
        guard allowsLiveReveal, model.shouldRevealLiveText(for: entry), !hidden else {
            pacer?.converge()
            pacer = nil
            fadeHolder.settle(hidden: true)
            if !allowsLiveReveal || reduceMotion || scenePhase != .active || !isBotWorking {
                model.completeLiveReveal(id: entry.id)
            }
            return
        }

        let entryID = entry.id
        let working = isBotWorking
        let progressHandler: (Int) -> Void = { count in
            model.noteLiveRevealProgress(id: entryID, count: count)
        }
        let completionHandler: (Int) -> Void = { total in
            if working {
                model.noteLiveRevealProgress(id: entryID, count: total)
            } else {
                model.completeLiveReveal(id: entryID)
            }
        }

        if let existing = pacer {
            existing.sync(
                text: rawText,
                live: working,
                hidden: false,
                onProgress: progressHandler,
                onComplete: completionHandler
            )
        } else {
            pacer = StreamTextController(
                text: rawText,
                live: working,
                arrivalReveal: true,
                hidden: false,
                initialShownCount: model.initialRevealedCount(for: entry),
                onProgress: progressHandler,
                onComplete: completionHandler
            )
        }
    }
}

struct ToolRow: View {
    let data: EntryData
    @State private var expanded = false

    private var icon: String {
        switch data.toolKind {
        case "read": "doc.text"
        case "edit": "pencil"
        case "delete": "trash"
        case "move": "arrow.right.doc.on.clipboard"
        case "search": "magnifyingglass"
        case "execute": "terminal"
        case "think": "brain"
        case "fetch": "globe"
        default: "wrench.and.screwdriver"
        }
    }

    private var orb: ThinkingOrb.State {
        switch data.toolKind {
        case "search": .searching
        case "fetch": .connecting
        case "edit": .shaping
        default: .working
        }
    }

    private var hasBody: Bool {
        !(data.output ?? "").isEmpty || !(data.diffs ?? []).isEmpty
    }

    var body: some View {
        if hasBody {
            Disclosure(isExpanded: $expanded) {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(data.diffs ?? [], id: \.self) { DiffView(diff: $0) }
                    if let out = data.output, !out.isEmpty { CodeBox(text: out) }
                }
            } label: { label }
        } else {
            label
        }
    }

    private var label: some View {
        let title = data.title ?? "Tool"
        return HStack(spacing: 8) {
            Image(systemName: icon).frame(width: 18).foregroundStyle(Palette.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(title == "Tool" ? L10n.tr("Tool") : title).font(.subheadline).foregroundStyle(Palette.text).lineLimit(2)
                if let line = data.subagent?.line {
                    Text(line).font(.footnote).foregroundStyle(Palette.tertiary).lineLimit(1)
                }
            }
            Spacer(minLength: 4)
            switch data.status {
            case "completed": Image(systemName: "checkmark").foregroundStyle(Palette.accent)
            case "failed": Image(systemName: "xmark").foregroundStyle(Palette.danger)
            default: ThinkingOrb(state: orb, size: 16, color: Palette.secondary)
            }
        }
        .font(.caption)
    }
}
