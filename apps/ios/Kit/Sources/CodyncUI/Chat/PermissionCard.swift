import CodyncKit
import SwiftUI

/// Approval card in the choice card's form: what the agent wants, where it runs, and the answers as the choice card's
/// numbered rows, in one tone. Once settled it folds to one line (`RequestLine`), which opens to where it ran and what
/// it asked with: no card, there is nothing left to answer.
struct PermissionCard: View {
    let entry: Entry
    let hostName: String
    /// The option whose answer is on its way to the computer, if any.
    var answering: String?
    let respond: (String?) -> Void
    /// In a run of requests that ended alike its line leaves the ending to the run's.
    var showsEnding = true
    @State private var expanded = false
    @State private var open = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var d: EntryData { entry.data }
    private var pending: Bool { d.status == "pending" }

    private var headline: String {
        switch d.toolKind {
        case "execute": "Wants to run a command"
        case "edit", "delete", "move": "Wants to change files"
        case "fetch": "Wants to access the web"
        case "read", "search": "Wants to read files"
        default: "Wants to use a tool"
        }
    }

    var body: some View {
        // Settled, there is nothing to answer: no card, one line that opens to where it ran and what it asked with.
        if !pending {
            let n = Request.name(d.title ?? "")
            VStack(alignment: .leading, spacing: 0) {
                RequestLine(icon: Request.icon(kind: d.toolKind, title: d.title), name: n.name, source: n.source, literal: n.literal,
                            ending: showsEnding ? Self.outcome(d) : nil, expanded: open) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { open.toggle() }
                }
                if open {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(whereText).font(.caption).foregroundStyle(Palette.tertiary)
                        if let command = d.command, !command.isEmpty { CodeBox(text: command) }
                        if let detail = Request.detail(d.detail) { CodeBox(text: String(detail.prefix(2000))) }
                        ForEach(d.diffs ?? [], id: \.self) { DiffView(diff: $0) }
                    }
                    .padding(.leading, 24)
                    .padding(.bottom, 6)
                }
            }
            .padding(.trailing, 40)
        } else {
            card
        }
    }

    private var whereText: String {
        L10n.fill(L10n.tr("Runs on {name}{path}"), ["name": hostName, "path": d.cwd.map { " · \(($0 as NSString).lastPathComponent)" } ?? ""])
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 10) {
            // The choice card's form: the question, where it runs with Details, the request in a well.
            VStack(alignment: .leading, spacing: 4) {
                Text(L10n.tr(headline)).font(.headline).foregroundStyle(Palette.text)
                HStack(spacing: 8) {
                    Text(whereText)
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    if hasDetail {
                        Button {
                            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { expanded.toggle() }
                        } label: {
                            HStack(spacing: 4) {
                                Text(L10n.tr("Details")).font(.caption.weight(.medium))
                                Image(systemName: "chevron.right").font(.caption2.weight(.semibold))
                                    .rotationEffect(.degrees(expanded ? 90 : 0))
                            }
                            .foregroundStyle(Palette.secondary)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityValue(expanded ? "Expanded" : "Collapsed")
                    }
                }
                Text(d.title ?? "")
                    .font(.subheadline.monospaced())
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(expanded ? nil : 3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .background(Palette.text.opacity(0.06), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .padding(.top, 4)
            }
            WorkThreadLine(data: d)

            if hasDetail && expanded {
                VStack(alignment: .leading, spacing: 8) {
                    if let command = d.command, !command.isEmpty {
                        CodeBox(text: command)
                    }
                    if let detail = Request.detail(d.detail) {
                        CodeBox(text: String(detail.prefix(2000)))
                    }
                    ForEach(d.diffs ?? [], id: \.self) { DiffView(diff: $0) }
                }
            }

            buttons
        }
        .padding(16)
        .frame(maxWidth: 420, alignment: .leading)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.trailing, 40)
    }

    private var hasDetail: Bool {
        !(d.command ?? "").isEmpty || Request.detail(d.detail) != nil || !(d.diffs ?? []).isEmpty
    }

    /// Ordered like Grok Bot: Allow once, Always allow, then Deny.
    private var ordered: [PermissionOption] {
        let rank = ["allow_once": 0, "allow_always": 1, "reject_once": 2, "reject_always": 3]
        return (d.options ?? []).sorted { (rank[$0.kind] ?? 9) < (rank[$1.kind] ?? 9) }
    }

    private static func hasDuplicateKind(_ kind: String, in options: [PermissionOption]) -> Bool {
        options.filter { $0.kind == kind }.count > 1
    }

    static func optionLabel(_ o: PermissionOption, in options: [PermissionOption]) -> String {
        let name = o.name.trimmingCharacters(in: .whitespacesAndNewlines)
        if hasDuplicateKind(o.kind, in: options), !name.isEmpty { return name }
        switch o.kind {
        case "allow_once": return L10n.tr("Allow once")
        case "allow_always": return L10n.tr("Always allow")
        case "reject_once": return L10n.tr("Deny")
        case "reject_always": return L10n.tr("Never")
        default: return o.name
        }
    }

    private var buttons: some View {
        let options = ordered
        return VStack(spacing: 2) {
            ForEach(Array(options.enumerated()), id: \.element.id) { i, o in
                Button { respond(o.optionId) } label: {
                    HStack(spacing: 10) {
                        RowNumber {
                            if answering == o.optionId { Spinner(size: 12) } else { Text(verbatim: "\(i + 1)") }
                        }
                        Text(Self.optionLabel(o, in: options)).font(.body).foregroundStyle(Palette.text)
                        Spacer(minLength: 0)
                    }
                    .decisionRow()
                }
                .buttonStyle(RowStyle())
                .disabled(answering != nil)
                .opacity(answering == nil || answering == o.optionId ? 1 : 0.4)
            }
        }
        .padding(.horizontal, -8)
    }

    static func outcome(_ d: EntryData) -> String {
        switch d.status {
        case "answered":
            let options = d.options ?? []
            let chosen = options.first { $0.optionId == d.selected }
            if let chosen {
                let name = chosen.name.trimmingCharacters(in: .whitespacesAndNewlines)
                if hasDuplicateKind(chosen.kind, in: options), !name.isEmpty { return name }
            }
            return switch chosen?.kind {
            case "allow_once": L10n.tr("Allowed once")
            case "allow_always": L10n.tr("Always allowed")
            case "reject_once": L10n.tr("Denied")
            case "reject_always": L10n.tr("Never allowed")
            default: chosen?.name ?? L10n.tr("Answered")
            }
        case "cancelled": return L10n.tr("Cancelled")
        default: return L10n.tr("Expired — the agent moved on")
        }
    }
}

struct CodeBox: View {
    let text: String

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Text(text)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .padding(8)
        }
        .frame(maxHeight: 240)
        .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
    }
}

struct DiffView: View {
    let diff: FileDiff

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Image(systemName: diff.isNew ? "doc.badge.plus" : "doc.text")
                Text((diff.path as NSString).lastPathComponent).lineLimit(1)
                Spacer()
                Text(L10n.tr("+\(diff.added)")).foregroundStyle(Palette.added)
                Text(L10n.tr("−\(diff.removed)")).foregroundStyle(Palette.removed)
            }
            .font(.caption.monospaced())
            .foregroundStyle(Palette.secondary)
            ScrollView(.horizontal, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(diff.patch.split(separator: "\n", omittingEmptySubsequences: false).prefix(80).enumerated()), id: \.offset) { _, line in
                        Text(line.isEmpty ? " " : String(line))
                            .foregroundStyle(line.hasPrefix("+") ? Palette.added : line.hasPrefix("-") ? Palette.removed : Palette.secondary)
                    }
                }
                .font(.system(.caption2, design: .monospaced))
                .padding(8)
            }
            .frame(maxHeight: 260)
            .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
        }
    }
}

/// "Work · <title>" under what a thread put in the chat (a card, a file, a voice note, an
/// artifact); nothing for the chat's own.
struct WorkThreadLine: View {
    let data: EntryData
    /// In a thread's own chat the line says nothing new.
    @Environment(\.workThread) private var open

    var body: some View {
        if open == nil, let thread = data.thread, !thread.title.isEmpty {
            Label(L10n.tr("Work") + " · " + thread.title, systemImage: "clock")
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
                .lineLimit(1)
        }
    }
}
