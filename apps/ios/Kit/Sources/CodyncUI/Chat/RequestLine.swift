import CodyncKit
import SwiftUI

/// How a settled request reads in a line (the desktop's `requests.ts`).
enum Request {
    /// An MCP tool's call ("memory-read_memory: read_memory") as its tool's name in words and the server it comes
    /// from; anything else (a command, a file) as the agent wrote it, literal.
    static func name(_ title: String) -> (name: String, source: String?, literal: Bool) {
        guard let (server, tool) = mcpTool(title) else {
            let t = title.trimmingCharacters(in: .whitespaces)
            let bare = t.count > 1 && t.hasPrefix("`") && t.hasSuffix("`") && !t.dropFirst().dropLast().contains("`")
            return (bare ? String(t.dropFirst().dropLast()) : t, nil, true)
        }
        let words = tool.replacingOccurrences(of: "_", with: " ")
        return (words.prefix(1).uppercased() + words.dropFirst(), server, false)
    }

    /// The symbol of a request, so one tells from the next: by the tool's kind, and for a tool of no kind (an MCP
    /// server's) by the verb its name starts with.
    static func icon(kind: String?, title: String?) -> String {
        let byKind = ["execute": "terminal", "edit": "pencil", "delete": "trash", "move": "pencil", "fetch": "globe", "read": "doc.text", "search": "magnifyingglass"]
        if let kind, let icon = byKind[kind] { return icon }
        let byVerb = ["read": "doc.text", "get": "doc.text", "search": "magnifyingglass", "find": "magnifyingglass", "list": "list.bullet",
                      "write": "pencil", "edit": "pencil", "update": "pencil", "create": "pencil", "add": "pencil", "save": "pencil",
                      "delete": "trash", "remove": "trash", "fetch": "globe", "open": "globe", "run": "terminal"]
        let verb = title.flatMap(mcpTool)?.tool.split(separator: "_").first.map(String.init)
        return verb.flatMap { byVerb[$0] } ?? "wrench.and.screwdriver"
    }

    /// What an answered (or cancelled, or expired) approval ended in; nil while it waits or for any other entry.
    static func ending(_ e: Entry) -> String? {
        guard e.kind == "permission", let status = e.data.status, status != "pending" else { return nil }
        let options = e.data.options ?? []
        let chosen = options.first { $0.optionId == e.data.selected }
        if let chosen {
            let name = chosen.name.trimmingCharacters(in: .whitespacesAndNewlines)
            if options.filter({ $0.kind == chosen.kind }).count > 1, !name.isEmpty {
                return "\(status):\(chosen.kind):\(name)"
            }
        }
        return "\(status):\(chosen?.kind ?? "")"
    }

    /// A request's detail as worth showing: without the Markdown fence agents wrap it in, nil when it is empty.
    static func detail(_ text: String?) -> String? {
        var bare = (text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if bare.hasPrefix("```"), bare.hasSuffix("```"), let open = bare.firstIndex(of: "\n") {
            bare = String(bare[bare.index(after: open)...].dropLast(3)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return ["", "{}", "[]"].contains(bare) ? nil : bare
    }

    /// "<server>-<tool>: <tool>"
    private static func mcpTool(_ title: String) -> (server: String, tool: String)? {
        let t = title.trimmingCharacters(in: .whitespaces)
        guard let colon = t.range(of: ": ") else { return nil }
        let tool = String(t[colon.upperBound...])
        let head = t[..<colon.lowerBound]
        guard !tool.isEmpty, tool.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }), head.hasSuffix("-" + tool) else { return nil }
        let server = String(head.dropLast(tool.count + 1))
        return server.isEmpty ? nil : (server, tool)
    }
}

/// A settled request (or a run of them) as one quiet line: its symbol, name and source, how it ended.
struct RequestLine: View {
    let icon: String
    let name: String
    var source: String?
    var literal = false
    var ending: String?
    /// A run's line: whether its own lines show.
    var expanded: Bool?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon).font(.caption).foregroundStyle(Palette.secondary).frame(width: 16)
                Text(name).font(literal ? .subheadline.monospaced() : .subheadline).foregroundStyle(Palette.text).lineLimit(1)
                if let source { Text(source).font(.subheadline).foregroundStyle(Palette.tertiary).lineLimit(1) }
                Spacer(minLength: 8)
                if let ending { Text(ending).font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1) }
                if let expanded {
                    Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(Palette.tertiary)
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                }
            }
            .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
            .padding(.horizontal, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowStyle())
        .padding(.horizontal, -8)
        .accessibilityValue(expanded.map { $0 ? "Expanded" : "Collapsed" } ?? "")
    }
}

/// Approvals in a row that ended alike, as one line: how many, where they come from and how they ended, with each
/// one's own line under it on demand.
struct RequestRun<Row: View>: View {
    let entries: [Entry]
    @ViewBuilder let row: (Entry) -> Row
    @State private var open = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let icons = Set(entries.map { Request.icon(kind: $0.data.toolKind, title: $0.data.title) })
        var sources: [String] = []
        for e in entries { if let s = Request.name(e.data.title ?? "").source, !sources.contains(s) { sources.append(s) } }
        return VStack(alignment: .leading, spacing: 0) {
            RequestLine(icon: icons.count == 1 ? icons.first! : "wrench.and.screwdriver",
                        name: L10n.fill(L10n.tr("{count} tool requests"), ["count": "\(entries.count)"]),
                        source: sources.isEmpty ? nil : sources.joined(separator: ", "),
                        ending: entries.first.map { PermissionCard.outcome($0.data) }, expanded: open) {
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { open.toggle() }
            }
            if open {
                VStack(alignment: .leading, spacing: 0) { ForEach(entries) { row($0) } }
                    .padding(.leading, 20)
            }
        }
        .padding(.trailing, 40)
    }
}
