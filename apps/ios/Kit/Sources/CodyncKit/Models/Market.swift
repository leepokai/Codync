import Foundation

// Marketplace wire types (host/src/market/): connectors are MCP servers,
// skills are instruction folders; agents come from `Hello.backends`.

public struct InstalledConnector: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var description: String
    public var registryName: String?
    /// `local` (a command the agent starts), `remote` (a URL) or `composio` (an app connected through Composio).
    public var kind: String
    public var command: String?
    public var url: String?
    /// Names of the keys and headers that are set; their values stay on the computer.
    public var keys: [String]
    /// Composio apps: the app's logo.
    public var logo: String?
    /// Remote servers: `none`, `signedOut` (waits for sign-in; bots don't get it yet) or `signedIn`.
    public var auth: String?

    public var needsSignIn: Bool { auth == "signedOut" }
}

/// Where a connector's sign-in page is and who catches its return: `app` (the phone's
/// sign-in sheet, then `finishConnectorSignIn`) or `host` (the computer's own browser).
public struct ConnectorSignIn: Decodable, Sendable {
    public var url: String
    public var callback: String
}

// MARK: Composio (host/src/market/composio.rs): apps connected through composio.dev

public struct ComposioStatus: Decodable, Sendable {
    public var configured: Bool
    /// Where to get an API key.
    public var keyUrl: String
}

public struct ComposioApp: Decodable, Hashable, Sendable, Identifiable {
    public struct Connection: Decodable, Hashable, Sendable {
        public var id: String
        /// Composio's status: `active`, `initiated`, `failed`, `expired`…
        public var status: String
    }

    public var slug: String
    public var name: String
    public var description: String?
    public var logo: String?
    public var connection: Connection?
    public var id: String { slug }
    public var connected: Bool { connection?.status == "active" }
    public init(slug: String, name: String) {
        self.slug = slug; self.name = name
    }
}

/// What connecting an app needs: a sign-in page, or fields for a key-based app.
public struct ComposioConnect: Decodable, Sendable {
    public struct Field: Decodable, Hashable, Sendable {
        public var name: String
        public var label: String
        public var description: String?
        public var secret: Bool
        public var required: Bool
    }

    /// `redirect` or `needsFields`.
    public var status: String
    public var url: String?
    public var connection: String?
    public var mode: String?
    public var fields: [Field]?
}

public struct ComposioConnectionState: Decodable, Sendable {
    public var id: String
    public var status: String
}

public struct MarketConnector: Codable, Hashable, Sendable, Identifiable {
    public var name: String
    public var title: String
    public var description: String?
    public var version: String?
    public var website: String?
    public var installed: Bool
    public var options: [InstallOption]
    public var id: String { name }

    public struct InstallOption: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        /// npm | pypi | oci | remote
        public var kind: String
        public var label: String
        public var inputs: [Input]
    }

    public struct Input: Codable, Hashable, Sendable, Identifiable {
        public var name: String
        public var description: String?
        public var secret: Bool
        public var required: Bool
        public var `default`: String?
        public var placeholder: String?
        public var id: String { name }
    }
}

/// Skills, connectors, rules and plugins an agent keeps for itself (`agentGear`). Codync's own skills and connectors are separate.
public struct AgentGear: Decodable, Sendable, Equatable {
    public var skills: [AgentGearItem]
    public var connectors: [AgentGearItem]
    public var rules: [AgentGearItem]
    public var plugins: [AgentGearItem]

    public init(skills: [AgentGearItem] = [], connectors: [AgentGearItem] = [], rules: [AgentGearItem] = [], plugins: [AgentGearItem] = []) {
        self.skills = skills
        self.connectors = connectors
        self.rules = rules
        self.plugins = plugins
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        skills = try c.decodeIfPresent([AgentGearItem].self, forKey: .skills) ?? []
        connectors = try c.decodeIfPresent([AgentGearItem].self, forKey: .connectors) ?? []
        rules = try c.decodeIfPresent([AgentGearItem].self, forKey: .rules) ?? []
        plugins = try c.decodeIfPresent([AgentGearItem].self, forKey: .plugins) ?? []
    }

    private enum CodingKeys: String, CodingKey { case skills, connectors, rules, plugins }
}

public struct AgentGearItem: Decodable, Sendable, Equatable, Identifiable {
    public var name: String
    public var description: String
    public var detail: String
    public var id: String { name }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        detail = try c.decodeIfPresent(String.self, forKey: .detail) ?? ""
    }

    private enum CodingKeys: String, CodingKey { case name, description, detail }
}

public struct InstalledSkill: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var description: String
    public var source: String
    public var path: String
}

public struct MarketSkill: Codable, Hashable, Sendable, Identifiable {
    /// Folder name in anthropics/skills.
    public var source: String
    public var name: String
    public var description: String
    public var installed: Bool
    public var id: String { source }
}

private struct Items<T: Decodable>: Decodable { var items: [T] }
private struct Installed: Decodable { var connector: InstalledConnector }

public extension HostClient {
    func connectors() async throws -> [InstalledConnector] {
        let res: Items<InstalledConnector> = try await call("connectors")
        return res.items
    }

    /// A page of the MCP Registry (featured first), or of a search (can take ~30 s).
    /// Pass the returned cursor to get the next page; nil means there is none.
    func marketConnectors(search: String, cursor: String? = nil) async throws -> (items: [MarketConnector], next: String?) {
        struct Page: Decodable { var items: [MarketConnector]; var nextCursor: String? }
        let res: Page = try await call("marketConnectors", ["search": search, "cursor": cursor ?? ""], timeout: 60)
        return (res.items, res.nextCursor)
    }

    @discardableResult
    func installConnector(registryName: String, option: String, inputs: [String: String]) async throws -> InstalledConnector {
        struct Body: Encodable { var registryName: String; var option: String; var inputs: [String: String] }
        let res: Installed = try await call("installConnector", Body(registryName: registryName, option: option, inputs: inputs), timeout: 45)
        return res.connector
    }

    /// A connector you describe yourself: a command line with env, or an https URL with headers.
    @discardableResult
    func addConnector(name: String, command: String?, url: String?, env: [String: String], headers: [String: String]) async throws -> InstalledConnector {
        struct Body: Encodable { var name: String; var command: String?; var url: String?; var env: [String: String]; var headers: [String: String] }
        let res: Installed = try await call("installConnector", Body(name: name, command: command, url: url, env: env, headers: headers), timeout: 45)
        return res.connector
    }

    /// Adds every server in a pasted MCP config (`mcpServers` / `servers` JSON, or one server's entry).
    func importConnectors(config: String) async throws -> [InstalledConnector] {
        let res: Items<InstalledConnector> = try await call("importConnectors", ["config": config], timeout: 60)
        return res.items
    }

    func connectorSignIn(_ id: String) async throws -> ConnectorSignIn {
        try await call("connectorSignIn", ["id": id], timeout: 60)
    }

    @discardableResult
    func finishConnectorSignIn(state: String, code: String?, error: String?) async throws -> InstalledConnector {
        struct Body: Encodable { var state: String; var code: String?; var error: String? }
        let res: Installed = try await call("connectorSignInFinish", Body(state: state, code: code, error: error), timeout: 60)
        return res.connector
    }

    func removeConnector(_ id: String) async throws {
        let _: Empty = try await call("removeConnector", ["id": id])
    }

    func composioStatus() async throws -> ComposioStatus { try await call("composioStatus") }

    /// Checks the key with Composio before keeping it; an empty key forgets Composio.
    func setComposioKey(_ key: String) async throws -> ComposioStatus {
        try await call("setComposioKey", ["key": key], timeout: 60)
    }

    func composioApps(search: String) async throws -> [ComposioApp] {
        let res: Items<ComposioApp> = try await call("composioToolkits", ["search": search], timeout: 60)
        return res.items
    }

    func composioConnect(_ app: String) async throws -> ComposioConnect {
        try await call("composioConnect", ["toolkit": app], timeout: 60)
    }

    func composioConnect(_ app: String, mode: String, fields: [String: String]) async throws -> ComposioConnectionState {
        struct Body: Encodable { var toolkit: String; var mode: String; var fields: [String: String] }
        return try await call("composioConnectFields", Body(toolkit: app, mode: mode, fields: fields), timeout: 60)
    }

    func composioConnection(_ id: String) async throws -> ComposioConnectionState {
        try await call("composioConnection", ["id": id], timeout: 30)
    }

    /// What `backend` already has in the user's home: skills, connectors, rules and plugins, not Codync's.
    func agentGear(backend: String) async throws -> AgentGear {
        try await call("agentGear", ["backend": backend])
    }

    func skills() async throws -> [InstalledSkill] {
        let res: Items<InstalledSkill> = try await call("skills")
        return res.items
    }

    func marketSkills() async throws -> [MarketSkill] {
        let res: Items<MarketSkill> = try await call("marketSkills", Empty(), timeout: 60)
        return res.items
    }

    func installSkill(source: String) async throws {
        let _: Empty = try await call("installSkill", ["source": source], timeout: 60)
    }

    func addSkill(name: String, description: String, instructions: String) async throws {
        let _: Empty = try await call("installSkill", ["name": name, "description": description, "instructions": instructions])
    }

    func removeSkill(_ id: String) async throws {
        let _: Empty = try await call("removeSkill", ["id": id])
    }

    /// A skill's `SKILL.md` without its front matter. `which` is `["id": …]` for an installed
    /// skill or `["source": …]` for one from the catalog.
    func skillDetails(_ which: [String: String]) async throws -> String {
        struct Details: Decodable { var markdown: String }
        let res: Details = try await call("skillDetails", which, timeout: 60)
        return res.markdown
    }
}

public struct ConnectionRequest: Codable, Hashable, Sendable {
    public var toolkit: String?
    public var kind: String
    public var status: String
    public var title: String
    public var botId: String
    public var registryName: String?
    public var connectorId: String?
    public var field: String?
    public var location: String?
    /// Login requests: the website's domain or the app's name.
    public var site: String?
    /// Agent sign-in requests: the agent (backend id) to sign in to.
    public var backend: String?
    /// App requests: the app's logo, when Composio lists it.
    public var logo: String?
}

/// A saved website or app login; the password stays on the computer.
public struct SavedLogin: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var site: String
    public var username: String
}

public struct CredentialStatus: Decodable, Sendable {
    public var provider: String
    public var onePasswordConnected: Bool
}

public extension HostClient {
    func connectorInfo(_ name: String) async throws -> MarketConnector {
        try await call("connectorInfo", ["registryName": name], timeout: 60)
    }

    func verifyConnector(_ id: String) async throws {
        struct Verified: Decodable { var status: String }
        let _: Verified = try await call("connectorVerify", ["id": id], timeout: 120)
    }

    func finishConnectionRequest(_ entryId: String, connectorId: String? = nil, value: String? = nil, username: String? = nil) async throws {
        struct Body: Encodable { var entryId: String; var connectorId: String?; var value: String?; var username: String? }
        struct Response: Decodable { var entry: Entry }
        let _: Response = try await call("connectorRequestFinish", Body(entryId: entryId, connectorId: connectorId, value: value, username: username), timeout: 120)
    }

    func logins() async throws -> [SavedLogin] {
        let res: Items<SavedLogin> = try await call("credentialLogins", timeout: 60)
        return res.items
    }

    func saveLogin(site: String, username: String, password: String) async throws {
        struct Response: Decodable { var login: SavedLogin }
        let _: Response = try await call("credentialSaveLogin", ["site": site, "username": username, "password": password], timeout: 60)
    }

    func removeLogin(_ id: String) async throws {
        let _: Empty = try await call("credentialRemoveLogin", ["id": id], timeout: 60)
    }

    func credentialStatus() async throws -> CredentialStatus { try await call("credentialStatus", timeout: 60) }
    func setOnePasswordToken(_ token: String) async throws -> CredentialStatus {
        try await call("credentialSetOnePassword", ["token": token], timeout: 60)
    }
}

public extension HostClient {
    func updateConnectorCredentials(_ id: String, fields: [String: String]) async throws {
        struct Body: Encodable { var id: String; var fields: [String: String] }
        struct Result: Decodable { var status: String }
        let _: Result = try await call("credentialUpdateConnector", Body(id: id, fields: fields), timeout: 120)
    }
}
