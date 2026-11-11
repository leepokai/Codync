import Foundation

public struct Routine: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var botId: String
    public var name: String
    public var instruction: String
    public var triggers: [RoutineTrigger]
    public var enabled: Bool
    public var createdAt: Int64
    public var updatedAt: Int64
    public var triggerDescriptions: [String]
    public var nextRunAt: Int64?
    public var timeoutSeconds: Int?
    public var lastError: String?
    /// What a due occurrence that could not run follows: `skip` or `runOnce` (older hosts send nothing).
    public var missedPolicy: String?
    /// What a run that would pass the bot's daily token limit follows: `wait` or `skip` (older hosts send nothing).
    public var limitPolicy: String?
    /// Optional on hosts predating workflow destinations.
    public var outputs: [RoutineOutput]?
    /// Its runs post in a thread of the bot chat instead of its main one.
    public var thread: Bool?

    /// A timeout as typed, in words: whole hours, else whole minutes, else seconds; anything else as typed.
    public static func timeoutText(_ text: String) -> String {
        guard let seconds = Int(text), seconds >= 0 else { return "\(text)s" }
        let unit: Duration.UnitsFormatStyle.Unit
        if seconds > 0, seconds % 3600 == 0 {
            unit = .hours
        } else if seconds > 0, seconds % 60 == 0 {
            unit = .minutes
        } else {
            unit = .seconds
        }
        return Duration.seconds(seconds).formatted(.units(allowed: [unit], width: .wide))
    }
}

/// Where a workflow's last reply goes: a channel, a chat, or the issue or pull request that started it.
public struct RoutineOutput: Codable, Hashable, Sendable {
    public var kind: String
    /// A channel id, a chat id, or (for GitHub) the repository, `owner/name`.
    public var target: String
    /// The Telegram bot to send as; other kinds ignore it.
    public var account: String?
    /// Labels, reviewers, or the issue title; other kinds ignore it.
    public var value: String?

    public init(kind: String = "slack", target: String = "", account: String? = nil, value: String? = nil) {
        self.kind = kind
        self.target = target
        self.account = account
        self.value = value
    }

    /// The most outputs one routine may have.
    public static let max = 10

    /// The kinds the editor offers, in the order the host lists them.
    public static let kinds: [(id: String, label: String)] = [
        ("slack", "Slack"), ("telegram", "Telegram"), ("whatsapp", "WhatsApp"), ("discord", "Discord"),
        ("discordReact", "Discord reaction"), ("githubComment", "GitHub comment"), ("githubIssue", "GitHub issue"),
        ("githubLabel", "GitHub label"), ("githubAssign", "GitHub assign"), ("githubReview", "Request review"),
    ]

    /// An example destination for the kind, shown until one is typed.
    public var placeholder: String {
        switch kind {
        case "githubComment", "githubIssue", "githubLabel", "githubAssign", "githubReview": "owner/name"
        case "telegram": "-1001234567890"
        case "whatsapp": "5215555555555@s.whatsapp.net"
        case "discordReact": "👍"
        default: "1234567890"
        }
    }

    /// The extra value a GitHub kind needs, or nil when the kind takes none.
    public var valuePlaceholder: String? {
        switch kind {
        case "githubIssue": "Issue title (optional)"
        case "githubLabel": "Labels, separated by commas"
        case "githubAssign", "githubReview": "People, separated by commas"
        default: nil
        }
    }

    /// Why this output can't be saved, or nil when it can (host `routines::outputs` checks the same rules).
    public var problem: String? {
        let target = target.trimmingCharacters(in: .whitespacesAndNewlines)
        if target.isEmpty { return "An output needs a destination." }
        if kind == "telegram", Int64(target) == nil { return "Use a Telegram chat id." }
        if kind == "whatsapp", target.range(of: #"^[\d-]+@(s\.whatsapp\.net|g\.us)$"#, options: .regularExpression) == nil {
            return "Use a WhatsApp chat id."
        }
        if ["githubComment", "githubIssue", "githubLabel", "githubAssign", "githubReview"].contains(kind),
           target.range(of: #"^[^/\s]+/[^/\s]+$"#, options: .regularExpression) == nil {
            return "Use owner/name."
        }
        if ["githubLabel", "githubAssign", "githubReview"].contains(kind),
           value?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false {
            return "This output needs a value."
        }
        if ["slack", "discord"].contains(kind), target.contains(where: \.isWhitespace) { return "Use a channel id." }
        if kind == "discordReact", target.contains(where: \.isWhitespace) { return "Use one emoji or name:id." }
        return nil
    }
}

public struct RoutineTrigger: Codable, Hashable, Sendable {
    public var type: String
    public var seconds: Int64?
    public var at: Int64?
    public var expression: String?
    public var timeZone: String?
    public var source: String?
    public var event: String?
    public var filters: [String: String]?
    /// `watch` triggers: `files` or `url`, the path or URL, and how often it's checked.
    public var kind: String?
    public var target: String?
    public var everyMinutes: Int64?

    public init(type: String, seconds: Int64? = nil, at: Int64? = nil, expression: String? = nil, timeZone: String? = nil) {
        self.type = type
        self.seconds = seconds
        self.at = at
        self.expression = expression
        self.timeZone = timeZone
    }
}

/// Occurrences that came due and did not run; `count` is how many.
public struct RoutineRunMissed: Codable, Hashable, Sendable {
    public var count: Int
    public var until: Int64?
}

public struct RoutineRun: Codable, Identifiable, Hashable, Sendable {
    public var id: String
    public var routineId: String
    public var botId: String
    public var status: String
    public var createdAt: Int64
    public var finishedAt: Int64?
    public var detail: String?
    /// A retry of a run that failed before acting waits until then.
    public var notBefore: Int64?
    /// The occurrence this run stands for, when its schedule named one.
    public var scheduledFor: Int64?
    /// Occurrences that came due and did not run (older hosts send nothing).
    public var missed: RoutineRunMissed?
    /// A run holds the bot's turn and it is asking a question: it waits on the user.
    public var needsAttention: Bool?
    public var isActive: Bool { ["pending", "starting", "running", "recovering"].contains(status) }

    /// A run's status in the routines list; a waiting retry says when it goes, a skipped run why.
    public var label: String {
        let missedSuffix = self.missed.map { $0.count > 0 ? " · \($0.count) missed" : "" } ?? ""
        switch status {
        case "pending":
            if let at = notBefore {
                let date = Date(milliseconds: at)
                return date > .now ? "Retrying at \(date.formatted(date: .omitted, time: .shortened))" : "Retrying"
            }
            return "Queued"
        case "starting": return "Starting"
        case "running": return needsAttention == true ? "Waiting on you" : "Running"
        case "recovering": return "Resuming after restart"
        case "failed": return "Failed\(missedSuffix)"
        case "interrupted": return "Interrupted\(missedSuffix)"
        case "cancelled":
            // A cancelled run that stands for a due occurrence is a skipped one; the rest just stopped.
            return scheduledFor != nil || missed != nil ? "Skipped\(missedSuffix)" : "Cancelled"
        default: return status.capitalized
        }
    }
}

public struct RoutineListing: Codable, Sendable {
    public var routines: [Routine]
    public var runs: [RoutineRun]
}

/// A routine's webhook: `url` is public through the Codync cloud (nil while the cloud is off),
/// `localUrl` works on the host itself.
public struct RoutineWebhook: Codable, Sendable, Equatable {
    public var url: String?
    public var localUrl: String
    public var key: String
    public var connected: Bool
}

extension HostClient {
    public func routines(botId: String) async throws -> RoutineListing {
        struct Body: Encodable { let botId: String }
        return try await call("routines", Body(botId: botId))
    }

    public func routineAction(_ method: String, botId: String, id: String, enabled: Bool? = nil) async throws {
        struct Body: Encodable { let botId: String; let id: String; let enabled: Bool? }
        let _: Empty = try await call(method, Body(botId: botId, id: id, enabled: enabled))
    }

    /// Runs a failed run's work again, with the event it had.
    public func retryRoutineRun(botId: String, runId: String) async throws {
        struct Body: Encodable { let botId: String; let runId: String }
        let _: Empty = try await call("retryRoutineRun", Body(botId: botId, runId: runId))
    }

    /// `rotate` replaces the key first; senders using the old one stop working.
    public func routineWebhook(botId: String, id: String, rotate: Bool = false) async throws -> RoutineWebhook {
        struct Body: Encodable { let botId: String; let id: String; let rotate: Bool }
        return try await call("routineWebhook", Body(botId: botId, id: id, rotate: rotate))
    }
}

extension HostClient {
    public func saveRoutine(botId: String, id: String?, name: String, instruction: String,
                            triggers: [RoutineTrigger], timeoutSeconds: Int,
                            missedPolicy: String? = nil, limitPolicy: String? = nil,
                            outputs: [RoutineOutput]? = nil, ifOutputs: [RoutineOutput]? = nil, thread: Bool = false) async throws -> Routine {
        struct Body: Encodable {
            let botId: String
            let id: String?
            let name: String
            let instruction: String
            let triggers: [RoutineTrigger]
            let timeoutSeconds: Int
            let missedPolicy: String?
            let limitPolicy: String?
            let outputs: [RoutineOutput]?
            let ifOutputs: [RoutineOutput]?
            let thread: Bool
        }
        struct Response: Decodable { let routine: Routine }
        let response: Response = try await call(outputs == nil ? "saveRoutine" : "saveRoutineWithOutputs", Body(botId: botId, id: id, name: name,
            instruction: instruction, triggers: triggers, timeoutSeconds: timeoutSeconds,
            missedPolicy: missedPolicy, limitPolicy: limitPolicy, outputs: outputs, ifOutputs: ifOutputs, thread: thread))
        return response.routine
    }
}
