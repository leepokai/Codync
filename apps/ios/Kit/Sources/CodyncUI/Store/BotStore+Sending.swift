import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

extension BotStore {
    func composerDraft(for botId: String) -> String {
        composerDrafts[botId] ?? ""
    }

    func setComposerDraft(_ text: String, for botId: String) {
        guard !retired else { return }
        if text.isEmpty {
            composerPasted[botId] = nil
        } else if let pasted = pastedInsertion(from: composerDrafts[botId] ?? "", to: text) {
            composerPasted[botId, default: []].append(pasted)
        }
        composerDrafts[botId] = text.isEmpty ? nil : text
        persistComposerDrafts()
    }

    /// Drafts saved before 2.20.1 were keyed `["<bot>",null]`; a thread's (`["<bot>","<root>"]`) is dropped.
    static func chatDrafts(_ saved: [String: String]) -> [String: String] {
        saved.reduce(into: [:]) { drafts, row in
            guard let old = try? JSONDecoder().decode([String?].self, from: Data(row.key.utf8)) else {
                drafts[row.key] = row.value
                return
            }
            if old.count == 2, let bot = old[0], old[1] == nil { drafts[bot] = row.value }
        }
    }

    func persistComposerDrafts() {
        storage.composerDrafts[computer.id] = composerDrafts.isEmpty ? nil : composerDrafts
    }

    /// The chat's draft, and the drafts of its artifacts (`artifactDraftKey`).
    func removeComposerDrafts(for botId: String) {
        composerDrafts = composerDrafts.filter { $0.key != botId && !$0.key.hasPrefix("\(botId)/") }
        persistComposerDrafts()
    }

    /// An artifact viewer's message box keeps its own draft, apart from the chat's.
    static func artifactDraftKey(_ artifact: Artifact) -> String { "\(artifact.botId)/artifact/\(artifact.id)" }

    /// Transfer to the outbox now; an eventual acknowledgement must not erase a newer draft.
    /// `draft`: whose draft it is (the chat's when nil). `replyTo`: the chat message it quotes.
    @discardableResult
    func sendComposerDraft(to botId: String, draft key: String? = nil, replyTo: ReplyQuote? = nil,
                           files: [OutgoingFile] = []) -> Bool {
        let key = key ?? botId
        let text = composerDraft(for: key)
        guard !retired, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !files.isEmpty else { return false }
        let pasted = composerPasted[key] ?? []
        send(text, to: botId, replyTo: replyTo, nonce: UUID().uuidString, files: files, pasted: pasted)
        setComposerDraft("", for: key)
        return true
    }

    public func send(_ text: String, to botId: String, files: [OutgoingFile] = []) {
        send(text, to: botId, replyTo: nil, nonce: UUID().uuidString, files: files)
    }

    /// Said on voice call `call` (on-device or speech to text): the chat shows it in the call's conversation.
    public func send(_ text: String, to botId: String, call: String) {
        send(text, to: botId, replyTo: nil, nonce: UUID().uuidString, call: call)
    }

    @discardableResult
    func send(_ text: String, to botId: String, replyTo: ReplyQuote?, nonce: String, files: [OutgoingFile] = [],
              pasted: [String] = [], call: String? = nil) -> Task<SendProgress, Never>? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty || !files.isEmpty, !retired else { return nil }
        lastSentNonce = nonce
        var data = EntryData(text: trimmed, status: "sending", clientNonce: nonce)
        data.call = call
        data.pasted = pasted.isEmpty ? nil : pasted
        // Shown at once; the computer's own quote replaces it.
        data.replyTo = replyTo
        if !files.isEmpty {
            data.attachments = files.map { Attachment(id: $0.id.uuidString, name: $0.name, size: Int64($0.data.count)) }
            outgoingFiles[nonce] = files
        }
        let local = Entry(
            id: "local-\(nonce)", seq: Int64.max, botId: botId, rev: 0, kind: "user",
            turn: 0, data: data, createdAt: Int64(Date.now.timeIntervalSince1970 * 1000), updatedAt: 0
        )
        upsert(local)
        storage.lastComputerId = computer.id
        if let bot = bots[botId] { onSent?(bot, .sending) }
        // The relay mailbox holds text only: files, and the call a message was said in, wait for the computer.
        if canQueue, files.isEmpty, call == nil, let remote = transport as? any RemoteTransport {
            return enqueue(remote, text: trimmed, botId: botId, replyTo: replyTo?.id, nonce: nonce, pasted: pasted)
        }
        return deliver(trimmed, botId: botId, replyTo: replyTo?.id, nonce: nonce, pasted: pasted, call: call)
    }

    private func deliver(_ text: String, botId: String, replyTo: String?, nonce: String, pasted: [String] = [],
                         call: String? = nil) -> Task<SendProgress, Never> {
        Task {
            do {
                // Safe to repeat: the computer skips a nonce it already has.
                let e = try await withLink(replay: true) { client in
                    var ids: [String]?
                    if let files = self.outgoingFiles[nonce] {
                        let uploaded = try await client.upload(botId: botId, files: files)
                        for (id, file) in zip(uploaded, files) { Self.cacheAttachment(id, file.data) }
                        ids = uploaded
                    }
                    try Task.checkCancellation()
                    return try await client.send(botId: botId, text: text, clientNonce: nonce, replyTo: replyTo, attachments: ids,
                                                 pasted: pasted, call: call)
                }
                outgoingFiles[nonce] = nil
                upsert(e)
                // Only once it arrived, so a resend counts once.
                if call == nil { onPromptSent() }
                if let bot = bots[botId] { onSent?(bot, .delivered) }
                return .delivered
            } catch {
                if outgoingFiles[nonce] != nil { lastError = "Couldn't send the files: \(error.localizedDescription)" }
                // With no relay holding it, a message waits here until the computer can be reached (`sendHeld`).
                let holds = computer.cloud == nil && (error as? HostError)?.isTransient == true
                markLocal(nonce: nonce, botId: botId, status: holds ? "held" : "failed")
                return holds ? .queued : .failed
            }
        }
    }

    /// The computer is offline: the message waits, sealed for it, in the relay mailbox.
    private func enqueue(_ remote: any RemoteTransport, text: String, botId: String, replyTo: String?, nonce: String,
                         pasted: [String]) -> Task<SendProgress, Never> {
        markLocal(nonce: nonce, botId: botId, status: "waiting")
        saveCache()
        return Task {
            defer { saveCache() }
            do {
                try Task.checkCancellation()
                try await remote.enqueue(botId: botId, text: text, clientNonce: nonce, replyTo: replyTo, pasted: pasted)
                onPromptSent()
                if let bot = bots[botId] { onSent?(bot, .queued) }
                return .queued
            } catch MailboxError.hostOnline {
                // Presence can race the stream state. Wait for the host's events channel
                // before trying the normal API, instead of turning that race into a failure.
                markLocal(nonce: nonce, botId: botId, status: "sending")
                return await deliverWhenOnline(text, botId: botId, replyTo: replyTo, nonce: nonce, pasted: pasted)
            } catch is CancellationError {
                // The shortcut that sent it was cancelled: the failed message says enough.
                markLocal(nonce: nonce, botId: botId, status: "failed")
                return .failed
            } catch {
                // A dropped relay link: the message's own "Failed to send" and Resend say enough.
                if (error as? HostError)?.isTransient != true { lastError = error.localizedDescription }
                markLocal(nonce: nonce, botId: botId, status: "failed")
                return .failed
            }
        }
    }

    private func deliverWhenOnline(_ text: String, botId: String, replyTo: String?, nonce: String, pasted: [String]) async -> SendProgress {
        let deadline = ContinuousClock.now + .seconds(30)
        while !retired, !Task.isCancelled, connection != .online, ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(200))
        }
        guard !retired, !Task.isCancelled else {
            markLocal(nonce: nonce, botId: botId, status: "failed")
            return .failed
        }
        guard connection == .online else {
            lastError = "The computer came online, but the message couldn't be sent. Retry it when the connection is ready."
            markLocal(nonce: nonce, botId: botId, status: "failed")
            saveCache()
            return .failed
        }
        let delivery = deliver(text, botId: botId, replyTo: replyTo, nonce: nonce, pasted: pasted)
        return await withTaskCancellationHandler {
            await delivery.value
        } onCancel: {
            delivery.cancel()
        }
    }

    /// Takes a waiting message back out of the mailbox, unless the computer already has it.
    public func cancelQueued(_ entry: Entry) {
        if computer.cloud == nil || entry.data.status == "held" {
            // No relay holds it: it only waits on this phone.
            discard(entry)
            if let bot = bots[entry.botId] { onSent?(bot, .cancelled) }
            saveCache()
            return
        }
        guard let nonce = entry.data.clientNonce, let remote = transport as? any RemoteTransport else { return }
        Task {
            switch await remote.cancelQueued(clientNonce: nonce) {
            case .cancelled:
                discard(entry)
                if let bot = bots[entry.botId] { onSent?(bot, .cancelled) }
            case .delivering:
                markLocal(nonce: nonce, botId: entry.botId, status: "delivering")
            case .unknown:
                lastError = "Couldn't take the message back. It may have been delivered already."
            }
            saveCache()
        }
    }

    func applyMailbox(_ event: MailboxEvent) {
        guard let entry = entries.values.lazy.flatMap({ $0 }).first(where: { $0.id == "local-\(event.nonce)" }) else { return }
        switch event {
        // The events stream's own copy replaces it (matched by clientNonce).
        case .delivered: markLocal(nonce: event.nonce, botId: entry.botId, status: "delivering")
        case .failed, .expired: markLocal(nonce: event.nonce, botId: entry.botId, status: "failed")
        }
        saveCache()
    }

    /// Messages the relay no longer holds were delivered, refused or expired while we were away.
    /// "Failed" is safe even if one was delivered: the events catch-up replaces it by clientNonce,
    /// and a resend reuses that nonce, so the computer never runs it twice.
    func reconcileQueued(_ remote: any RemoteTransport) async {
        let local = entries.values.flatMap { $0 }
        // What waits on this phone goes now, also when the computer has joined an account since. With no relay
        // now, what a relay held can't be asked about: it goes with the same nonce too.
        sendHeld(local.filter { $0.data.status == "held" || ($0.data.status == "waiting" && computer.cloud == nil) })
        let waiting = local.filter { $0.data.status == "waiting" }
        guard computer.cloud != nil, !waiting.isEmpty else { return }
        let relayItems: [QueuedItem]
        do {
            relayItems = try await remote.listQueued()
        } catch {
            log.debug("mailbox reconciliation deferred: \(error.localizedDescription)")
            return
        }
        let inRelay = Dictionary(relayItems.map { ($0.nonce, $0.state) }, uniquingKeysWith: { a, _ in a })
        guard !retired else { return }
        for e in waiting {
            guard let nonce = e.data.clientNonce else { continue }
            switch inRelay[nonce] {
            case "queued": break
            case .some: markLocal(nonce: nonce, botId: e.botId, status: "delivering")
            case nil: markLocal(nonce: nonce, botId: e.botId, status: "failed")
            }
        }
        saveCache()
    }

    /// The computer is back: what waited on this phone goes now, with the same nonces.
    private func sendHeld(_ held: [Entry]) {
        guard !held.isEmpty else { return }
        for entry in held.sorted(by: { $0.createdAt < $1.createdAt }) {
            if filesGone(entry), let nonce = entry.data.clientNonce {
                markLocal(nonce: nonce, botId: entry.botId, status: "failed")
            } else {
                retry(entry)
            }
        }
        saveCache()
    }

    /// Files are kept only in memory: a message whose files went with the app can't go again, or it would go half sent.
    private func filesGone(_ entry: Entry) -> Bool {
        entry.data.attachments?.isEmpty == false && entry.data.clientNonce.flatMap { outgoingFiles[$0] } == nil
    }

    public func retry(_ entry: Entry) {
        guard !filesGone(entry) else {
            lastError = "This message's files aren't on this device anymore. Delete it and attach them again."
            return
        }
        let files = entry.data.clientNonce.flatMap { outgoingFiles[$0] } ?? []
        guard let text = entry.data.text, !text.isEmpty || !files.isEmpty else { return }
        entries[entry.botId]?.removeAll { $0.id == entry.id }
        // The same clientNonce: if the computer got it after all, it won't run twice (the host
        // skips a nonce it has). Every seal still takes a fresh ephemeral key (§6.4).
        send(text, to: entry.botId, replyTo: entry.data.replyTo, nonce: entry.data.clientNonce ?? UUID().uuidString,
             files: files, pasted: entry.data.pasted ?? [], call: entry.data.call)
    }

    /// A sent file's bytes: on disk after the first fetch (sent files never change).
    public func attachmentData(_ file: Attachment, bot botId: String) async -> Data? {
        guard let url = Self.attachmentCache(file.id) else { return nil }
        if let data = try? Data(contentsOf: url) { return data }
        guard let client = live, let data = try? await client.readUpload(botId: botId, id: file.id) else { return nil }
        Self.cacheAttachment(file.id, data)
        return data
    }

    /// A file a bot shared, read whole and checked: on disk after the first fetch (shared files never change).
    public func sharedFileData(_ file: SharedFile, entryId: String) async -> Data? {
        guard let url = Self.attachmentCache(file.id) else { return nil }
        if let data = try? Data(contentsOf: url) { return data }
        guard let client = live, let data = try? await FileDownload().data(client: client, entryId: entryId, file: file) else { return nil }
        Self.cacheAttachment(file.id, data)
        return data
    }

    private static func attachmentCache(_ id: String) -> URL? {
        guard let uuid = UUID(uuidString: id) else { return nil }
        return URL.cachesDirectory.appending(path: "codync-attachments").appending(path: uuid.uuidString)
    }

    private static func cacheAttachment(_ id: String, _ data: Data) {
        guard let url = attachmentCache(id) else { return }
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url)
    }

    public func discard(_ entry: Entry) {
        if let nonce = entry.data.clientNonce { outgoingFiles[nonce] = nil }
        entries[entry.botId]?.removeAll { $0.id == entry.id }
        scheduleSave()
    }

    private func markLocal(nonce: String, botId: String, status: String) {
        guard var list = entries[botId], let i = list.firstIndex(where: { $0.id == "local-\(nonce)" }) else { return }
        list[i].data.status = status
        entries[botId] = list
        scheduleSave()
        if status == "failed", let bot = bots[botId] { onSent?(bot, .failed) }
    }
}
