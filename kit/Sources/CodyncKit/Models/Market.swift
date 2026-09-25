import Foundation

// Marketplace wire types (host/src/market.rs): connectors are MCP servers,
// skills are instruction folders; agents come from `Hello.backends`.

public struct InstalledConnector: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var description: String
    public var registryName: String?
    /// `local` (a command the agent starts) or `remote` (a URL).
    public var kind: String
    public var command: String?
    public var url: String?
    /// Names of the keys and headers that are set; their values stay on the computer.
    public var keys: [String]
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
