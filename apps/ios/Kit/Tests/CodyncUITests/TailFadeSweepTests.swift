import SwiftUI
import Testing
@testable import CodyncUI

@Test func tailFadeSweepUsesExactDurationStaggerAndLinearOpacity() {
    var state = TailFadeSweepState()
    state.sync(text: "alpha beta", isStreaming: true, now: 10.0)
    #expect(state.batches.count == 2)
    #expect(state.batches[0].startTime == 10.0)
    #expect(abs(state.batches[1].startTime - (10.0 + TailFadeSweepState.staggerInterval)) < 0.0001)
    #expect(state.batches[0].duration == TailFadeSweepState.duration)

    // First batch hits 0.5 opacity halfway through its 0.5s duration.
    let mid = state.opacity(forCharacterAt: 0, at: 10.25)
    #expect(abs(mid - 0.5) < 0.001)
    #expect(state.opacity(forCharacterAt: 0, at: 10.50) == 1.0)
}

@Test func tailFadeSweepCapsStaggerDelayAtMaxTotalDelay() {
    var state = TailFadeSweepState()
    let burst = (0 ..< 40).map { "word\($0)" }.joined(separator: " ")
    state.sync(text: burst, isStreaming: true, now: 5.0)

    #expect(state.batches.count <= TailFadeSweepState.maxWaveUnits)
    for batch in state.batches {
        #expect(batch.startTime <= 5.0 + TailFadeSweepState.maxTotalDelay + 0.0001)
    }
}

@Test func tailFadeSweepEvictsOldestBatchesPastMaxBatches() {
    var state = TailFadeSweepState()
    var text = ""
    for step in 0 ..< 45 {
        text += "t\(step) "
        state.sync(text: text, isStreaming: true, now: Double(step) * 0.005)
        #expect(state.batches.count <= TailFadeSweepState.maxBatches)
    }

    #expect(state.batches.count == TailFadeSweepState.maxBatches)
    #expect(state.activeOffset > 0)
    #expect(state.opacity(forCharacterAt: 0, at: 0.25) == 1.0)
}

@Test func tailFadeSweepStopsImmediatelyWhenNotStreamingOrHidden() {
    var state = TailFadeSweepState()
    state.sync(text: "Hello streaming", isStreaming: true, now: 1.0)
    #expect(state.isAnimating)

    state.sync(text: "Hello streaming", isStreaming: false, now: 1.1)
    #expect(!state.isAnimating)
    #expect(state.batches.isEmpty)

    state.sync(text: "Hello streaming again", isStreaming: true, hidden: true, now: 2.0)
    #expect(!state.isAnimating)
    #expect(state.batches.isEmpty)
}

@MainActor @Test func tailFadeSweepApplyFadePreservesTextVerbatimWithZeroZeroWidthCharacters() {
    let raw = "Check [link](https://example.com) and `code` tail"
    var attributed = MarkdownText.inline(raw)
    let expectedPlain = String(attributed.characters)

    var state = TailFadeSweepState()
    state.sync(text: expectedPlain, isStreaming: true, now: 1.0)
    TailFadeSweepState.applyFade(to: &attributed, state: state, at: 1.1, baseColor: .primary)

    let afterPlain = String(attributed.characters)
    #expect(afterPlain == expectedPlain)
    #expect(!afterPlain.contains("\u{200B}"))
    #expect(!afterPlain.contains("\u{FEFF}"))
}

@Test func thoughtChunkingSplitsAtNewlinesNear1KBWithoutAlteringText() {
    var lines: [String] = []
    for i in 0 ..< 50 {
        lines.append("Line \(i): reasoning step with 中文 and emoji 🙂 to verify byte boundary.")
    }
    let full = lines.joined(separator: "\n")
    let pieces = ThoughtChunking.pieces(of: full)

    #expect(pieces.count > 1)
    #expect(pieces.joined() == full)
    for piece in pieces.dropLast() {
        #expect(piece.utf8.count >= ThoughtChunking.pieceLength)
        #expect(piece.hasSuffix("\n"))
    }
}

@MainActor @Test func markdownFadeCoordinatorAnimatesTableCellsAndSettlesOnReduceMotion() {
    let coordinator = MarkdownFadeCoordinator()
    let blockCache = MarkdownBlockCache()

    let blocks = blockCache.render("| A | B |\n| - | - |\n| 1 | 2", isStreaming: true)
    coordinator.sync(blocks: blocks, isStreaming: true, hidden: false, now: 1.0)
    #expect(coordinator.hasActiveAnimations)
    #expect(!coordinator.tableCellStates(forBlock: 0).isEmpty)

    coordinator.settleAll(at: 1.1, hidden: true)
    #expect(!coordinator.hasActiveAnimations)
    #expect(coordinator.tableCellStates(forBlock: 0).isEmpty)
}
