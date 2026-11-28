import Foundation

/// What a thread the user starts runs on instead of its bot's setup (`startThread`'s `setup`, the desktop's
/// `ThreadSetup`). A thread without a bot names all of it: there is no bot's setup to fall back on.
public struct ThreadSetup: Encodable, Hashable, Sendable {
    public struct Model: Encodable, Hashable, Sendable {
        /// Nil: the agent's default.
        public var id: String?
        public var options: [String: String]
    }

    public struct Approval: Encodable, Hashable, Sendable {
        /// `ask` / `auto`.
        public var permission: String
        /// Nil: the agent's default mode.
        public var mode: String?
    }

    public var backend: String
    /// The agent's login profile; "" its default login.
    public var loginProfile: String
    public var cwd: String?
    public var model: Model
    public var approval: Approval

    /// A thread without a bot on `login`'s agent, in `home`, on `model`: it asks, in the agent's Auto mode, as a new
    /// bot does (the desktop's `PLAIN_APPROVAL`).
    public static func plain(_ login: AgentLogin, home: String?, model: String?, options: [String: String]) -> ThreadSetup {
        ThreadSetup(backend: login.backend, loginProfile: login.loginProfile ?? "", cwd: home,
                    model: Model(id: model, options: options), approval: Approval(permission: "ask", mode: "auto"))
    }
}
