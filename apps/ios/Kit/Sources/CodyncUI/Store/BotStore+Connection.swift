import CodyncKit
import Foundation

extension BotStore {
    /// Permanently detach a store (account switch, computer removed). Late async
    /// callbacks still hold it, but it writes nothing and calls no hooks anymore.
    public func retire() {
        let calls = Array(voiceCalls.values)
        voiceCalls.removeAll()
        setActive(false)
        retired = true
        fileDownloads.retire()
        for call in calls { call.end() }
        saveTask?.cancel()
        dropTimer?.cancel()
        onConnected = nil
        onBotUpdated = nil
        onUsageChanged = nil
        onSent = nil
        onRosterChanged = nil
        onComputerChanged = nil
        client = nil
        selection = nil
        screenRequest = nil
        showProfile = false
        showPlugins = false
    }

    public func setActive(_ active: Bool) {
        guard !retired else { return }
        isActive = active
        if active {
            // Returning to the foreground must not interrupt a live call's sends.
            if voiceCalls.isEmpty || streamTask == nil { restartStream() }
        } else if voiceCalls.isEmpty {
            // Disconnect so the host knows we're gone and sends pushes instead.
            stopTransport()
            saveCache()
        }
    }

    public func restartStream() {
        guard isActive || !voiceCalls.isEmpty, !retired else { return }
        stopTransport()
        if connection != .online { setConnection(.connecting) }
        streamTask = Task { [weak self] in await self?.runStream() }
    }

    /// Audio owns this subscription, so background calls don't depend on SwiftUI updates.
    func beginVoiceCall(_ id: UUID, botId: String, speak: @escaping @MainActor (String) -> Void,
                        announce: (@MainActor (String) -> Void)? = nil, end: @escaping @MainActor () -> Void) {
        guard !retired, voiceCalls[id] == nil else { return }
        voiceCalls[id] = VoiceCall(botId: botId, startRev: rev, speak: speak, announce: announce ?? speak, end: end)
        if streamTask == nil { restartStream() }
    }

    func endVoiceCall(_ id: UUID) {
        guard voiceCalls.removeValue(forKey: id) != nil else { return }
        if !isActive && voiceCalls.isEmpty {
            stopTransport()
            saveCache()
        }
    }

    private func stopTransport() {
        streamTask?.cancel()
        streamTask = nil
        eventsTask?.cancel()
        eventsTask = nil
        dropTimer?.cancel()
        dropTimer = nil
        heldDrop = nil
        if let remote = transport as? any RemoteTransport {
            Task { await remote.shutdown() }
        }
        transport = nil
    }

    private func runStream() async {
        var backoff: Double = 1
        var transport: any HostTransport
        while true {
            do {
                transport = try await makeTransport()
                break
            } catch HostError.unauthorized(let message) {
                if !Task.isCancelled, !retired { setConnection(.unauthorized(message)) }
                return
            } catch {
                guard !Task.isCancelled, !retired else { return }
                setConnection(.offline(error.localizedDescription))
                try? await Task.sleep(for: .seconds(backoff))
                backoff = min(backoff * 2, 30)
                guard !Task.isCancelled else { return }
            }
        }
        guard !Task.isCancelled, !retired else {
            if let remote = transport as? any RemoteTransport { await remote.shutdown() }
            return
        }
        self.transport = transport
        let client = HostClient(transport: transport)
        self.client = client
        if let remote = transport as? any RemoteTransport { watch(remote) }
        for await state in transport.states() {
            guard !Task.isCancelled, !retired else { return }
            switch state {
            case let .ready(route):
                setHostRoute(route)
                if eventsTask == nil {
                    if connection != .online { setConnection(.connecting) }
                    eventsTask = Task { [weak self] in await self?.runEvents(client) }
                    // Mailbox outcomes that happened while this app wasn't listening (§10.2).
                    if let remote = transport as? any RemoteTransport {
                        Task { [weak self] in await self?.reconcileQueued(remote) }
                    }
                }
                onConnected?(self)
            case .connecting:
                stopEvents()
                setConnection(.connecting)
            case let .hostOffline(lastSeen):
                stopEvents()
                setConnection(.computerOffline(lastSeen: lastSeen))
                if let remote = transport as? any RemoteTransport {
                    Task { [weak self] in await self?.reconcileQueued(remote) }
                }
            case let .unauthorized(message):
                stopEvents()
                setConnection(.unauthorized(message))
            case let .failed(message):
                stopEvents()
                setConnection(.offline(message))
            }
        }
    }

    /// Connection changes animate wherever they show (banners, headers, captions, rows).
    /// Give initial connection failures a second to recover and online drops five seconds.
    /// Relay presence can lag a reconnect, so "computer offline" gets the same grace.
    /// Authorization failures still show immediately.
    func setConnection(_ new: Connection) {
        let transient = switch new {
        case .connecting, .offline, .computerOffline: true
        default: false
        }
        let recovering = new == .online && (connection != .online || heldDrop != nil)
        let initialFailure = connection == .connecting && transient && new != .connecting
        if transient && (connection == .online || initialFailure) {
            let grace = connection == .online ? Self.dropGrace : Self.initialConnectionGrace
            heldDrop = new
            if dropTimer == nil {
                dropTimer = Task { [weak self] in
                    try? await Task.sleep(for: grace)
                    guard !Task.isCancelled, let self, let held = self.heldDrop else { return }
                    self.dropTimer = nil
                    self.heldDrop = nil
                    Motion.animate { self.connection = held }
                }
            }
            return
        }
        dropTimer?.cancel()
        dropTimer = nil
        heldDrop = nil
        if new != connection { Motion.animate { connection = new } }
        if recovering {
            for scope in Set(readingViews.values) { markRead(scope.botId, thread: scope.thread) }
        }
    }

    private func setHostRoute(_ new: HostRoute?) {
        guard new != hostRoute else { return }
        Motion.animate { hostRoute = new }
    }

    private func stopEvents() {
        eventsTask?.cancel()
        eventsTask = nil
        setHostRoute(nil)
    }

    /// The channel's side streams: merged computer info and mailbox outcomes.
    private func watch(_ remote: any RemoteTransport) {
        let updates = remote.computerUpdates()
        let mailbox = remote.mailboxEvents()
        Task { [weak self] in
            for await computer in updates {
                guard let self, !self.retired else { return }
                // The color is picked on this device; the transport's copy may be older.
                self.updateComputer { current in
                    let color = current.color
                    current = computer
                    current.color = color
                }
            }
        }
        Task { [weak self] in
            for await event in mailbox {
                guard let self, !self.retired else { return }
                self.applyMailbox(event)
            }
        }
    }
}
