import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "CloudMachine")

/// The account's Codync Cloud computer, for Computers → Codync Cloud: the key to the user's own Boat account,
/// whether this account has or may create one, creating it, and deleting its snapshots or the whole machine
/// (docs/plans/codync-cloud.md).
@MainActor
@Observable
public final class CloudMachineStore {
    public enum Lookup: Equatable {
        case loading
        /// No cloud computer, and this account may not create one: the section stays hidden.
        case unavailable
        /// No cloud computer yet; this account may create one.
        case creatable
        case machine(CloudMachine)
    }

    public enum Work: Equatable { case savingKey, removingKey, creating, deletingSnapshots, deleting }

    public private(set) var lookup = Lookup.loading
    /// The key to the user's own provider account, as the cloud shows it; nil until they give one.
    public private(set) var credential: CloudCredential?
    /// How to make that key.
    public private(set) var keySetup: CloudKeySetup?
    public private(set) var working: Work?
    /// Why the last action failed (the server's message where it gave one), shown in the section.
    public var error: String?

    /// The account's computers changed: a new machine claimed itself in, or `removed` went with it.
    var onAccountChanged: (@MainActor (_ removed: ComputerID?) async -> Void)?

    private let cloud: CloudClient
    private var poll: Task<Void, Never>?
    /// How often a machine still claiming itself in is asked about.
    var pollInterval: Duration = .seconds(3)

    init(cloud: CloudClient) { self.cloud = cloud }

    /// Created, but not in the account yet: it's booting and installing the host (about ten seconds).
    public var isPreparing: Bool {
        if case let .machine(machine) = lookup { machine.computerId == nil } else { false }
    }

    /// A computer can be made once the account gave a key; creating it says what to fix if Boat still turns it down.
    public var canCreate: Bool { credential != nil }

    /// The key can go while no computer runs on it (the cloud needs it to put one to sleep).
    public var canRemoveKey: Bool { lookup == .creatable }

    public func refresh() async {
        do {
            let status = try await cloud.cloudStatus()
            if status.credential != credential { Motion.animate { credential = status.credential } }
            keySetup = status.keySetup
            show(status.cloudComputer.map(Lookup.machine) ?? (status.available == true ? .creatable : .unavailable))
        } catch {
            // Asked on every visit and while preparing: the next look settles it, the section shows no error.
            log.info("cloud computer lookup failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Saves the key; false when the cloud refused it (`error` says why).
    @discardableResult
    public func saveKey(_ key: String) async -> Bool {
        await run(.savingKey) { [cloud, weak self] in
            do {
                let saved = try await cloud.saveCloudKey(key.trimmingCharacters(in: .whitespacesAndNewlines))
                Analytics.shared.capture(.cloudKeySaved(.saved))
                Motion.animate { self?.credential = saved }
            } catch {
                Analytics.shared.capture(.cloudKeySaved(.init(error)))
                throw error
            }
            return nil
        }
    }

    public func removeKey() async {
        await run(.removingKey) { [cloud, weak self] in
            try await cloud.deleteCloudKey()
            Motion.animate { self?.credential = nil }
            return nil
        }
    }

    public func create() async {
        await run(.creating) { [cloud] in .machine(try await cloud.createCloudMachine()) }
    }

    public func deleteSnapshots() async {
        await run(.deletingSnapshots) { [cloud] in
            try await cloud.deleteCloudSnapshots()
            return nil
        }
    }

    public func delete() async {
        guard case let .machine(machine) = lookup else { return }
        await run(.deleting) { [cloud] in
            try await cloud.deleteCloudMachine()
            return .creatable
        }
        if lookup == .creatable { await onAccountChanged?(machine.computerId) }
    }

    func retire() {
        poll?.cancel()
        onAccountChanged = nil
    }

    /// One action at a time; its result replaces the lookup unless it returns nil. True when it ran and worked.
    @discardableResult
    private func run(_ work: Work, _ action: @escaping @MainActor () async throws -> Lookup?) async -> Bool {
        guard working == nil else { return false }
        error = nil
        Motion.animate { working = work }
        defer { Motion.animate { working = nil } }
        do {
            if let new = try await action() { show(new) }
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func show(_ new: Lookup) {
        let wasPreparing = isPreparing
        if new != lookup { Motion.animate { lookup = new } }
        if isPreparing {
            if poll == nil { poll = Task { [weak self] in await self?.watchClaim() } }
        } else {
            poll?.cancel()
            poll = nil
            if wasPreparing { Task { await onAccountChanged?(nil) } }
        }
    }

    /// Asks again until the new machine's computer is in the account.
    private func watchClaim() async {
        while !Task.isCancelled, isPreparing {
            try? await Task.sleep(for: pollInterval)
            guard !Task.isCancelled else { return }
            await refresh()
        }
    }
}
