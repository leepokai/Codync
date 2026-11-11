import Foundation

/// Foreground, on-device transcript gating; it never starts a network wake-word service.
struct WakePhrase {
    let enabled: Bool
    private var awakeUntil: Date?

    init(enabled: Bool) { self.enabled = enabled }
    mutating func reset() { awakeUntil = nil }
    mutating func accept(_ text: String, now: Date = .now) -> String? {
        let input = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard enabled else { return input.isEmpty ? nil : input }
        if let until = awakeUntil, until > now { reset(); return input.isEmpty ? nil : input }
        reset()
        guard let range = input.range(of: #"^hey[\s\p{P}]+hermes\b[\s\p{P}]*"#, options: [.regularExpression, .caseInsensitive]) else { return nil }
        let command = String(input[range.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
        if command.isEmpty { awakeUntil = now.addingTimeInterval(15); return nil }
        return command
    }
}
