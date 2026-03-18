import SwiftUI

public enum SessionStatus: String, Codable, Sendable {
    case working
    case idle
    case needsInput
    case compacting
    case error
    case completed

    public var label: String {
        switch self {
        case .working: return "Working"
        case .idle: return "Idle"
        case .needsInput: return "Needs Input"
        case .compacting: return "Compacting"
        case .error: return "Error"
        case .completed: return "Completed"
        }
    }

    public var isActive: Bool {
        switch self {
        case .working, .idle, .needsInput, .compacting, .error: return true
        case .completed: return false
        }
    }
}

public enum WaitingReason: String, Codable, Sendable {
    case permissionPrompt    // Red — needs permission approval
    case commandComplete     // Yellow — Stop hook, command finished
    case askUserQuestion     // Yellow — AskUserQuestion tool
    case elicitation         // Yellow — elicitation dialog
    case unknown             // Yellow — fallback

    public var label: String {
        switch self {
        case .permissionPrompt: return "Needs Permission"
        case .commandComplete: return "Waiting for Input"
        case .askUserQuestion: return "Question"
        case .elicitation: return "Waiting for Input"
        case .unknown: return "Needs Input"
        }
    }

    public var isPermission: Bool { self == .permissionPrompt }
}

/// Type-safe hook signal types between ClaudeHookServer and TranscriptWatcher.
public enum HookSignalType: String, Sendable {
    case permissionRequest = "permission_request"
    case askUserQuestion = "ask_user_question"
    case elicitationDialog = "elicitation_dialog"
    case stop
    case userPromptSubmit = "user_prompt_submit"
    case preToolUse = "pre_tool_use"
    case postToolUse = "post_tool_use"
}
