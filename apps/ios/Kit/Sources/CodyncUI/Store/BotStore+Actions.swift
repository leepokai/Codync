import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

extension BotStore {
    /// The client while the link is really up: not during a reconnect the grace period keeps
    /// quiet, nor right after coming back to the foreground, when the old link is already gone.
    var live: HostClient? {
        connection == .online && heldDrop == nil && eventsTask != nil && !retired ? client : nil
    }

    /// The client once the link is up. A reconnect in progress is waited out (headers show it,
    /// see `shownConnection`), and an unreachable computer gets one fresh attempt first. A computer
    /// that is off or refusing this device throws at once; so does a reconnect that outlasts `deadline`.
    func ready(until deadline: ContinuousClock.Instant = .now + BotStore.actionPatience) async throws -> HostClient {
        var counted = false
        var retried = false
        defer { if counted { Motion.animate { waiting -= 1 } } }
        while !retired {
            if let live { return live }
            switch connection {
            case .online, .connecting: break
            case .offline where !retried:
                // The link retries on a backoff of up to 30 s: with someone waiting, try now.
                retried = true
                restartStream()
            case .offline, .unpaired: throw HostError.unreachable
            // The relay reports when it comes back, so there is no attempt of ours to wait for.
            case let .computerOffline(lastSeen): throw HostError.computerOffline(lastSeen: lastSeen)
            case let .unauthorized(message): throw HostError.unauthorized(message)
            }
            guard ContinuousClock.now < deadline else { break }
            if !counted {
                counted = true
                Motion.animate { waiting += 1 }
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw HostError.unreachable
    }

    /// Runs `op` once the link is up. With `replay`, only for calls the computer can safely get
    /// twice, a call the link dropped under is tried again after the reconnect.
    func withLink<T>(replay: Bool = false, _ op: @MainActor (HostClient) async throws -> T) async throws -> T {
        let deadline = ContinuousClock.now + Self.actionPatience
        while true {
            let client = try await ready(until: deadline)
            do {
                return try await op(client)
            } catch let error as HostError where replay && error.isTransient && ContinuousClock.now < deadline {
                log.info("retrying after: \(error.localizedDescription)")
                try await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    /// Fire-and-forget actions; only what waiting and retrying couldn't settle reaches the dialog.
    private func perform(replay: Bool = false, _ op: @escaping @MainActor (HostClient) async throws -> Void) {
        Task {
            do {
                try await withLink(replay: replay, op)
            } catch {
                lastError = error.localizedDescription
            }
        }
    }

    public func stop(_ botId: String) { perform(replay: true) { try await $0.stop(botId) } }
    public func logCall(_ botId: String, seconds: Int) { perform { try await $0.logCall(botId, seconds: seconds) } }
    public func newSession(_ botId: String) { perform { try await $0.newSession(botId) } }

    // MARK: remote screen

    /// Takes control from bots (they can still look) or hands it back.
    public func screenTakeover(_ on: Bool) {
        perform(replay: true) { [weak self] in self?.screen = try await $0.screenTakeover(on) }
    }

    /// Quick reactions offered on every message (Slack's hover bar).
    public static let quickReactions = ["👍", "❤️", "😂", "🎉", "👀", "✅"]

    /// Toggles the user's reaction; shown at once, then replaced by the host's copy.
    public func react(_ entry: Entry, _ emoji: String) {
        guard let i = entries[entry.botId]?.firstIndex(where: { $0.id == entry.id }) else { return }
        var reactions = entries[entry.botId]?[i].data.reactions ?? []
        if let at = reactions.firstIndex(of: emoji) { reactions.remove(at: at) } else { reactions.append(emoji) }
        entries[entry.botId]?[i].data.reactions = reactions
        perform { client in
            let e = try await client.react(entryId: entry.id, emoji: emoji)
            self.upsert(e)
        }
    }

    public func respond(_ entry: Entry, option: String?) {
        guard answering[entry.id] == nil else { return }
        Motion.animate { answering[entry.id] = option ?? "" }
        Task {
            do {
                try await withLink(replay: true) { try await $0.respondPermission(entryId: entry.id, optionId: option) }
                // The card's own update follows on the events stream; hold the spinner until then.
                try? await Task.sleep(for: .seconds(2))
            } catch {
                lastError = error.localizedDescription
            }
            Motion.animate { answering[entry.id] = nil }
        }
    }

    struct ReadingScope: Hashable {
        let botId: String
        let thread: String?
    }

    /// Views register only while visible in an active scene. Multiple windows may
    /// read the same conversation without unregistering each other.
    func setReading(_ token: UUID, botId: String, thread: String?, active: Bool) {
        if active {
            let scope = ReadingScope(botId: botId, thread: thread)
            readingViews[token] = scope
            if connection == .online { markRead(botId, thread: thread) }
        } else {
            readingViews[token] = nil
        }
    }

    func acknowledgeVisibleConversations(_ botId: String, entry: Entry? = nil) {
        guard connection == .online else { return }
        for scope in Set(readingViews.values) where scope.botId == botId {
            if let entry, (!entry.isChat || entry.threadId != scope.thread) { continue }
            markRead(botId, thread: scope.thread)
        }
    }

    /// The main chat on screen (or the thread on `thread`) is read; the host ignores it
    /// when nothing there is unread.
    public func markRead(_ botId: String, thread: String? = nil) {
        // Entry events may arrive before the roster's unread count. The host is
        // authoritative and treats a redundant scoped acknowledgement as a no-op.
        Task {
            // Opening a cached conversation can race the foreground reconnect.
            // This background acknowledgement must never interrupt the conversation.
            if live == nil {
                try? await Task.sleep(for: Self.initialConnectionGrace)
            }
            guard !Task.isCancelled, isActive, let client = live else { return }
            do {
                try await client.markRead(botId, threadId: thread)
            } catch {
                // The visible scope is acknowledged again when the link recovers.
                log.debug("read acknowledgement deferred: \(error.localizedDescription)")
            }
        }
    }

    /// The roster's "Mark as read": the chat and all its threads.
    public func markAllRead(_ botId: String) {
        guard (bots[botId]?.unread ?? 0) > 0 else { return }
        bots[botId]?.unread = 0
        perform(replay: true) { try await $0.markRead(botId, all: true) }
    }

    public func setPinned(_ bot: Bot, _ pinned: Bool) {
        var d = BotDraft(bot)
        d.pinned = pinned
        bots[bot.id]?.pinned = pinned
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    public func setHidden(_ bot: Bot, _ hidden: Bool) {
        var d = BotDraft(bot)
        d.hidden = hidden
        bots[bot.id]?.hidden = hidden
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    public func delete(_ bot: Bot) {
        removeComposerDrafts(for: bot.id)
        bots[bot.id] = nil
        entries[bot.id] = nil
        perform(replay: true) { try await $0.deleteBot(bot.id) }
    }

    /// Creates a group chat (or opens the one these bots already share) and selects it.
    public func createGroup(name: String, description: String = "", members: [String]) async throws -> Bot {
        let group = try await ready().createGroup(GroupDraft(name: name, description: description, members: members))
        bots[group.id] = group
        selection = group.id
        return group
    }

    public func updateGroup(_ draft: GroupDraft) async throws {
        let group = try await ready().updateGroup(draft)
        bots[group.id] = group
    }

    /// Pages in a thread's replies (the stream only carries recent entries).
    public func loadThread(_ botId: String, root: String) async {
        guard let client, let replies = try? await client.thread(botId: botId, rootId: root) else { return }
        for e in replies { upsert(e) }
    }

    /// The bot-to-bot notices between a chat and a peer. Never `upsert`ed: old notices in the
    /// mirror would break `loadOlder`, which pages from the oldest loaded `seq`.
    public func botConversation(_ botId: String, peer: String) async throws -> [Entry] {
        try await ready().botConversation(botId: botId, peerId: peer)
    }

    public func save(_ draft: BotDraft) async throws -> Bot {
        let client = try await ready()
        let bot = draft.id == nil ? try await client.createBot(draft) : try await client.updateBot(draft)
        bots[bot.id] = bot
        return bot
    }

    public func listDirs(_ path: String?) async throws -> DirListing {
        try await ready().listDirs(path)
    }

    public func refreshUsage() async {
        guard !retired, let client else { return }
        if let u = try? await client.usage(refresh: true), !Task.isCancelled { setUsage(u) }
    }

    public func loadOlder(_ botId: String) async {
        guard let client, !historyComplete.contains(botId) else { return }
        let first = chat(botId).first { $0.seq > 0 && $0.seq != Int64.max }?.seq ?? Int64.max
        guard let older = try? await client.history(botId: botId, beforeSeq: first, limit: 100) else { return }
        if older.count < 100 { historyComplete.insert(botId) }
        for e in older { upsert(e) }
    }
}
