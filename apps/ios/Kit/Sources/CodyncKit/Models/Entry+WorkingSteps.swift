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

    /// What the running turn has done so far, in order.
    static func workingSteps(_ entries: [Entry]) -> [Entry] {
        let turn = runningTurn(entries, working: true)
        return entries.filter { $0.turn == turn && $0.isWorkingStep }
    }
}
