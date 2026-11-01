import Foundation

/// Bare web addresses in message text (`https://…`, `http://…`, `www.…`), found by the same rule
/// as Android, the desktop and the terminal: the address runs to a space, quote, angle bracket,
/// backtick or CJK punctuation, without the sentence's closing punctuation or an unmatched
/// closing bracket.
enum Autolink {
    /// One address: its characters' offsets in the text, and what it opens (`www.` addresses over https).
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
            let opens = address.lowercased().hasPrefix("www.") ? "https://\(address)" : address
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

    /// Makes the addresses outside links and code spans in `text` links.
    static func link(_ text: inout AttributedString) {
        for found in find(String(text.characters)) {
            let start = text.characters.index(text.startIndex, offsetBy: found.range.lowerBound)
            let end = text.characters.index(start, offsetBy: found.range.count)
            let taken = text[start ..< end].runs.contains {
                $0.link != nil || $0.inlinePresentationIntent?.contains(.code) == true
            }
            guard !taken else { continue }
            text[start ..< end].link = found.url
            text[start ..< end].underlineStyle = .single
        }
    }

    /// The length of the address starting at `at`, 0 for none. An address doesn't start inside a word.
    private static func length(_ chars: [Character], at: Int) -> Int {
        if at > 0, chars[at - 1].isLetter || chars[at - 1].isNumber || chars[at - 1] == "_" { return 0 }
        guard let prefix = prefixes.first(where: { p in
            p.count <= chars.count - at && zip(p, chars[at...]).allSatisfy { $0.lowercased() == $1.lowercased() }
        }) else { return 0 }
        var end = chars[at...].firstIndex(where: stops) ?? chars.count
        while end > at {
            let last = chars[end - 1]
            let url = chars[at ..< end]
            let unmatched = switch last {
            case ")": url.count(where: { $0 == ")" }) > url.count(where: { $0 == "(" })
            case "]": url.count(where: { $0 == "]" }) > url.count(where: { $0 == "[" })
            default: false
            }
            if trailing.contains(last) || unmatched { end -= 1 } else { break }
        }
        let body = at + prefix.count
        return end > body && chars[body ..< end].contains(where: { $0.isLetter || $0.isNumber }) ? end - at : 0
    }

    private static func stops(_ c: Character) -> Bool {
        guard let scalar = c.unicodeScalars.first?.value else { return true }
        return c.isWhitespace || "<>\"`".contains(c) || (0x3000 ... 0x303F).contains(scalar) || (0xFF00 ... 0xFFEF).contains(scalar)
    }
}
