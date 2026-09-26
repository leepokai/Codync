import Foundation

// Wire types for the Codync cloud (`/v1`, cloud/src/api.ts), the host's loopback-only
// account methods (host/src/api/mod.rs) and the relay mailbox. Times are epoch milliseconds.
// Decoding is lenient: unknown fields are ignored and anything the server may omit is optional.

/// `GET /v1/me`.
public struct CloudAccount: Codable, Hashable, Sendable {
    public var userId: String
    public var email: String?
    public var createdAt: Int64?
}

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

    public var id: ComputerID { computerId }
    public var isOnline: Bool { online ?? false }
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

/// A pending request as the computer shows it (loopback `accessRequests`).
public struct AccessRequest: Codable, Hashable, Sendable, Identifiable {
    public var requestId: String
    public var deviceKey: String
    public var deviceName: String
    public var platform: String?
    public var email: String?
    /// The SAS; nil until the phone revealed its nonce (Approve stays disabled).
    public var code: String?
    public var createdAt: Int64?
    public var expiresAt: Int64?
    public var id: String { requestId }
}

/// A device the host authorized (loopback `devices`).
public struct AuthorizedDevice: Codable, Hashable, Sendable, Identifiable {
    public var key: String
    public var name: String
    public var platform: String?
    /// `local` (QR) or `account`.
    public var source: String
    public var scopes: [String]?
    public var leaseUntil: Int64?
    public var createdAt: Int64?
    public var lastSeenAt: Int64?
    public var connected: Bool?
    public var id: String { key }
}

public struct CloudOwner: Codable, Hashable, Sendable {
    public var userId: String
    public var email: String?
}

/// The host's cloud connection (loopback `cloudStatus`, `setCloud`, `cloud` events).
public struct CloudStatus: Codable, Hashable, Sendable {
    public var enabled: Bool
    public var url: String?
    public var registered: Bool?
    public var connected: Bool?
    public var owner: CloudOwner?
    public var lastError: String?
    /// Missing from older hosts: they always ask for the code.
    public var approval: AccountApproval?
}

/// How devices on the computer's account get in (loopback `setApproval`).
public enum AccountApproval: String, Codable, Hashable, Sendable {
    /// Compare the 6-digit code and approve each device at the computer.
    case code
    /// Let them in without anyone comparing the code.
    case auto

    public init(from decoder: any Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .code
    }
}

/// `POST /v1/claims`.
public struct ClaimChallenge: Codable, Hashable, Sendable {
    public var claimId: String
    public var nonce: String
    public var expiresAt: Int64
}

/// What the host signs for a claim (loopback `claimSign`), passed on to `/v1/claims/{id}/complete`.
public struct ClaimSignature: Codable, Hashable, Sendable {
    public var computerId: ComputerID
    public var signKey: String
    public var boxKey: String
    public var name: String
    public var platform: String
    public var device: String?
    public var version: String
    public var sig: String
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
