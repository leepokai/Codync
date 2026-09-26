import CodyncKit
import SwiftUI

/// "Full conversation": everything the agent did — narration, thinking, tool
/// calls with output and diffs, plans — grouped by turn.
/// A chat's trace, or one thread's (`thread`: its root).
public struct TraceView: View {
    let botId: String
    let thread: String?

    public init(botId: String, thread: String? = nil) {
        self.botId = botId
        self.thread = thread
    }
    @Environment(BotStore.self) private var model

    public var body: some View {
        let turns = Dictionary(grouping: model.allEntries(botId).filter { $0.threadId == thread }, by: \.turn)
            .sorted { $0.key < $1.key }
        VStack(spacing: 0) {
            ModalHeader("Full conversation")
            ScrollView {
            LazyVStack(alignment: .leading, spacing: InterfaceMetrics.value(mac: 14, mobile: 22)) {
                if turns.isEmpty {
                    Text("Nothing yet.").foregroundStyle(Palette.tertiary)
                }
                ForEach(turns, id: \.key) { turn, entries in
                    VStack(alignment: .leading, spacing: 8) {
                        // A plain one-line turn header (CardSection's own title wraps).
                        Text(entries.first { $0.kind == "user" }?.data.text ?? "Turn \(turn)")
                            .lineLimit(1)
                            .font(InterfaceMetrics.secondary.weight(.semibold))
                            .foregroundStyle(Palette.text)
                            .padding(.leading, 4)
                        CardSection {
                            ForEach(entries) { TraceRow(entry: $0) }
                        }
                    }
                }
            }
            .padding(InterfaceMetrics.value(mac: 14, mobile: 20))
            .frame(maxWidth: .infinity, alignment: .leading)
            }
            .defaultScrollAnchor(.bottom)
        }
        .background(Palette.background)
    }
}

private struct TraceRow: View {
    let entry: Entry
    @State private var expanded = false

    var body: some View {
        let d = entry.data
        switch entry.kind {
        case "user":
            Label { Text(d.text ?? "").foregroundStyle(Palette.text) } icon: { Image(systemName: "person.fill").foregroundStyle(Palette.secondary) }
                .font(.subheadline)
        case "agent":
            VStack(alignment: .leading, spacing: 4) {
                Text(d.final == true ? "Reply" : "Said").font(.caption2.bold()).foregroundStyle(Palette.tertiary)
                MarkdownText(d.text ?? "")
            }
        case "thought":
            Disclosure(isExpanded: $expanded) {
                Text(d.text ?? "").font(.footnote).foregroundStyle(Palette.secondary).textSelection(.enabled)
            } label: {
                Label { Text("Thinking") } icon: {
                    ThinkingOrb(size: 16, color: Palette.secondary, animated: false)
                }.font(.subheadline).foregroundStyle(Palette.secondary)
            }
        case "tool":
            ToolRow(data: d)
        case "plan":
            VStack(alignment: .leading, spacing: 6) {
                Label("Plan", systemImage: "checklist").font(.subheadline.bold())
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
            Text(d.text ?? "").font(.footnote).foregroundStyle(Palette.tertiary)
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
        HStack(spacing: 8) {
            Image(systemName: icon).frame(width: 18).foregroundStyle(Palette.secondary)
            Text(data.title ?? "Tool").font(.subheadline).foregroundStyle(Palette.text).lineLimit(2)
            Spacer(minLength: 4)
            switch data.status {
            case "completed": Image(systemName: "checkmark").foregroundStyle(Palette.accent)
            case "failed": Image(systemName: "xmark").foregroundStyle(Palette.danger)
            default: ThinkingOrb(state: data.toolKind == "search" ? .searching : data.toolKind == "fetch" ? .connecting : .working, size: 16, color: Palette.secondary)
            }
        }
        .font(.caption)
    }
}
