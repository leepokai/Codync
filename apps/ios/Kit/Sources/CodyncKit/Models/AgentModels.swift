import Foundation

/// Model identifiers and display names advertised by the selected computer's ACP agent.
public struct AgentModels: Decodable, Sendable {
    public struct Model: Decodable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let description: String?
        /// The model's own settings (effort, fast…); absent from hosts that don't list them.
        public let options: [Option]?
        public let optionsError: String?
    }

    /// A model setting and the values the agent accepts.
    public struct Option: Decodable, Identifiable, Sendable {
        public struct Value: Decodable, Sendable {
            public let id: String
            public let name: String
        }

        public let id: String
        public let name: String
        public let values: [Value]
        public let `default`: String?
        public let kind: String?
        public var isReasoning: Bool {
            kind == "reasoning" || ["effort", "reasoning_effort", "reasoning-effort", "thought_level"].contains(id)
        }
    }

    public let models: [Model]
    public let currentModelId: String?
    /// The agent offers Fast mode (absent on older hosts).
    public let fastMode: Bool?
    public let defaultModelOptions: Bool?
}
