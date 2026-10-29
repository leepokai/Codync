import Foundation

/// Presentation only; the ActivityKit payload and host push contract stay unchanged.
public struct BotActivityPresentation: Hashable, Sendable {
    public enum Phase: Hashable, Sendable {
        /// `sending` and `queued` are this phone's own (a message on its way, or waiting in the
        /// relay mailbox for an offline computer); the computer only pushes the others.
        case sending, queued, working, needsInput, completed, failed, stale, waiting
        /// This phone is on a call with the bot, which isn't working or waiting on you.
        case onCall(CallState)
    }

    public let phase: Phase
    public let activity: String
    /// The voice call with the bot this phone is on (`ContentState.call`), if any.
    public let call: CallState?

    public init(status: String, activity: String, isStale: Bool = false, call: CallState? = nil) {
        let phase: Phase = switch status {
        case "sending": .sending
        case "queued": .queued
        case "working": .working
        case "needsInput": .needsInput
        case "idle", "completed": .completed
        case "error": .failed
        default: .waiting
        }
        let shown = isStale && (phase == .working || phase == .needsInput || phase == .sending) ? .stale : phase
        // On a call, a bot that isn't working or waiting on you shows the call instead of "Done".
        self.phase = if let call, [.completed, .stale, .waiting].contains(shown) { .onCall(call) } else { shown }
        self.activity = activity
        self.call = call
    }

    public var title: String {
        switch phase {
        case .sending: "Sending"
        case .queued: "Waiting for your computer"
        case .working: "Working"
        case .needsInput: "Needs you"
        case .completed: "Done"
        case .failed: "Failed"
        case .stale: "Update delayed"
        case .waiting: "Waiting for update"
        case .onCall(.connecting): "Connecting the call"
        case .onCall(.listening): "On a call · Listening"
        case .onCall(.speaking): "On a call · Speaking"
        case .onCall(.paused): "Call paused"
        }
    }

    /// The one line of text beside the orb: the bot's current step while it's live, else the state.
    public var caption: String {
        switch phase {
        case .working, .needsInput: activity.isEmpty ? title : activity
        case .sending, .queued, .completed, .failed, .stale, .waiting, .onCall: title
        }
    }

    public var symbol: String {
        switch phase {
        case .completed: "checkmark"
        case .failed: "exclamationmark.triangle.fill"
        case .queued: "desktopcomputer"
        case .onCall: "waveform"
        case .sending, .working, .needsInput, .stale, .waiting: "ellipsis"
        }
    }
}
