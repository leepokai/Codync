import CodyncKit
import SwiftUI

/// Approval card in Grok Bot's choice-card style: what the agent wants, where
/// it runs, and the answers as a list of rows.
struct PermissionCard: View {
    let entry: Entry
    let hostName: String
    let respond: (String?) -> Void
    @State private var expanded = false

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
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(headline).font(.headline).foregroundStyle(Palette.text)
                if pending {
                    Circle().fill(Palette.warning).frame(width: 7, height: 7)
                }
            }
            Text(d.title ?? "")
                .font(.subheadline.monospaced())
                .foregroundStyle(Palette.secondary)
                .lineLimit(expanded ? nil : 3)
            Label("Runs on \(hostName)\(d.cwd.map { " · \(($0 as NSString).lastPathComponent)" } ?? "")", systemImage: "desktopcomputer")
                .font(.caption)
                .foregroundStyle(Palette.tertiary)

            if hasDetail {
                Disclosure(isExpanded: $expanded) {
                    VStack(alignment: .leading, spacing: 8) {
                        if let command = d.command, !command.isEmpty {
                            CodeBox(text: command)
                        }
                        if let detail = d.detail, !detail.isEmpty {
                            CodeBox(text: String(detail.prefix(2000)))
                        }
                        ForEach(d.diffs ?? [], id: \.self) { DiffView(diff: $0) }
                    }
                    .padding(.top, 6)
                } label: {
                    Text("Details").font(.caption.weight(.medium)).foregroundStyle(Palette.secondary)
                }
            }

            if pending {
                buttons
            } else {
                Text(outcome)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Palette.secondary)
            }
        }
        .padding(16)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.trailing, 40)
    }

    private var hasDetail: Bool {
        !(d.command ?? "").isEmpty || !(d.detail ?? "").isEmpty || !(d.diffs ?? []).isEmpty
    }

    /// Ordered like Grok Bot: Allow once, Always allow, then Deny.
    private var ordered: [PermissionOption] {
        let rank = ["allow_once": 0, "allow_always": 1, "reject_once": 2, "reject_always": 3]
        return (d.options ?? []).sorted { (rank[$0.kind] ?? 9) < (rank[$1.kind] ?? 9) }
    }

    private func label(_ o: PermissionOption) -> String {
        switch o.kind {
        case "allow_once": "Allow once"
        case "allow_always": "Always allow"
        case "reject_once": "Deny"
        case "reject_always": "Never"
        default: o.name
        }
    }

    private var buttons: some View {
        let options = ordered
        return VStack(spacing: 0) {
            ForEach(Array(options.enumerated()), id: \.element.id) { i, o in
                if i > 0 { Rectangle().fill(Palette.border).frame(height: 0.5) }
                Button { respond(o.optionId) } label: {
                    Text(label(o))
                        .font(.body.weight(o.kind == "allow_once" ? .semibold : .regular))
                        .foregroundStyle(o.kind.hasPrefix("allow") ? Palette.text : Palette.danger)
                        .frame(maxWidth: .infinity, minHeight: 46, alignment: .leading)
                        .padding(.horizontal, 14)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .background(Palette.background, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .padding(.top, 4)
    }

    private var outcome: String {
        switch d.status {
        case "answered":
            let chosen = d.options?.first { $0.optionId == d.selected }
            return switch chosen?.kind {
            case "allow_once": "Allowed once"
            case "allow_always": "Always allowed"
            case "reject_once": "Denied"
            case "reject_always": "Never allowed"
            default: chosen?.name ?? "Answered"
            }
        case "cancelled": return "Cancelled"
        default: return "Expired — the agent moved on"
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
                Text("+\(diff.added)").foregroundStyle(Palette.added)
                Text("−\(diff.removed)").foregroundStyle(Palette.removed)
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
