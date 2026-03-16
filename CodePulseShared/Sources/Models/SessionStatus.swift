import SwiftUI

public enum SessionStatus: String, Codable, Sendable {
    case working
    case idle
    case needsInput
    case error
    case completed

    public var color: Color {
        switch self {
        case .working: return .green
        case .idle: return .cyan
        case .needsInput: return .orange
        case .error: return .red
        case .completed: return .gray
        }
    }

    public var label: String {
        switch self {
        case .working: return "Working"
        case .idle: return "Idle"
        case .needsInput: return "Needs Input"
        case .error: return "Error"
        case .completed: return "Completed"
        }
    }

    public var isActive: Bool {
        switch self {
        case .working, .idle, .needsInput, .error: return true
        case .completed: return false
        }
    }
}
