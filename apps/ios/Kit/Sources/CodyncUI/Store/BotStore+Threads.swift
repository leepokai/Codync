import CodyncKit
import Foundation

// Threads: a side conversation opened from a chat, so one message can be followed without filling the chat
// (OpenBot Mobile's thread). Each keeps its own title, its branch origin, an archive flag and its own read mark.

extension BotStore {
    /// A chat's live threads, or its archive drawer, newest first; the host never sends their lines.
    func refreshThreads(_ botId: String, archived: Bool = false) {
        Task {
            guard let list = try? await withLink { try await $0.threads(botId: botId, archived: archived) } else { return }
            if archived { archivedThreads[botId] = list } else { threads[botId] = list }
        }
    }

    /// Reads a thread: its lines join the mirror and the host marks it read.
    func readThread(_ botId: String, root: String) async -> ThreadInfo? {
        guard let read = try? await withLink { try await $0.thread(botId: botId, root: root) } else { return nil }
        threadEntries[root] = read.entries
        if let info = read.thread { replace(info, botId: botId) }
        patch(botId: botId, root: root) { $0.unread = 0 }
        return read.thread
    }

    /// Opens a thread, optionally branched from one of the chat's messages, and shows it.
    @discardableResult
    func startThread(_ botId: String, from entry: Entry? = nil, title: String = "") async -> String? {
        guard let root = try? await withLink(replay: true) {
            try await $0.createThread(botId: botId, fromEntryId: entry?.id, title: title)
        } else { return nil }
        let now = Int64(Date.now.timeIntervalSince1970 * 1000)
        replace(ThreadInfo(id: root, title: title, fromEntry: entry?.id, branchText: entry?.replyQuote.text,
                           archived: false, createdAt: now, updatedAt: now, unread: 0, lastText: nil, lastAuthor: nil),
                botId: botId)
        _ = await readThread(botId, root: root)
        return root
    }

    func renameThread(_ root: String, botId: String, title: String) {
        let trimmed = String(title.trimmingCharacters(in: .whitespacesAndNewlines).prefix(120))
        patch(botId: botId, root: root) { $0.title = trimmed }
        Task { _ = try? await withLink { try await $0.renameThread(root, title: trimmed) } }
    }

    /// Moves a thread between the chat and its archive drawer.
    func setThreadArchived(_ root: String, botId: String, archived: Bool) {
        patch(botId: botId, root: root) { $0.archived = archived }
        guard let info = [threads[botId], archivedThreads[botId]].compactMap({ $0 }).flatMap({ $0 }).first(where: { $0.id == root })
        else { return }
        threads[botId]?.removeAll { $0.id == root }
        archivedThreads[botId]?.removeAll { $0.id == root }
        if info.archived { archivedThreads[botId, default: []].insert(info, at: 0) } else { threads[botId, default: []].insert(info, at: 0) }
        Task { _ = try? await withLink { try await $0.archiveThread(root, archived: archived) } }
    }

    /// Takes the thread with its replies; the chat keeps everything else.
    func deleteThread(_ root: String, botId: String) {
        threads[botId]?.removeAll { $0.id == root }
        archivedThreads[botId]?.removeAll { $0.id == root }
        threadEntries[root] = nil
        setComposerDraft("", for: Self.threadDraftKey(botId: botId, thread: root))
        Task { _ = try? await withLink { try await $0.deleteThread(root) } }
    }

    /// A thread's chip on its root line, from the host's live `thread` event: how many replied, and how many got
    /// no answer yet. A gone thread clears it.
    func applyThreadSummary(_ threadId: String, botId: String, summary: ThreadSummary?) {
        guard var list = entries[botId], let i = list.firstIndex(where: { $0.id == threadId }) else { return }
        list[i].data.thread = summary
        entries[botId] = list
    }

    /// A thread's lines, as this device holds them.
    public func threadChat(_ botId: String, _ root: String) -> [Entry] {
        (threadEntries[root] ?? []).filter { $0.botId == botId }.visibleEntries()
    }

    private func patch(botId: String, root: String, _ edit: (inout ThreadInfo) -> Void) {
        for keyPath in [\BotStore.threads, \BotStore.archivedThreads] {
            guard var list = self[keyPath: keyPath][botId], let i = list.firstIndex(where: { $0.id == root }) else { continue }
            edit(&list[i])
            self[keyPath: keyPath][botId] = list
        }
    }

    private func replace(_ info: ThreadInfo, botId: String) {
        var list = threads[botId] ?? []
        if let i = list.firstIndex(where: { $0.id == info.id }) { list[i] = info } else { list.insert(info, at: 0) }
        threads[botId] = list
    }
}
