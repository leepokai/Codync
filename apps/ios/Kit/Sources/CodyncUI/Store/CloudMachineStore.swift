import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "CloudMachine")

/// The account's Codync Cloud computer, for Computers → Codync Cloud: the providers it can run on with the keys to the
/// user's own accounts there, whether this account has or may create one, creating it on a connected provider, and
/// deleting its snapshots or the whole machine (docs/plans/codync-cloud.md).
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

    /// One action at a time; a key's names its provider.
    public enum Work: Equatable {
        case savingKey(String), removingKey(String), creating, deletingSnapshots, deleting

        /// The provider a key's work is for.
        var keyProvider: String? {
            switch self {
            case let .savingKey(provider), let .removingKey(provider): provider
            case .creating, .deletingSnapshots, .deleting: nil
            }
        }
    }

    public private(set) var lookup = Lookup.loading
    /// Every provider the cloud offers, each with the account's key for it, if any.
    public private(set) var providers: [CloudProvider] = []
    public private(set) var working: Work?
    /// Why the last action failed (the server's message where it gave one), shown in the section.
    public var error: String?
    /// The action `error` came from.
    private var failed: Work?

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

    /// `error` where the providers sheet shows it: under `provider`'s key when saving or removing that key failed, else
    /// (nil) below every provider.
    public func error(forKey provider: String?) -> String? { failed?.keyProvider == provider ? error : nil }

    /// Where a computer can be created: the providers the account gave a key. Creating says what to fix if one still
    /// turns its key down.
    public var connected: [CloudProvider] { providers.filter { $0.credential != nil } }

    /// The Cloud providers row: the connected providers' names, or what to do.
    public var providersSummary: (text: String, needsAttention: Bool) {
        if connected.contains(where: { $0.credential?.problem != nil }) { return ("Needs attention", true) }
        return connected.isEmpty ? ("Connect one", false) : (connected.map(\.name).joined(separator: ", "), false)
    }

    /// A key can go unless the computer runs on that provider (the cloud needs it to put the computer to sleep).
    public func canRemoveKey(_ provider: String) -> Bool {
        if case let .machine(machine) = lookup { machine.hosting.provider != provider } else { true }
    }

    public func refresh() async {
        do {
            let status = try await cloud.cloudStatus()
            let listed = status.providers ?? []
            if listed != providers { Motion.animate { providers = listed } }
            show(status.cloudComputer.map(Lookup.machine) ?? (status.available == true ? .creatable : .unavailable))
        } catch {
            // Asked on every visit and while preparing: the next look settles it, the section shows no error.
            log.info("cloud computer lookup failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Saves `provider`'s key; false when the cloud refused it (`error` says why).
    @discardableResult
    public func saveKey(_ key: String, provider: String) async -> Bool {
        await run(.savingKey(provider)) { [cloud, weak self] in
            do {
                let saved = try await cloud.saveCloudKey(key.trimmingCharacters(in: .whitespacesAndNewlines), provider: provider)
                Analytics.shared.capture(.cloudKeySaved(.saved, provider: provider))
                self?.setCredential(saved, provider: provider)
            } catch {
                Analytics.shared.capture(.cloudKeySaved(.init(error), provider: provider))
                throw error
            }
            return nil
        }
    }

    public func removeKey(provider: String) async {
        await run(.removingKey(provider)) { [cloud, weak self] in
            try await cloud.deleteCloudKey(provider: provider)
            self?.setCredential(nil, provider: provider)
            return nil
        }
    }

    private func setCredential(_ credential: CloudCredential?, provider: String) {
        guard let i = providers.firstIndex(where: { $0.provider == provider }) else { return }
        Motion.animate { providers[i].credential = credential }
    }

    /// On `provider`, one of `connected`.
    public func create(on provider: String) async {
        await run(.creating) { [cloud] in .machine(try await cloud.createCloudMachine(provider: provider)) }
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
        failed = nil
        Motion.animate { working = work }
        defer { Motion.animate { working = nil } }
        do {
            if let new = try await action() { show(new) }
            return true
        } catch {
            self.error = error.localizedDescription
            failed = work
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
