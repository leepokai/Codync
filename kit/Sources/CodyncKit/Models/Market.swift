import Foundation

// Marketplace wire types (host/src/market.rs): connectors are MCP servers,
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
}

// MARK: Composio (host/src/composio.rs): apps connected through composio.dev

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

public extension HostClient {
    func connectors() async throws -> [InstalledConnector] {
        let res: Items<InstalledConnector> = try await call("connectors")
        return res.items
    }

    /// Featured connectors, or MCP Registry search results (search can take ~30 s).
    func marketConnectors(search: String) async throws -> [MarketConnector] {
        let res: Items<MarketConnector> = try await call("marketConnectors", ["search": search], timeout: 60)
        return res.items
    }

    func installConnector(registryName: String, option: String, inputs: [String: String]) async throws {
        struct Body: Encodable { var registryName: String; var option: String; var inputs: [String: String] }
        let _: Empty = try await call("installConnector", Body(registryName: registryName, option: option, inputs: inputs), timeout: 30)
    }

    /// A connector you describe yourself: a command line, or an https URL with optional headers.
    func addConnector(name: String, command: String?, url: String?, env: [String: String]) async throws {
        struct Body: Encodable { var name: String; var command: String?; var url: String?; var env: [String: String] }
        let _: Empty = try await call("installConnector", Body(name: name, command: command, url: url, env: env))
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
}
