import Foundation

/// Bare web addresses in message text (`https://…`, `http://…`, `www.…`), found by the same rule
/// as Android, the desktop and the terminal: the address runs to a space, quote, angle bracket,
/// backtick, `**` or CJK punctuation, without the sentence's closing punctuation or an unmatched
/// closing bracket.
enum Autolink {
    /// One address: its characters' offsets in the text, and what it opens (`www.` addresses over
    /// https, and the scheme in lowercase).
    struct Found: Equatable {
        let range: Range<Int>
        let url: URL
    }

    private static let prefixes = ["https://", "http://", "www."]
    private static let trailing = Set(".,:;!?'\"*_~")

    /// The addresses in `text`, in order.
    static func find(_ text: String) -> [Found] {
        let chars = Array(text)
        var found: [Found] = []
        var i = 0
        while i < chars.count {
            let length = length(chars, at: i)
            guard length > 0 else { i += 1; continue }
            let address = String(chars[i ..< i + length])
            let opens = if address.lowercased().hasPrefix("www.") {
                "https://\(address)"
            } else {
                address.prefix { $0 != ":" }.lowercased() + address.drop { $0 != ":" }
            }
            if let url = URL(string: opens) { found.append(Found(range: i ..< i + length, url: url)) }
            i += length
        }
        return found
    }

    /// Plain text (a message the user wrote) with its addresses as underlined links.
    static func attributed(_ text: String) -> AttributedString {
        var out = AttributedString(text)
        link(&out)
        return out
    }

    /// Makes the addresses outside links and code spans in `text` links. Each run is searched on
    /// its own, so code or bold text right after an address doesn't join it. Apple's Markdown
    /// links bare addresses by its own rule (`www.` over http, `**` kept inside), so those links
    /// are found again by this one; a link written in `source` (`[label](url)`, `<url>`) stays.
    static func link(_ text: inout AttributedString, source: String = "") {
        for run in text.runs {
            guard let url = run.link?.absoluteString else { continue }
            let shown = String(text[run.range].characters)
            let written = source.contains("](\(url)") || source.contains("](<\(url)") || source.contains("<\(url)>")
            if !written, url == shown || url == "http://\(shown)" { text[run.range].link = nil }
        }
        var ranges: [(Range<AttributedString.Index>, URL)] = []
        for run in text.runs where run.link == nil && run.inlinePresentationIntent?.contains(.code) != true {
            let piece = text[run.range]
            for found in find(String(piece.characters)) {
                let start = piece.characters.index(piece.startIndex, offsetBy: found.range.lowerBound)
                let end = piece.characters.index(start, offsetBy: found.range.count)
                ranges.append((start ..< end, found.url))
            }
        }
        for (range, url) in ranges {
            text[range].link = url
            text[range].underlineStyle = .single
        }
    }

    /// The length of the address starting at `at`, 0 for none. An address doesn't start inside a word.
    private static func length(_ chars: [Character], at: Int) -> Int {
        if at > 0, chars[at - 1].isLetter || chars[at - 1].isNumber || chars[at - 1] == "_" { return 0 }
        guard let prefix = prefixes.first(where: { p in
            p.count <= chars.count - at && zip(p, chars[at...]).allSatisfy { $0.lowercased() == $1.lowercased() }
        }) else { return 0 }
        var end = (at ..< chars.count).first { stops(chars[$0]) || chars[$0...].starts(with: "**") } ?? chars.count
        // Bracket counts are kept while trimming, so a long run of `)` costs one pass.
        let url = chars[at ..< end]
        let open: [Character: Int] = [")": url.count(where: { $0 == "(" }), "]": url.count(where: { $0 == "[" })]
        var close: [Character: Int] = [")": url.count(where: { $0 == ")" }), "]": url.count(where: { $0 == "]" })]
        while end > at {
            let last = chars[end - 1]
            if let closing = close[last] {
                guard closing > open[last]! else { break }
                close[last] = closing - 1
            } else if !trailing.contains(last) {
                break
            }
            end -= 1
        }
        let body = at + prefix.count
        return end > body && chars[body ..< end].contains(where: { $0.isLetter || $0.isNumber }) ? end - at : 0
    }

    private static func stops(_ c: Character) -> Bool {
        guard let scalar = c.unicodeScalars.first?.value else { return true }
        return c.isWhitespace || "<>\"`".contains(c) || (0x3000 ... 0x303F).contains(scalar) || (0xFF00 ... 0xFFEF).contains(scalar)
    }
}
