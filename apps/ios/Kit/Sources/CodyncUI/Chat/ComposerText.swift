import SwiftUI

/// Text and its selection travel together when the composer replaces a draft.
struct ComposerText {
    var text = ""
    var selection: TextSelection?

    mutating func replace(with replacement: String) {
        // iOS 18 converts the old selection to UTF-16 offsets during the next field
        // update. Clearing or shortening text without clearing its indices can trap.
        selection = nil
        text = replacement
    }

    mutating func insertNewline() {
        guard !text.isEmpty else { return }
        let range = if case .selection(let range) = selection?.indices,
                       range.upperBound <= text.endIndex {
            range
        } else {
            text.endIndex..<text.endIndex
        }
        // A newline after a lone carriage return forms one Character; count scalars.
        let offset = text.unicodeScalars.distance(from: text.startIndex, to: range.lowerBound)
        var replacement = text
        replacement.replaceSubrange(range, with: "\n")
        replace(with: replacement)
        selection = TextSelection(insertionPoint: text.unicodeScalars.index(text.startIndex, offsetBy: offset + 1))
    }
}
