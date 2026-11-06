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
        Task { await attempt(replay: replay, op) }
    }

    /// `perform` for a caller that waits: whether the computer took it. A refusal runs `undo`, taking
    /// back what the action already showed, before it reaches the dialog.
    @discardableResult
    private func attempt(replay: Bool = false, undo: @MainActor () -> Void = {},
                         _ op: @MainActor (HostClient) async throws -> Void) async -> Bool {
        do {
            try await withLink(replay: replay, op)
            return true
        } catch {
            undo()
            lastError = error.localizedDescription
            return false
        }
    }

    public func stop(_ botId: String) { perform(replay: true) { try await $0.stop(botId) } }
    /// A call's audio is on: the lead speaks first, on the call (an older host, which has no lead, never gets this).
    public func callStarted(_ botId: String, call: String) {
        guard bots[botId]?.lead == true else { return }
        perform { try await $0.callStarted(botId, call: call) }
    }
    /// The bot's own agent again, off the fallback it runs on.
    public func switchBack(_ botId: String) { perform(replay: true) { try await $0.switchBack(botId) } }

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

    /// A card's button: `entry` shows `choice` as answering (its spinner) while `op` runs.
    private func answer(_ entry: Entry, _ choice: String, _ op: @escaping @MainActor (HostClient) async throws -> Void) {
        guard answering[entry.id] == nil else { return }
        Motion.animate { answering[entry.id] = choice }
        Task {
            do {
                try await withLink(replay: true, op)
            } catch {
                lastError = error.localizedDescription
            }
            Motion.animate { answering[entry.id] = nil }
        }
    }

    public func respond(_ entry: Entry, option: String?) {
        answer(entry, option ?? "") {
            try await $0.respondPermission(entryId: entry.id, optionId: option)
            // The card's own update follows on the events stream; hold the spinner until then.
            try? await Task.sleep(for: .seconds(2))
        }
    }

    /// Views register only while visible in an active scene. Multiple windows may
    /// read the same conversation without unregistering each other.
    func setReading(_ token: UUID, botId: String, active: Bool) {
        if active {
            readingViews[token] = botId
            if connection == .online { markRead(botId) }
        } else {
            readingViews[token] = nil
        }
    }

    func acknowledgeVisibleConversations(_ botId: String, entry: Entry? = nil) {
        guard connection == .online else { return }
        guard readingViews.values.contains(botId), entry?.isChat != false else { return }
        markRead(botId)
    }

    /// The chat on screen is read; the host ignores it when nothing there is unread.
    public func markRead(_ botId: String) {
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
                try await client.markRead(botId)
            } catch {
                // The visible scope is acknowledged again when the link recovers.
                log.debug("read acknowledgement deferred: \(error.localizedDescription)")
            }
        }
    }

    /// The roster's "Mark as read": everything in the chat.
    public func markAllRead(_ botId: String) {
        guard (bots[botId]?.unread ?? 0) > 0 else { return }
        bots[botId]?.unread = 0
        perform(replay: true) { try await $0.markRead(botId) }
    }

    public func setPinned(_ bot: Bot, _ pinned: Bool) {
        var d = BotDraft(bot)
        d.pinned = pinned
        bots[bot.id]?.pinned = pinned
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// Makes `bot` the lead, in place of the one before; a refusal gives the star back.
    public func setLead(_ bot: Bot) {
        let before = bots.values.first(where: \.lead)?.id
        let show = { (lead: String?) in for id in self.bots.keys { self.bots[id]?.lead = id == lead } }
        show(bot.id)
        Task { await attempt(replay: true, undo: { show(before) }) { try await $0.setLead(bot.id) } }
    }

    public func setHidden(_ bot: Bot, _ hidden: Bool) {
        var d = BotDraft(bot)
        d.hidden = hidden
        bots[bot.id]?.hidden = hidden
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// Whether the bot may see and use the computer's screen.
    public func setComputer(_ bot: Bot, _ computer: Bool) {
        var d = BotDraft(bot)
        d.computer = computer
        bots[bot.id]?.computer = computer
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// Whether the bot uses the memory all bots share.
    public func setSharedMemory(_ bot: Bot, _ shared: Bool) {
        var d = BotDraft(bot)
        d.sharedMemory = shared
        bots[bot.id]?.sharedMemory = shared
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// Whether the bot's harness keeps its own memory on beside Codync's.
    public func setHarnessMemory(_ bot: Bot, _ on: Bool) {
        var d = BotDraft(bot)
        d.harnessMemory = on
        bots[bot.id]?.harnessMemory = on
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// Saves the user's version of a handoff that hasn't been sent. Whether it did.
    public func editHandoff(_ entry: Entry, text: String) async -> Bool {
        do {
            try await withLink(replay: true) { try await $0.editHandoff(entryId: entry.id, text: text) }
            return true
        } catch {
            lastError = error.localizedDescription
            return false
        }
    }

    /// Carries on with the bot's current conversation instead of the handoff.
    public func keepConversation(_ entry: Entry) {
        answer(entry, "handoff") { try await $0.keepConversation(entryId: entry.id) }
    }

    /// Runs a failed routine run's work again, from its notice; the notice's update drops the button.
    public func retryRoutineRun(_ entry: Entry) {
        guard let runId = entry.data.runId else { return }
        answer(entry, "retry") { try await $0.retryRoutineRun(botId: entry.botId, runId: runId) }
    }

    public func setFastMode(_ bot: Bot, _ fastMode: Bool) {
        var d = BotDraft(bot)
        d.fastMode = fastMode
        bots[bot.id]?.fastMode = fastMode
        perform(replay: true) { _ = try await $0.updateBot(d) }
    }

    /// The composer's `/fast`: the bot's Fast mode flips; in a group, every member that shows it
    /// goes to the opposite of the group's state.
    public func toggleFastMode(_ bot: Bot) {
        guard bot.isGroup else { return setFastMode(bot, !bot.fastMode) }
        let group = GroupFastMode(members(of: bot))
        for member in group.members { setFastMode(member, group.state == .off) }
    }

    // MARK: roster order

    /// Puts the roster in `ids` order (the lead and pinned bots still sort first) and sends it; the host
    /// syncs it to every device. The order it already has sends nothing.
    public func reorder(_ ids: [String]) {
        guard ids != roster.map(\.id) else { return }
        for (position, id) in ids.enumerated() { bots[id]?.position = position }
        perform(replay: true) { try await $0.reorderBots(ids) }
    }

    /// One place up (-1) or down (+1) among the bots it moves with (`isRosterPeer`: its band and, below the top, its section; VoiceOver).
    public func move(_ botId: String, by offset: Int) {
        guard let bot = bots[botId] else { return }
        var peers = roster.filter { $0.isRosterPeer(of: bot) }.map(\.id)
        guard let i = peers.firstIndex(of: botId), peers.indices.contains(i + offset) else { return }
        peers.swapAt(i, i + offset)
        drop(botId, in: peers)
    }

    /// A drop of `botId` where `order` (the rows on screen, top first) puts it among the bots it
    /// moves with; every other bot keeps its place. Dropped among others (another section, the
    /// pinned bots), it goes first or last of its own. Nothing moves unless `order` holds all of them.
    public func drop(_ botId: String, in order: [String]) {
        guard let bot = bots[botId] else { return }
        let ids = roster.map(\.id)
        let isPeer = { (id: String) in self.bots[id]?.isRosterPeer(of: bot) == true }
        let placed = order.filter(isPeer)
        guard Set(placed) == Set(ids.filter(isPeer)) else { return }
        var next = placed.makeIterator()
        reorder(ids.map { isPeer($0) ? next.next() ?? $0 : $0 })
    }

    // MARK: sections

    /// Puts the bot in `section` (nil: out of its own), shown at once and taken back if the
    /// computer refuses the name (it allows 40 characters, no control characters).
    public func setSection(_ bot: Bot, to section: String?) {
        let moves = show([bot.id], in: section)
        Task { await send(moves) { try await $0.setSection(bot.id, to: section) } }
    }

    /// Moves every bot of this computer in section `name` to `newName` (a name already in use
    /// merges the two); nil removes the section, its bots staying without one. Whether the
    /// computer took it: a refusal is taken back and shown.
    @discardableResult
    public func renameSection(from name: String, to newName: String?) async -> Bool {
        let moves = show(bots.values.filter { $0.section == name }.map(\.id), in: newName)
        return await send(moves) { try await $0.renameSection(from: name, to: newName) }
    }

    /// Shows the bots `ids` in `section` before the computer confirms it (its `bot` events follow),
    /// each move kept as pending until it's answered.
    private func show(_ ids: [String], in section: String?) -> [SectionMove] {
        let moves = ids.map { SectionMove(bot: $0, from: bots[$0]?.section, to: section) }
        pendingSectionMoves += moves
        Motion.animate { for id in ids { bots[id]?.section = section } }
        return moves
    }

    /// Sends `op` for the `moves` shown; a refusal takes back each one nothing changed since.
    private func send(_ moves: [SectionMove], _ op: @MainActor (HostClient) async throws -> Void) async -> Bool {
        let ids = Set(moves.map(\.id))
        defer { pendingSectionMoves.removeAll { ids.contains($0.id) } }
        return await attempt(replay: true, undo: {
            let back = RosterSection.takeBack(ids, pending: &pendingSectionMoves, sections: bots.compactMapValues(\.section))
            Motion.animate { for (id, section) in back { bots[id]?.section = section } }
        }, op)
    }

    public func delete(_ bot: Bot) {
        removeComposerDrafts(for: bot.id)
        bots[bot.id] = nil
        entries[bot.id] = nil
        perform(replay: true) { try await $0.deleteBot(bot.id) }
    }

    /// Creates a group chat (or opens the one these bots already share) and selects it.
    public func createGroup(name: String, description: String = "", members: [String]) async throws -> Bot {
        let group = putHostBot(try await ready().createGroup(GroupDraft(name: name, description: description, members: members)))
        selection = group.id
        return group
    }

    public func updateGroup(_ draft: GroupDraft) async throws {
        putHostBot(try await ready().updateGroup(draft))
    }

    /// What a voice call carried, for its conversation. Never `upsert`ed, like `botConversation`.
    public func callEntries(_ botId: String, call: String) async throws -> [Entry] {
        try await ready().callEntries(botId: botId, call: call)
    }

    /// The bot-to-bot notices between a chat and a peer. Never `upsert`ed: old notices in the
    /// mirror would break `loadOlder`, which pages from the oldest loaded `seq`.
    public func botConversation(_ botId: String, peer: String) async throws -> [Entry] {
        try await ready().botConversation(botId: botId, peerId: peer)
    }

    public func save(_ draft: BotDraft) async throws -> Bot {
        let client = try await ready()
        return putHostBot(draft.id == nil ? try await client.createBot(draft) : try await client.updateBot(draft))
    }

    public func selectWorkspace(_ path: String) async throws -> String {
        try await ready().selectWorkspace(path)
    }

    public func listDirs(_ path: String?) async throws -> DirListing {
        try await ready().listDirs(path)
    }

    public func refreshUsage() async {
        guard !retired, let client else { return }
        if let u = try? await client.usage(refresh: true), !Task.isCancelled { setUsage(u) }
    }

    /// Reads the network again; the host asks the other computers when what it heard is stale, and
    /// the `network` event brings what they answer.
    public func refreshNetwork() async {
        guard !retired, let client else { return }
        if let view = try? await client.networkView(), !Task.isCancelled { network = view }
    }

    public func loadOlder(_ botId: String) async {
        guard let client, !historyComplete.contains(botId) else { return }
        let first = chat(botId).first { $0.seq > 0 && $0.seq != Int64.max }?.seq ?? Int64.max
        guard let older = try? await client.history(botId: botId, beforeSeq: first, limit: 100) else { return }
        if older.count < 100 { historyComplete.insert(botId) }
        for e in older { upsert(e) }
    }
}
