import CodyncKit
import Foundation

/// Metadata only. Provider credentials stay in the host's isolated profile.
struct AgentAccount: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let backend: String
    let createdAt: Int64
}

extension BotStore {
    func loadAgentModels(backend: String, account: String?) async throws -> AgentModels {
        let computerID = computer.id
        let accountID = storage.id
        let client = try await ready()
        let host = hostId
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        let response = try await client.agentModels(backend, account: account)
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        return response
    }

    func loadAgentAccounts(backend: String) async throws -> [AgentAccount] {
        struct Response: Decodable, Sendable { let accounts: [AgentAccount] }
        let computerID = computer.id
        let accountID = storage.id
        let client = try await ready()
        let host = hostId
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        let response: Response = try await client.call("listAccounts", ["backend": backend], timeout: 30)
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        return response.accounts.filter { $0.backend == backend }
    }

    /// The provider's browser sign-in runs on the computer, not on this phone.
    func authenticateAgentAccount(backend: String) async throws -> AgentAccount {
        struct Response: Decodable, Sendable { let account: AgentAccount }
        let computerID = computer.id
        let accountID = storage.id
        let client = try await ready()
        let host = hostId
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        let response: Response = try await client.call("accountAuthenticate", ["backend": backend], timeout: 660)
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        guard response.account.backend == backend else { throw HostError.unreachable }
        return response.account
    }

    /// Removes an isolated account's profile; the host refuses one a bot still uses.
    func deleteAgentAccount(id: String) async throws {
        let computerID = computer.id
        let accountID = storage.id
        let client = try await ready()
        let host = hostId
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        _ = try await client.call("deleteAccount", ["accountId": id], timeout: 30)
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        await refreshBackends()
    }

    /// Signs out of the computer's own account, leaving its isolated profiles alone.
    func signOutAgent(backend: String) async throws {
        let computerID = computer.id
        let accountID = storage.id
        let client = try await ready()
        let host = hostId
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        _ = try await client.call("agentSignOut", ["backend": backend], timeout: 60)
        try checkAgentAccountContext(computer: computerID, account: accountID, host: host)
        await refreshBackends()
    }

    private func checkAgentAccountContext(computer computerID: String, account accountID: String, host: String?) throws {
        try Task.checkCancellation()
        guard !retired, computer.id == computerID, storage.id == accountID, hostId == host,
              connection == .online else { throw CancellationError() }
    }
}
