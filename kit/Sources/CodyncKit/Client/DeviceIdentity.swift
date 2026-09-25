import CryptoKit
import Foundation
import Security

/// This app install's keys for one account context (§3.2): an Ed25519 device key that hosts
/// authorize, and an X25519 push key that opens notification contents. Private keys stay in the Keychain.
public struct DeviceIdentity: Sendable {
    /// Ed25519, b64url.
    public let publicKey: String
    /// X25519, b64url (§6.7).
    public let pushKey: String
    let signing: Curve25519.Signing.PrivateKey
    let push: Curve25519.KeyAgreement.PrivateKey

    init(signing: Curve25519.Signing.PrivateKey, push: Curve25519.KeyAgreement.PrivateKey) {
        self.signing = signing
        self.push = push
        publicKey = signing.publicKey.rawRepresentation.base64URL
        pushKey = push.publicKey.rawRepresentation.base64URL
    }

    static let deviceService = "com.pokai.Codync.device-key"
    static let pushService = "com.pokai.Codync.push-key"

    /// Loads this context's keys, creating whichever is missing.
    public static func load(context: SharedStore.Context) throws -> DeviceIdentity {
        let signing = try Keychain.read(service: deviceService, account: context.id)
            .map { try Curve25519.Signing.PrivateKey(rawRepresentation: $0) }
            ?? Keychain.create(service: deviceService, account: context.id) { Curve25519.Signing.PrivateKey() }
        let push = try Keychain.read(service: pushService, account: context.id)
            .map { try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: $0) }
            ?? Keychain.create(service: pushService, account: context.id) { Curve25519.KeyAgreement.PrivateKey() }
        return DeviceIdentity(signing: signing, push: push)
    }

    /// Signing out or removing an account: hosts and the cloud can no longer tie anything to it.
    public static func delete(context: SharedStore.Context) {
        Keychain.delete(service: deviceService, account: context.id)
        Keychain.delete(service: pushService, account: context.id)
    }

    var deviceKey: Data { signing.publicKey.rawRepresentation }

    func sign(_ data: Data) throws -> Data { try signing.signature(for: data) }

    /// The `Codync-Sig` header value (§5).
    public func signatureHeader(method: String, authority: String, pathAndQuery: String, body: Data) throws -> String {
        try signatureHeader(method: method, authority: authority, pathAndQuery: pathAndQuery, body: body,
                            ts: Int64(Date.now.timeIntervalSince1970 * 1000), nonce: Data.random(16).base64URL)
    }

    func signatureHeader(method: String, authority: String, pathAndQuery: String, body: Data, ts: Int64, nonce: String) throws -> String {
        let input = RelayCrypto.requestSigInput(method: method, authority: authority, pathAndQuery: pathAndQuery, ts: ts, nonce: nonce, body: body)
        return "v=1,kid=\(publicKey),ts=\(ts),nonce=\(nonce),sig=\(try sign(input).base64URL)"
    }

    /// For the Notification Service Extension: opens §6.7's `sealed` with the push key of
    /// `contextID`. Never creates keys; nil when anything doesn't check out.
    public static func openPush(sealed: String, computerId: ComputerID, contextID: String) -> (title: String, body: String)? {
        guard let raw = try? Keychain.read(service: pushService, account: contextID),
              let key = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: raw) else { return nil }
        return openPush(sealed: sealed, computerId: computerId, key: key)
    }

    static func openPush(sealed: String, computerId: ComputerID, key: Curve25519.KeyAgreement.PrivateKey) -> (title: String, body: String)? {
        struct Alert: Decodable { var title: String; var body: String }
        guard let plain = try? RelayCrypto.openPush(sealed: sealed, computerId: computerId, pushPrivate: key),
              let alert = try? JSONDecoder().decode(Alert.self, from: plain) else { return nil }
        return (alert.title, alert.body)
    }
}

/// Generic-password items holding 32-byte raw keys (§3.2): this device only, never synced,
/// readable after first unlock (pushes arrive while locked). macOS uses the login keychain:
/// the Mac app has no entitlements, so the data protection keychain would fail with -34018.
enum Keychain {
    struct Failure: LocalizedError {
        let status: OSStatus
        var errorDescription: String? { "Keychain error \(status)" }
    }

    /// iOS app, widgets and notification extension share `CodyncKeychainGroup` from Info.plist.
    /// An unsigned build leaves `$(AppIdentifierPrefix)` empty; a group without the team prefix
    /// would fail with -34018, so it is left unset there.
    private static let accessGroup: String? = {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "CodyncKeychainGroup") as? String,
              group.range(of: #"^[A-Z0-9]{10}\."#, options: .regularExpression) != nil else { return nil }
        return group
    }()

    private static func query(service: String, account: String) -> [CFString: Any] {
        var q: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecAttrSynchronizable: false,
        ]
        if let accessGroup, !accessGroup.isEmpty { q[kSecAttrAccessGroup] = accessGroup }
        return q
    }

    static func read(service: String, account: String) throws -> Data? {
        var q = query(service: service, account: account)
        q[kSecReturnData] = true
        q[kSecMatchLimit] = kSecMatchLimitOne
        var out: CFTypeRef?
        switch SecItemCopyMatching(q as CFDictionary, &out) {
        case errSecSuccess: return out as? Data
        case errSecItemNotFound: return nil
        case let status: throw Failure(status: status)
        }
    }

    /// Stores a new key; if another process won the race, returns the one it stored.
    static func create<K: RawKey>(service: String, account: String, _ make: () -> K) throws -> K {
        let key = make()
        var q = query(service: service, account: account)
        q[kSecValueData] = key.rawRepresentation
        q[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        switch SecItemAdd(q as CFDictionary, nil) {
        case errSecSuccess: return key
        case errSecDuplicateItem:
            guard let raw = try read(service: service, account: account) else { throw Failure(status: errSecItemNotFound) }
            return try K(rawRepresentation: raw)
        case let status: throw Failure(status: status)
        }
    }

    static func delete(service: String, account: String) {
        SecItemDelete(query(service: service, account: account) as CFDictionary)
    }
}

protocol RawKey {
    var rawRepresentation: Data { get }
    init<D: ContiguousBytes>(rawRepresentation: D) throws
}

extension Curve25519.Signing.PrivateKey: RawKey {}
extension Curve25519.KeyAgreement.PrivateKey: RawKey {}
