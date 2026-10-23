import Foundation

/// Model identifiers and display names advertised by the selected computer's ACP agent.
public struct AgentModels: Decodable, Sendable {
    public struct Model: Decodable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let description: String?
        /// The model's own settings (effort, fast…); absent from hosts that don't list them.
        public let options: [Option]?
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
    }

    public let models: [Model]
    public let currentModelId: String?
    /// The agent offers Fast mode (absent on older hosts).
    public let fastMode: Bool?

    /// The settings to show for a bot's model. Default (`""`) shows those of the model the agent
    /// runs by default, which is the one a bot on Default gets.
    public func settings(for model: String) -> [Option] {
        let id = model.isEmpty ? currentModelId : model
        return models.first { $0.id == id }?.options ?? []
    }
}
