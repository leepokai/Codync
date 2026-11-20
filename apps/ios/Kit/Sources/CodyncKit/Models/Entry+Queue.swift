import Foundation

public extension Entry {
    /// A message of the user's waiting behind the bot's work: the computer hasn't given it to the agent yet.
    var isQueued: Bool { kind == "user" && data.status == "queued" }
}

public extension Array where Element == Entry {
    /// While the bot works, the user's waiting messages show above the composer instead of in the chat (sent now
    /// with `steer`, taken back with `unqueue`), oldest first. A bot that isn't working starts them at once.
    func queue(working: Bool) -> [Entry] {
        working ? filter(\.isQueued).sorted { $0.seq < $1.seq } : []
    }

    /// The chat without the messages `queue(working:)` shows above the composer.
    func hidingQueue(working: Bool) -> [Entry] {
        working ? filter { !$0.isQueued } : self
    }
}
