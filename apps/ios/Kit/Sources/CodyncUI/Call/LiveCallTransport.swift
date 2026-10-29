import CodyncKit
import Foundation

/// Ordered forwarding between the Live data channel and the host. No tool or transcript logic runs here.
@MainActor
final class LiveCallTransport {
    typealias EventRequest = @MainActor (String, Data) async throws -> [Data]
    typealias UpdateRequest = @MainActor (String, CallCue.Kind, String) async throws -> [Data]
    private let eventRequest: EventRequest
    private let updateRequest: UpdateRequest
    private let send: (Data) -> Void
    private let ready: () -> Void
    private let failed: (String) -> Void
    private var queue: Task<Void, Never>?
    private var queued = 0
    private var stopped = false
    private var closing = false
    private var started = false
    private var pending: [(CallCue.Kind, String)] = []
    private(set) var sessionId: String?
    private(set) var isClosed = false

    static func isModel(_ model: String) -> Bool {
        model.hasPrefix("gpt-live-") && !model.contains("transcribe")
    }

    init(event: @escaping EventRequest, update: @escaping UpdateRequest,
         send: @escaping (Data) -> Void, ready: @escaping () -> Void, failed: @escaping (String) -> Void) {
        eventRequest = event
        updateRequest = update
        self.send = send
        self.ready = ready
        self.failed = failed
    }

    convenience init(link: CallLink, send: @escaping (Data) -> Void,
                     ready: @escaping () -> Void, failed: @escaping (String) -> Void) {
        self.init(event: { session, event in
            try await link.client().callLiveEvent(call: link.call, sessionId: session, event: event)
        }, update: { session, kind, text in
            try await link.client().callLiveUpdate(call: link.call, sessionId: session, kind: kind, text: text)
        }, send: send, ready: ready, failed: failed)
    }

    func attach(_ sessionId: String) { self.sessionId = sessionId }

    func receive(_ data: Data) {
        guard let message = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let type = message["type"] as? String else { return }
        if type == "session.closed" {
            isClosed = true
            if !closing && !stopped { failed("The GPT-Live session ended.") }
            return
        }
        guard !stopped, !closing else { return }
        if type == "error" || type == "session.error" {
            let error = message["error"] as? [String: Any]
            failed(error?["message"] as? String ?? "GPT-Live could not continue the call.")
            return
        }
        if type == "session.started" {
            guard !started else { return }
            started = true
            ready()
            let waiting = pending
            pending.removeAll()
            for (kind, text) in waiting { update(kind, text) }
            return
        }
        guard ["response.event", "session.input_transcript.delta", "session.output_transcript.delta",
               "session.delegation.created"].contains(type), let sessionId else { return }
        enqueue { [eventRequest] in try await eventRequest(sessionId, data) }
    }

    func update(_ kind: CallCue.Kind, _ text: String) {
        guard !stopped, !closing else { return }
        guard started, let sessionId else {
            if kind == .progress { pending.removeAll { $0.0 == .progress } }
            guard pending.count < 32 else { return failed("Too many voice updates are waiting for GPT-Live.") }
            pending.append((kind, text))
            return
        }
        enqueue { [updateRequest] in try await updateRequest(sessionId, kind, text) }
    }

    /// Stop accepting work immediately; a result from the old session can never reach its replacement.
    func stop() {
        stopped = true
        queue?.cancel()
        queue = nil
        pending.removeAll()
    }

    func requestClose() {
        guard !closing, !isClosed else { return }
        closing = true
        queue?.cancel()
        pending.removeAll()
        send(Data(#"{"type":"session.close"}"#.utf8))
    }

    private func enqueue(_ operation: @escaping @MainActor () async throws -> [Data]) {
        guard queued < 256 else { return failed("The computer cannot keep up with GPT-Live events.") }
        queued += 1
        let previous = queue
        queue = Task { [weak self] in
            await previous?.value
            guard let self else { return }
            defer { self.queued -= 1 }
            guard !Task.isCancelled, !self.stopped, !self.closing else { return }
            do {
                let commands = try await operation()
                guard !Task.isCancelled, !self.stopped, !self.closing else { return }
                for command in commands { self.send(command) }
            } catch {
                guard !Task.isCancelled, !self.stopped, !self.closing else { return }
                self.failed(error.localizedDescription)
            }
        }
    }
}
