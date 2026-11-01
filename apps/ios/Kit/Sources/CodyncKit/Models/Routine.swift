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

public struct RoutineTrigger: Codable, Hashable, Sendable {
    public var type: String
    public var seconds: Int64?
    public var at: Int64?
    public var expression: String?
    public var timeZone: String?
    public var source: String?
    public var event: String?
    public var filters: [String: String]?

    public init(type: String, seconds: Int64? = nil, at: Int64? = nil, expression: String? = nil, timeZone: String? = nil) {
        self.type = type
        self.seconds = seconds
        self.at = at
        self.expression = expression
        self.timeZone = timeZone
    }
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
    public var isActive: Bool { ["pending", "starting", "running", "recovering"].contains(status) }
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

    /// Every bot's routines on this computer (the account menu's Routines).
    public func allRoutines() async throws -> [Routine] {
        struct Response: Decodable { let routines: [Routine] }
        let response: Response = try await call("allRoutines", Empty())
        return response.routines
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
                            triggers: [RoutineTrigger], timeoutSeconds: Int) async throws -> Routine {
        struct Body: Encodable {
            let botId: String
            let id: String?
            let name: String
            let instruction: String
            let triggers: [RoutineTrigger]
            let timeoutSeconds: Int
        }
        struct Response: Decodable { let routine: Routine }
        let response: Response = try await call("saveRoutine", Body(botId: botId, id: id, name: name,
            instruction: instruction, triggers: triggers, timeoutSeconds: timeoutSeconds))
        return response.routine
    }
}

/// One bot's routines in the account menu's list of every routine.
public struct RoutineSection: Identifiable, Sendable {
    public var bot: Bot
    public var routines: [Routine]
    public var id: String { bot.id }
}

extension Routine {
    /// Sections in roster order (`bots`), each bot's routines by name; routines of bots this app doesn't know stay out.
    public static func sections(_ routines: [Routine], bots: [Bot]) -> [RoutineSection] {
        bots.compactMap { bot in
            let own = routines.filter { $0.botId == bot.id }
                .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
            return own.isEmpty ? nil : RoutineSection(bot: bot, routines: own)
        }
    }

    /// The bots a new routine can belong to: agents, not groups.
    public static func owners(_ bots: [Bot]) -> [Bot] { bots.filter { !$0.isGroup } }
}
