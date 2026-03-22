import Foundation

/// Simplify Claude model identifiers to display labels.
/// Shared across main app and widget extension.
public func modelDisplayLabel(_ model: String) -> String {
    let lower = model.lowercased()
    if lower.contains("opus") { return "Opus" }
    if lower.contains("sonnet") { return "Sonnet" }
    if lower.contains("haiku") { return "Haiku" }
    return model
}
