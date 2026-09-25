import Foundation

/// An error from the Codync cloud: `{"error":{"code","message"}}`.
public struct CloudError: LocalizedError, Sendable, Equatable {
    public var status: Int
    /// `notFound`, `conflict`, `requestExpired`, … (spec §8.3).
    public var code: String
    public var message: String?

    public init(status: Int, code: String, message: String? = nil) {
        self.status = status
        self.code = code
        self.message = message
    }

    public var errorDescription: String? {
        switch code {
        case "requestExpired": "The request expired. Ask again."
        case "accountDeleted": "This account was deleted."
        case "rateLimited": "Too many requests. Try again in a little while."
        default: message ?? "Codync cloud error (\(code))"
        }
    }
}

/// The user-facing `/v1` API (§8.4): account, devices, computers, access requests, claims.
/// The Clerk session token comes from the app; kit doesn't depend on ClerkKit.
public struct CloudClient: Sendable {
    public let baseURL: URL
    let identity: DeviceIdentity?
    let token: @Sendable () async throws -> String
    let session: URLSession
    /// How often `requestAccess` polls for the host's nonce.
    var pollInterval: Duration = .seconds(2)

    public init(baseURL: URL, identity: DeviceIdentity?, token: @escaping @Sendable () async throws -> String) {
        self.init(baseURL: baseURL, identity: identity, session: .shared, token: token)
    }

    init(baseURL: URL, identity: DeviceIdentity?, session: URLSession, token: @escaping @Sendable () async throws -> String) {
        self.baseURL = baseURL
        self.identity = identity
        self.session = session
        self.token = token
    }

    // MARK: account and devices

    public func me() async throws -> CloudAccount { try await request("GET", "v1/me") }

    /// Registers this device key with the account (Clerk + proof of the key).
    public func registerDevice(name: String, platform: String) async throws {
        struct Res: Decodable {}
        let _: Res = try await request("POST", "v1/devices", body: ["name": name, "platform": platform], signed: true)
    }

    public func devices() async throws -> [CloudDevice] {
        struct Res: Decodable { var devices: [CloudDevice] }
        let res: Res = try await request("GET", "v1/devices")
        return res.devices
    }

    public func revokeDevice(_ deviceId: String) async throws {
        try await send("DELETE", "v1/devices/\(deviceId)")
    }

    // MARK: computers

    /// With a device identity, each computer says whether this device has access.
    public func computers() async throws -> [CloudComputer] {
        struct Res: Decodable { var computers: [CloudComputer] }
        let res: Res = try await request("GET", "v1/computers", signed: identity != nil)
        return res.computers
    }

    public func renameComputer(_ id: ComputerID, name: String) async throws {
        try await send("PATCH", "v1/computers/\(id)", body: ["name": name])
    }

    /// Removes the computer from the account; every device's access to it through the account ends.
    public func removeComputer(_ id: ComputerID) async throws {
        try await send("DELETE", "v1/computers/\(id)")
    }

    public func grants(_ id: ComputerID) async throws -> [CloudGrant] {
        struct Res: Decodable { var grants: [CloudGrant] }
        let res: Res = try await request("GET", "v1/computers/\(id)/grants")
        return res.grants
    }

    public func revokeGrant(_ id: ComputerID, grantId: String) async throws {
        try await send("DELETE", "v1/computers/\(id)/grants/\(grantId)")
    }

    // MARK: access requests (§4.2 B, commit-then-reveal SAS)

    /// Commit → wait for the host's nonce → reveal. Returns once `code` can be shown.
    /// The device nonce lives only in memory here, so a cloud can't learn it before the host committed.
    public func requestAccess(_ id: ComputerID, signKey: String) async throws -> AccessTicket {
        guard let identity else { throw CloudError(status: 0, code: "noDeviceKey", message: "Sign in again on this device.") }
        guard let hostKey = Data(base64URL: signKey), hostKey.count == 32, RelayCrypto.computerId(signKey: hostKey) == id else {
            throw CloudError(status: 0, code: "badSignKey", message: "This computer's key doesn't match its ID.")
        }
        let nD = Data.random(32)
        let commit = RelayCrypto.sasCommit(deviceKey: identity.deviceKey, deviceNonce: nD).base64URL
        struct Created: Decodable { var requestId: String; var expiresAt: Int64 }
        let created: Created = try await request("POST", "v1/computers/\(id)/access-requests", body: ["commit": commit], signed: true)

        var hostNonce: Data?
        while hostNonce == nil {
            let status = try await accessStatus(created.requestId)
            guard status.status == .pending else {
                throw CloudError(status: 0, code: status.status == .expired ? "requestExpired" : status.status.rawValue, message: "The computer didn't take the request.")
            }
            if let raw = status.hostNonce {
                guard let n = Data(base64URL: raw), n.count == 32 else { throw CloudError(status: 0, code: "badNonce", message: nil) }
                hostNonce = n
            } else {
                guard Int64(Date.now.timeIntervalSince1970 * 1000) < created.expiresAt else {
                    throw CloudError(status: 410, code: "requestExpired", message: nil)
                }
                try await Task.sleep(for: pollInterval)
            }
        }
        try await send("POST", "v1/access-requests/\(created.requestId)/reveal", body: ["nonce": nD.base64URL], signed: true)
        let code = RelayCrypto.sasCode(hostSignKey: hostKey, deviceKey: identity.deviceKey, deviceNonce: nD, hostNonce: hostNonce ?? Data())
        return AccessTicket(requestId: created.requestId, expiresAt: created.expiresAt, code: code, computerId: id, signKey: signKey)
    }

    public func accessStatus(_ requestId: String) async throws -> AccessStatus {
        try await request("GET", "v1/access-requests/\(requestId)")
    }

    public func cancelAccess(_ requestId: String) async throws {
        try await send("DELETE", "v1/access-requests/\(requestId)")
    }

    // MARK: claims (§4.2 A)

    public func createClaim() async throws -> ClaimChallenge { try await request("POST", "v1/claims", body: [String: String]()) }

    public func completeClaim(_ claimId: String, signed: ClaimSignature) async throws -> CloudComputer {
        struct Res: Decodable { var computer: CloudComputer }
        let res: Res = try await request("POST", "v1/claims/\(claimId)/complete", body: signed)
        return res.computer
    }

    // MARK: plumbing

    private struct NoBody: Encodable {}
    private struct Failure: Decodable {
        struct Detail: Decodable { var code: String; var message: String? }
        var error: Detail
    }
    private struct Ignored: Decodable {}

    private func send(_ method: String, _ path: String, body: (some Encodable)? = NoBody?.none, signed: Bool = false) async throws {
        let _: Ignored = try await request(method, path, body: body, signed: signed)
    }

    func request<T: Decodable>(_ method: String, _ path: String, body: (some Encodable)? = NoBody?.none, signed: Bool = false) async throws -> T {
        let url = baseURL.appending(path: path)
        var req = URLRequest(url: url, timeoutInterval: 20)
        req.httpMethod = method
        let data = try body.map { try JSONEncoder().encode($0) } ?? Data()
        if body != nil {
            req.httpBody = data
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        req.setValue("Bearer \(try await token())", forHTTPHeaderField: "Authorization")
        if signed, let identity {
            let header = try identity.signatureHeader(method: method, authority: RelayCrypto.authority(of: url),
                                                      pathAndQuery: RelayCrypto.pathAndQuery(of: url), body: data)
            req.setValue(header, forHTTPHeaderField: "Codync-Sig")
        }
        let (bytes, response) = try await session.data(for: req)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let e = try? JSONDecoder().decode(Failure.self, from: bytes)
            throw CloudError(status: status, code: e?.error.code ?? "http\(status)", message: e?.error.message)
        }
        return try JSONDecoder().decode(T.self, from: bytes.isEmpty ? Data("{}".utf8) : bytes)
    }
}
