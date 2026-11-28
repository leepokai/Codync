import Foundation

// A saved workflow, read only: the phone shows workflows and their runs, and answers approval steps. Field names
// match the host's camelCase JSON. An unknown step kind stays on the node, so a later host's graph still shows.

public enum WorkflowNodeKind: Hashable, Sendable {
    case trigger, prompt, agent, condition, approval, output, text, note, shape
    case unknown(String)

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

extension WorkflowNodeKind: Decodable {
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
}

/// A live workflow event. `change` is the only required discriminator besides the workflow id.
public struct WorkflowEvent: Decodable, Equatable, Sendable {
    public var change: Change
    public var workflowId: String
    public var runId: String?
    public var stepId: String?

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

extension JSONValue: Decodable {
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
}

public struct WorkflowPoint: Decodable, Hashable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

public struct WorkflowNode: Decodable, Hashable, Sendable {
    public var id: String
    public var kind: WorkflowNodeKind
    public var at: WorkflowPoint
    public var fields: [String: JSONValue]
    public var schedule: RoutineTrigger?

    public init(id: String, kind: WorkflowNodeKind, at: WorkflowPoint, fields: [String: JSONValue] = [:], schedule: RoutineTrigger? = nil) {
        self.id = id
        self.kind = kind
        self.at = at
        self.fields = fields
        self.schedule = schedule
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        kind = try container.decode(WorkflowNodeKind.self, forKey: .kind)
        at = try container.decode(WorkflowPoint.self, forKey: .at)
        fields = try container.decodeIfPresent([String: JSONValue].self, forKey: .fields) ?? [:]
        schedule = try container.decodeIfPresent(RoutineTrigger.self, forKey: .schedule)
    }

    private enum CodingKeys: String, CodingKey { case id, kind, at, fields, schedule }

    public func string(_ key: String) -> String { fields[key]?.string ?? "" }
}

public struct WorkflowEdge: Decodable, Hashable, Sendable {
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

    private enum CodingKeys: String, CodingKey {
        case id, from, to, port = "type", branch
    }
}

public struct WorkflowGraph: Decodable, Hashable, Sendable {
    public var nodes: [WorkflowNode]
    public var edges: [WorkflowEdge]

    public init(nodes: [WorkflowNode], edges: [WorkflowEdge]) {
        self.nodes = nodes
        self.edges = edges
    }

    public func node(_ id: String) -> WorkflowNode? { nodes.first { $0.id == id } }

    /// 1, 2, 3… for the steps on the list, in reading order. Triggers and notes are not numbered.
    public func stepNumbers() -> [String: Int] {
        var map: [String: Int] = [:]
        for id in WorkflowLayout.readingOrder(of: self) {
            guard let node = node(id), node.kind != .trigger, !node.kind.isAnnotation else { continue }
            map[id] = map.count + 1
        }
        return map
    }

    /// Rows for the line view. A step sits one row below the deepest numbered step that feeds it.
    public func stepRanks() -> [[String]] {
        let numbers = stepNumbers()
        let order = WorkflowLayout.readingOrder(of: self).filter { numbers[$0] != nil }
        let numbered = Set(order)
        var memo: [String: Int] = [:]
        var stack = Set<String>()
        func rank(_ id: String) -> Int {
            if let known = memo[id] { return known }
            if !stack.insert(id).inserted { return 0 }
            let value = parents(of: id).filter { numbered.contains($0) }.map { rank($0) }.max().map { $0 + 1 } ?? 0
            stack.remove(id)
            memo[id] = value
            return value
        }
        var rows: [[String]] = []
        for id in order {
            let level = rank(id)
            if rows.count <= level {
                rows.append(contentsOf: Array(repeating: [], count: level + 1 - rows.count))
            }
            rows[level].append(id)
        }
        return rows.filter { !$0.isEmpty }
    }

    /// Steps that feed this one, in edge order, each once.
    public func parents(of id: String) -> [String] {
        var seen = Set<String>()
        return edges.compactMap { edge in
            guard edge.to == id, seen.insert(edge.from).inserted else { return nil }
            return edge.from
        }
    }
}

public struct WorkflowLastRun: Decodable, Hashable, Sendable {
    public var number: Int64
    public var status: String
    public var endedAt: Int64?
}

public struct WorkflowSummary: Decodable, Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var revision: Int64
    public var updatedAt: Int64
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

    /// What the Workflows list's search finds: the items whose title holds `query` or whose id starts with it; all of
    /// them for a blank one.
    public static func matching(_ items: [WorkflowSummary], _ query: String) -> [WorkflowSummary] {
        let words = query.trimmingCharacters(in: .whitespaces)
        guard !words.isEmpty else { return items }
        return items.filter { $0.title.localizedCaseInsensitiveContains(words) || $0.id.hasPrefix(words) }
    }
}

public struct WorkflowDetail: Decodable, Identifiable, Hashable, Sendable {
    public var id: String
    public var title: String
    public var revision: Int64
    public var updatedAt: Int64
    public var running: String?
    public var lastRun: WorkflowLastRun?
    public var graph: WorkflowGraph
}

public struct WorkflowRunSummary: Decodable, Identifiable, Hashable, Sendable {
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

public struct WorkflowStep: Decodable, Identifiable, Hashable, Sendable {
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

extension WorkflowRunDetail: Decodable {
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
