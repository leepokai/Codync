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
    /// A connector set up with a token reads `none` until then, so this waits for `signedIn` itself.
    public func waitForSignIn(_ id: String) async throws {
        for _ in 0..<150 {
            try await Task.sleep(for: .seconds(2))
            await refreshPlugins()
            if installedConnectors.first(where: { $0.id == id })?.auth == "signedIn" { return }
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

    /// What `backend` already has in the user's home.
    public func agentBrings(_ backend: String) async throws -> AgentGear {
        try await ready().agentGear(backend: backend)
    }

    /// The agent list a reply carries replaces the one on screen. The reply is awaited before this is called:
    /// `hello?.backends = try await …` tells the screens of the change as the request leaves, so they draw the old
    /// list again and miss the new one.
    private func showBackends(_ backends: [Backend]) {
        hello?.backends = backends
    }

    /// A new login profile for `backend`: an account one's sign-in follows, an API key one's key form.
    func addLoginProfile(_ backend: String, kind: LoginProfile.Kind) async throws -> LoginProfile {
        let added = try await ready().addLoginProfile(backend, kind: kind)
        showBackends(added.backends)
        return added.loginProfile
    }

    /// After a login profile's sign-in terminal closes: whether it went through, and as whom.
    func checkLoginProfile(_ backend: String, loginProfile: String) async throws {
        showBackends(try await ready().loginProfileAuth(backend, loginProfile: loginProfile))
    }

    /// For an agent that signs its login profiles in itself: waits for the browser sign-in on the computer.
    func signInLoginProfile(_ backend: String, loginProfile: String) async throws {
        showBackends(try await ready().signInLoginProfile(backend, loginProfile: loginProfile))
    }

    /// Names a login profile (`loginProfile` nil: the default one); an empty name clears it.
    func renameLoginProfile(_ backend: String, loginProfile: String?, name: String) async throws {
        showBackends(try await ready().renameLoginProfile(backend, loginProfile: loginProfile, name: name))
    }

    /// Fails, in words to show, while something still uses the login profile.
    func removeLoginProfile(_ loginProfile: String) async throws {
        showBackends(try await ready().removeLoginProfile(loginProfile))
    }

    /// Saves an API key login profile's typed values (an empty one removes its variable) and imports `importing`
    /// from the computer's login shell.
    func setLoginProfileKeys(_ loginProfile: String, vars: [String: String] = [:], importing: [String] = []) async throws {
        showBackends(try await ready().setLoginProfileKeys(loginProfile, vars: vars, importing: importing))
    }

    /// The variable names the computer's login shell has, to import.
    func loginShellVariables() async throws -> [String] {
        try await ready().loginShellVariables()
    }

    /// Re-detects agents on the computer (after an install or sign-in).
    public func refreshBackends() async {
        guard let client, let backends = try? await client.refreshBackends() else { return }
        hello?.backends = backends
    }
}
