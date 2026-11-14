import Foundation

// MARK: mailbox (§6.4, §7.4)

extension ChannelTransport {
    private func relaySocket() throws -> Link {
        guard let link, link.route == .relay else { throw HostError.unreachable }
        return link
    }

    /// `pasted`, as `send` takes it: what memory never takes for the user's own words.
    public func enqueue(botId: String, text: String, clientNonce: String, replyTo: String? = nil, pasted: [String]? = nil) async throws {
        guard pairingCode == nil, let bk = computer.boxKey.flatMap(Data.init(base64URL:)), bk.count == 32 else {
            throw HostError.computerOffline(lastSeen: lastSeen)
        }
        if case let .ready(route) = state, route == .direct { throw MailboxError.hostOnline }
        let link = try relaySocket()
        guard puts[clientNonce] == nil else { throw MailboxError.rejected("busy") }
        struct Send: Encodable { var botId: String; var text: String; var clientNonce: String; var replyTo: String?; var pasted: [String]? }
        struct Inner: Encodable { var m = "send"; var b: Send; var ts: Int64 }
        let send = Send(botId: botId, text: text, clientNonce: clientNonce, replyTo: replyTo, pasted: pasted?.isEmpty == false ? pasted : nil)
        let inner = try JSONEncoder().encode(Inner(b: send, ts: Int64(Date.now.timeIntervalSince1970 * 1000)))
        // A fresh ephemeral key on every seal, retries included (§6.4 MUST).
        let sealed = try RelayCrypto.sealMailbox(inner, hostBoxKey: bk, computerId: computer.id, deviceKey: identity.deviceKey,
                                                 clientNonce: clientNonce, sign: identity.sign)
        struct Put: Encodable { var t = "mbox.put"; var nonce: String; var d: String }
        let put = String(decoding: try JSONEncoder().encode(Put(nonce: clientNonce, d: sealed.blob.base64URL)), as: UTF8.self)
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Void, Error>) in
            puts[clientNonce] = c
            link.outbox.yield(put)
            Task {
                try? await Task.sleep(for: .seconds(15))
                self.expirePut(clientNonce)
            }
        }
    }

    private func expirePut(_ nonce: String) {
        puts.removeValue(forKey: nonce)?.resume(throwing: HostError.unreachable)
    }

    public func cancelQueued(clientNonce: String) async -> MailboxCancel {
        guard let link = try? relaySocket(), cancels[clientNonce] == nil else { return .unknown }
        let message = String(decoding: (try? JSONSerialization.data(withJSONObject: ["t": "mbox.cancel", "nonce": clientNonce])) ?? Data(), as: UTF8.self)
        return await withCheckedContinuation { c in
            cancels[clientNonce] = c
            link.outbox.yield(message)
            Task {
                try? await Task.sleep(for: .seconds(10))
                self.expireCancel(clientNonce)
            }
        }
    }

    private func expireCancel(_ nonce: String) {
        cancels.removeValue(forKey: nonce)?.resume(returning: .unknown)
    }

    public func listQueued() async throws -> [QueuedItem] {
        let link = try relaySocket()
        let key = UUID()
        return try await withCheckedThrowingContinuation { (c: CheckedContinuation<[QueuedItem], Error>) in
            lists[key] = c
            link.outbox.yield(#"{"t":"mbox.list"}"#)
            Task {
                try? await Task.sleep(for: .seconds(10))
                self.expireList(key)
            }
        }
    }

    private func expireList(_ key: UUID) {
        lists.removeValue(forKey: key)?.resume(throwing: HostError.unreachable)
    }
}
