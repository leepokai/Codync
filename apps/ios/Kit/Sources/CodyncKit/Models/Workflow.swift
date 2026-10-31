import Foundation

// The graph a workflow saves. Field names match the host's camelCase JSON.
// An unknown step kind stays on the node so a later host's graph can be shown read-only.

public enum WorkflowNodeKind: Hashable, Sendable {
    case trigger, prompt, agent, condition, approval, output, text, note, shape
    case unknown(String)

    public var raw: String {
        switch self {
        case .trigger: "trigger"
        case .prompt: "prompt"
        case .agent: "agent"
        case .condition: "condition"
        case .approval: "approval"
        case .output: "output"
        case .text: "text"
        case .note: "note"
        case .shape: "shape"
        case let .unknown(raw): raw
        }
    }

    public var isUnknown: Bool {
        if case .unknown = self { true } else { false }
    }

    public var isAnnotation: Bool {
        switch self {
        case .text, .note, .shape: true
        default: false
        }
    }

    /// A step the walker runs. Notes, drawings and unknown kinds are not steps.
    public var isStep: Bool {
        switch self {
        case .trigger, .prompt, .agent, .condition, .approval, .output: true
        default: false
        }
    }
}

extension WorkflowNodeKind: Codable {
    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        switch raw {
        case "trigger": self = .trigger
        case "prompt": self = .prompt
        case "agent": self = .agent
        case "condition": self = .condition
        case "approval": self = .approval
        case "output": self = .output
        case "text": self = .text
        case "note": self = .note
        case "shape": self = .shape
        default: self = .unknown(raw)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(raw)
    }
}

public enum WorkflowEditMode: String, Codable, Hashable, Sendable {
    case outline, fields, script, readOnly
}

/// A live workflow event. `change` is the only required discriminator besides the workflow id.
public struct WorkflowEvent: Decodable, Equatable, Sendable {
    public var change: Change
    public var workflowId: String
    public var runId: String?
    public var stepId: String?
    public var enabled: Bool?

    public init(change: Change, workflowId: String, runId: String? = nil, stepId: String? = nil, enabled: Bool? = nil) {
        self.change = change
        self.workflowId = workflowId
        self.runId = runId
        self.stepId = stepId
        self.enabled = enabled
    }

    public enum Change: String, Decodable, Equatable, Sendable {
        case graph, enabled, run, step, deleted
    }
}

public enum JSONValue: Hashable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public var string: String? {
        if case let .string(value) = self { value } else { nil }
    }
}

extension JSONValue: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Int64.self) {
            self = .number(Double(value))
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case let .bool(value): try container.encode(value)
        case let .number(value): try container.encode(value)
        case let .string(value): try container.encode(value)
        case let .array(value): try container.encode(value)
        case let .object(value): try container.encode(value)
        }
    }
}

public struct WorkflowPoint: Codable, Hashable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

public struct WorkflowSize: Codable, Hashable, Sendable {
    public var width: Double
    public var height: Double

    public init(width: Double, height: Double) {
        self.width = width
        self.height = height
    }
}

public struct WorkflowNode: Hashable, Sendable {
    public var id: String
    public var kind: WorkflowNodeKind
    public var at: WorkflowPoint
    public var size: WorkflowSize?
    public var locked: Bool
    public var fields: [String: JSONValue]
    public var schedule: RoutineTrigger?

    public init(id: String, kind: WorkflowNodeKind, at: WorkflowPoint, size: WorkflowSize? = nil, locked: Bool = false,
                fields: [String: JSONValue] = [:], schedule: RoutineTrigger? = nil) {
        self.id = id
        self.kind = kind
        self.at = at
        self.size = size
        self.locked = locked
        self.fields = fields
        self.schedule = schedule
    }

    public func string(_ key: String) -> String { fields[key]?.string ?? "" }

    public mutating func setString(_ key: String, _ value: String, keepEmpty: Bool = false) {
        if value.isEmpty, !keepEmpty {
            fields.removeValue(forKey: key)
        } else {
            fields[key] = .string(value)
        }
    }
}

extension WorkflowNode: Codable {
    private enum CodingKeys: String, CodingKey {
        case id, kind, at, size, locked, fields, schedule
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        kind = try container.decode(WorkflowNodeKind.self, forKey: .kind)
        at = try container.decode(WorkflowPoint.self, forKey: .at)
        size = try container.decodeIfPresent(WorkflowSize.self, forKey: .size)
        locked = try container.decodeIfPresent(Bool.self, forKey: .locked) ?? false
        fields = try container.decodeIfPresent([String: JSONValue].self, forKey: .fields) ?? [:]
        schedule = try container.decodeIfPresent(RoutineTrigger.self, forKey: .schedule)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(kind, forKey: .kind)
        try container.encode(at, forKey: .at)
        try container.encodeIfPresent(size, forKey: .size)
        try container.encode(locked, forKey: .locked)
        try container.encode(fields, forKey: .fields)
        try container.encodeIfPresent(schedule, forKey: .schedule)
    }
}

public struct WorkflowEdge: Hashable, Sendable {
    public var id: String
    public var from: String
    public var to: String
    /// Wire key `type`. The first version only uses `text`.
    public var port: String
    public var branch: String?

    public init(id: String, from: String, to: String, port: String = "text", branch: String? = nil) {
        self.id = id
        self.from = from
        self.to = to
        self.port = port
        self.branch = branch
    }
}

extension WorkflowEdge: Codable {
    private enum CodingKeys: String, CodingKey {
        case id, from, to, port = "type", branch
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        from = try container.decode(String.self, forKey: .from)
        to = try container.decode(String.self, forKey: .to)
        port = try container.decode(String.self, forKey: .port)
        branch = try container.decodeIfPresent(String.self, forKey: .branch)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(from, forKey: .from)
        try container.encode(to, forKey: .to)
        try container.encode(port, forKey: .port)
        try container.encodeIfPresent(branch, forKey: .branch)
    }
}

public struct WorkflowStroke: Codable, Hashable, Sendable {
    public var id: String
    public var brush: String
    public var color: String
    public var width: Double
    public var points: [WorkflowPoint]

    public init(id: String, brush: String, color: String, width: Double, points: [WorkflowPoint]) {
        self.id = id
        self.brush = brush
        self.color = color
        self.width = width
        self.points = points
    }
}

public struct WorkflowGraph: Hashable, Sendable {
    public var nodes: [WorkflowNode]
    public var edges: [WorkflowEdge]
    public var strokes: [WorkflowStroke]

    public init(nodes: [WorkflowNode], edges: [WorkflowEdge], strokes: [WorkflowStroke] = []) {
        self.nodes = nodes
        self.edges = edges
        self.strokes = strokes
    }

    public static func manualTrigger() -> WorkflowGraph {
        let node = WorkflowNode(id: UUID().uuidString, kind: .trigger, at: WorkflowPoint(x: 80, y: 80),
                                fields: ["trigger": .string("manual")])
        return WorkflowGraph(nodes: [node], edges: [])
    }

    public func node(_ id: String) -> WorkflowNode? { nodes.first { $0.id == id } }

    public var hasUnknownKind: Bool { nodes.contains { $0.kind.isUnknown } }
}

extension WorkflowGraph: Codable {
    private enum CodingKeys: String, CodingKey { case nodes, edges, strokes }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        nodes = try container.decode([WorkflowNode].self, forKey: .nodes)
        edges = try container.decode([WorkflowEdge].self, forKey: .edges)
        strokes = try container.decodeIfPresent([WorkflowStroke].self, forKey: .strokes) ?? []
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(nodes, forKey: .nodes)
        try container.encode(edges, forKey: .edges)
        try container.encode(strokes, forKey: .strokes)
    }
}

public struct WorkflowLastRun: Codable, Hashable, Sendable {
    public var number: Int64
    public var status: String
    public var endedAt: Int64?
}

public struct WorkflowSummary: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var revision: Int64
    public var enabled: Bool
    public var updatedAt: Int64
    public var createdAt: Int64?
    public var running: String?
    public var lastRun: WorkflowLastRun?

    /// Open runs first, then most recently updated.
    public static func listOrder(_ items: [WorkflowSummary]) -> [WorkflowSummary] {
        items.sorted { lhs, rhs in
            let openL = lhs.running != nil
            let openR = rhs.running != nil
            if openL != openR { return openL }
            if lhs.updatedAt != rhs.updatedAt { return lhs.updatedAt > rhs.updatedAt }
            return lhs.id < rhs.id
        }
    }
}

public struct WorkflowWebhook: Codable, Hashable, Sendable {
    public var nodeId: String
    public var url: String
    public var secret: String?
}

public struct WorkflowDetail: Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var revision: Int64
    public var enabled: Bool
    public var updatedAt: Int64
    public var createdAt: Int64
    public var running: String?
    public var lastRun: WorkflowLastRun?
    public var graph: WorkflowGraph
    public var editMode: WorkflowEditMode
    public var webhooks: [WorkflowWebhook]

    /// Unknown kinds cannot be saved without dropping them, whatever the host called the mode.
    public var resolvedEditMode: WorkflowEditMode {
        graph.hasUnknownKind ? .readOnly : editMode
    }

    public var summary: WorkflowSummary {
        WorkflowSummary(id: id, title: title, revision: revision, enabled: enabled, updatedAt: updatedAt,
                        createdAt: createdAt, running: running, lastRun: lastRun)
    }
}

extension WorkflowDetail: Codable {
    private enum CodingKeys: String, CodingKey {
        case id, title, revision, enabled, updatedAt, createdAt, running, lastRun, graph, editMode, webhooks
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        title = try container.decode(String.self, forKey: .title)
        revision = try container.decode(Int64.self, forKey: .revision)
        enabled = try container.decode(Bool.self, forKey: .enabled)
        updatedAt = try container.decode(Int64.self, forKey: .updatedAt)
        createdAt = try container.decodeIfPresent(Int64.self, forKey: .createdAt) ?? 0
        running = try container.decodeIfPresent(String.self, forKey: .running)
        lastRun = try container.decodeIfPresent(WorkflowLastRun.self, forKey: .lastRun)
        graph = try container.decode(WorkflowGraph.self, forKey: .graph)
        let mode = try container.decodeIfPresent(String.self, forKey: .editMode) ?? ""
        editMode = WorkflowEditMode(rawValue: mode) ?? .readOnly
        webhooks = try container.decodeIfPresent([WorkflowWebhook].self, forKey: .webhooks) ?? []
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(title, forKey: .title)
        try container.encode(revision, forKey: .revision)
        try container.encode(enabled, forKey: .enabled)
        try container.encode(updatedAt, forKey: .updatedAt)
        try container.encode(createdAt, forKey: .createdAt)
        try container.encodeIfPresent(running, forKey: .running)
        try container.encodeIfPresent(lastRun, forKey: .lastRun)
        try container.encode(graph, forKey: .graph)
        try container.encode(editMode, forKey: .editMode)
        try container.encode(webhooks, forKey: .webhooks)
    }
}

public struct WorkflowVersion: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var revision: Int64
    public var kind: String
    public var createdAt: Int64
}

public struct WorkflowRunSummary: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var number: Int64
    public var source: String
    public var status: String
    public var startedAt: Int64
    public var endedAt: Int64?
    public var error: String?

    public var isOpen: Bool { ["running", "approval", "stopping"].contains(status) }
}

public struct WorkflowRunPage: Decodable, Sendable {
    public var runs: [WorkflowRunSummary]
    public var hasMore: Bool
}

public struct WorkflowStep: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var nodeId: String
    public var attempt: Int64
    public var requestId: String
    public var kind: String
    public var botId: String?
    public var status: String
    public var input: String
    public var output: String?
    public var branch: String?
    public var passes: Int64
    public var truncated: Bool
    public var error: String?
    public var startedAt: Int64?
    public var endedAt: Int64?
    public var title: String
}

public struct WorkflowRunDetail: Identifiable, Hashable, Sendable {
    public var id: String
    public var number: Int64
    public var source: String
    public var status: String
    public var startedAt: Int64
    public var endedAt: Int64?
    public var error: String?
    public var graph: WorkflowGraph
    public var steps: [WorkflowStep]

    public var summary: WorkflowRunSummary {
        WorkflowRunSummary(id: id, number: number, source: source, status: status, startedAt: startedAt, endedAt: endedAt, error: error)
    }
}

extension WorkflowRunDetail: Codable {
    private enum CodingKeys: String, CodingKey {
        case id, number, source, status, startedAt, endedAt, error, graph, steps
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        number = try container.decode(Int64.self, forKey: .number)
        source = try container.decode(String.self, forKey: .source)
        status = try container.decode(String.self, forKey: .status)
        startedAt = try container.decode(Int64.self, forKey: .startedAt)
        endedAt = try container.decodeIfPresent(Int64.self, forKey: .endedAt)
        error = try container.decodeIfPresent(String.self, forKey: .error)
        graph = try container.decode(WorkflowGraph.self, forKey: .graph)
        steps = try container.decodeIfPresent([WorkflowStep].self, forKey: .steps) ?? []
    }
}

public struct WorkflowSaveResult: Decodable, Sendable {
    public var ok: Bool
    public var conflict: Bool?
    public var revision: Int64?
    public var workflow: WorkflowDetail

    public var isConflict: Bool { conflict == true || !ok }
}

