import Foundation

/// Presentation only; the ActivityKit payload and host push contract stay unchanged.
public struct BotActivityPresentation: Hashable, Sendable {
    public enum Phase: String, CaseIterable, Sendable {
        case working, needsInput, completed, failed, stale, waiting
    }

    public let phase: Phase
    public let activity: String
    public let startedAt: Date?

    public init(status: String, activity: String, startedAt: Date?, isStale: Bool = false) {
        let phase: Phase = switch status {
        case "working": .working
        case "needsInput": .needsInput
        case "idle", "completed": .completed
        case "error": .failed
        default: .waiting
        }
        self.phase = isStale && (phase == .working || phase == .needsInput) ? .stale : phase
        self.activity = activity
        self.startedAt = startedAt
    }

    public var title: String {
        switch phase {
        case .working: "Working"
        case .needsInput: "Needs you"
        case .completed: "Done"
        case .failed: "Stopped"
        case .stale: "Update delayed"
        case .waiting: "Waiting for update"
        }
    }

    public var detail: String {
        switch phase {
        case .stale: "Open Codync to check the latest status."
        case .waiting: "Open Codync to check this task."
        case .completed: "Your bot has finished. Open the conversation for the result."
        case .failed: "The task stopped with an error. Open the conversation for details."
        case .needsInput: activity.isEmpty ? "Open the conversation to respond." : activity
        case .working: activity.isEmpty ? "Your bot is working on the computer." : activity
        }
    }

    public var symbol: String {
        switch phase {
        case .working: "ellipsis"
        case .needsInput: "hand.raised.fill"
        case .completed: "checkmark"
        case .failed: "exclamationmark.triangle.fill"
        case .stale, .waiting: "clock.badge.questionmark"
        }
    }

    public var showsTimer: Bool { phase == .working && startedAt != nil }
}
