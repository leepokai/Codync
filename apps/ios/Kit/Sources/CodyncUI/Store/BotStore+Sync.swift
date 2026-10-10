import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

extension BotStore {
    /// Events (catch-up since `rev`, then live) for as long as the link stays ready.
    func runEvents(_ client: HostClient) async {
        var backoff: Double = 1
        while !Task.isCancelled {
            do {
                try await refreshHello(client)
                guard !Task.isCancelled, !retired else { return }
                if mismatch != nil {
                    // Reached, but nothing to sync until one side updates. A host update restarts
                    // the link (and this loop) anyway; this catches one that doesn't.
                    setConnection(.online)
                    try await Task.sleep(for: Self.mismatchRecheck)
                    continue
                }
                requestedSince = rev
                windowFloors = [:]
                for try await event in client.events(since: requestedSince, client: clientKind) {
                    guard !Task.isCancelled, !retired else { return }
                    setConnection(.online)
                    backoff = 1
                    apply(event)
                }
            } catch is CancellationError {
                return
            } catch {
                if Task.isCancelled || retired { return }
                log.info("stream ended: \(error.localizedDescription)")
                if case let HostError.unauthorized(message) = error {
                    setConnection(.unauthorized(message))
                    return
                }
            }
            try? await Task.sleep(for: .seconds(backoff))
            backoff = min(backoff * 2, 20)
        }
    }

    /// Asked on every (re)connect: the host may have been updated while this app was away.
    private func refreshHello(_ client: HostClient) async throws {
        let h: Hello
        do {
            h = try await client.hello()
        } catch let error as DecodingError {
            // Its version and `minApp` still say which side to update.
            let version: HostVersion = try await client.call("hello")
            guard !Task.isCancelled, !retired else { return }
            log.error("undecodable hello from host \(version.version)")
            noteHostVersion(version)
            Motion.animate { unreadable = true }
            if mismatch == nil { throw error }
            return
        }
        guard !Task.isCancelled, !retired else { return }
        // hello first: whatever the version change shows (notices, reminders) reads it.
        hello = h
        noteHostVersion(HostVersion(version: h.version, minApp: h.minApp))
    }

    private func noteHostVersion(_ new: HostVersion) {
        let cached = cacheStamp.map { $0 != "\(Self.appBuild)/\(new.version)" } ?? false
        if cached || (hostVersion.map { $0.version != new.version } ?? false) {
            // A different host version or app build: data may carry new fields, so fetch it all again.
            refetchAll()
            rewound = false
            unreadable = false
        }
        cacheStamp = nil
        // An update notice may appear or go away with it.
        if new != hostVersion { Motion.animate { hostVersion = new } }
    }

    private func apply(_ event: HostEvent) {
        switch event {
        case let .hello(id, hostRev, newUsage, newScreen):
            if let hostId, hostId != id {
                // Different host database: start over.
                resetMirror()
            }
            hostId = id
            setUsage(newUsage)
            screen = newScreen
            let hostRewound = hostRev < rev
            if hostRewound { rev = 0 }
            windowFloors = requestedSince > 0 && !hostRewound ? loadedFloors() : [:]
            catchUpRev = hostRev
            bump(rev)
        case let .bot(bot):
            let neededInput = bots[bot.id]?.needsInput == true
            bots[bot.id] = bot
            bump(bot.rev)
            onBotUpdated?(bot)
            onRosterChanged?()
            if bot.unread > 0 { acknowledgeVisibleConversations(bot.id) }
            if bot.needsInput && !neededInput {
                for call in Array(voiceCalls.values) where call.botId == bot.id {
                    call.announce("\(bot.name) needs your approval in the chat.")
                }
            }
        case let .botDeleted(id, r):
            removeComposerDrafts(for: id)
            bots[id] = nil
            entries[id] = nil
            windowFloors[id] = nil
            bump(r)
            if selection == id { selection = nil }
            onRosterChanged?()
        case let .entry(e):
            if Self.isOutsideLoadedWindow(e, floor: windowFloors[e.botId], held: entries[e.botId]?.contains { $0.id == e.id } == true) {
                bump(e.rev)
                break
            }
            let known = entries[e.botId]?.first(where: { $0.id == e.id })
            upsert(e)
            bump(e.rev)
            acknowledgeVisibleConversations(e.botId, entry: e)
            let calls = Array(voiceCalls.values).filter { $0.botId == e.botId && e.rev > $0.startRev }
            if e.kind == "agent", e.threadId == nil, e.data.final == true, known?.data.final != true, let text = e.data.text {
                calls.forEach { $0.speak(spokenReply(e, text)) }
            }
            // A group has no status of its own: its members' approvals arrive as cards.
            if e.kind == "permission", known == nil, e.data.status == "pending", bots[e.botId]?.isGroup == true {
                let name = e.data.author.flatMap { bots[$0]?.name } ?? "A bot"
                calls.forEach { $0.announce("\(name) needs your approval in the chat.") }
            }
        case let .usage(u):
            setUsage(u)
        case let .screen(s):
            screen = s
        case .accessRequests, .cloud:
            break // The computer's own screens only (loopback).
        case .resync:
            restartEvents()
        case let .undecodable(type):
            // Never skip past data we couldn't read: rewind once and fetch everything again.
            log.error("undecodable \(type) event")
            if !rewound {
                rewound = true
                refetchAll()
                restartEvents()
            } else if !unreadable {
                Motion.animate { unreadable = true }
                // A newer host this app can't follow: stop syncing and ask for an update.
                if mismatch != nil { restartEvents() }
            }
        }
        scheduleSave()
    }

    /// In a group, a reply is said with its speaker's name.
    private func spokenReply(_ e: Entry, _ text: String) -> String {
        guard bots[e.botId]?.isGroup == true, let name = e.data.author.flatMap({ bots[$0]?.name }) else { return text }
        return "\(name): \(text)"
    }

    private func resetMirror() {
        composerDrafts = [:]
        persistComposerDrafts()
        bots = [:]
        entries = entries.mapValues { $0.filter { $0.id.hasPrefix("local-") } }.filter { !$0.value.isEmpty }
        selection = nil
        onRosterChanged?()
        rev = 0
        hostId = nil
        historyComplete = []
        windowFloors = [:]
        screen = nil
        saveCache()
    }

    /// Fetches everything again from rev 0. That catch-up brings only each chat's newest entries, so
    /// the main-chat ones held now go too: an older one kept would hide the gap from `loadOlder`, which
    /// pages from the oldest one held. Threads (loaded whole when opened) and messages still sending stay.
    private func refetchAll() {
        rev = 0
        entries = entries.mapValues { $0.filter { $0.threadId != nil || $0.id.hasPrefix("local-") } }
            .filter { !$0.value.isEmpty }
        historyComplete = []
    }

    /// Resubscribes from the current `rev` on the same link.
    private func restartEvents() {
        guard let client, eventsTask != nil else { return }
        eventsTask?.cancel()
        eventsTask = Task { [weak self] in await self?.runEvents(client) }
    }

    private func bump(_ r: Int64) {
        rev = max(rev, r)
        if !caughtUp && rev >= catchUpRev { caughtUp = true }
    }

    /// Per bot, the lowest synced main-chat `seq` in the mirror (optimistic entries don't count).
    /// Only entries the mirror had at `requestedSince`: newer ones (a send's RPC response landing
    /// before this hello) come again in this catch-up and say nothing about the window.
    private func loadedFloors() -> [String: Int64] {
        entries.compactMapValues { list in
            list.filter { $0.threadId == nil && $0.seq > 0 && $0.seq != .max && $0.rev <= requestedSince }.map(\.seq).min()
        }
    }

    /// An entry event the mirror doesn't hold, in the main chat, below the floor the mirror had
    /// when this connection started: it existed before the window and arrives only because its
    /// rev was bumped (a rewrite, a reaction). Inserting it would hide the gap from `loadOlder`,
    /// which pages from the oldest loaded `seq`; history paging brings it. `seq` follows insert
    /// order, so anything created since the mirror was last synced sits above the floor.
    static func isOutsideLoadedWindow(_ e: Entry, floor: Int64?, held: Bool) -> Bool {
        guard e.threadId == nil, !held, let floor else { return false }
        return e.seq < floor
    }

    func upsert(_ e: Entry) {
        var list = entries[e.botId] ?? []
        if let i = list.firstIndex(where: { $0.id == e.id }) {
            // A late API response mustn't undo a newer SSE update.
            guard e.rev >= list[i].rev else { return }
            list[i] = e
        } else {
            // Replace the optimistic copy of a message we sent.
            if e.kind == "user", let nonce = e.data.clientNonce, !nonce.isEmpty {
                list.removeAll { $0.id == "local-\(nonce)" }
            }
            if let last = list.last, last.seq > e.seq, e.seq > 0 {
                let i = list.firstIndex { $0.seq > e.seq } ?? list.endIndex
                list.insert(e, at: i)
            } else {
                list.append(e)
            }
        }
        entries[e.botId] = list
    }

    func setUsage(_ u: Usage) {
        guard u != usage, !retired else { return }
        usage = u
        storage.usage[computer.id] = u
        onUsageChanged?(computer.id, u)
    }
}
