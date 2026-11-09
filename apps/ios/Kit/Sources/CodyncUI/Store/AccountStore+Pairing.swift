import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Account")

/// Pairing with a computer, and bringing one into this account (docs/plans/optional-sign-in.md).
extension AccountStore {
    /// QR pairing (§4.1). Signed in, with this account's certificate: a computer in another account turns the phone
    /// away, one in none asks its user, who may have it join this account. Signed out: only a computer in no account
    /// lets it in, and not over the Codync relay. The computer is saved in this context and connected.
    public func pair(_ pairing: Pairing, deviceName: String, platform: String) async throws -> Computer {
        let identity = try DeviceIdentity.load(context: storage)
        let pairing = cloud == nil ? pairing.withoutRelay : pairing
        guard !pairing.computer.urls.isEmpty || pairing.computer.cloud != nil else { throw Self.noDirectAddress }
        var certificate: DeviceCertificate?
        var claim: CloudClaim?
        if let cloud {
            certificate = try await pairingCertificate(cloud, deviceName: deviceName, platform: platform)
            // Without a claim the computer still pairs and stays out of the account; the phone can add it later.
            do { claim = try await cloud.createClaim() } catch {
                log.info("pairing without a claim: \(error.localizedDescription, privacy: .public)")
            }
        }
        pairingCheck = pairing.checkCode(for: identity)
        defer { pairingCheck = nil }
        let paired = try await HostConnector.pair(pairing, identity: identity, deviceName: deviceName, platform: platform,
                                                  certificate: certificate, claim: claim)
        guard !retired else { throw CancellationError() }
        var computer = paired.computer
        if let previous = saved.first(where: { $0.id == computer.id }) { computer.keepDevicePicks(from: previous) }
        save(computer)
        open(computer)
        if let claim, let signed = paired.claim {
            Task {
                do { try await joinAccount(computer.id) { _ in (claim, signed) } } catch {
                    log.error("paired computer didn't join the account: \(error.localizedDescription, privacy: .public)")
                }
            }
        }
        return computer
    }

    public static let noCertificate = CloudError(status: 0, code: "noCertificate",
                                                 message: "Connect to the internet to pair, so Codync can confirm your account.")
    public static let noDirectAddress = CloudError(status: 0, code: "noDirectAddress",
                                                   message: "This computer shows no Wi-Fi or Tailscale address. Connect it to a network, or sign in to pair through Cloudflare.")
    public static let signInForShortCode = CloudError(status: 0, code: "signInForShortCode",
                                                      message: "Short codes need an account. Scan the computer's QR code instead, or sign in.")

    /// This device's certificate: the one held while more than a day is left, else a new one, registering
    /// the device again if the cloud doesn't know it. Offline, one that is still valid will do.
    func pairingCertificate(_ cloud: CloudClient, deviceName: String, platform: String) async throws -> DeviceCertificate {
        if let certificate, certificate.isFresh() { return certificate }
        do {
            let fresh: DeviceCertificate
            do {
                fresh = try await cloud.deviceCertificate()
            } catch let error as CloudError where error.status == 403 {
                try await cloud.registerDevice(name: deviceName, platform: platform)
                fresh = try await cloud.deviceCertificate()
            }
            certificate = fresh
            return fresh
        } catch let error as CloudError where !error.isTransient {
            throw error
        } catch {
            if let certificate, certificate.isValid() { return certificate }
            log.info("no device certificate: \(error.localizedDescription, privacy: .public)")
            throw Self.noCertificate
        }
    }

    /// Whether this phone can offer to add a computer it reaches to its account: it's signed in, and the computer is
    /// in no account yet.
    public func canAddToAccount(_ store: BotStore) -> Bool {
        cloud != nil && store.connection == .online && store.hello?.canJoinAccount == true
    }

    /// The 6 digits the computer shows while it's asked to join this account; nil when it isn't being asked.
    public func addToAccountCheck(_ id: ComputerID) -> String? { addingToAccount[id] }

    /// Add to account: the computer asks its user, then signs the claim. Without it, the computer stays reachable
    /// only directly.
    public func addToAccount(_ id: ComputerID, deviceName: String, platform: String) {
        guard let store = stores[id], let client = store.client, addingToAccount[id] == nil, !joining.contains(id),
              let identity = try? DeviceIdentity.load(context: storage) else { return }
        Analytics.shared.capture(.addToAccountAsked)
        addingToAccount[id] = store.computer.checkCode(for: identity)
        Task {
            defer { addingToAccount[id] = nil }
            do {
                try await joinAccount(id) { cloud in
                    let certificate = try await self.pairingCertificate(cloud, deviceName: deviceName, platform: platform)
                    let claim = try await cloud.createClaim()
                    return (claim, try await client.joinAccount(claim, certificate: certificate))
                }
                Analytics.shared.capture(.addToAccountCompleted)
            } catch {
                log.error("computer didn't join the account: \(error.localizedDescription, privacy: .public)")
                Analytics.shared.capture(.addToAccountFailed(.init(error)))
                if !retired { lastError = error.localizedDescription }
            }
        }
    }

    /// The computer signs this account's claim (`sign`: while letting this phone in, or later when asked to from
    /// here), and the phone, which holds the account's session, completes it. One at a time per computer.
    private func joinAccount(_ id: ComputerID, sign: @MainActor (CloudClient) async throws -> (CloudClaim, Data)) async throws {
        guard let cloud, !retired, joining.insert(id).inserted else { return }
        defer { joining.remove(id) }
        let (claim, signed) = try await sign(cloud)
        try await cloud.completeClaim(claimId: claim.claimId, claim: signed)
        guard !retired else { return }
        await refreshCloud()
    }
}
