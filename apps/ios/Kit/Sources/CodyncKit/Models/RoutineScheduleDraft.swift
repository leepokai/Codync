import Foundation

/// Form values only. The host hydrates, compiles, validates and describes schedules.
public struct RoutineScheduleDraft: Codable, Hashable, Sendable {
    public var kind = "cron"
    public var amount = "1"
    public var unit: Int64 = 3600
    public var calendarStyle = "daily"
    public var weekday = 1
    public var selectedDays: Set<Int> = []
    public var monthDay = 1
    public var minuteStep = 15
    public var hour = 9
    public var minute = 0
    public var at: Int64 = 0
    public var expression = ""
    public var zone = ""
    public var original: [RoutineTrigger] = []

    public init() {}

    /// DatePicker's Date is converted only for wire encoding; no scheduling is done here.
    public var date: Date {
        get { Date(timeIntervalSince1970: Double(at) / 1000) }
        set { at = Int64((newValue.timeIntervalSince1970 * 1000).rounded()) }
    }

    /// `hour` and `minute` as a time-of-day Date for a DatePicker shown in GMT
    /// (`.environment(\.timeZone, .gmt)`), so no offset or daylight saving shifts them.
    public var timeOfDay: Date {
        get { Date(timeIntervalSinceReferenceDate: TimeInterval(hour * 3600 + minute * 60)) }
        set {
            let seconds = Int(newValue.timeIntervalSinceReferenceDate.rounded(.down)) % 86_400
            let day = seconds < 0 ? seconds + 86_400 : seconds
            hour = day / 3600
            minute = day % 3600 / 60
        }
    }

    /// The Repeat choices the phone offers for a schedule, in menu order.
    public static let repeatChoices = [("hourly", "Hourly"), ("daily", "Daily"), ("weekdays", "Weekdays"),
                                       ("weekly", "Weekly"), ("custom", "Custom")].map { (id: $0.0, label: $0.1) }

    /// The phone edits the `repeatChoices` styles; the host's others (every N minutes, some days, monthly)
    /// open as Custom, whose expression already holds their cron. A trigger it has no fields for is kept.
    public func forMobileEditor() -> Self {
        var draft = self
        if !Self.repeatChoices.contains(where: { $0.id == calendarStyle }) { draft.calendarStyle = "custom" }
        if !["cron", "webhook"].contains(kind) { draft.kind = "keep" }
        return draft
    }

    /// Switches the repeat style; Custom starts from `compiled`, the cron the host made of the current
    /// choice, empty while that check is still out (never the cron the routine opened with).
    public func repeating(_ style: String, compiled: String?) -> Self {
        guard style != calendarStyle else { return self }
        var draft = self
        draft.calendarStyle = style
        if style == "custom" { draft.expression = compiled ?? "" }
        return draft
    }
}

public struct RoutineSchedulePreview: Codable, Sendable {
    public var draft: RoutineScheduleDraft
    public var triggers: [RoutineTrigger]
    public var summary: String
    public var nextRunAt: Int64?
    public var warning: String?

    /// The cron the host compiled, when the schedule is one.
    public var cronExpression: String? {
        triggers.first.flatMap { $0.type == "cron" ? $0.expression : nil }
    }
}

extension HostClient {
    public func routineSchedule(triggers: [RoutineTrigger], timeZone: String) async throws -> RoutineSchedulePreview {
        struct Body: Encodable { let triggers: [RoutineTrigger]; let timeZone: String }
        return try await call("routineSchedule", Body(triggers: triggers, timeZone: timeZone))
    }

    public func routineSchedule(draft: RoutineScheduleDraft) async throws -> RoutineSchedulePreview {
        struct Body: Encodable { let draft: RoutineScheduleDraft }
        return try await call("routineSchedule", Body(draft: draft))
    }

    public func saveRoutine(botId: String, id: String?, name: String, instruction: String,
                            schedule: RoutineScheduleDraft, timeoutSeconds: String) async throws -> Routine {
        struct Body: Encodable {
            let botId: String
            let id: String?
            let name: String
            let instruction: String
            let schedule: RoutineScheduleDraft
            let timeoutSeconds: String
        }
        struct Response: Decodable { let routine: Routine }
        let response: Response = try await call("saveRoutine", Body(botId: botId, id: id, name: name,
            instruction: instruction, schedule: schedule, timeoutSeconds: timeoutSeconds))
        return response.routine
    }
}
