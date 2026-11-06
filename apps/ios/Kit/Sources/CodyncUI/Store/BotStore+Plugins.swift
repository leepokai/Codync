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

    public func marketAgents(search: String = "", category: String = "") async throws -> [MarketAgent] {
        try await ready().marketAgents(search: search, category: category)
    }

    public func marketAgent(_ id: String) async throws -> MarketAgentDetail {
        try await ready().marketAgent(id)
    }

    public func marketApps(search: String = "") async throws -> [MarketApp] {
        try await ready().marketApps(search: search)
    }

    /// Creates the agent's bot and selects it.
    @discardableResult
    public func installAgent(_ id: String) async throws -> Bot {
        let bot = try await ready().installAgent(id)
        bots[bot.id] = bot
        selection = bot.id
        onRosterChanged?()
        return bot
    }

    /// Adds an app with the values typed. Its sheet runs the sign-in when the connector needs one.
    @discardableResult
    public func installApp(_ app: MarketApp, values: [String: String]) async throws -> InstalledConnector {
        var headers: [String: String] = [:]
        var env: [String: String] = [:]
        var url = app.server.url
        for field in app.fields {
            let typed = values[field.id]?.trimmingCharacters(in: .whitespaces) ?? ""
            if field.url { if !typed.isEmpty { url = typed } }
            else if let header = field.header { headers[header] = typed }
            else if let name = field.env { env[name] = typed }
        }
        return try await addConnector(name: app.server.name.isEmpty ? app.name : app.server.name,
                                      command: app.server.command, url: url, env: env, headers: headers,
                                      args: app.server.args, catalog: app.id, description: app.description)
    }

    @discardableResult
    public func installConnector(_ item: MarketConnector, option: String, inputs: [String: String]) async throws -> InstalledConnector {
        let c = try await ready().installConnector(registryName: item.name, option: option, inputs: inputs)
        await refreshPlugins()
        return c
    }

    @discardableResult
    public func addConnector(name: String, command: String?, url: String?, env: [String: String], headers: [String: String],
                             args: [String] = [], catalog: String? = nil, description: String? = nil) async throws -> InstalledConnector {
        let c = try await ready().addConnector(name: name, command: command, url: url, env: env, headers: headers,
                                               args: args, catalog: catalog, description: description)
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
