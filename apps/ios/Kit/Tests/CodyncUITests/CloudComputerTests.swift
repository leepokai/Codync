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
        return (200, #"{"cloudComputers":[]}"#)
    }
}

/// An account that can't have a cloud computer keeps the section hidden; creating one anyway says why not, in the New
/// cloud computer sheet.
@MainActor @Test func cloudComputerStaysHiddenUntilTheAccountMayHaveOne() async {
    let store = CloudMachineStore(cloud: stubbedCloud(UnavailableStub.self))
    await store.refresh()
    #expect(!store.isShown && !store.available)
    #expect(await store.create(on: "boat", name: "") == false)
    #expect(!store.isShown)
    #expect(store.error(at: .newComputer) == "Codync Cloud isn't available for this account yet.")
    #expect(store.error(at: .card) == nil)
}

/// An account that may have a cloud computer on two providers but gave neither a key yet. The first key it gives is
/// refused, and so is the first removal; each request's path is kept.
private final class KeyStub: CloudComputerStub {
    static let puts = OSAllocatedUnfairLock(initialState: 0)
    static let deletes = OSAllocatedUnfairLock(initialState: 0)
    static let paths = OSAllocatedUnfairLock(initialState: [String]())

    override class func answer(_ request: URLRequest) -> (Int, String) {
        paths.withLock { $0.append("\(request.httpMethod ?? "") \(request.url?.path ?? "")") }
        switch request.httpMethod {
        case "PUT":
            let n = puts.withLock { $0 += 1; return $0 }
            return n == 1
                ? (422, #"{"error":{"code":"keyRefused","message":"This key can manage your whole account. Make one limited to Codync instead."}}"#)
                : (200, #"{"credential":{"hint":"boat_…9abc","problem":null}}"#)
        case "DELETE":
            let n = deletes.withLock { $0 += 1; return $0 }
            return n == 1 ? (409, #"{"error":{"code":"conflict","message":"A cloud computer still runs on Boat."}}"#) : (200, "{}")
        default:
            let setup = #""setup":{"page":"https://boat.dev/dashboard","commands":"boat env new codync"},"credential":null"#
            return (200, #"{"cloudComputers":[],"available":true,"limit":3,"providers":[{"provider":"boat","name":"Boat","region":"EU","keyPrompt":"boat_…",\#(setup)},"#
                + #"{"provider":"nimbus","name":"Nimbus","region":"Asia","keyPrompt":"nim_…",\#(setup),"#
                + #""offer":{"label":"Get a free sandbox","detail":"A free plan.","url":"https://nimbus.example/pricing"}}]}"#)
        }
    }
}

/// A provider's key comes first: a refusal keeps the cloud's reason, a saved key shows only by its hint, and only then
/// can a computer be made, on that provider; with no computer on it, the key can go again.
@MainActor @Test func cloudKeyComesBeforeTheComputer() async {
    let store = CloudMachineStore(cloud: stubbedCloud(KeyStub.self))
    await store.refresh()
    #expect(store.isShown && store.machines.isEmpty && !store.atLimit)
    #expect(store.providers.map(\.name) == ["Boat", "Nimbus"])
    #expect(store.providers.first?.setup.commands == "boat env new codync")
    // A free plan only where the cloud names one.
    #expect(store.providers.map(\.offer?.url.absoluteString) == [nil, "https://nimbus.example/pricing"])
    #expect(store.providers.allSatisfy { $0.credential == nil })
    #expect(await store.saveKey("boat_admin_key", provider: "boat") == false)
    #expect(store.error == "This key can manage your whole account. Make one limited to Codync instead.")
    // Shown under Boat's key, not below every provider, nor in New cloud computer (only a removal shows there).
    #expect(store.error(at: .key("boat")) == store.error && store.error(at: .key("nimbus")) == nil && store.error(at: .card) == nil)
    #expect(store.keyRemovalError == nil)
    #expect(await store.saveKey(" boat_live_key9abc ", provider: "boat"))
    #expect(store.providers.map(\.credential?.hint) == ["boat_…9abc", nil])
    #expect(store.providers.map(\.isReady) == [true, false])
    #expect(store.canRemoveKey("boat") && store.error == nil)
    // A removal that fails keeps the key and says why, in the key sheet and New cloud computer, until it's seen.
    await store.removeKey(provider: "boat")
    #expect(store.providers.first?.credential != nil)
    #expect(store.keyRemovalError == "A cloud computer still runs on Boat." && store.error(at: .key("boat")) == store.keyRemovalError)
    store.clearError(at: .key("boat"))
    #expect(store.keyRemovalError == nil)
    await store.removeKey(provider: "boat")
    #expect(store.providers.allSatisfy { $0.credential == nil })
    #expect(KeyStub.paths.withLock { $0 }.filter { !$0.hasPrefix("GET") }
        == ["PUT /v1/cloud-credentials/boat", "PUT /v1/cloud-credentials/boat", "DELETE /v1/cloud-credentials/boat",
            "DELETE /v1/cloud-credentials/boat"])
}

/// A running machine, and a sleeping one that hasn't claimed itself in yet; snapshots can't be deleted while one sleeps.
private final class PreparingStub: CloudComputerStub {
    static let claimed = OSAllocatedUnfairLock(initialState: false)

    override class func answer(_ request: URLRequest) -> (Int, String) {
        if request.httpMethod == "DELETE" {
            return (409, #"{"error":{"code":"conflict","message":"Your cloud computer is asleep."}}"#)
        }
        let computer = claimed.withLock { $0 } ? #""abc""# : "null"
        return (200, #"{"cloudComputers":[\#(machineJSON("cm_0", "Desk", computer: #""def""#, state: "running")),"#
            + #"\#(machineJSON("cm_1", "Cloud computer", computer: computer))],"available":true}"#)
    }
}

/// A cloud computer as `GET /v1/cloud-computers` lists it.
private func machineJSON(_ id: String, _ name: String, provider: String = "boat", computer: String = "null",
                         state: String = "stopped", snapshots: Bool = true) -> String {
    let providerName = provider == "boat" ? "Boat" : "InstaBox"
    return #"{"cloudComputerId":"\#(id)","name":"\#(name)","provider":"\#(provider)","providerName":"\#(providerName)","region":"EU","state":"\#(state)","computerId":\#(computer),"snapshots":\#(snapshots)}"#
}

/// A new machine is "preparing" until its computer is in the account, then the account's list is refreshed;
/// a refused action shows the cloud's reason under the card's rows and keeps the machine.
@MainActor @Test func preparingCloudComputerWaitsForItsClaim() async throws {
    let store = CloudMachineStore(cloud: stubbedCloud(PreparingStub.self))
    store.pollInterval = .milliseconds(20)
    let changed = OSAllocatedUnfairLock(initialState: false)
    store.onAccountChanged = { _ in changed.withLock { $0 = true } }
    defer { store.retire() }

    await store.refresh()
    #expect(store.isPreparing && store.machines.map(\.isPreparing) == [false, true])
    #expect(store.machines.map { store.status(of: $0).text } == ["Running", "Preparing your cloud computer…"])
    PreparingStub.claimed.withLock { $0 = true }
    for _ in 0..<100 where store.isPreparing {
        try await Task.sleep(for: .milliseconds(20))
    }
    #expect(!store.isPreparing)
    for _ in 0..<50 where !changed.withLock({ $0 }) {
        try await Task.sleep(for: .milliseconds(20))
    }
    #expect(changed.withLock { $0 })

    await store.deleteSnapshots("cm_1")
    #expect(store.error == "Your cloud computer is asleep." && store.error(at: .card) == store.error)
    #expect(store.error(at: .newComputer) == nil && store.error(at: .key("boat")) == nil)
    #expect(store.machines.map(\.computerId) == ["def", "abc"])
    #expect(store.machines.last?.hosting.state == .stopped)
}

/// One cloud computer that failed before it claimed itself in; each lookup is counted.
private final class FailedStub: CloudComputerStub {
    static let lookups = OSAllocatedUnfairLock(initialState: 0)

    override class func answer(_ request: URLRequest) -> (Int, String) {
        lookups.withLock { $0 += 1 }
        return (200, #"{"cloudComputers":[\#(machineJSON("cm_1", "Cloud computer", state: "failed"))],"available":true}"#)
    }
}

/// A machine that failed before its claim isn't preparing: it says it failed, and nothing asks about it again.
@MainActor @Test func failedCloudComputerIsNotPolled() async throws {
    let store = CloudMachineStore(cloud: stubbedCloud(FailedStub.self))
    store.pollInterval = .milliseconds(20)
    defer { store.retire() }
    await store.refresh()
    #expect(store.machines.map(\.hosting.state) == [.failed] && !store.isPreparing)
    #expect(store.machines.map { store.status(of: $0).text } == ["Failed"] && store.status(of: store.machines[0]).isError)
    try await Task.sleep(for: .milliseconds(200))
    #expect(FailedStub.lookups.withLock { $0 } == 1)
}

/// Two cloud computers: Boat's, and InstaBox's named "Cloud computer 3", which keeps no snapshots. Another is made on the
/// provider and with the name sent; each request is kept.
private final class FleetStub: CloudComputerStub {
    static let requests = OSAllocatedUnfairLock(initialState: [String]())
    static let created = OSAllocatedUnfairLock(initialState: [String: String]())

    override class func answer(_ request: URLRequest) -> (Int, String) {
        let method = request.httpMethod ?? ""
        let path = request.url?.path ?? ""
        requests.withLock { $0.append("\(method) \(path)") }
        let sent = (try? JSONDecoder().decode([String: String].self, from: body(of: request))) ?? [:]
        switch method {
        case "POST":
            created.withLock { $0 = sent }
            return (200, #"{"cloudComputer":\#(machineJSON("cm_3", sent["name"] ?? "", provider: sent["provider"] ?? ""))}"#)
        case "PATCH":
            return (200, #"{"cloudComputer":\#(machineJSON("cm_2", sent["name"] ?? "", provider: "instabox", computer: #""def""#, state: "running", snapshots: false))}"#)
        case "DELETE":
            return (200, "{}")
        default:
            let setup = #""setup":{"page":"https://boat.dev/dashboard","commands":"boat env new codync"}"#
            return (200, #"{"cloudComputers":[\#(machineJSON("cm_1", "Cloud computer", computer: #""abc""#, state: "running")),"#
                + #"\#(machineJSON("cm_2", "Cloud computer 3", provider: "instabox", computer: #""def""#, state: "running", snapshots: false))],"#
                + #""available":true,"limit":3,"providers":[{"provider":"boat","name":"Boat","region":"EU","keyPrompt":"boat_…",\#(setup),"credential":{"hint":"boat_…9abc"}},"#
                + #"{"provider":"instabox","name":"InstaBox","region":"US","keyPrompt":"ibk_…",\#(setup),"credential":{"hint":"ibk_…1234","problem":"Your InstaBox key expired.","blocking":true}},"#
                + #"{"provider":"nimbus","name":"Nimbus","region":"Asia","keyPrompt":"nim_…",\#(setup),"credential":null}]}"#)
        }
    }

    /// The JSON it was sent: URLSession hands a protocol the body as a stream.
    private static func body(of request: URLRequest) -> Data {
        if let data = request.httpBody { return data }
        guard let stream = request.httpBodyStream else { return Data() }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 1024)
        while stream.hasBytesAvailable {
            let n = stream.read(&buffer, maxLength: buffer.count)
            guard n > 0 else { break }
            data.append(buffer, count: n)
        }
        return data
    }
}

/// Several cloud computers, oldest first: a key goes only once no computer runs on its provider, a new one gets the first
/// free default name and fills the account up to its limit, a rename reaches the account's computer while only that
/// machine shows busy, and a deleted one takes its computer out of the account.
@MainActor @Test func severalCloudComputersAreNamedRenamedAndDeleted() async {
    let store = CloudMachineStore(cloud: stubbedCloud(FleetStub.self))
    let removed = OSAllocatedUnfairLock(initialState: [ComputerID?]())
    store.onAccountChanged = { id in removed.withLock { $0.append(id) } }
    // Each rename, with which machines showed busy meanwhile.
    let renamed = OSAllocatedUnfairLock(initialState: [String]())
    store.onRenamed = { [store] id, name in
        let busy = "\(store.isBusy("cm_2")) \(store.isBusy("cm_1"))"
        renamed.withLock { $0.append("\(id) \(name) \(busy)") }
    }
    defer { store.retire() }

    await store.refresh()
    #expect(store.machines.map(\.name) == ["Cloud computer", "Cloud computer 3"])
    #expect(store.machines.map(\.snapshots) == [true, false])
    #expect(!store.canRemoveKey("boat") && !store.canRemoveKey("instabox") && store.canRemoveKey("nimbus"))
    #expect(store.providers.map(\.isReady) == [true, false, false])
    // New cloud computer creates where the key works: InstaBox's is refused, Nimbus has none yet (picked, it's chosen
    // once it has one).
    #expect(store.provider(picked: nil) == "boat" && store.provider(picked: "instabox") == "boat" && store.provider(picked: "nimbus") == "boat")
    #expect(store.suggestedName == "Cloud computer 2" && !store.atLimit && store.limitNote == nil)

    #expect(await store.create(on: "boat", name: "  "))
    #expect(FleetStub.created.withLock { $0 } == ["provider": "boat", "name": "Cloud computer 2"])
    #expect(store.machines.map(\.id) == ["cm_1", "cm_2", "cm_3"] && store.isPreparing && store.atLimit)
    // The cloud's own words for a create over the limit, which the catalogs translate.
    #expect(store.limitNote == "You can have up to 3 cloud computers. Delete one to make another.")
    #expect(store.suggestedName == "Cloud computer 4")

    await store.rename("cm_2", to: " Lab ")
    #expect(store.machines.map(\.name) == ["Cloud computer", "Lab", "Cloud computer 2"])
    #expect(renamed.withLock { $0 } == ["def Lab true false"])
    // Unchanged or empty: nothing to send.
    await store.rename("cm_2", to: "Lab")
    await store.rename("cm_1", to: " ")

    await store.delete("cm_1")
    #expect(store.machines.map(\.id) == ["cm_2", "cm_3"] && store.canRemoveKey("boat") == false)
    #expect(removed.withLock { $0 } == ["abc"])
    #expect(store.error == nil && store.working == nil)
    #expect(FleetStub.requests.withLock { $0 }.filter { !$0.hasPrefix("GET") }
        == ["POST /v1/cloud-computers", "PATCH /v1/cloud-computers/cm_2", "DELETE /v1/cloud-computers/cm_1"])
}

/// A cloud computer whose provider turns the key down says what to fix in its state's place, as an error; the other
/// shows its state.
@MainActor @Test func cloudComputerShowsItsKeysProblemInPlaceOfItsState() async {
    let store = CloudMachineStore(cloud: stubbedCloud(FleetStub.self))
    defer { store.retire() }
    await store.refresh()
    #expect(store.machines.map { store.status(of: $0).text } == ["Running", "Your InstaBox key expired."])
    #expect(store.machines.map { store.status(of: $0).isError } == [false, true])
}

/// A key turned down for the plan or the account still creates, which tries again; one turned down for its own sake
/// (revoked, expired) doesn't until it's replaced.
@Test func onlyAKeyRefusedForItsOwnSakeStopsCreating() throws {
    func provider(_ credential: String) throws -> CloudProvider {
        try JSONDecoder().decode(CloudProvider.self, from: Data(
            #"{"provider":"boat","name":"Boat","region":"EU","keyPrompt":"boat_…","setup":{"page":"https://boat.dev","commands":""},"credential":\#(credential)}"#.utf8))
    }
    #expect(try provider(#"{"hint":"boat_…9abc","problem":"This account needs an active plan.","blocking":false}"#).isReady)
    #expect(try !provider(#"{"hint":"boat_…9abc","problem":"This key has expired. Make a new one.","blocking":true}"#).isReady)
    #expect(try !provider("null").isReady)
}

/// A name stops where the cloud's 64 UTF-16 units do, without splitting a character.
@Test func cloudComputerNamesStopAtTheCloudsLimit() {
    #expect(CloudMachineStore.limited(String(repeating: "a", count: 70)).count == 64)
    #expect(CloudMachineStore.limited("short") == "short")
    // Each flag is four UTF-16 units: 16 fit.
    #expect(CloudMachineStore.limited(String(repeating: "🇹🇼", count: 20)) == String(repeating: "🇹🇼", count: 16))
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

/// A provider that keeps no snapshots (InstaBox) says so, for the apps to leave *Delete snapshots* out; older clouds
/// don't say, and all of theirs kept some.
@Test func cloudMachineSaysWhetherItKeepsSnapshots() throws {
    let decode = { (json: String) in try JSONDecoder().decode(CloudMachine.self, from: Data(json.utf8)) }
    let instabox = try decode(#"{"cloudComputerId":"cm_1","name":"Lab","provider":"instabox","providerName":"InstaBox","region":"US","state":"running","snapshots":false}"#)
    #expect(instabox.snapshots == false && instabox.name == "Lab" && instabox.id == "cm_1" && instabox.isPreparing)
    #expect(try decode(#"{"cloudComputerId":"cm_2","name":"Cloud computer","provider":"boat","providerName":"Boat","region":"EU","state":"running"}"#).snapshots)
}
