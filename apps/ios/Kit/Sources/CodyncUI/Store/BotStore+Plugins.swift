import CodyncKit
import Foundation

extension BotStore {
    public func refreshPlugins() async {
        guard let client else { return }
        async let c = client.connectors()
        async let s = client.skills()
        if let c = try? await c { installedConnectors = c }
        if let s = try? await s { installedSkills = s }
    }

    public func marketConnectors(search: String, cursor: String? = nil) async throws -> (items: [MarketConnector], next: String?) {
        try await ready().marketConnectors(search: search, cursor: cursor)
    }

    public func marketSkills() async throws -> [MarketSkill] {
        try await ready().marketSkills()
    }

    @discardableResult
    public func installConnector(_ item: MarketConnector, option: String, inputs: [String: String]) async throws -> InstalledConnector {
        let c = try await ready().installConnector(registryName: item.name, option: option, inputs: inputs)
        await refreshPlugins()
        return c
    }

    @discardableResult
    public func addConnector(name: String, command: String?, url: String?, env: [String: String], headers: [String: String]) async throws -> InstalledConnector {
        let c = try await ready().addConnector(name: name, command: command, url: url, env: env, headers: headers)
        await refreshPlugins()
        return c
    }

    public func importConnectors(config: String) async throws -> [InstalledConnector] {
        let added = try await ready().importConnectors(config: config)
        await refreshPlugins()
        return added
    }

    public func connectorSignIn(_ id: String) async throws -> ConnectorSignIn {
        try await ready().connectorSignIn(id)
    }

    public func finishConnectorSignIn(state: String, code: String?, error: String?) async throws {
        try await ready().finishConnectorSignIn(state: state, code: code, error: error)
        await refreshPlugins()
    }

    /// Waits while the user signs in in the computer's browser (the host catches the return).
    public func waitForSignIn(_ id: String) async throws {
        for _ in 0..<150 {
            try await Task.sleep(for: .seconds(2))
            await refreshPlugins()
            if installedConnectors.first(where: { $0.id == id })?.needsSignIn == false { return }
        }
    }

    public func removeConnector(_ id: String) async throws {
        try await ready().removeConnector(id)
        await refreshPlugins()
    }

    public func installSkill(source: String) async throws {
        try await ready().installSkill(source: source)
        await refreshPlugins()
    }

    public func addSkill(name: String, description: String, instructions: String) async throws {
        try await ready().addSkill(name: name, description: description, instructions: instructions)
        await refreshPlugins()
    }

    public func removeSkill(_ id: String) async throws {
        try await ready().removeSkill(id)
        await refreshPlugins()
    }

    func skillDetails(_ skill: SkillRef) async throws -> String {
        try await ready().skillDetails(skill.request)
    }

    /// Re-detects agents on the computer (after an install or sign-in).
    public func refreshBackends() async {
        guard let client, let backends = try? await client.refreshBackends() else { return }
        hello?.backends = backends
    }
}
