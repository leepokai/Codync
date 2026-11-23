import Foundation

/// The running turn's steps, which the working indicator unfolds and Full conversation opens on:
/// docs/features/chat-messages.md, "While it works".
public extension Entry {
    /// Thinking, narration, a tool call or a plan; never the bot's bookkeeping, nor a call the host didn't flag
    /// (one older than 2.25.0 flags none).
    var isWorkingStep: Bool {
        switch kind {
        case "thought": !(data.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        case "agent": data.final != true && !Self.isPass(data.text ?? "")
        case "tool": data.bookkeeping == false
        case "plan": true
        default: false
        }
    }

    /// No text, or a pass the host keeps out of the chat (`is_pass` in host/src/chat/group.rs).
    private static func isPass(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "\".*")).lowercased()
        return t.isEmpty || t == "(pass)" || t == "pass"
    }

    /// The turn running now while the bot works in this chat; a queued message already holds the next turn.
    static func runningTurn(_ entries: [Entry], working: Bool) -> Int64? {
        working ? entries.filter { $0.data.status != "queued" }.map(\.turn).max() : nil
    }

    /// Steps with each subagent's steps right under the call that launched it (`parent`, in the same turn), `depth`
    /// deeper. A step whose launch isn't among them stays where it is.
    static func nested(_ steps: [Entry]) -> [(entry: Entry, depth: Int)] {
        // A call is known by its turn and id: a launch is looked for in its own turn only.
        func key(_ e: Entry, _ id: String) -> String { "\(e.turn):\(id)" }
        let ids = Set(steps.compactMap { e in e.data.toolCallId.map { key(e, $0) } })
        func launch(_ e: Entry) -> String? {
            e.data.parent.flatMap { $0 != e.data.toolCallId && ids.contains(key(e, $0)) ? key(e, $0) : nil }
        }
        let children = Dictionary(grouping: steps.filter { launch($0) != nil }) { launch($0) ?? "" }
        var out: [(entry: Entry, depth: Int)] = []
        func visit(_ e: Entry, _ depth: Int) {
            out.append((e, depth))
            for child in e.data.toolCallId.flatMap({ children[key(e, $0)] }) ?? [] { visit(child, depth + 1) }
        }
        for e in steps where launch(e) == nil { visit(e, 0) }
        return out
    }

    /// What the running turn has done so far, in order.
    static func workingSteps(_ entries: [Entry]) -> [Entry] {
        let turn = runningTurn(entries, working: true)
        return entries.filter { $0.turn == turn && $0.isWorkingStep }
    }
}
