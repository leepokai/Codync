import Foundation

// MARK: calls and streams

extension ChannelTransport {
    public nonisolated func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await perform(method, body: body, timeout: timeout)
    }

    private func nextId() -> UInt32 {
        nextRequestId = nextRequestId == .max ? 1 : nextRequestId + 1
        return nextRequestId
    }

    /// Waits (bounded) while connecting; throws what the link state means otherwise.
    func ready(within limit: TimeInterval) async throws {
        let deadline = ContinuousClock.now + .seconds(max(limit, 0))
        while true {
            if closed { throw HostError.unreachable }
            switch state {
            case .ready where link?.sealer != nil: return
            case .ready, .connecting:
                guard ContinuousClock.now < deadline else { throw HostError.unreachable }
                await stateChange(until: deadline)
            case let .hostOffline(lastSeen): throw HostError.computerOffline(lastSeen: lastSeen)
            case let .unauthorized(message): throw HostError.unauthorized(message)
            case .failed: throw needsUpgrade ? HostError.upgradeRequired : HostError.unreachable
            }
        }
    }

    func perform(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await ready(within: timeout)
        let b: Any = body.isEmpty ? NSNull() : try JSONSerialization.jsonObject(with: body, options: .fragmentsAllowed)
        let id = nextId()
        let payload = try JSONSerialization.data(withJSONObject: ["id": id, "m": method, "b": b] as [String: Any])
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (c: CheckedContinuation<Data, Error>) in
                calls[id] = c
                do {
                    try sendInner(payload)
                } catch {
                    calls.removeValue(forKey: id)?.resume(throwing: error)
                    return
                }
                Task {
                    try? await Task.sleep(for: .seconds(timeout))
                    self.abandon(id, error: HostError.unreachable)
                }
            }
        } onCancel: {
            Task { await self.abandon(id, error: CancellationError()) }
        }
    }

    /// Timed out or cancelled: the host drops the answer (an already started mutation still runs).
    private func abandon(_ id: UInt32, error: Error) {
        guard let c = calls.removeValue(forKey: id) else { return }
        c.resume(throwing: error)
        try? sendInner(JSONSerialization.data(withJSONObject: ["id": id, "cancel": true]))
    }

    private func sendInner(_ message: Data) throws {
        guard var sealer = link?.sealer, let link else { throw HostError.unreachable }
        let frames = try sealer.seal(message)
        self.link?.sealer = sealer
        for frame in frames {
            link.outbox.yield(#"{"t":"f","c":\#(frame.c),"d":"\#(frame.d.base64URL)"}"#)
        }
        // Counters must never wrap: replace the channel (§3.5).
        if sealer.exhausted {
            restartRequested = true
            link.socket.close(code: 4011)
        }
    }

    public nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> {
        let policy: AsyncThrowingStream<Data, Error>.Continuation.BufferingPolicy = if case .screenCandidates = request { .bufferingOldest(ScreenCandidate.maxCount + 2) } else { .unbounded }
        return AsyncThrowingStream(bufferingPolicy: policy) { continuation in
            let key = UUID()
            let task = Task { await self.openStream(request, key: key, continuation) }
            continuation.onTermination = { _ in
                task.cancel()
                Task { await self.closeStream(key) }
            }
        }
    }

    private func openStream(_ request: HostStreamRequest, key: UUID, _ continuation: AsyncThrowingStream<Data, Error>.Continuation) async {
        do {
            try await ready(within: 10)
            // The consumer left while we waited: `closeStream` already ran and found nothing to stop.
            try Task.checkCancellation()
            let id = nextId()
            let sub: [String: Any] = switch request {
            case let .events(since, client): ["id": id, "sub": "events", "b": ["since": since, "client": client]]
            case let .screenCandidates(session): ["id": id, "sub": "screenCandidates", "b": ["session": session]]
            case let .term(term): ["id": id, "sub": "term", "b": ["term": term]]
            }
            streams[id] = continuation
            streamIds[key] = id
            try sendInner(JSONSerialization.data(withJSONObject: sub))
        } catch {
            continuation.finish(throwing: error)
        }
    }

    func sendCancellation(_ id: UInt32) throws {
        try sendInner(JSONSerialization.data(withJSONObject: ["id": id, "cancel": true]))
    }

    /// The consumer went away: stop the host's stream.
    private func closeStream(_ key: UUID) {
        guard let id = streamIds.removeValue(forKey: key), streams.removeValue(forKey: id) != nil else { return }
        try? sendInner(JSONSerialization.data(withJSONObject: ["id": id, "cancel": true]))
    }
}
