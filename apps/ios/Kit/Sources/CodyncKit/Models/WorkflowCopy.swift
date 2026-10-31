import Foundation

/// English sentences the phone shows for a step. Callers pass them through `L10n.tr`.
public enum WorkflowCopy {
    /// `Run {number}` is shorter than the phrases `L10n` interpolates, so the number is filled here.
    public static func runTitle(_ number: Int64) -> String {
        L10n.tr("Run {number}").replacingOccurrences(of: "{number}", with: String(number))
    }

    public static func status(_ status: String?) -> String? {
        return switch status {
        case "running": "Running"
        case "approval": "Needs you"
        case "stopping": "Stopping"
        case "done": "Done"
        case "failed": "Failed"
        case "stopped": "Stopped"
        case "skipped": "Skipped"
        case nil: nil
        default: status
        }
    }

    public static func title(_ node: WorkflowNode, botName: (String) -> String?) -> String {
        return switch node.kind {
        case .trigger:
            switch node.string("trigger") {
            case "schedule": "Schedule"
            case "webhook": "Webhook"
            default: "Manual"
            }
        case .prompt:
            firstLine(node.string("text")) ?? "Prompt"
        case .agent:
            namedBot(node.string("botId"), botName: botName) ?? "Agent"
        case .condition:
            "Condition"
        case .approval:
            firstLine(node.string("question")) ?? "Approval"
        case .output:
            if node.string("target") == "chat" {
                namedBot(node.string("botId"), botName: botName) ?? "Choose a bot"
            } else {
                "Notification"
            }
        case .text, .note, .shape:
            "Note"
        case let .unknown(raw):
            raw
        }
    }

    public static func summary(_ node: WorkflowNode, botName: (String) -> String?, schedule: String?) -> String {
        return switch node.kind {
        case .trigger:
            switch node.string("trigger") {
            case "schedule": schedule ?? "Schedule"
            case "webhook": "Webhook"
            default: "Manual"
            }
        case .agent:
            namedBot(node.string("botId"), botName: botName) ?? "Choose a bot"
        case .output:
            if node.string("target") == "chat" {
                namedBot(node.string("botId"), botName: botName) ?? "Choose a bot"
            } else {
                "Notification"
            }
        case .prompt:
            firstLine(node.string("text")) ?? "Prompt"
        case .condition:
            condition(node)
        case .approval:
            firstLine(node.string("question")) ?? "Approve this step?"
        default:
            title(node, botName: botName)
        }
    }

    public static func symbol(_ kind: WorkflowNodeKind) -> String {
        return switch kind {
        case .trigger: "bolt.fill"
        case .prompt: "text.alignleft"
        case .agent: "person.fill"
        case .condition: "arrow.triangle.branch"
        case .approval: "checkmark.seal"
        case .output: "bell.fill"
        case .text, .note, .shape: "note.text"
        case .unknown: "questionmark"
        }
    }

    public static func duration(from start: Int64, to end: Int64) -> String {
        let seconds = max(0, (end - start) / 1000)
        if seconds < 60 { return "\(seconds)s" }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes)m" }
        let hours = minutes / 60
        let rest = minutes % 60
        return rest == 0 ? "\(hours)h" : "\(hours)h \(rest)m"
    }

    private static func namedBot(_ id: String, botName: (String) -> String?) -> String? {
        let trimmed = id.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return botName(trimmed)
    }

    private static func condition(_ node: WorkflowNode) -> String {
        let value = node.string("value")
        return switch node.string("op") {
        case "equals": value.isEmpty ? "Equals" : "Equals \(value)"
        case "regex": value.isEmpty ? "Matches" : "Matches \(value)"
        case "empty": "Empty"
        default: value.isEmpty ? "Contains" : "Contains \(value)"
        }
    }

    private static func firstLine(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return trimmed.split(whereSeparator: \.isNewline).first.map(String.init)
    }
}

public extension WorkflowNode {
    /// A new step with the fields the host accepts for that kind.
    static func make(_ kind: WorkflowNodeKind, id: String = UUID().uuidString) -> WorkflowNode {
        var fields: [String: JSONValue] = [:]
        switch kind {
        case .trigger: fields["trigger"] = .string("manual")
        case .prompt: fields["text"] = .string("")
        case .agent: fields["task"] = .string("")
        case .condition: fields["op"] = .string("contains")
        case .approval: fields["question"] = .string("")
        case .output: fields["target"] = .string("notify")
        case .text: fields["text"] = .string("")
        case .note: fields["text"] = .string("")
        case .shape:
            fields["shape"] = .string("rectangle")
            fields["text"] = .string("")
        case .unknown:
            break
        }
        return WorkflowNode(id: id, kind: kind, at: WorkflowPoint(x: 80, y: 80), fields: fields)
    }
}
