import Foundation

// Wire types for the Codync cloud (`/v1`, cloud/src/api.ts) and the relay mailbox. Times are epoch milliseconds.
// Decoding is lenient: unknown fields are ignored and anything the server may omit is optional.

/// A computer in the account (`GET /v1/computers`). Its `boxKey` is for display only.
public struct CloudComputer: Codable, Hashable, Sendable, Identifiable {
    public var computerId: ComputerID
    public var name: String
    public var platform: String?
    public var device: String?
    /// Delivered by the cloud: trusted only after the SAS matched (`AccessTicket.signKey`).
    public var signKey: String
    public var boxKey: String?
    public var version: String?
    public var online: Bool?
    public var lastSeenAt: Int64?
    public var claimedAt: Int64?
    /// `none` · `pending` · `granted` (only when the request carried this device's signature).
    public var access: String?
    /// Set for a Codync Cloud computer; nil for the user's own (and from clouds that predate it).
    public var cloud: CloudHosting?

    public var id: ComputerID { computerId }
    public var isOnline: Bool { online ?? false }
    /// This device was approved on it through the account.
    public var isGranted: Bool { access == "granted" }
}

/// Who runs a Codync Cloud computer, where, and whether it's awake (`cloud` in `GET /v1/computers`).
public struct CloudHosting: Codable, Hashable, Sendable {
    public var provider: String
    /// Shown as the cloud names it ("Boat"), never hard-coded here.
    public var providerName: String
    public var region: String
    public var state: CloudMachineState

    /// "Runs on Boat · EU", translated: under the computer's name.
    public var runsOn: String {
        L10n.tr("Runs on {provider} · {region}").replacingOccurrences(of: "{provider}", with: providerName)
            .replacingOccurrences(of: "{region}", with: region)
    }
}

public enum CloudMachineState: String, Codable, Sendable {
    case starting, running, stopping, stopped, failed
    /// A state this version doesn't know.
    case unknown

    public init(from decoder: any Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown
    }

    /// Asleep or falling asleep: a connection or a message wakes it, so it isn't offline.
    public var isAsleep: Bool { self == .stopped || self == .stopping }
}

/// One of the account's Codync Cloud computers (`GET` / `POST` / `PATCH /v1/cloud-computers`).
public struct CloudMachine: Decodable, Hashable, Sendable, Identifiable {
    public var cloudComputerId: String
    /// The name the user gave it.
    public var name: String
    public var hosting: CloudHosting
    /// Nil until the machine has booted and claimed itself into the account.
    public var computerId: ComputerID?
    /// Whether its provider keeps snapshots (earlier states, a backup) that *Delete snapshots* erases; older clouds
    /// don't say, and they all did.
    public var snapshots: Bool

    public var id: String { cloudComputerId }
    /// Created, but not in the account yet (nor failed): it's booting and installing the host.
    public var isPreparing: Bool { computerId == nil && hosting.state != .failed }

    private enum CodingKeys: String, CodingKey { case cloudComputerId, name, computerId, snapshots }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        cloudComputerId = try c.decode(String.self, forKey: .cloudComputerId)
        name = try c.decode(String.self, forKey: .name)
        computerId = try c.decodeIfPresent(ComputerID.self, forKey: .computerId)
        snapshots = try c.decodeIfPresent(Bool.self, forKey: .snapshots) ?? true
        // Provider, region and state sit beside `computerId`.
        hosting = try CloudHosting(from: decoder)
    }
}

/// The account's Codync Cloud as the cloud describes it (`GET /v1/cloud-computers`).
public struct CloudStatus: Decodable, Hashable, Sendable {
    /// Oldest first.
    public var cloudComputers: [CloudMachine]?
    /// Whether this account may have them (the feature isn't public yet).
    public var available: Bool?
    /// How many it may have at once.
    public var limit: Int?
    /// Every provider the computers can run on, each with the account's key for it.
    public var providers: [CloudProvider]?
}

/// A provider the cloud can run the computer on, as it lists them: where it runs, how to make its key (worded by the
/// cloud, so a new provider needs no app release) and the account's key for it.
public struct CloudProvider: Decodable, Hashable, Sendable, Identifiable {
    public var provider: String
    public var name: String
    public var region: String
    /// What its keys start with, for the field's placeholder (`boat_…`).
    public var keyPrompt: String
    public var setup: CloudKeySetup
    public var credential: CloudCredential?
    /// Its own free plan, if it has one (older clouds don't say).
    public var offer: CloudProviderOffer?

    public var id: String { provider }
    /// The account gave it a key that isn't turned down for its own sake: a computer can be made there (or tried again,
    /// when the provider said no for the plan or the account).
    public var isReady: Bool { credential.map { $0.blocking != true } ?? false }
}

/// A key the account gave a provider, as the cloud shows it: never the key itself.
public struct CloudCredential: Decodable, Hashable, Sendable {
    /// Its kind and last four, `boat_…9abc`.
    public var hint: String
    /// What the owner has to fix while the provider turns the key down, in a sentence the app translates.
    public var problem: String?
    /// The key itself was turned down (revoked, expired): nothing can be made with it until it's replaced.
    public var blocking: Bool?
}

/// A provider's own free plan, in the cloud's words (the app translates `label` and `detail`), and its page.
public struct CloudProviderOffer: Decodable, Hashable, Sendable {
    public var label: String
    public var detail: String
    public var url: URL
}

/// How the user makes a provider's key: commands to run, and the provider's page for doing it there instead.
public struct CloudKeySetup: Decodable, Hashable, Sendable {
    public var page: URL
    public var commands: String
}

/// A started access request, after both nonces were exchanged: `code` is ready to show.
public struct AccessTicket: Codable, Hashable, Sendable, Identifiable {
    public var requestId: String
    public var expiresAt: Int64
    /// The 6-digit SAS to compare with the one the computer shows.
    public var code: String
    public var computerId: ComputerID
    /// The host key the SAS was computed with; pinned once the request is approved.
    public var signKey: String
    public var id: String { requestId }

    public init(requestId: String, expiresAt: Int64, code: String, computerId: ComputerID, signKey: String) {
        self.requestId = requestId
        self.expiresAt = expiresAt
        self.code = code
        self.computerId = computerId
        self.signKey = signKey
    }
}

public enum AccessState: String, Codable, Sendable {
    case pending, approved, denied, expired, cancelled

    public init(from decoder: Decoder) throws {
        // A state this version doesn't know is over as far as it can tell.
        self = AccessState(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .expired
    }
}

/// `GET /v1/access-requests/{id}`.
public struct AccessStatus: Codable, Hashable, Sendable {
    public var requestId: String
    public var computerId: ComputerID?
    public var status: AccessState
    public var expiresAt: Int64?
    public var hostNonce: String?
    public var grantId: String?
}

/// `GET /v1/devices`.
public struct CloudDevice: Codable, Hashable, Sendable, Identifiable {
    public var deviceId: String
    public var deviceKey: String
    public var name: String
    public var platform: String?
    public var createdAt: Int64?
    public var lastUsedAt: Int64?
    public var revoked: Bool?
    public var id: String { deviceId }
}

/// The cloud's word that this device key is in the account (`POST /v1/devices/certificate`), shown to a computer
/// when pairing; it checks the signature offline. `payload` is the signed JSON text, passed on unchanged.
public struct DeviceCertificate: Codable, Hashable, Sendable {
    public var payload: String
    public var sig: String

    public init(payload: String, sig: String) {
        self.payload = payload
        self.sig = sig
    }

    /// When computers stop taking it; 0 when the payload can't be read.
    public var exp: Int64 {
        struct Claims: Decodable { var exp: Int64 }
        return (try? JSONDecoder().decode(Claims.self, from: Data(payload.utf8)))?.exp ?? 0
    }

    /// Valid for more than another day: reused rather than asked for again.
    public func isFresh(at now: Date = .now) -> Bool { isValid(at: now.addingTimeInterval(86_400)) }

    public func isValid(at now: Date = .now) -> Bool { exp > Int64(now.timeIntervalSince1970 * 1000) }
}

/// A claim the account opened (`POST /v1/claims`), for a computer in no account to sign while pairing.
public struct CloudClaim: Codable, Hashable, Sendable {
    public var claimId: String
    public var nonce: String
}

/// `GET /v1/computers/{id}/grants`.
public struct CloudGrant: Codable, Hashable, Sendable, Identifiable {
    public var grantId: String
    public var deviceId: String
    public var deviceName: String?
    public var platform: String?
    public var scopes: [String]?
    public var createdAt: Int64?
    public var id: String { grantId }
}

/// A message waiting in the relay mailbox for a computer that is offline.
public struct QueuedItem: Codable, Hashable, Sendable, Identifiable {
    public var nonce: String
    public var exp: Int64?
    /// `queued`, or `delivering` (handed to the computer, not acknowledged yet).
    public var state: String
    public var id: String { nonce }
}

public enum MailboxEvent: Sendable, Equatable {
    case delivered(nonce: String)
    case failed(nonce: String, code: String)
    case expired(nonce: String)

    public var nonce: String {
        switch self {
        case let .delivered(nonce), let .failed(nonce, _), let .expired(nonce): nonce
        }
    }
}

public enum MailboxCancel: Sendable, Equatable {
    /// Removed before the computer saw it.
    case cancelled
    /// Already handed to the computer: it will run.
    case delivering
    /// The relay doesn't have it (delivered, expired, or not reachable right now).
    case unknown
}

/// Why the relay refused a mailbox message.
public enum MailboxError: LocalizedError, Sendable, Equatable {
    /// The computer came online: send over the channel instead.
    case hostOnline
    case rejected(String)

    public var errorDescription: String? {
        switch self {
        case .hostOnline: "Your computer just came online. Send again."
        case .rejected("full"): "Too many messages are waiting for this computer."
        case .rejected("tooLarge"): "This message is too long to hold while the computer is offline."
        case let .rejected(code): "Couldn't hold the message for your computer (\(code))."
        }
    }
}
