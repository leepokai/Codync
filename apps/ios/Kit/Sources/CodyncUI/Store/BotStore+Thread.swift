import CodyncKit
import Foundation

/// Writing to a task's thread: its messages are entries of the thread's transcript (`threadEntries`), never a chat's.
extension BotStore {
    /// A thread's entry as it came in. The computer's copy of a message sent from here replaces the one shown at once.
    func putThreadEntry(_ e: Entry, thread: String) {
        // ponytail: re-sorts the thread's transcript on every entry; insert in place if threads grow past hundreds.
        threadEntries[thread] = (threadEntries[thread] ?? []).mergingThreadEntries([e])
    }

    func dropThreadEntry(_ id: String, thread: String) {
        threadEntries[thread]?.removeAll { $0.id == id }
    }

    /// A message for a task's thread, as text: in its transcript at once, then as the computer holds it (waiting
    /// behind the thread's turn, or starting one). A refusal leaves it failed and says why.
    func send(_ text: String, to thread: WorkThread, nonce: String = UUID().uuidString) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !retired else { return }
        lastSentNonce = nonce
        var local = Entry(id: Entry.localId(nonce), seq: .max, botId: thread.threadId, rev: 0, kind: "user", turn: 0,
                          data: EntryData(text: trimmed, status: "sending", clientNonce: nonce),
                          createdAt: Int64(Date.now.timeIntervalSince1970 * 1000), updatedAt: 0)
        putThreadEntry(local, thread: thread.threadId)
        Task {
            do {
                // Safe to repeat: the computer answers a nonce it already has with the same entry.
                let e = try await withLink(replay: true) {
                    try await $0.send(botId: thread.botId, text: trimmed, clientNonce: nonce, taskId: thread.taskId)
                }
                putThreadEntry(e, thread: thread.threadId)
            } catch {
                // A dropped link: the message's own "Failed to send" and Resend say enough.
                if (error as? HostError)?.isTransient != true { lastError = error.localizedDescription }
                // Unless the computer's copy came in on the events stream after all.
                guard threadEntries[thread.threadId]?.contains(where: { $0.id == local.id }) == true else { return }
                local.data.status = "failed"
                putThreadEntry(local, thread: thread.threadId)
            }
        }
    }

    /// Resend on a message that failed here; one the computer failed (a restart) goes again as a new message.
    func resend(_ entry: Entry, to thread: WorkThread) {
        guard let text = entry.data.text else { return }
        let local = entry.data.clientNonce.map(Entry.localId) == entry.id
        if local { dropThreadEntry(entry.id, thread: thread.threadId) }
        send(text, to: thread, nonce: (local ? entry.data.clientNonce : nil) ?? UUID().uuidString)
    }

    /// Has the bot's conversation open its details on `task`'s thread once it is on screen.
    public func requestThread(of botId: String, task: WorkTask) { threadRequest = (botId, task) }

    /// Consumed once, including when the conversation was already on screen.
    func takeThreadRequest(for botId: String) -> WorkTask? {
        guard let request = threadRequest, request.botId == botId else { return nil }
        threadRequest = nil
        return request.task
    }
}
