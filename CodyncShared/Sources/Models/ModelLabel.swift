import Foundation

/// Parsed model metadata from a Claude model ID string.
public struct ModelInfo: Sendable {
    public let family: String       // "Opus", "Sonnet", "Haiku"
    public let version: String      // "4.6", "3.5", "" if unknown
    public let displayLabel: String // "Opus 4.6", "Sonnet", etc.
    public let contextWindow: Int   // 1_000_000 or 200_000

    /// Parse a raw Claude model ID into structured info.
    /// Examples: "claude-opus-4-6", "claude-3-5-sonnet-20241022", "claude-haiku-4-5-20251001"
    public static func parse(_ model: String) -> ModelInfo {
        let lower = model.lowercased()

        // Detect family
        let family: String
        if lower.contains("opus") { family = "Opus" }
        else if lower.contains("sonnet") { family = "Sonnet" }
        else if lower.contains("haiku") { family = "Haiku" }
        else {
            return ModelInfo(family: "", version: "", displayLabel: model, contextWindow: 200_000)
        }

        // Extract version — try new format first: claude-{family}-{major}-{minor}
        // then old format: claude-{major}-{minor}-{family}
        let version = extractVersion(from: lower, family: family.lowercased())

        let displayLabel = version.isEmpty ? family : "\(family) \(version)"

        // Context window: 1M for version >= 4.6 or explicit [1m] suffix
        let has1mSuffix = lower.contains("[1m]")
        let isLargeContext = has1mSuffix || versionAtLeast(version, major: 4, minor: 6)
        let contextWindow = isLargeContext ? 1_000_000 : 200_000

        return ModelInfo(
            family: family,
            version: version,
            displayLabel: displayLabel,
            contextWindow: contextWindow
        )
    }

    /// Extract version number from a model ID string.
    /// Handles both new format (claude-opus-4-6) and legacy format (claude-3-5-sonnet-20241022).
    private static func extractVersion(from lower: String, family: String) -> String {
        // New format: claude-{family}-{major}-{minor}[-date]
        // e.g., "claude-opus-4-6", "claude-haiku-4-5-20251001"
        if let range = lower.range(of: "\(family)-") {
            let afterFamily = lower[range.upperBound...]
            let version = parseVersionDigits(String(afterFamily))
            if !version.isEmpty { return version }
        }

        // Old format: claude-{major}-{minor}-{family}[-date]
        // e.g., "claude-3-5-sonnet-20241022"
        if let range = lower.range(of: "-\(family)") {
            let beforeFamily = lower[lower.startIndex..<range.lowerBound]
            let parts = beforeFamily.split(separator: "-")
            if parts.count >= 2,
               let major = Int(parts[parts.count - 2]),
               let minor = Int(parts[parts.count - 1]) {
                return "\(major).\(minor)"
            }
        }

        return ""
    }

    /// Parse "4-6..." into "4.6", "4-5-20251001" into "4.5"
    private static func parseVersionDigits(_ s: String) -> String {
        let parts = s.split(separator: "-")
        guard parts.count >= 2,
              let major = Int(parts[0]) else { return "" }
        // Minor part may have trailing non-numeric chars (e.g., "6[1m]")
        let minorStr = String(parts[1]).prefix(while: \.isNumber)
        guard let minor = Int(minorStr) else { return "" }
        return "\(major).\(minor)"
    }

    private static func versionAtLeast(_ version: String, major: Int, minor: Int) -> Bool {
        let parts = version.split(separator: ".")
        guard parts.count == 2,
              let vMajor = Int(parts[0]),
              let vMinor = Int(parts[1]) else { return false }
        return vMajor > major || (vMajor == major && vMinor >= minor)
    }
}

/// Simplify Claude model identifiers to display labels.
/// Shared across main app and widget extension.
public func modelDisplayLabel(_ model: String) -> String {
    ModelInfo.parse(model).displayLabel
}
