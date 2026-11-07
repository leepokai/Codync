import Foundation
import os
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// Answers each request with the subclass's `answer(_:)` (status, JSON).
private class CloudComputerStub: URLProtocol {
    class func answer(_ request: URLRequest) -> (Int, String) { (200, "{}") }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func stopLoading() {}

    override func startLoading() {
        let (status, json) = Self.answer(request)
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
}

private func stubbedCloud(_ stub: CloudComputerStub.Type, floor: AppFloor = AppFloor()) -> CloudClient {
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [stub]
    return CloudClient(baseURL: URL(string: "https://cloud.test")!, identity: DeviceIdentity(signing: .init(), push: .init()),
                       session: URLSession(configuration: config), floor: floor) { "token" }
}

private let cloudKey = Data(repeating: 7, count: 32)
private let ownKey = Data(repeating: 8, count: 32)

/// The account has a sleeping cloud computer and one of the user's own, neither usable here yet; access
/// requests get their host nonce at once and are never approved.
private final class AutoAccessStub: CloudComputerStub {
    /// Paths of committed access requests, and every request's `Codync-App` header.
    static let commits = OSAllocatedUnfairLock(initialState: [String]())
    static let appHeaders = OSAllocatedUnfairLock(initialState: Set<String?>())

    override class func answer(_ request: URLRequest) -> (Int, String) {
        appHeaders.withLock { _ = $0.insert(request.value(forHTTPHeaderField: "Codync-App")) }
        let path = request.url?.path ?? ""
        if path.hasSuffix("/access-requests") { commits.withLock { $0.append(path) } }
        if path.hasSuffix("/v1/computers") {
            let cloud = #"{"provider":"boat","providerName":"Boat","region":"EU","state":"stopped"}"#
            return (200, #"{"computers":[{"computerId":"\#(RelayCrypto.computerId(signKey: cloudKey))","name":"Cloud","signKey":"\#(cloudKey.base64URL)","access":"none","cloud":\#(cloud)},"#
                + #"{"computerId":"\#(RelayCrypto.computerId(signKey: ownKey))","name":"Desk","signKey":"\#(ownKey.base64URL)","access":"none","cloud":null}]}"#)
        }
        if path.hasSuffix("/access-requests") { return (200, #"{"requestId":"r1","expiresAt":4102444800000}"#) }
        if path.hasSuffix("/v1/access-requests/r1") {
            return (200, #"{"requestId":"r1","status":"pending","hostNonce":"\#(Data(repeating: 9, count: 32).base64URL)"}"#)
        }
        return (200, "{}")
    }
}

/// A cloud computer this device can't use yet is asked for access by itself, once per session; the user's
/// own computer still waits for a tap. Every request says which app and `minHost` it comes from.
@MainActor @Test func cloudComputerIsAskedForAccessOnceByItself() async throws {
    let suite = "CloudAccess.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let accounts = AccountStore(storage: SharedStore.Context(accountID: "account", suite: suite), cloud: stubbedCloud(AutoAccessStub.self))
    defer { accounts.retire() }
    let id = RelayCrypto.computerId(signKey: cloudKey)

    await accounts.refreshCloud()
    #expect(accounts.cloudComputers.first { $0.computerId == id }?.cloud.map { "\($0.providerName) · \($0.region)" } == "Boat · EU")
    #expect(accounts.cloudComputers.first { $0.computerId == id }?.cloud?.state.isAsleep == true)
    for _ in 0..<100 where accounts.pendingAccess[id] == nil {
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(accounts.pendingAccess[id] != nil)
    await accounts.refreshCloud()
    try await Task.sleep(for: .milliseconds(200))
    #expect(AutoAccessStub.commits.withLock { $0 } == ["/v1/computers/\(id)/access-requests"])
    #expect(AutoAccessStub.appHeaders.withLock { $0 } == ["ios/\(AppVersion.current); minHost=\(VersionMismatch.minHost)"])
}

/// No cloud computer, and the account isn't on the list yet.
private final class UnavailableStub: CloudComputerStub {
    override class func answer(_ request: URLRequest) -> (Int, String) {
        if request.httpMethod == "POST" {
            return (403, #"{"error":{"code":"forbidden","message":"Codync Cloud isn't available for this account yet."}}"#)
        }
        return (200, #"{"cloudComputer":null}"#)
    }
}

/// An account that can't have a cloud computer keeps the section hidden; creating one anyway shows why not.
@MainActor @Test func cloudComputerStaysHiddenUntilTheAccountMayHaveOne() async {
    let store = CloudMachineStore(cloud: stubbedCloud(UnavailableStub.self))
    await store.refresh()
    #expect(store.lookup == .unavailable)
    await store.create(on: "boat")
    #expect(store.lookup == .unavailable)
    #expect(store.error == "Codync Cloud isn't available for this account yet.")
}

/// An account that may have a cloud computer on two providers but gave neither a key yet. The first key it gives is
/// refused; each request's path is kept.
private final class KeyStub: CloudComputerStub {
    static let puts = OSAllocatedUnfairLock(initialState: 0)
    static let paths = OSAllocatedUnfairLock(initialState: [String]())

    override class func answer(_ request: URLRequest) -> (Int, String) {
        paths.withLock { $0.append("\(request.httpMethod ?? "") \(request.url?.path ?? "")") }
        switch request.httpMethod {
        case "PUT":
            let n = puts.withLock { $0 += 1; return $0 }
            return n == 1
                ? (422, #"{"error":{"code":"keyRefused","message":"This key can manage your whole Boat account. Make one limited to Codync instead."}}"#)
                : (200, #"{"credential":{"hint":"boat_…9abc","problem":null}}"#)
        case "DELETE":
            return (200, "{}")
        default:
            let setup = #""setup":{"page":"https://boat.dev/dashboard","commands":"boat env new codync"},"credential":null"#
            return (200, #"{"cloudComputer":null,"available":true,"providers":[{"provider":"boat","name":"Boat","region":"EU","keyPrompt":"boat_…",\#(setup)},"#
                + #"{"provider":"nimbus","name":"Nimbus","region":"Asia","keyPrompt":"nim_…",\#(setup)}]}"#)
        }
    }
}

/// A provider's key comes first: a refusal keeps the cloud's reason, a saved key shows only by its hint, and only then
/// can a computer be made, on that provider; with no computer on it, the key can go again.
@MainActor @Test func cloudKeyComesBeforeTheComputer() async {
    let store = CloudMachineStore(cloud: stubbedCloud(KeyStub.self))
    await store.refresh()
    #expect(store.lookup == .creatable)
    #expect(store.providers.map(\.name) == ["Boat", "Nimbus"])
    #expect(store.providers.first?.setup.commands == "boat env new codync")
    #expect(store.connected.isEmpty && store.providersSummary.text == "Connect one")
    #expect(await store.saveKey("boat_admin_key", provider: "boat") == false)
    #expect(store.error == "This key can manage your whole Boat account. Make one limited to Codync instead.")
    #expect(await store.saveKey(" boat_live_key9abc ", provider: "boat"))
    #expect(store.connected.map(\.provider) == ["boat"] && store.providers.first?.credential?.hint == "boat_…9abc")
    #expect(store.providersSummary.text == "Boat" && store.canRemoveKey("boat") && store.error == nil)
    await store.removeKey(provider: "boat")
    #expect(store.connected.isEmpty)
    #expect(KeyStub.paths.withLock { $0 }.filter { !$0.hasPrefix("GET") }
        == ["PUT /v1/cloud-credentials/boat", "PUT /v1/cloud-credentials/boat", "DELETE /v1/cloud-credentials/boat"])
}

/// A sleeping machine that hasn't claimed itself in yet; its snapshots can't be deleted while it sleeps.
private final class PreparingStub: CloudComputerStub {
    static let claimed = OSAllocatedUnfairLock(initialState: false)

    override class func answer(_ request: URLRequest) -> (Int, String) {
        if request.httpMethod == "DELETE" {
            return (409, #"{"error":{"code":"conflict","message":"Your cloud computer is asleep."}}"#)
        }
        let computer = claimed.withLock { $0 } ? #""abc""# : "null"
        return (200, #"{"cloudComputer":{"cloudComputerId":"cm_1","provider":"boat","providerName":"Boat","region":"EU","state":"stopped","computerId":\#(computer)},"available":true}"#)
    }
}

/// A new machine is "preparing" until its computer is in the account, then the account's list is refreshed;
/// a refused action shows the cloud's reason and keeps the machine.
@MainActor @Test func preparingCloudComputerWaitsForItsClaim() async throws {
    let store = CloudMachineStore(cloud: stubbedCloud(PreparingStub.self))
    store.pollInterval = .milliseconds(20)
    let changed = OSAllocatedUnfairLock(initialState: false)
    store.onAccountChanged = { _ in changed.withLock { $0 = true } }
    defer { store.retire() }

    await store.refresh()
    #expect(store.isPreparing)
    PreparingStub.claimed.withLock { $0 = true }
    for _ in 0..<100 where store.isPreparing {
        try await Task.sleep(for: .milliseconds(20))
    }
    #expect(!store.isPreparing)
    for _ in 0..<50 where !changed.withLock({ $0 }) {
        try await Task.sleep(for: .milliseconds(20))
    }
    #expect(changed.withLock { $0 })

    await store.deleteSnapshots()
    #expect(store.error == "Your cloud computer is asleep.")
    guard case let .machine(machine) = store.lookup else { Issue.record("machine gone"); return }
    #expect(machine.computerId == "abc")
    #expect(machine.hosting.state == .stopped)
}

/// Every Worker request is refused below the emergency floor.
private final class FloorStub: CloudComputerStub {
    override class func answer(_ request: URLRequest) -> (Int, String) {
        (426, #"{"error":{"code":"appUpdateRequired","message":"This version has a security problem."}}"#)
    }
}

/// Below the cloud's emergency floor, the app learns why it has to update.
@MainActor @Test func emergencyFloorKeepsTheCloudsReason() async {
    let floor = AppFloor()
    let cloud = stubbedCloud(FloorStub.self, floor: floor)
    await #expect(throws: CloudError.self) { try await cloud.computers() }
    #expect(floor.reason == "This version has a security problem.")
}
