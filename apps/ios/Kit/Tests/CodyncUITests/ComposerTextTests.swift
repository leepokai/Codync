import SwiftUI
import Testing
@testable import CodyncUI

@MainActor @Test(arguments: ["", "1", "中文", "👨‍👩‍👧‍👦", "replacement text"])
func replacingComposerTextDiscardsPreviousSelection(_ replacement: String) {
    var input = ComposerText(text: "previous draft")
    input.selection = TextSelection(insertionPoint: input.text.endIndex)

    input.replace(with: replacement)

    #expect(input.text == replacement)
    #expect(input.selection == nil)
}

@MainActor @Test func composerReturnReplacesSelectionAndMovesCursorAfterNewline() {
    var input = ComposerText(text: "A👨‍👩‍👧‍👦中B")
    let start = input.text.index(after: input.text.startIndex)
    let end = input.text.index(before: input.text.endIndex)
    input.selection = TextSelection(range: start..<end)

    input.insertNewline()

    #expect(input.text == "A\nB")
    guard case .selection(let range) = input.selection?.indices else {
        Issue.record("Expected an insertion point after Return")
        return
    }
    #expect(range.isEmpty)
    #expect(range.lowerBound == input.text.index(input.text.startIndex, offsetBy: 2))
}

@MainActor @Test func composerReturnAfterCarriageReturnKeepsValidCursor() {
    var input = ComposerText(text: "a\r")
    input.insertNewline()
    #expect(input.text == "a\r\n")
    guard case .selection(let range) = input.selection?.indices else {
        Issue.record("Expected a cursor at the end")
        return
    }
    #expect(range.lowerBound == input.text.endIndex)
}

@MainActor @Test func composerReturnAfterSendingDoesNotRestoreOldDraft() {
    var input = ComposerText(text: "1")
    input.selection = TextSelection(insertionPoint: input.text.endIndex)
    input.replace(with: "")
    input.insertNewline()
    #expect(input.text.isEmpty)
    #expect(input.selection == nil)
}
