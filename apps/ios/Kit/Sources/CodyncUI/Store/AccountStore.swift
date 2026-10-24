import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Account")

/// One bot across an account context's computers, for lists that mix them.
public struct RosterItem: Identifiable, Sendable {
    public var ref: BotReference
    public var bot: Bot
    public var computer: Computer
    public var id: BotReference { ref }
}

/// Account → computers → bots: one `BotStore` per computer this context can reach
/// (QR-paired or approved through the account),
/// plus the account's computers from the cloud that this device can still ask access to.
///
/// Multiple computers are a future direction, not a current priority: the product targets one
/// computer today. This structure stays because the relay, accounts and SSH are built on it and it
/// works unchanged with a single computer; don't extend or polish multi-computer features for now.
@MainActor
@Observable
public final class AccountStore {
    public let storage: SharedStore.Context
    public var accountId: String? { storage.accountID }
    public private(set) var computers: [Computer] = []
    public private(set) var stores: [ComputerID: BotStore] = [:]
    /// Everything in the account, including computers this device isn't authorized on yet.
    public private(set) var cloudComputers: [CloudComputer] = []
    /// Access requests waiting for approval on the computer, with the SAS to show.
    public private(set) var pendingAccess: [ComputerID: AccessTicket] = [:]
    /// The actual request approved most recently, distinct from a previously paired computer.
    public private(set) var approvedAccess: [ComputerID: String] = [:]
    public var lastError: String?
    public var selection: BotReference? {
        didSet {
            guard !retired else { return }
            if selection != oldValue {
                for store in stores.values { store.cancelVoiceCallRequest() }
            }
            if let id = selection?.computerId { storage.lastComputerId = id }
        }
    }

    public var onBotUpdated: (@MainActor (BotReference, Bot) -> Void)?
    public var onConnected: (@MainActor (BotStore) -> Void)?
    public var onSent: (@MainActor (BotReference, Bot, SendProgress) -> Void)?
    /// The combined roster changed: a bot came, changed or went, or a computer did.
    public var onRosterChanged: (@MainActor ([RosterItem]) -> Void)?

    private let cloud: CloudClient?
    private let computerOrder: ComputerOrderStore
    private let makeStore: @MainActor (Computer) -> BotStore
    /// Persisted in the context (channel route).
    private var saved: [Computer]
    private var accessPolls: [ComputerID: Task<Void, Never>] = [:]
    /// A list that comes back after a newer request was started is out of date.
    private var cloudListVersion = 0
    /// Access requests still waiting for their code, joined by a second ask.
    private var asks: [ComputerID: Task<AccessTicket, any Error>] = [:]
    private var isActive = true
    private var retired = false

    public convenience init(storage: SharedStore.Context, cloud: CloudClient?) {
        self.init(storage: storage, cloud: cloud) { computer in
            BotStore(computer: computer, storage: storage)
        }
    }

    init(storage: SharedStore.Context, cloud: CloudClient?,
         makeStore: @escaping @MainActor (Computer) -> BotStore) {
        self.storage = storage
        self.cloud = cloud
        computerOrder = ComputerOrderStore(storage: storage)
        self.makeStore = makeStore
        saved = storage.computers.filter(\.isConsistent)
        for computer in saved { open(computer) }
        computerOrder.onChanged = { [weak self] in self?.refreshList() }
        refreshList()
    }

    // MARK: derived

    /// Every computer's visible bots, computer by computer, each in its roster order.
    public var roster: [RosterItem] {
        computers.flatMap { computer -> [RosterItem] in
            guard let store = stores[computer.id] else { return [] }
            return store.roster.map {
                RosterItem(ref: BotReference(accountId: accountId, computerId: computer.id, botId: $0.id), bot: $0, computer: store.computer)
            }
        }
    }

    public func store(for id: ComputerID) -> BotStore? { stores[id] }

    // MARK: computers

    /// QR pairing (§4.1); the computer is saved in this context and connected.
    public func pair(_ pairing: Pairing, deviceName: String, platform: String) async throws -> Computer {
        let identity = try DeviceIdentity.load(context: storage)
        var computer = try await HostConnector.pair(pairing, identity: identity, deviceName: deviceName, platform: platform)
        guard !retired else { throw CancellationError() }
        // Picked on this device: re-pairing keeps them.
        if let previous = saved.first(where: { $0.id == computer.id }) {
            computer.color = previous.color
            computer.route = previous.route
        }
        save(computer)
        open(computer)
        return computer
    }

    public func forget(_ id: ComputerID) {
        saved.removeAll { $0.id == id }
        persist()
        close(id)
        accessPolls.removeValue(forKey: id)?.cancel()
        setPending(id, nil)
        if selection?.computerId == id { selection = nil }
        refreshList()
    }

    /// Saved with the computer on this device; its channel reconnects over the new order right away.
    public func setRoute(_ id: ComputerID, _ route: ConnectionRoute) {
        let value: ConnectionRoute? = route == .automatic ? nil : route
        if let i = saved.firstIndex(where: { $0.id == id }) {
            saved[i].route = value
            persist()
        }
        stores[id]?.updateComputer { $0.route = value }
        stores[id]?.restartStream()
        refreshList()
    }

    public func setColor(_ id: ComputerID, _ color: String) {
        if let i = saved.firstIndex(where: { $0.id == id }) {
            saved[i].color = color
            persist()
        }
        stores[id]?.updateComputer { $0.color = color }
        refreshList()
    }

    /// Moves a computer section on this device only.
    public func move(_ id: ComputerID, to target: ComputerID) {
        computerOrder.move(id, to: target, available: computers.map(\.id))
    }

    // MARK: account (§4.2 B)

    public func refreshCloud() async {
        guard let cloud, !retired else { return }
        cloudListVersion += 1
        let version = cloudListVersion
        do {
            let list = try await cloud.computers()
            guard !retired, version == cloudListVersion else { return }
            if list != cloudComputers { Motion.animate { cloudComputers = list } }
        } catch let error as CloudError where !error.isTransient {
            lastError = error.localizedDescription
        } catch {
            // Runs on every launch and foreground: a dropped connection, a busy cloud or a token
            // not ready yet fixes itself on the next refresh, so only answers that stay reach the dialog.
            log.info("cloud refresh failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Asks the computer for access; the code arrives in `pendingAccess`, where it must match the one
    /// the computer shows. The request runs here, not in the caller: it outlives a closed sheet, so the
    /// computer never waits on a request nobody reveals, and asking again while it runs joins it
    /// instead of committing a second one. Explicit renewal cancels and joins that request first.
    /// Approval is then picked up in the background and the computer joins `stores`.
    public func requestAccess(_ id: ComputerID, renew: Bool = false) async throws {
        if !renew, let ask = asks[id], !ask.isCancelled {
            _ = try await ask.value
            return
        }
        guard let cloud, let target = cloudComputers.first(where: { $0.computerId == id }) else {
            throw CloudError(status: 404, code: "notFound", message: "That computer isn't in your account.")
        }
        let previous = asks[id]
        accessPolls[id]?.cancel()
        if renew { previous?.cancel() }
        let ask = Task {
            // Finish cancellation cleanup before revoking and creating the replacement.
            if renew, let previous { _ = await previous.result }
            try Task.checkCancellation()
            if renew { try await cloud.revokeDeviceAccess(id) }
            return try await cloud.requestAccess(id, signKey: target.signKey)
        }
        asks[id] = ask
        accessPolls[id] = Task { [weak self] in
            await withTaskCancellationHandler {
                let result = await ask.result
                guard let self else { return }
                if self.asks[id] == ask { self.asks[id] = nil }
                let ticket: AccessTicket
                do {
                    ticket = try result.get()
                } catch {
                    // With the sheet closed nothing else sees it; an open sheet shows it too.
                    log.info("access request failed: \(error.localizedDescription, privacy: .public)")
                    return
                }
                guard !self.retired, !Task.isCancelled else { return }
                Analytics.shared.capture(.accessRequested)
                self.setPending(id, ticket)
                await self.awaitApproval(ticket, target: target, cloud: cloud)
            } onCancel: {
                ask.cancel()
            }
        }
        _ = try await ask.value
    }

    /// An offline account computer named like one this device uses or sees online: most likely the same
    /// machine under an earlier identity (its host keys were reset), left behind in the account.
    /// Seen after that one joined the account, it ran beside it: another machine with the same name.
    public func isOlderCopy(_ computer: CloudComputer) -> Bool {
        guard !computer.isOnline else { return false }
        let seen = computer.lastSeenAt
        // The current copy may be QR-paired without being listed in the account.
        return computers.contains { $0.id != computer.computerId && $0.name == computer.name && replaced(lastSeen: seen, by: $0.id) }
            || cloudComputers.contains { other in
                other.computerId != computer.computerId && other.name == computer.name
                    && (stores[other.computerId] != nil || other.isOnline) && replaced(lastSeen: seen, by: other.computerId)
            }
    }

    /// A computer this device uses that won't come back as it is: it turned this device away (access
    /// revoked, or its identity changed after a reset or reinstall), or it's offline while a computer
    /// with its name is reachable that joined the account after it was last seen, the same machine
    /// under a new identity. Account computers offer access recovery; only independently paired computers offer removal.
    public func isStale(_ id: ComputerID) -> Bool {
        guard let store = stores[id] else { return false }
        switch store.connection {
        case .unauthorized:
            return true
        case .computerOffline, .offline:
            let name = store.computer.name
            let seen = lastSeen(store)
            return stores.values.contains {
                $0.computer.id != id && $0.computer.name == name && $0.connection == .online && replaced(lastSeen: seen, by: $0.computer.id)
            } || cloudComputers.contains {
                $0.computerId != id && $0.name == name && $0.isOnline && replaced(lastSeen: seen, by: $0.computerId)
            }
        default:
            return false
        }
    }

    /// A same-named computer last seen (epoch ms) before `id` joined the account was replaced by it;
    /// seen after, it ran beside it: another machine. Without either time, the name alone decides.
    private func replaced(lastSeen seen: Int64?, by id: ComputerID) -> Bool {
        guard let seen, let claimed = cloudComputers.first(where: { $0.computerId == id })?.claimedAt else { return true }
        return seen <= claimed
    }

    /// When the account or, for a computer only paired here, the relay last saw it.
    private func lastSeen(_ store: BotStore) -> Int64? {
        if let seen = cloudComputers.first(where: { $0.computerId == store.computer.id })?.lastSeenAt { return seen }
        guard case let .computerOffline(date?) = store.connection else { return nil }
        return Int64(date.timeIntervalSince1970 * 1000)
    }

    /// Access requests appear and settle with an animation, like connection changes.
    private func setPending(_ id: ComputerID, _ ticket: AccessTicket?) {
        guard pendingAccess[id] != ticket else { return }
        Motion.animate { pendingAccess[id] = ticket }
    }

    /// Withdraws a pending access request; the computer drops it from its approval list.
    public func cancelAccess(_ id: ComputerID) async {
        guard let ticket = pendingAccess[id] else { return }
        accessPolls.removeValue(forKey: id)?.cancel()
        setPending(id, nil)
        Analytics.shared.capture(.accessRequestFailed(.cancelled))
        do {
            try await cloud?.cancelAccess(ticket.requestId)
        } catch {
            lastError = error.localizedDescription
        }
    }

    private func awaitApproval(_ ticket: AccessTicket, target: CloudComputer, cloud: CloudClient) async {
        while !Task.isCancelled, !retired {
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled, !retired else { return }
            guard let status = try? await cloud.accessStatus(ticket.requestId) else { continue }
            guard !retired, pendingAccess[ticket.computerId]?.requestId == ticket.requestId else { return }
            switch status.status {
            case .pending:
                continue
            case .approved:
                approvedAccess[ticket.computerId] = ticket.requestId
                setPending(ticket.computerId, nil)
                // Pin the key the SAS was computed with; the first hello brings its mailbox key and addresses.
                var computer = Computer(id: ticket.computerId, name: target.name, signKey: ticket.signKey,
                                        cloud: cloud.baseURL, device: target.device)
                if let previous = saved.first(where: { $0.id == computer.id && $0.signKey == computer.signKey }) {
                    computer.urls = previous.urls
                    computer.boxKey = previous.boxKey
                    computer.color = previous.color
                    computer.route = previous.route
                }
                save(computer)
                open(computer)
                Analytics.shared.capture(.computerPaired(via: .account, computer: computer.id))
                await refreshCloud()
                return
            case .denied, .expired, .cancelled:
                setPending(ticket.computerId, nil)
                let reason: AnalyticsEvent.AccessFailure = switch status.status {
                case .denied: .denied
                case .expired: .expired
                default: .cancelled
                }
                Analytics.shared.capture(.accessRequestFailed(reason))
                lastError = switch reason {
                case .denied: "\(target.name) declined the request."
                case .expired: "The request expired. Ask again."
                case .cancelled: "The request was withdrawn: \(target.name) or this iPhone was removed from your account."
                }
                // Withdrawn by the account: the list shows what's left of it.
                if reason == .cancelled { await refreshCloud() }
                return
            }
        }
    }

    // MARK: lifecycle

    public func setActive(_ active: Bool) {
        guard !retired else { return }
        isActive = active
        for store in stores.values { store.setActive(active) }
    }

    /// Account switched or signed out: nothing from here writes into storage or calls back anymore.
    public func retire() {
        retired = true
        computerOrder.retire()
        for store in stores.values { store.retire() }
        for poll in accessPolls.values { poll.cancel() }
        accessPolls = [:]
        onBotUpdated = nil
        onConnected = nil
        onSent = nil
        onRosterChanged = nil
        selection = nil
    }

    // MARK: plumbing

    private func open(_ computer: Computer) {
        close(computer.id)
        let store = makeStore(computer)
        let id = computer.id
        store.onBotUpdated = { [weak self] bot in
            guard let self, !self.retired else { return }
            self.onBotUpdated?(BotReference(accountId: self.accountId, computerId: id, botId: bot.id), bot)
        }
        store.onSent = { [weak self] bot, progress in
            guard let self, !self.retired else { return }
            self.onSent?(BotReference(accountId: self.accountId, computerId: id, botId: bot.id), bot, progress)
        }
        store.onConnected = { [weak self] store in
            guard let self, !self.retired else { return }
            self.onConnected?(store)
        }
        store.onComputerChanged = { [weak self] computer in self?.computerChanged(computer) }
        store.onRosterChanged = { [weak self] in self?.rosterChanged() }
        stores[id] = store
        if isActive { store.setActive(true) }
    }

    private func close(_ id: ComputerID) {
        stores.removeValue(forKey: id)?.retire()
    }

    private func computerChanged(_ computer: Computer) {
        guard !retired else { return }
        if let i = saved.firstIndex(where: { $0.id == computer.id }) {
            saved[i] = computer
            persist()
        }
        refreshList()
    }

    private func save(_ computer: Computer) {
        saved.removeAll { $0.id == computer.id }
        saved.insert(computer, at: 0)
        persist()
        refreshList()
    }

    private func persist() {
        guard !retired else { return }
        storage.computers = saved
    }

    private func refreshList() {
        let byID = Dictionary(saved.map { ($0.id, $0) }, uniquingKeysWith: { _, newest in newest })
        let ordered = computerOrder.state.orderedIDs(saved.map(\.id)).compactMap { byID[$0] }
        if saved != ordered {
            saved = ordered
            persist()
        }
        if ordered != computers { Motion.animate { computers = ordered } }
        rosterChanged()
    }

    private func rosterChanged() {
        guard !retired else { return }
        onRosterChanged?(roster)
    }
}
