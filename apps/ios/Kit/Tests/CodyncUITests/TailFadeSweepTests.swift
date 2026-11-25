import SwiftUI
import Testing
@testable import CodyncUI

@Test func tailFadeSweepUsesExactDurationStaggerAndLinearOpacity() {
    #expect(TailFadeSweepState.duration == 0.715)
    #expect(TailFadeSweepState.staggerInterval == 0.028)
    #expect(TailFadeSweepState.maxTotalDelay == 0.36)

    var state = TailFadeSweepState()
    state.sync(text: "alpha beta", isStreaming: true, now: 10.0)
    #expect(state.batches.count == 2)
    #expect(state.batches[0].startTime == 10.0)
    #expect(abs(state.batches[1].startTime - (10.0 + TailFadeSweepState.staggerInterval)) < 0.0001)
    #expect(state.batches[0].duration == TailFadeSweepState.duration)

    // First batch hits 0.5 opacity halfway through its 0.715s duration.
    let mid = state.opacity(forCharacterAt: 0, at: 10.0 + TailFadeSweepState.duration * 0.5)
    #expect(abs(mid - 0.5) < 0.001)
    #expect(state.opacity(forCharacterAt: 0, at: 10.0 + TailFadeSweepState.duration) == 1.0)
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

@MainActor @Test func fadeSchedulersPauseWhenLastBatchExpiresAndResumeOnNewText() {
    let coordinator = MarkdownFadeCoordinator()
    let blockCache = MarkdownBlockCache()

    let initialBlocks = blockCache.render("First chunk", isStreaming: true)
    coordinator.sync(blocks: initialBlocks, isStreaming: true, hidden: false, now: 1.0)
    #expect(coordinator.hasActiveAnimations)
    let firstDeadline = coordinator.latestEndTime ?? 0
    #expect(firstDeadline > 1.0)
    #expect(coordinator.pruneAndCheckActive(at: firstDeadline - 0.05))
    #expect(!coordinator.pruneAndCheckActive(at: firstDeadline + 0.01))
    #expect(!coordinator.hasActiveAnimations)
    #expect(coordinator.latestEndTime == nil)

    let nextBlocks = blockCache.render("First chunk second chunk", isStreaming: true)
    coordinator.sync(blocks: nextBlocks, isStreaming: true, hidden: false, now: firstDeadline + 0.5)
    #expect(coordinator.hasActiveAnimations)
    #expect((coordinator.latestEndTime ?? 0) > firstDeadline + 0.5)

    let holder = ThoughtTailFadeHolder()
    holder.sync(pieces: ["Thinking chunk"], isStreaming: true, hidden: false, now: 2.0)
    #expect(holder.isAnimating)
    let thoughtDeadline = holder.latestEndTime ?? 0
    #expect(!holder.pruneAndCheckActive(at: thoughtDeadline + 0.01))
    #expect(!holder.isAnimating)
    #expect(holder.latestEndTime == nil)

    holder.sync(pieces: ["Thinking chunk more"], isStreaming: true, hidden: false, now: thoughtDeadline + 0.2)
    #expect(holder.isAnimating)
}

@Test func longThoughtMergesIntoSingleSelectableTextWhenFadeInactiveOrFinalized() {
    let longLine = String(repeating: "a", count: 2_600)
    let fullThought = "Prefix line\n" + longLine
    let pieces = ThoughtChunking.pieces(of: fullThought)
    #expect(pieces.count > 1)

    let active = ThoughtDisplaySegments.build(text: fullThought, pieces: pieces, isActivelyFading: true)
    #expect(active == .chunked(pieces))

    let finalizedOrPaused = ThoughtDisplaySegments.build(text: fullThought, pieces: pieces, isActivelyFading: false)
    #expect(finalizedOrPaused == .unified(fullThought))
}
