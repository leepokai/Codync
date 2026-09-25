import CryptoKit
import Foundation
import Testing
@testable import CodyncKit

/// docs/remote-relay-vectors.json, the single source of truth shared with the host and the cloud.
struct Vectors: Sendable {
    let data: Data
    var json: [String: Any] { (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:] }

    static let shared = Vectors(data: (try? Data(contentsOf: URL(filePath: #filePath).deletingLastPathComponent()
            .appending(path: "../../../docs/remote-relay-vectors.json").standardized)) ?? Data())

    subscript(_ section: String) -> [String: Any] { json[section] as? [String: Any] ?? [:] }

    func b64(_ section: String, _ key: String) -> Data { Data(base64URL: self[section][key] as? String ?? "") ?? Data() }
    func str(_ section: String, _ key: String) -> String { self[section][key] as? String ?? "" }

    var hostSign: Curve25519.Signing.PrivateKey { try! .init(rawRepresentation: b64("keys", "hostSignSeed")) }
    var hostBox: Curve25519.KeyAgreement.PrivateKey { try! .init(rawRepresentation: b64("keys", "hostBoxPriv")) }
    var deviceSign: Curve25519.Signing.PrivateKey { try! .init(rawRepresentation: b64("keys", "deviceSignSeed")) }
    var computer: Computer {
        Computer(id: str("keys", "computerId"), name: "Mac", signKey: str("keys", "hostSignPub"), boxKey: str("keys", "hostBoxPub"))
    }
    var identity: DeviceIdentity { DeviceIdentity(signing: deviceSign, push: try! .init(rawRepresentation: b64("push", "pushPriv"))) }
}

private let v = Vectors.shared
private func raw(_ key: SymmetricKey) -> Data { key.withUnsafeBytes { Data($0) } }

@Test func vectorFileLoads() {
    #expect(!v.json.isEmpty)
}

@Test func base64URLIsStrict() {
    #expect(Data([0xfb, 0xff]).base64URL == "-_8")
    #expect(Data(base64URL: "-_8") == Data([0xfb, 0xff]))
    #expect(Data(base64URL: "+/8") == nil)
    #expect(Data(base64URL: "-_8=") == nil)
    #expect(Data(base64URL: "A") == nil)
}

@Test func keysAndComputerId() {
    #expect(v.hostSign.publicKey.rawRepresentation.base64URL == v.str("keys", "hostSignPub"))
    #expect(v.hostBox.publicKey.rawRepresentation.base64URL == v.str("keys", "hostBoxPub"))
    #expect(v.deviceSign.publicKey.rawRepresentation.base64URL == v.str("keys", "deviceSignPub"))
    #expect(RelayCrypto.computerId(signKey: v.b64("keys", "hostSignPub")) == v.str("keys", "computerId"))
    #expect(v.computer.isConsistent)
    #expect(v.identity.publicKey == v.str("keys", "deviceSignPub"))
    #expect(v.identity.pushKey == v.str("push", "pushPub"))
}

@Test func handshake() throws {
    let ekD = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: v.b64("handshake", "deviceEphPriv"))
    let ekH = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: v.b64("handshake", "hostEphPriv"))
    #expect(ekD.publicKey.rawRepresentation == v.b64("handshake", "deviceEphPub"))
    #expect(ekH.publicKey.rawRepresentation == v.b64("handshake", "hostEphPub"))
    let hs = try RelayCrypto.Handshake(computer: v.computer, deviceKey: v.deviceSign.publicKey.rawRepresentation,
                                       ephemeral: ekD, n: v.b64("handshake", "n"))
    #expect(hs.signInput == v.b64("handshake", "hs1SignInput"))
    // CryptoKit's Ed25519 signatures are randomized: check both verify rather than compare bytes.
    #expect(v.deviceSign.publicKey.isValidSignature(v.b64("handshake", "hs1Sig"), for: hs.signInput))
    #expect(v.deviceSign.publicKey.isValidSignature(try v.identity.sign(hs.signInput), for: hs.signInput))

    let th = RelayCrypto.transcriptHash(cid: hs.cid, dk: hs.dk, ekD: hs.ekD, n: hs.n, ekH: ekH.publicKey.rawRepresentation)
    #expect(th == v.b64("handshake", "transcriptHash"))
    #expect(try RelayCrypto.sharedSecret(ekD, ekH.publicKey.rawRepresentation) == v.b64("handshake", "sharedSecret"))

    let keys = try hs.finish(ekH: ekH.publicKey.rawRepresentation, sig: v.b64("handshake", "hs2Sig"))
    #expect(keys.prk == v.b64("handshake", "prk"))
    #expect(raw(keys.d2h) == v.b64("handshake", "kD2H"))
    #expect(raw(keys.h2d) == v.b64("handshake", "kH2D"))

    // A welcome signed by any other key is an attack, not a retry.
    let forged = try Curve25519.Signing.PrivateKey().signature(for: th)
    #expect(throws: RelayCryptoError.badSignature) { try hs.finish(ekH: ekH.publicKey.rawRepresentation, sig: forged) }
}

@Test func zeroSharedSecretIsRejected() throws {
    let priv = Curve25519.KeyAgreement.PrivateKey()
    #expect(throws: (any Error).self) { try RelayCrypto.sharedSecret(priv, Data(count: 32)) }
}

@Test func frames() throws {
    let kD2H = SymmetricKey(data: v.b64("handshake", "kD2H"))
    let kH2D = SymmetricKey(data: v.b64("handshake", "kH2D"))
    let frames = v.json["frames"] as? [[String: Any]] ?? []
    #expect(frames.count == 3)
    for f in frames {
        let c = UInt64(f["c"] as? Int ?? -1)
        let final = f["final"] as? Bool ?? true
        let message = Data((f["message"] as? String ?? "").utf8)
        let d = Data(base64URL: f["d"] as? String ?? "") ?? Data()
        #expect(Data([final ? 0 : 1]) + message == Data(base64URL: f["plaintext"] as? String ?? ""))
        let key = f["dir"] as? String == "d2h" ? kD2H : kH2D
        #expect(try RelayCrypto.sealFrame(key: key, counter: c, final: final, chunk: message) == d)
        let opened = try RelayCrypto.openFrame(key: key, counter: c, d: d)
        #expect(opened.final == final && opened.chunk == message)
    }
    // Device side: the first d2h message, then the h2d answer.
    var sealer = RelayCrypto.FrameSealer(key: kD2H)
    let first = try sealer.seal(Data("{\"id\":1,\"m\":\"hello\",\"b\":{}}".utf8))
    #expect(first.count == 1 && first[0].c == 0 && first[0].d.base64URL == frames[0]["d"] as? String)
    var opener = RelayCrypto.FrameOpener(key: kH2D)
    let answer = try opener.open(c: 0, d: Data(base64URL: frames[1]["d"] as? String ?? "") ?? Data())
    #expect(answer == Data("{\"id\":1,\"ok\":{\"name\":\"Mac\"}}".utf8))
}

@Test func chunkingReassemblesAndEnforcesOrderAndLimit() throws {
    let key = SymmetricKey(size: .bits256)
    let big = Data((0..<(600 * 1024)).map { UInt8($0 % 251) })
    var sealer = RelayCrypto.FrameSealer(key: key)
    let frames = try sealer.seal(big)
    #expect(frames.map(\.c) == [0, 1, 2])
    var opener = RelayCrypto.FrameOpener(key: key)
    #expect(try opener.open(c: 0, d: frames[0].d) == nil)
    #expect(try opener.open(c: 1, d: frames[1].d) == nil)
    #expect(try opener.open(c: 2, d: frames[2].d) == big)

    // Replayed or reordered frames close the channel.
    var strict = RelayCrypto.FrameOpener(key: key)
    #expect(throws: RelayCryptoError.badCounter) { try strict.open(c: 1, d: frames[1].d) }
    var small = RelayCrypto.FrameOpener(key: key, limit: 300 * 1024)
    _ = try small.open(c: 0, d: frames[0].d)
    #expect(throws: RelayCryptoError.tooLarge) { try small.open(c: 1, d: frames[1].d) }
    // Empty messages are still one final frame.
    var empty = RelayCrypto.FrameSealer(key: key)
    #expect(try empty.seal(Data()).count == 1)
}

@Test func sas() {
    let dk = v.b64("keys", "deviceSignPub")
    let nD = v.b64("sas", "deviceNonce")
    #expect(RelayCrypto.sasCommit(deviceKey: dk, deviceNonce: nD) == v.b64("sas", "commit"))
    #expect(RelayCrypto.sasCode(hostSignKey: v.b64("keys", "hostSignPub"), deviceKey: dk, deviceNonce: nD,
                                hostNonce: v.b64("sas", "hostNonce")) == v.str("sas", "code"))
}

@Test func claimAndACLSignaturesVerify() {
    let host = v.hostSign.publicKey
    #expect(host.isValidSignature(v.b64("claim", "sig"), for: Data(v.str("claim", "canonical").utf8)))
    #expect(v.b64("acl", "d") == Data(v.str("acl", "json").utf8))
    #expect(host.isValidSignature(v.b64("acl", "sig"), for: v.b64("acl", "d")))
}

@Test func offerId() {
    #expect(RelayCrypto.offerId(code: v.b64("pairing", "code")) == v.str("pairing", "offerId"))
}

@Test func mailbox() throws {
    let eph = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: v.b64("mailbox", "ephPriv"))
    #expect(eph.publicKey.rawRepresentation == v.b64("mailbox", "ephPub"))
    #expect(try RelayCrypto.sharedSecret(eph, v.b64("keys", "hostBoxPub")) == v.b64("mailbox", "sharedSecret"))
    let cid = try #require(RelayCrypto.computerIdRaw(v.str("keys", "computerId")))
    let dk = v.b64("keys", "deviceSignPub")
    #expect(raw(RelayCrypto.mailboxKey(sharedSecret: v.b64("mailbox", "sharedSecret"), cid: cid, dk: dk, epk: eph.publicKey.rawRepresentation))
        == v.b64("mailbox", "key"))
    let sealed = try RelayCrypto.sealMailbox(Data(v.str("mailbox", "plaintext").utf8), hostBoxKey: v.b64("keys", "hostBoxPub"),
                                             computerId: v.str("keys", "computerId"), deviceKey: dk,
                                             clientNonce: v.str("mailbox", "clientNonce"), sign: v.identity.sign, ephemeral: eph)
    #expect(sealed.ciphertext == v.b64("mailbox", "ciphertext"))
    let signed = RelayCrypto.label("codync/mbox/v1") + cid + sealed.epk + sealed.ciphertext
    #expect(v.deviceSign.publicKey.isValidSignature(sealed.sig, for: signed))
    #expect(v.deviceSign.publicKey.isValidSignature(v.b64("mailbox", "sig"), for: signed))
    #expect(sealed.epk + v.b64("mailbox", "sig") + sealed.ciphertext == v.b64("mailbox", "blob"))
    #expect(sealed.blob.count == v.b64("mailbox", "blob").count)
}

@Test func mailboxNeverReusesEphemeralKeys() throws {
    let seal = {
        try RelayCrypto.sealMailbox(Data("same".utf8), hostBoxKey: v.b64("keys", "hostBoxPub"), computerId: v.str("keys", "computerId"),
                                    deviceKey: v.b64("keys", "deviceSignPub"), clientNonce: "n", sign: v.identity.sign)
    }
    let a = try seal()
    let b = try seal()
    #expect(a.epk != b.epk)
    #expect(a.ciphertext != b.ciphertext)
}

@Test func push() throws {
    let pushPriv = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: v.b64("push", "pushPriv"))
    let eph = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: v.b64("push", "ephPriv"))
    #expect(eph.publicKey.rawRepresentation == v.b64("push", "ephPub"))
    let ss = try RelayCrypto.sharedSecret(pushPriv, eph.publicKey.rawRepresentation)
    #expect(ss == v.b64("push", "sharedSecret"))
    let cid = try #require(RelayCrypto.computerIdRaw(v.str("keys", "computerId")))
    #expect(raw(RelayCrypto.pushKey(sharedSecret: ss, cid: cid, pushKey: v.b64("push", "pushPub"), epk: eph.publicKey.rawRepresentation))
        == v.b64("push", "key"))
    #expect(try RelayCrypto.openPush(sealed: v.str("push", "sealed"), computerId: v.str("keys", "computerId"), pushPrivate: pushPriv)
        == Data(v.str("push", "plaintext").utf8))
    let alert = try #require(DeviceIdentity.openPush(sealed: v.str("push", "sealed"), computerId: v.str("keys", "computerId"), key: pushPriv))
    #expect(alert.title == "Reviewer" && alert.body == "Run cargo test --all?")
    // Bound to the computer (aad) and to this device's key.
    #expect(DeviceIdentity.openPush(sealed: v.str("push", "sealed"), computerId: "AAAAAAAAAAAAAAAAAAAAAA", key: pushPriv) == nil)
    #expect(DeviceIdentity.openPush(sealed: v.str("push", "sealed"), computerId: v.str("keys", "computerId"), key: .init()) == nil)
}

@Test func requestSignature() throws {
    let input = RelayCrypto.requestSigInput(method: "get", authority: "Codync-Cloud-Staging.example.workers.dev",
                                            pathAndQuery: v.str("requestSig", "pathAndQuery"), ts: 1_790_000_000_000,
                                            nonce: v.str("requestSig", "nonce"), body: Data())
    #expect(input == Data(v.str("requestSig", "canonical").utf8))
    #expect(RelayCrypto.sha256(Data()).base64URL == v.str("requestSig", "bodySha256"))
    #expect(v.deviceSign.publicKey.isValidSignature(v.b64("requestSig", "sig"), for: input))

    let header = try v.identity.signatureHeader(method: "GET", authority: v.str("requestSig", "authority"),
                                                pathAndQuery: v.str("requestSig", "pathAndQuery"), body: Data(),
                                                ts: 1_790_000_000_000, nonce: v.str("requestSig", "nonce"))
    let expected = v.str("requestSig", "header").replacingOccurrences(of: "Codync-Sig: ", with: "")
    #expect(header.components(separatedBy: ",sig=")[0] == expected.components(separatedBy: ",sig=")[0])
    let sig = try #require(Data(base64URL: header.components(separatedBy: ",sig=")[1]))
    #expect(v.deviceSign.publicKey.isValidSignature(sig, for: input))

    let url = try #require(URL(string: "https://Codync-Cloud-Staging.example.workers.dev/v1/relay/device/NHUPmL1Z_PyUbaRaqr6TOw?v=1"))
    #expect(RelayCrypto.authority(of: url) == v.str("requestSig", "authority"))
    #expect(RelayCrypto.pathAndQuery(of: url) == v.str("requestSig", "pathAndQuery"))
    #expect(RelayCrypto.authority(of: URL(string: "http://127.0.0.1:8787/v1")!) == "127.0.0.1:8787")
    #expect(RelayCrypto.authority(of: URL(string: "https://a.dev:443/v1")!) == "a.dev")
}
