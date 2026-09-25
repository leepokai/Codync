import CodyncKit
import Foundation
import Observation

/// One bot across an account context's computers, for lists that mix them.
public struct RosterItem: Identifiable, Sendable {
    public var ref: BotReference
    public var bot: Bot
    public var computer: Computer
    public var id: BotReference { ref }
}

/// Account → computers → bots: one `BotStore` per computer this context can reach
/// (QR-paired, approved through the account, or attached over loopback / SSH on the Mac),
/// plus the account's computers from the cloud that this device can still ask access to.
@MainActor
@Observable
public final class AccountStore {
    public let storage: SharedStore.Context
    public var accountId: String? { storage.accountID }
    /// Attached (loopback) computers first, then the saved ones.
    public private(set) var computers: [Computer] = []
    public private(set) var stores: [ComputerID: BotStore] = [:]
    /// Everything in the account, including computers this device isn't authorized on yet.
    public private(set) var cloudComputers: [CloudComputer] = []
    /// Access requests waiting for approval on the computer, with the SAS to show.
    public private(set) var pendingAccess: [ComputerID: AccessTicket] = [:]
    public var lastError: String?
    public var selection: BotReference? {
        didSet { if let id = selection?.computerId, !retired { storage.lastComputerId = id } }
    }

    public var onBotUpdated: (@MainActor (BotReference, Bot) -> Void)?
    public var onConnected: (@MainActor (BotStore) -> Void)?
    public var onSent: (@MainActor (BotReference, Bot) -> Void)?
    /// The combined roster changed: a bot came, changed or went, or a computer did.
    public var onRosterChanged: (@MainActor ([RosterItem]) -> Void)?

    private let clientKind: String
    private let cloud: CloudClient?
    private let makeStore: @MainActor (Computer, BotStore.Route) -> BotStore
    /// Persisted in the context (channel route).
    private var saved: [Computer]
    /// This session only: the Mac's own host and SSH tunnels.
    private var attached: [Computer] = []
    private var accessPolls: [ComputerID: Task<Void, Never>] = [:]
    private var isActive = true
    private var retired = false

    public convenience init(storage: SharedStore.Context, clientKind: String, cloud: CloudClient?) {
        self.init(storage: storage, clientKind: clientKind, cloud: cloud) { computer, route in
            BotStore(computer: computer, route: route, clientKind: clientKind, storage: storage)
        }
    }

    init(storage: SharedStore.Context, clientKind: String, cloud: CloudClient?,
         makeStore: @escaping @MainActor (Computer, BotStore.Route) -> BotStore) {
        self.storage = storage
        self.clientKind = clientKind
        self.cloud = cloud
        self.makeStore = makeStore
        saved = storage.computers.filter(\.isConsistent)
        for computer in saved { open(computer, route: .channel) }
        refreshList()
    }

    // MARK: derived

    /// Every computer's visible bots, pinned first, then most recent activity.
    public var roster: [RosterItem] {
        computers.flatMap { computer -> [RosterItem] in
            guard let store = stores[computer.id] else { return [] }
            return store.roster.map {
                RosterItem(ref: BotReference(accountId: accountId, computerId: computer.id, botId: $0.id), bot: $0, computer: store.computer)
            }
        }
        .sorted {
            if $0.bot.pinned != $1.bot.pinned { return $0.bot.pinned }
            return $0.bot.lastAt > $1.bot.lastAt
        }
    }

    public func store(for id: ComputerID) -> BotStore? { stores[id] }

    // MARK: computers

    /// QR pairing (§4.1); the computer is saved in this context and connected.
    public func pair(_ pairing: Pairing, deviceName: String, platform: String) async throws -> Computer {
        let identity = try DeviceIdentity.load(context: storage)
        var computer = try await HostConnector.pair(pairing, identity: identity, deviceName: deviceName, platform: platform)
        guard !retired else { throw CancellationError() }
        computer.color = saved.first { $0.id == computer.id }?.color
        save(computer)
        if !attached.contains(where: { $0.id == computer.id }) { open(computer, route: .channel) }
        return computer
    }

    /// The Mac's own host or an SSH tunnel: not saved, it lasts until `detach`.
    public func attach(_ computer: Computer, route: BotStore.Route) {
        guard !retired else { return }
        attached.removeAll { $0.id == computer.id }
        attached.append(computer)
        open(computer, route: route)
        refreshList()
    }

    /// Ends an attachment; a saved computer with the same ID goes back to the channel.
    public func detach(_ id: ComputerID) {
        attached.removeAll { $0.id == id }
        close(id)
        if let computer = saved.first(where: { $0.id == id }), !retired { open(computer, route: .channel) }
        refreshList()
    }

    public func forget(_ id: ComputerID) {
        attached.removeAll { $0.id == id }
        saved.removeAll { $0.id == id }
        persist()
        close(id)
        accessPolls.removeValue(forKey: id)?.cancel()
        pendingAccess[id] = nil
        if selection?.computerId == id { selection = nil }
        refreshList()
    }

    public func setColor(_ id: ComputerID, _ color: String) {
        if let i = saved.firstIndex(where: { $0.id == id }) {
            saved[i].color = color
            persist()
        }
        if let i = attached.firstIndex(where: { $0.id == id }) { attached[i].color = color }
        stores[id]?.updateComputer { $0.color = color }
        refreshList()
    }

    // MARK: account (§4.2 B)

    public func refreshCloud() async {
        guard let cloud, !retired else { return }
        do {
            let list = try await cloud.computers()
            guard !retired else { return }
            cloudComputers = list
        } catch {
            lastError = error.localizedDescription
        }
    }

    /// Starts an access request; the returned `code` must match the one the computer shows.
    /// Approval is then picked up in the background and the computer joins `stores`.
    public func requestAccess(_ id: ComputerID) async throws -> AccessTicket {
        guard let cloud, let target = cloudComputers.first(where: { $0.computerId == id }) else {
            throw CloudError(status: 404, code: "notFound", message: "That computer isn't in your account.")
        }
        let ticket = try await cloud.requestAccess(id, signKey: target.signKey)
        guard !retired else { throw CancellationError() }
        pendingAccess[id] = ticket
        accessPolls[id]?.cancel()
        accessPolls[id] = Task { [weak self] in await self?.awaitApproval(ticket, target: target, cloud: cloud) }
        return ticket
    }

    /// Withdraws a pending access request; the computer drops it from its approval list.
    public func cancelAccess(_ id: ComputerID) async {
        guard let ticket = pendingAccess[id] else { return }
        accessPolls.removeValue(forKey: id)?.cancel()
        pendingAccess[id] = nil
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
                pendingAccess[ticket.computerId] = nil
                // Pin the key the SAS was computed with; the first hello brings its mailbox key and addresses.
                let computer = Computer(id: ticket.computerId, name: target.name, signKey: ticket.signKey,
                                        cloud: cloud.baseURL, device: target.device)
                save(computer)
                if !attached.contains(where: { $0.id == computer.id }) { open(computer, route: .channel) }
                await refreshCloud()
                return
            case .denied, .expired, .cancelled:
                pendingAccess[ticket.computerId] = nil
                lastError = status.status == .denied ? "\(target.name) declined the request." : "The request expired. Ask again."
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

    private func open(_ computer: Computer, route: BotStore.Route) {
        close(computer.id)
        let store = makeStore(computer, route)
        let id = computer.id
        store.onBotUpdated = { [weak self] bot in
            guard let self, !self.retired else { return }
            self.onBotUpdated?(BotReference(accountId: self.accountId, computerId: id, botId: bot.id), bot)
        }
        store.onSent = { [weak self] bot in
            guard let self, !self.retired else { return }
            self.onSent?(BotReference(accountId: self.accountId, computerId: id, botId: bot.id), bot)
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
        if let i = saved.firstIndex(where: { $0.id == computer.id }), stores[computer.id]?.route == .channel {
            saved[i] = computer
            persist()
        }
        if let i = attached.firstIndex(where: { $0.id == computer.id }) { attached[i] = computer }
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
        computers = attached + saved.filter { c in !attached.contains { $0.id == c.id } }
        rosterChanged()
    }

    private func rosterChanged() {
        guard !retired else { return }
        onRosterChanged?(roster)
    }
}
