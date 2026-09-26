import Foundation

/// Chat replies turned into what a call reads aloud.
enum SpokenText {
    /// A reply as it should sound: markdown syntax and code blocks dropped.
    static func from(_ markdown: String) -> String {
        var text = markdown
        let rules: [(String, String)] = [
            (#"```[\s\S]*?```"#, ""),                  // code blocks
            (#"!?\[([^\]]*)\]\([^)]*\)"#, "$1"),       // links and images → their text
            (#"`([^`]*)`"#, "$1"),                     // inline code
            (#"(?m)^\s{0,3}(#{1,6}|>|[-*+]|\d+\.)\s+"#, ""), // headings, quotes, list markers
            (#"(\*\*|\*|~~)(\S[^\n]*?)\1"#, "$2"),   // emphasis (not `_`: it's in identifiers)
            (#"(?m)^[ \t]*\|?[ \t:|-]{3,}\|?[ \t]*$"#, ""), // table rules
            (#"(?m)^[ \t]*\|[ \t]*|[ \t]*\|[ \t]*$"#, ""), // table edges
            (#"[ \t]*\|[ \t]*"#, ", "),                      // table cells
            (#"\n{2,}"#, "\n"),
        ]
        for (pattern, template) in rules {
            text = text.replacingOccurrences(of: pattern, with: template, options: .regularExpression)
        }
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
