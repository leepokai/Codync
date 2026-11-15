import CodyncKit
import Foundation
import Observation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "CloudMachine")

/// The account's Codync Cloud computers, for Computers → Codync Cloud: the providers they can run on with the keys to
/// the user's own accounts there, whether this account may create them, creating another on a provider with a working
/// key, and renaming one, deleting its snapshots or the whole machine (docs/plans/codync-cloud.md).
@MainActor
@Observable
public final class CloudMachineStore {
    /// One action at a time; a key's names its provider, a machine's its `cloudComputerId`.
    public enum Work: Hashable {
        case savingKey(String), removingKey(String), creating, renaming(String), deletingSnapshots(String), deleting(String)

        /// Where its failure shows.
        var place: Place {
            switch self {
            case let .savingKey(provider), let .removingKey(provider): .key(provider)
            case .creating: .newComputer
            case .renaming, .deletingSnapshots, .deleting: .card
            }
        }

        /// The machine it's for.
        var machine: String? {
            switch self {
            case let .renaming(id), let .deletingSnapshots(id), let .deleting(id): id
            case .savingKey, .removingKey, .creating: nil
            }
        }
    }

    /// Where a failed action says why: under the key in its provider's key sheet, in the New cloud computer sheet, or
    /// under the Codync Cloud card's rows.
    public enum Place: Equatable { case key(String), newComputer, card }

    /// Whether this account may create cloud computers; false until the cloud first answers.
    public private(set) var available = false
    /// Oldest first.
    public private(set) var machines: [CloudMachine] = []
    /// How many the account may have at once.
    public private(set) var limit = 3
    /// Every provider the cloud offers, each with the account's key for it, if any.
    public private(set) var providers: [CloudProvider] = []
    public private(set) var working: Work?
    /// Why the last action failed (the server's message where it gave one), shown where it was (`error(at:)`).
    public private(set) var error: String?
    /// The action `error` came from.
    private var failed: Work?

    /// The account's computers changed: a new machine claimed itself in, or `removed` went with it.
    var onAccountChanged: (@MainActor (_ removed: ComputerID?) async -> Void)?
    /// A claimed machine was renamed: its computer goes by `name` from now on.
    var onRenamed: (@MainActor (ComputerID, _ name: String) async -> Void)?

    private let cloud: CloudClient
    private var poll: Task<Void, Never>?
    /// Bumped by every change made here (create, rename, delete): a lookup sent before one is stale when it comes back.
    private var generation = 0
    /// How often machines still claiming themselves in are asked about.
    var pollInterval: Duration = .seconds(3)

    init(cloud: CloudClient) { self.cloud = cloud }

    /// The Codync Cloud card shows: this account may create one, or still has one.
    public var isShown: Bool { available || !machines.isEmpty }

    /// One was created but isn't in the account yet: it's booting and installing the host (about ten seconds).
    public var isPreparing: Bool { machines.contains(where: \.isPreparing) }

    /// The account has as many as it may: one has to go before another can be made.
    public var atLimit: Bool { machines.count >= limit }

    /// Why New cloud computer can't make another, untranslated: the cloud's own words for a create over the limit.
    public var limitNote: String? {
        atLimit ? "You can have up to \(limit) cloud computers. Delete one to make another." : nil
    }

    /// An action on `machine` is running.
    public func isBusy(_ machine: String) -> Bool { working?.machine == machine }

    /// `error` where `place` shows it: only an action that belongs there.
    public func error(at place: Place) -> String? { failed?.place == place ? error : nil }

    /// A key removal's failure, whichever provider's: New cloud computer shows one asked from a row's `⋯` there.
    public var keyRemovalError: String? {
        if case .removingKey = failed { return error }
        return nil
    }

    /// Forgets an earlier failure shown at `place` (a sheet opened afresh).
    public func clearError(at place: Place) {
        guard failed?.place == place else { return }
        error = nil
        failed = nil
    }

    /// A cloud computer's state after where it runs, untranslated: what to fix while its provider turns the key down (an
    /// error), else that it's preparing, else its state (an error when it failed).
    public func status(of machine: CloudMachine) -> (text: String, isError: Bool) {
        if let problem = providers.first(where: { $0.provider == machine.hosting.provider })?.credential?.problem {
            return (problem, true)
        }
        if machine.isPreparing { return ("Preparing your cloud computer…", false) }
        return (machine.hosting.state.label, machine.hosting.state == .failed)
    }

    /// The provider New cloud computer creates on: `picked` while its key works, else the first whose key does. One
    /// picked before its key was added (or fixed) is chosen once it is.
    public func provider(picked: String?) -> String? {
        let ready = providers.filter(\.isReady)
        return (ready.first { $0.provider == picked } ?? ready.first)?.provider
    }

    /// A key can go unless a computer runs on that provider (the cloud needs it to put them to sleep).
    public func canRemoveKey(_ provider: String) -> Bool { !machines.contains { $0.hosting.provider == provider } }

    /// What a new computer is called when the user leaves its name empty: "Cloud computer", or "Cloud computer {n}" with
    /// the smallest n from 2 that no computer has.
    public var suggestedName: String {
        let taken = Set(machines.map(\.name))
        let first = L10n.tr("Cloud computer")
        guard taken.contains(first) else { return first }
        let numbered = L10n.tr("Cloud computer {n}")
        return (2...).lazy.map { L10n.fill(numbered, ["n": "\($0)"]) }.first { !taken.contains($0) } ?? first
    }

    /// Cut to what the cloud takes for a name: 64 UTF-16 units, counted as it counts them.
    nonisolated public static func limited(_ name: String) -> String {
        var name = name
        while name.utf16.count > 64 { name.removeLast() }
        return name
    }

    public func refresh() async {
        let sent = generation
        do {
            let status = try await cloud.cloudStatus()
            guard sent == generation else { return }
            let listed = status.providers ?? []
            if listed != providers { Motion.animate { providers = listed } }
            let open = status.available == true
            if open != available { Motion.animate { available = open } }
            limit = status.limit ?? limit
            show(status.cloudComputers ?? [])
        } catch {
            // Asked on every visit and while preparing: the next look settles it, the section shows no error.
            log.info("cloud computer lookup failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Saves `provider`'s key; false when the cloud refused it (`error` says why).
    @discardableResult
    public func saveKey(_ key: String, provider: String) async -> Bool {
        await run(.savingKey(provider)) {
            let saved = try await tracked(.cloudKeySaved(.saved, provider: provider), failed: { .cloudKeySaved(.init($0), provider: provider) }) {
                try await cloud.saveCloudKey(key.trimmingCharacters(in: .whitespacesAndNewlines), provider: provider)
            }
            setCredential(saved, provider: provider)
        }
    }

    public func removeKey(provider: String) async {
        await run(.removingKey(provider)) {
            try await tracked(.cloudKeyRemoved(.removed, provider: provider), failed: { .cloudKeyRemoved(.init($0), provider: provider) }) {
                try await cloud.deleteCloudKey(provider: provider)
            }
            setCredential(nil, provider: provider)
        }
    }

    private func setCredential(_ credential: CloudCredential?, provider: String) {
        guard let i = providers.firstIndex(where: { $0.provider == provider }) else { return }
        Motion.animate { providers[i].credential = credential }
    }

    /// Makes another computer on `provider` (one whose key works), called `name`, or `suggestedName` when it's empty;
    /// true when the cloud made it.
    @discardableResult
    public func create(on provider: String, name: String) async -> Bool {
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let named = typed.isEmpty ? suggestedName : typed
        return await run(.creating) {
            let made = try await tracked(.cloudComputerCreated(.created, provider: provider), failed: { .cloudComputerCreated(.init($0), provider: provider) }) {
                try await cloud.createCloudMachine(provider: provider, name: named)
            }
            generation += 1
            show(machines + [made])
        }
    }

    public func rename(_ id: String, to name: String) async {
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !typed.isEmpty, typed != machines.first(where: { $0.id == id })?.name else { return }
        await run(.renaming(id)) {
            let renamed = try await tracked(.cloudComputerRenamed(.renamed), failed: { _ in .cloudComputerRenamed(.failed) }) {
                try await cloud.renameCloudMachine(id, name: typed)
            }
            generation += 1
            show(machines.map { $0.id == id ? renamed : $0 })
            if let computer = renamed.computerId { await onRenamed?(computer, renamed.name) }
        }
    }

    /// `id` is a listed machine (its row asked); one gone from the list meanwhile has nothing left to act on.
    public func deleteSnapshots(_ id: String) async {
        guard let provider = machines.first(where: { $0.id == id })?.hosting.provider else { return }
        await run(.deletingSnapshots(id)) {
            try await tracked(.cloudSnapshotsDeleted(.deleted, provider: provider), failed: { .cloudSnapshotsDeleted(.init($0), provider: provider) }) {
                try await cloud.deleteCloudSnapshots(id)
            }
        }
    }

    /// `id` is a listed machine, as for `deleteSnapshots`.
    public func delete(_ id: String) async {
        guard let machine = machines.first(where: { $0.id == id }) else { return }
        let provider = machine.hosting.provider
        let deleted = await run(.deleting(id)) {
            try await tracked(.cloudComputerDeleted(.deleted, provider: provider), failed: { .cloudComputerDeleted(.init($0), provider: provider) }) {
                try await cloud.deleteCloudMachine(id)
            }
            generation += 1
            show(machines.filter { $0.id != id })
        }
        if deleted { await onAccountChanged?(machine.computerId) }
    }

    func retire() {
        poll?.cancel()
        onAccountChanged = nil
        onRenamed = nil
    }

    /// `call`, reporting what came of it: `ok` once the cloud answered, `failed` for its error. A cancelled call came to
    /// nothing and reports nothing (Android too).
    @discardableResult
    private func tracked<T>(_ ok: AnalyticsEvent, failed: (any Error) -> AnalyticsEvent, _ call: () async throws -> T) async throws -> T {
        do {
            let result = try await call()
            Analytics.shared.capture(ok)
            return result
        } catch {
            if !(error is CancellationError || (error as? URLError)?.code == .cancelled) { Analytics.shared.capture(failed(error)) }
            throw error
        }
    }

    /// One action at a time; true when it ran and worked.
    @discardableResult
    private func run(_ work: Work, _ action: () async throws -> Void) async -> Bool {
        guard working == nil else { return false }
        error = nil
        failed = nil
        Motion.animate { working = work }
        defer { Motion.animate { working = nil } }
        do {
            try await action()
            return true
        } catch {
            self.error = error.localizedDescription
            failed = work
            return false
        }
    }

    /// Polls while any machine prepares; the account's list is refreshed once one has claimed itself in.
    private func show(_ list: [CloudMachine]) {
        let claimed = machines.contains { old in old.computerId == nil && list.contains { $0.id == old.id && $0.computerId != nil } }
        if list != machines { Motion.animate { machines = list } }
        if isPreparing {
            if poll == nil { poll = Task { [weak self] in await self?.watchClaims() } }
        } else {
            poll?.cancel()
            poll = nil
        }
        if claimed { Task { await onAccountChanged?(nil) } }
    }

    /// Asks again until every new machine's computer is in the account.
    private func watchClaims() async {
        while !Task.isCancelled, isPreparing {
            try? await Task.sleep(for: pollInterval)
            guard !Task.isCancelled else { return }
            await refresh()
        }
    }
}
