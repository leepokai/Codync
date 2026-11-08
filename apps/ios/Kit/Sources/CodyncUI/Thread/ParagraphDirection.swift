import SwiftUI

/// The base direction of a single paragraph, taken from its first strong character: Arabic, Hebrew and
/// their presentation forms read right to left. Digits, punctuation and emoji don't decide; a paragraph
/// with no strong letter stays left to right. Code keeps its own direction (always left to right).
enum ParagraphDirection {
    /// The right-to-left blocks of Unicode, strongest first: Hebrew, Arabic and its supplements,
    /// Syriac, Thaana, NKo, Samaritan, Mandaic, and the Arabic presentation forms.
    private static let rightToLeft: [ClosedRange<UInt32>] = [
        0x0590 ... 0x05FF,
        0x0600 ... 0x06FF,
        0x0700 ... 0x074F,
        0x0750 ... 0x077F,
        0x0780 ... 0x07BF,
        0x07C0 ... 0x07FF,
        0x0800 ... 0x083F,
        0x0840 ... 0x085F,
        0x08A0 ... 0x08FF,
        0xFB1D ... 0xFB4F,
        0xFB50 ... 0xFDFF,
        0xFE70 ... 0xFEFF,
    ]

    static func of(_ text: String) -> LayoutDirection {
        for scalar in text.unicodeScalars {
            if rightToLeft.contains(where: { $0.contains(scalar.value) }) { return .rightToLeft }
            if scalar.properties.isAlphabetic { return .leftToRight }
        }
        return .leftToRight
    }
}

extension View {
    /// Reads this paragraph in the direction of its own text. Outermost, so paddings and overlays on
    /// the way also turn around.
    func paragraphDirection(of text: String) -> some View {
        environment(\.layoutDirection, ParagraphDirection.of(text))
    }
}
