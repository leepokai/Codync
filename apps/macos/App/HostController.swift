import AppKit
import CodyncKit
import CodyncUI
import Foundation
import Observation
import ServiceManagement

/// A device asking a computer this Mac manages (itself or over SSH) for access.
@MainActor
struct Approval: Identifiable {
    let store: BotStore
    let request: AccessRequest
    let id: String

    init(store: BotStore, request: AccessRequest) {
        self.store = store
        self.request = request
        id = "\(store.computer.id)/\(request.requestId)"
    }
}

/// Manages the local codync-host (binary, background service) and owns the `AccountStore`
/// the menu and the chat window share: this Mac over loopback, SSH computers through their
/// tunnels, and the account's other computers over the encrypted channel.
@MainActor
@Observable
final class HostController {
    enum State: Equatable {
        case missingBinary
        case notInstalled
        case starting
        case running
        case failed(String)
    }

    /// `CODYNC_PORT` / `CODYNC_HOME` point the app at a dev host started with `codync-host serve`.
    static let devPort = ProcessInfo.processInfo.environment["CODYNC_PORT"].flatMap(Int.init)
    static let port = devPort ?? 19222
    static let dataDir = ProcessInfo.processInfo.environment["CODYNC_HOME"].map { URL(filePath: $0) }
        ?? FileManager.default.homeDirectoryForCurrentUser.appending(path: ".codync")
    static let baseURL = URL(string: "http://127.0.0.1:\(port)")!

    let account: AccountSession
    let ssh = SSHComputers()
    /// One per account context; switching accounts retires it.
    private(set) var accounts: AccountStore
    /// The signed-in account's cloud; nil when signed out or the build has no cloud.
    private(set) var cloud: CloudClient?
    private(set) var contextID: String
    /// This Mac's own host, once it answered.
    private(set) var local: Computer?
    private var localToken: String?

    private(set) var state: State = .starting
    var launchAtLogin: Bool = SMAppService.mainApp.status == .enabled
    /// Codync Screen (capture + input for Remote screen), a launchd agent inside this app.
    private let screenAgent = SMAppService.agent(plistName: "com.pokai.Codync.screen.plist")
    private(set) var screenAgentNeedsApproval = false
    private(set) var screenError: String?
    /// Approvals closed with "later"; they come back when the request changes (e.g. its code arrives).
    private var deferred: Set<String> = []

    private var streamTask: Task<Void, Never>?

    init(account: AccountSession) {
        #if DEBUG
        SSH.selfCheck()
        #endif
        self.account = account
        let storage = SharedStore.Context(accountID: account.userID)
        SharedStore.activeAccountID = account.userID
        contextID = storage.id
        (accounts, cloud) = Self.makeAccounts(storage, session: account)
        ssh.onAttach = { [weak self] attachment in
            self?.accounts.attach(attachment.computer, route: .loopback(baseURL: attachment.baseURL, token: attachment.token))
        }
        ssh.onDetach = { [weak self] id in self?.accounts.detach(id) }
        // ssh children outlive the app unless stopped.
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.ssh.disconnectAll() }
        }
    }

    /// The Mac's own host store.
    var store: BotStore? { local.flatMap { accounts.store(for: $0.id) } }

    /// Computers this Mac manages over loopback (itself and SSH tunnels): they take approvals, pairing and claims.
    var managedStores: [BotStore] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }.filter(\.isLoopback)
    }

    var approvals: [Approval] {
        managedStores.flatMap { store in store.accessRequests.map { Approval(store: store, request: $0) } }
    }

    /// The approval to show now: the first one not put off, or put off before its code arrived.
    var currentApproval: Approval? {
        approvals.first { !deferred.contains(deferKey($0)) }
    }

    func deferApproval(_ approval: Approval) { deferred.insert(deferKey(approval)) }

    /// Brings every waiting request back (the menu's Review).
    func reviewApprovals() { deferred = [] }

    private func deferKey(_ a: Approval) -> String { "\(a.id)/\(a.request.code ?? "")" }

    var roster: [RosterItem] { accounts.roster }
    var usage: Usage { store?.usage ?? Usage() }
    var screen: ScreenState? { store?.screen }
    var version: String? { store?.hello?.version }
    var needsAttention: Bool { roster.contains(where: \.bot.needsInput) || !approvals.isEmpty }
    var working: Int { roster.filter(\.bot.isWorking).count }

    func isSSH(_ id: ComputerID) -> Bool { ssh.attachments.values.contains { $0.computer.id == id } }

    /// Bundled next to the app executable, else a Homebrew / cargo install.
    var binaryURL: URL? {
        let candidates = [
            Bundle.main.bundleURL.appending(path: "Contents/MacOS/codync-host"),
            URL(filePath: "/opt/homebrew/bin/codync-host"),
            URL(filePath: "/usr/local/bin/codync-host"),
            FileManager.default.homeDirectoryForCurrentUser.appending(path: ".cargo/bin/codync-host"),
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    private var plistURL: URL {
        FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/LaunchAgents/com.pokai.codync.host.plist")
    }

    private var tokenURL: URL { Self.dataDir.appending(path: "token") }

    var logURL: URL { Self.dataDir.appending(path: "host.log") }

    func start() {
        refresh()
        ssh.connectAll()
        watchAutoClaim()
        watchCloudDefault()
    }

    /// Cloudflare is on by default, so a Mac without Tailscale is reachable away from home; turning it
    /// off (privacy: Wi-Fi and Tailscale only) is remembered and never undone behind the user's back.
    private func watchCloudDefault() {
        let target = withObservationTracking {
            cloudDefaultTarget
        } onChange: { [weak self] in
            Task { @MainActor in self?.watchCloudDefault() }
        }
        guard let target, let client = target.client, !enablingCloud else { return }
        enablingCloud = true
        Task {
            do { _ = try await enableCloud(client, store: target) } catch { accounts.lastError = error.localizedDescription }
            enablingCloud = false
        }
    }

    private var enablingCloud = false
    private static let cloudTurnedOffKey = "cloudTurnedOff"

    private var cloudDefaultTarget: BotStore? {
        guard account.cloudURL != nil, let store, store.connection == .online,
              let status = store.cloud, !status.enabled,
              !UserDefaults.standard.bool(forKey: Self.cloudTurnedOffKey) else { return nil }
        return store
    }

    func refresh() {
        guard binaryURL != nil else {
            state = .missingBinary
            return
        }
        guard Self.devPort != nil || FileManager.default.fileExists(atPath: plistURL.path) else {
            state = .notInstalled
            return
        }
        connect()
    }

    /// `codync-host install` also routes Claude Code's status line through the host.
    func install() {
        guard let bin = binaryURL else { return }
        state = .starting
        Task {
            let result = await Self.run(bin, ["install", "--port", "\(Self.port)"])
            if result.status != 0 {
                state = .failed(result.output.isEmpty ? "Install failed" : result.output)
            } else {
                try? await Task.sleep(for: .seconds(1))
                connect()
            }
        }
    }

    func restart() {
        let uid = getuid()
        Task {
            _ = await Self.run(URL(filePath: "/bin/launchctl"), ["kickstart", "-k", "gui/\(uid)/com.pokai.codync.host"])
            try? await Task.sleep(for: .seconds(1))
            connect()
        }
    }

    func uninstall() {
        guard let bin = binaryURL else { return }
        streamTask?.cancel()
        Task {
            _ = await Self.run(bin, ["uninstall"])
            if let local { accounts.detach(local.id) }
            local = nil
            localToken = nil
            state = .notInstalled
        }
    }

    func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {}
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    /// Remote screen: starts/stops Codync Screen and tells the host (which only accepts this from the Mac itself).
    func setRemoteScreen(_ on: Bool) {
        Task {
            do {
                if on { try screenAgent.register() } else { try await screenAgent.unregister() }
                try await store?.setScreenEnabled(on)
                screenError = nil
            } catch {
                screenError = error.localizedDescription
            }
            screenAgentNeedsApproval = screenAgent.status == .requiresApproval
        }
    }

    func openLoginItemsSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }

    /// Keeps Codync Screen registered while the host has Remote screen on (e.g. after the app moved).
    private func syncScreenAgent() {
        guard screen?.enabled == true, screenAgent.status != .enabled else {
            screenAgentNeedsApproval = false
            return
        }
        try? screenAgent.register()
        screenAgentNeedsApproval = screenAgent.status == .requiresApproval
    }

    func stop(_ item: RosterItem) {
        accounts.store(for: item.ref.computerId)?.stop(item.bot.id)
    }

    // MARK: account

    func switchAccount(to userID: String?) {
        let storage = SharedStore.Context(accountID: userID)
        guard storage.id != contextID else { return }
        accounts.retire()
        SharedStore.activeAccountID = userID
        contextID = storage.id
        deferred = []
        (accounts, cloud) = Self.makeAccounts(storage, session: account)
        // This Mac and the SSH tunnels belong to the Mac, not to an account: they follow into the new context.
        if let local, let localToken { accounts.attach(local, route: .loopback(baseURL: Self.baseURL, token: localToken)) }
        for attachment in ssh.attachments.values {
            accounts.attach(attachment.computer, route: .loopback(baseURL: attachment.baseURL, token: attachment.token))
        }
    }

    /// Signing out forgets the account on this Mac: its device keys, computers and caches (spec §3.2).
    func signOut() async {
        guard let userID = account.userID else { return }
        await account.signOut()
        guard account.userID != userID, !account.accounts.contains(where: { $0.id == userID }) else { return }
        switchAccount(to: account.userID)
        SharedStore.Context(accountID: userID).erase()
    }

    /// §4.2 A: makes a computer this Mac manages part of the signed-in account. Its relay is turned on first,
    /// since joining the account is about reaching it from anywhere.
    func claim(_ store: BotStore) async {
        guard let cloud, let userID = account.userID, let client = store.client else {
            accounts.lastError = "Sign in first."
            return
        }
        Self.setKeptOut(store.computer.id, false, userID: userID)
        do {
            if store.cloud?.enabled != true { _ = try await enableCloud(client, store: store) }
            let challenge = try await cloud.createClaim()
            let signed = try await client.claimSign(claimId: challenge.claimId, nonce: challenge.nonce, userId: userID)
            _ = try await cloud.completeClaim(challenge.claimId, signed: signed)
            await accounts.refreshCloud()
        } catch {
            accounts.lastError = error.localizedDescription
        }
    }

    /// Signed in, this Mac joins the account on its own, unless the user took it out of that account.
    private func watchAutoClaim() {
        let pending = withObservationTracking {
            autoClaimTarget
        } onChange: { [weak self] in
            Task { @MainActor in self?.watchAutoClaim() }
        }
        guard let pending, !autoClaiming else { return }
        autoClaiming = true
        Task {
            await claim(pending)
            autoClaiming = false
        }
    }

    private var autoClaiming = false

    /// This Mac's host, online, in no account yet, for a signed-in user who hasn't removed it.
    private var autoClaimTarget: BotStore? {
        guard let userID = account.userID, let store, store.connection == .online,
              let status = store.cloud, status.owner == nil,
              !Self.keptOut(userID).contains(store.computer.id) else { return nil }
        return store
    }

    /// Computers the user removed from an account on this Mac; auto-join leaves them out.
    private static func keptOut(_ userID: String) -> Set<String> {
        Set(UserDefaults.standard.stringArray(forKey: "keptOutOfAccount.\(userID)") ?? [])
    }

    private static func setKeptOut(_ id: String, _ out: Bool, userID: String) {
        var ids = keptOut(userID)
        if out { ids.insert(id) } else { ids.remove(id) }
        UserDefaults.standard.set(Array(ids), forKey: "keptOutOfAccount.\(userID)")
    }

    func unclaim(_ store: BotStore) async {
        if let userID = account.userID { Self.setKeptOut(store.computer.id, true, userID: userID) }
        do {
            try await store.client?.unclaim()
            await accounts.refreshCloud()
        } catch {
            accounts.lastError = error.localizedDescription
        }
    }

    func setCloud(_ store: BotStore, enabled: Bool) async {
        guard let client = store.client else { return }
        if store.computer.id == self.store?.computer.id { UserDefaults.standard.set(!enabled, forKey: Self.cloudTurnedOffKey) }
        do {
            _ = enabled ? try await enableCloud(client, store: store) : try await client.setCloud(enabled: false)
        } catch {
            accounts.lastError = error.localizedDescription
        }
    }

    /// A host without a cloud URL of its own gets this app's.
    private func enableCloud(_ client: HostClient, store: BotStore) async throws -> CloudStatus {
        let url = store.cloud?.url == nil ? account.cloudURL : nil
        guard store.cloud?.url != nil || url != nil else {
            throw CloudError(status: 400, code: "badRequest", message: "This build of Codync has no cloud to connect to.")
        }
        return try await client.setCloud(enabled: true, url: url)
    }

    func decide(_ approval: Approval, approve: Bool) async throws {
        guard let client = approval.store.client else { throw HostError.unreachable }
        try await client.decideAccessRequest(approval.request.requestId, approve: approve)
    }

    private static func makeAccounts(_ storage: SharedStore.Context, session: AccountSession) -> (AccountStore, CloudClient?) {
        let identity = storage.accountID == nil ? nil : try? DeviceIdentity.load(context: storage)
        let cloud = session.cloudClient(for: storage.accountID, identity: identity)
        let accounts = AccountStore(storage: storage, clientKind: "mac", cloud: cloud)
        if let cloud {
            // Signed in: make this Mac known to the account, then list its computers.
            Task { [weak accounts] in
                do {
                    try await cloud.registerDevice(name: Host.current().localizedName ?? "Mac", platform: "macos")
                } catch {
                    accounts?.lastError = error.localizedDescription
                }
                await accounts?.refreshCloud()
            }
        }
        return (accounts, cloud)
    }

    // MARK: local host

    private func readToken() -> String? {
        (try? String(contentsOf: tokenURL, encoding: .utf8)).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
    }

    /// Watches the host's health; the BotStore handles the event stream itself.
    private func connect() {
        streamTask?.cancel()
        streamTask = Task { [weak self] in
            var failures = 0
            while !Task.isCancelled {
                guard let self else { return }
                if await self.attachLocal() {
                    failures = 0
                    self.state = .running
                    self.syncScreenAgent()
                } else {
                    failures += 1
                    if failures > 5 { self.state = .failed("The host isn't responding. See the log for details.") }
                    else if self.state != .running { self.state = .starting }
                }
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    /// Attaches the local host once it answers, and again when its token or identity changed.
    private func attachLocal() async -> Bool {
        guard let token = readToken(), let health = await HostHealth.fetch(Self.baseURL), let id = health.computerId else {
            return false
        }
        if local?.id == id, localToken == token, store != nil { return true }
        guard let hello = try? await HostClient(baseURL: Self.baseURL, token: token).hello(),
              hello.computerId == id, let signKey = hello.signKey else { return false }
        let computer = Computer(id: id, name: hello.name, signKey: signKey, boxKey: hello.boxKey, urls: hello.urls ?? [],
                                cloud: hello.cloud.flatMap(URL.init(string:)), device: hello.device,
                                color: store?.computer.color)
        if let old = local, old.id != id { accounts.detach(old.id) }
        local = computer
        localToken = token
        accounts.attach(computer, route: .loopback(baseURL: Self.baseURL, token: token))
        return true
    }

    nonisolated static func run(_ bin: URL, _ args: [String]) async -> (status: Int32, output: String) {
        // The host rebuilds PATH from the login shell itself (backends::hydrate_path).
        let result = await ProcessRunner.run(bin, args)
        return (result.status, (result.text + result.stderr).trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

extension BotStore {
    var isLoopback: Bool {
        if case .loopback = route { true } else { false }
    }
}
