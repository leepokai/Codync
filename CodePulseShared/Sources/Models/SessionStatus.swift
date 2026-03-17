import SwiftUI

public enum SessionStatus: String, Codable, Sendable {
    case working
    case idle
    case needsInput
    case compacting
    case error
    case completed

    public var color: Color {
        switch self {
        case .working: return .blue
        case .idle: return .secondary
        case .needsInput: return .orange
        case .compacting: return .purple
        case .error: return .orange
        case .completed: return .gray
        }
    }

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
