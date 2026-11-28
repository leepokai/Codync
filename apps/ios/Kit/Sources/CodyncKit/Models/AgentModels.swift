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
    /// The agent's modes, its Auto mode first and listed as `auto` (nil from older hosts).
    public let modes: [AgentMode]?
}

public extension AgentModels {
    /// The agent's own "Default (recommended)" entry: the same choice as leaving the model unset.
    var agentDefault: Model? { models.first { $0.name.lowercased().hasPrefix("default") } }

    /// What a model picker lists (Android's `AgentModels.options`): Default first (id "", the model left unset, the
    /// agent's own default folded into it), the agent's other models, then `selection` when the agent doesn't list it.
    /// `catalog` nil (not loaded yet): Default and `selection`.
    static func choices(_ catalog: AgentModels?, selection: String?) -> [(id: String, name: String)] {
        let fallback = catalog?.agentDefault
        let available = (catalog?.models ?? []).filter { $0.id != fallback?.id }
        let current = available.first { $0.id == catalog?.currentModelId }?.name
        var result = [(id: "", name: fallback?.name ?? current.map { "Default · \($0)" } ?? "Default")]
        result += available.map { (id: $0.id, name: $0.name) }
        if let selection, !selection.isEmpty, selection != fallback?.id, !available.contains(where: { $0.id == selection }) {
            result.append((id: selection, name: selection))
        }
        return result
    }

    /// `model` as a picker's list has it: "" for Default (unset, or the agent's own default).
    static func listValue(_ catalog: AgentModels?, model: String?) -> String {
        model == catalog?.agentDefault?.id ? "" : model ?? ""
    }

    /// The listed model `model` runs on: the agent's default (or current) one when unset.
    static func selected(_ catalog: AgentModels?, model: String?) -> Model? {
        catalog?.models.first { $0.id == (model ?? catalog?.agentDefault?.id ?? catalog?.currentModelId) }
    }

    /// The settings shown for `model`: none on Default unless the agent gives its default model settings.
    static func settings(_ catalog: AgentModels?, model: String?) -> [Option] {
        guard !listValue(catalog, model: model).isEmpty || catalog?.defaultModelOptions == true else { return [] }
        return selected(catalog, model: model)?.options ?? []
    }

    /// Whether Standard / Fast shows for `backend`: the agent offers it, it is on, or `bot`'s sessions on that agent do.
    static func showsFast(_ catalog: AgentModels?, backend: String, on: Bool, bot: Bot?) -> Bool {
        (backend != "custom" && catalog?.fastMode == true) || on || (bot?.backend == backend && bot?.fastModeShown == true)
    }
}
