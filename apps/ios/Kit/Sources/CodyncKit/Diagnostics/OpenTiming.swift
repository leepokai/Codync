import Foundation
import Observation

public enum OpenPhase: String, Sendable { case linkReady = "link_ready", hello, catchUp = "catch_up", chatList = "chat_list", openChat = "open_chat" }
public enum OpenKind: String, Sendable { case cold, foreground }

/// Milliseconds from each cold or foreground open to its connection and rendering milestones.
@Observable
@MainActor
public final class OpenTiming {
    public static let shared = OpenTiming()
    public private(set) var opens = 0

    private let now: () -> ContinuousClock.Instant
    private let capture: ((PhonePerformance.OpenStep) -> Void)?
    private var openedAt: ContinuousClock.Instant?
    private var kind = OpenKind.cold
    private var backgrounded = false
    private var sent: Set<Key> = []

    private struct Key: Hashable {
        let computer: ComputerID
        let phase: OpenPhase
    }

    init(now: @escaping () -> ContinuousClock.Instant = { .now },
         capture: ((PhonePerformance.OpenStep) -> Void)? = nil) {
        self.now = now
        self.capture = capture
    }

    /// Starts before the saved computers open their streams during app initialization.
    public func beginCold() {
        opens += 1
        openedAt = now()
        kind = .cold
        sent = []
    }

    public func becameActive() {
        if backgrounded {
            opens += 1
            openedAt = now()
            kind = .foreground
            sent = []
        }
        backgrounded = false
    }

    public func enteredBackground() {
        backgrounded = true
    }

    public func hasRecorded(_ phase: OpenPhase, computer: ComputerID) -> Bool {
        sent.contains(Key(computer: computer, phase: phase))
    }

    public func record(_ phase: OpenPhase, computer: ComputerID,
                       chats: Int? = nil, messages: Int? = nil) -> PhonePerformance.OpenStep? {
        guard let openedAt, sent.insert(Key(computer: computer, phase: phase)).inserted else { return nil }
        let ms = Int((now() - openedAt) / .milliseconds(1))
        let step = PhonePerformance.OpenStep(phase: phase, open: kind, ms: ms, chats: chats, messages: messages)
        capture?(step)
        return step
    }
}
