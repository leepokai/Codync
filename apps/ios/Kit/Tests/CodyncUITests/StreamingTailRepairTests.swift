import Testing
@testable import CodyncUI

@Test func streamingTailRepairHoldsBareOpeningDelimiters() {
    #expect(StreamingTailRepair.repair("**") == "")
    #expect(StreamingTailRepair.repair("hello **") == "hello")
    #expect(StreamingTailRepair.repair("hello __") == "hello")
    #expect(StreamingTailRepair.repair("hello `") == "hello")
    #expect(StreamingTailRepair.repair("hello ~~") == "hello")
}

@Test func streamingTailRepairClosesHalfTypedInlineEmphasisAndCode() {
    #expect(StreamingTailRepair.repair("See **bold") == "See **bold**")
    #expect(StreamingTailRepair.repair("See *ital") == "See *ital*")
    #expect(StreamingTailRepair.repair("See ~~strike") == "See ~~strike~~")
    #expect(StreamingTailRepair.repair("Use `code") == "Use `code`")
    #expect(StreamingTailRepair.repair("Use ``code") == "Use ``code``")
}

@Test func streamingTailRepairPreservesArithmeticMultiplicationAsterisk() {
    #expect(StreamingTailRepair.repair("x = 3 *") == "x = 3 *")
    #expect(StreamingTailRepair.repair("a _ b") == "a _ b")
}

@Test func streamingTailRepairPreservesEscapedDelimiters() {
    #expect(StreamingTailRepair.repair(#"\*hi"#) == #"\*hi"#)
    #expect(StreamingTailRepair.repair(#"trailing \"#) == "trailing")
}

@Test func streamingTailRepairHoldsHalfTypedFenceOpenerAndCloser() {
    #expect(StreamingTailRepair.repair("Intro\n```swift") == "Intro\n")
    #expect(StreamingTailRepair.repair("```swift\nlet x = 1\n``") == "```swift\nlet x = 1\n")
    #expect(StreamingTailRepair.repair("```swift\nlet x = 1") == "```swift\nlet x = 1")
}

@Test func streamingTailRepairSynthesizesTableSeparatorForStreamingHeaders() {
    let headerWithNewline = "| Col1 | Col2 |\n"
    #expect(StreamingTailRepair.repair(headerWithNewline) == "| Col1 | Col2 |\n| --- | --- |")

    let partialDelimiter = "| Col1 | Col2 |\n| ---"
    #expect(StreamingTailRepair.repair(partialDelimiter) == "| Col1 | Col2 |\n| --- | --- |")

    let unclosedTrailingCell = "| Col1 | Col2"
    #expect(StreamingTailRepair.repair(unclosedTrailingCell) == "| Col1 | Col2 |\n| --- | --- |")

    let headingAndHeader = "## Summary\n| Key | Val |"
    #expect(StreamingTailRepair.repair(headingAndHeader) == "## Summary\n| Key | Val |\n| --- | --- |")
}

@Test func streamingTailRepairLeavesSinglePipeWithoutSecondColumnLiteral() {
    #expect(StreamingTailRepair.repair("| single") == "| single")
}

@Test func markdownBlocksParsesRepairedStreamingTableAndClosedEmphases() {
    let tableBlocks = MarkdownBlocks.parse("| Name | Score |\n", isStreaming: true)
    #expect(tableBlocks == [.table(header: ["Name", "Score"], rows: [])])

    let listBlocks = MarkdownBlocks.parse("- first\n-", isStreaming: true)
    #expect(listBlocks == [.bullet("first", marker: "•", depth: 0)])
}

@Test func streamingTailRepairDropsMarkerOnlyTrailingLines() {
    #expect(StreamingTailRepair.repair("- first\n-") == "- first\n")
    #expect(StreamingTailRepair.repair("Intro\n##") == "Intro\n")
    #expect(StreamingTailRepair.repair("Intro\n>") == "Intro\n")
    #expect(StreamingTailRepair.repair("1. one\n2.") == "1. one\n")
    #expect(StreamingTailRepair.repair("- [ ] todo\n- [ ]") == "- [ ] todo\n")
    #expect(StreamingTailRepair.repair("- parent\n  -") == "- parent\n")
}

@Test func streamingTailRepairRespectsNewlineTerminatedLines() {
    #expect(StreamingTailRepair.repair("Dangling **bold\n") == "Dangling **bold\n")
    #expect(StreamingTailRepair.repair("Line 1\nLine 2\n") == "Line 1\nLine 2\n")
}

@Test func streamingTailRepairHoldsPartialLinksImagesTagsAndEntities() {
    #expect(StreamingTailRepair.repair("Look ![alt") == "Look")
    #expect(StreamingTailRepair.repair("Look ![alt](https://example.com/a") == "Look")
    #expect(StreamingTailRepair.repair("Read [doc](https://example.com") == "Read [doc](https://example.com)")
    #expect(StreamingTailRepair.repair("Tag <sp") == "Tag")
    #expect(StreamingTailRepair.repair("Entity &nb") == "Entity")
    #expect(StreamingTailRepair.repair("<span>ok</span> &amp;") == "<span>ok</span> &amp;")
}

@Test func streamingTailRepairSkipsIndentedCodeAndBlockMath() {
    #expect(StreamingTailRepair.repair("    let x = **") == "    let x = **")
    #expect(StreamingTailRepair.repair("$$\na * b") == "$$\na * b")
    #expect(StreamingTailRepair.repair("Formula $a + b") == "Formula $a + b")
}

@Test func streamingTailRepairPreservesGraphemeClustersAndMultiByteUTF8() {
    let emojiInput = "Family 👨‍👩‍👧‍👦 **加粗"
    let repairedEmoji = StreamingTailRepair.repair(emojiInput)
    #expect(repairedEmoji == "Family 👨‍👩‍👧‍👦 **加粗**")

    let bareAfterEmoji = "Hi 👨‍👩‍👧‍👦 **"
    #expect(StreamingTailRepair.repair(bareAfterEmoji) == "Hi 👨‍👩‍👧‍👦")

    let imageAfterEmoji = "Hi 👨‍👩‍👧‍👦 ![pic](https://x"
    #expect(StreamingTailRepair.repair(imageAfterEmoji) == "Hi 👨‍👩‍👧‍👦")
}
