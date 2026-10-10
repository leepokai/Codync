import Testing
import UIKit
@testable import CodyncUI

@Test func jumpToLatestMovesForwardWithoutOvershooting() {
    var motion = ConversationScrollMotion()
    var position: CGFloat = 120
    for _ in 0..<120 {
        let next = motion.step(from: position, to: 12_000, elapsed: 1.0 / 60)
        #expect(next >= position)
        #expect(next <= 12_000)
        position = next
    }
    #expect(motion.isSettled(at: position, target: 12_000))
}

@Test func growingReplyRetargetsAnOngoingJumpWithoutRestarting() {
    var motion = ConversationScrollMotion()
    var position: CGFloat = 0
    for frame in 0..<180 {
        let target: CGFloat = frame < 15 ? 1_000 : 1_600
        let next = motion.step(from: position, to: target, elapsed: 1.0 / 60)
        #expect(next >= position)
        #expect(next <= target)
        if frame == 15 { #expect(next - position < 100) }
        position = next
    }
    #expect(motion.isSettled(at: position, target: 1_600))
}

@Test func jumpHasTheSamePaceOnStandardAndProMotionDisplays() {
    func position(after frames: Int, rate: Double) -> CGFloat {
        var motion = ConversationScrollMotion()
        var position: CGFloat = 0
        for _ in 0..<frames { position = motion.step(from: position, to: 4_000, elapsed: 1 / rate) }
        return position
    }
    #expect(abs(position(after: 24, rate: 60) - position(after: 48, rate: 120)) < 0.01)
}

@Test func shrinkingContentDoesNotProduceABouncePastTheEnd() {
    var motion = ConversationScrollMotion()
    var position: CGFloat = 0
    for _ in 0..<10 { position = motion.step(from: position, to: 4_000, elapsed: 1.0 / 60) }
    let target = position + 1
    let next = motion.step(from: position, to: target, elapsed: 1.0 / 60)
    #expect(next <= target)
    #expect(next >= position)
}

@MainActor @Test func layoutCannotPinTheListWhileAJumpOwnsItsOffset() {
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: TallConversationLayout())
    view.following = false
    view.layoutIfNeeded()
    view.contentOffset.y = 400
    view.prepareToScrollToEnd(animated: true)
    view.following = true
    view.setNeedsLayout()
    view.layoutIfNeeded()
    view.pinToEnd()
    #expect(view.contentOffset.y == 400)
    #expect(view.scrollingToEnd)
    view.cancelScrollingToEnd()
    #expect(!view.scrollingToEnd)
    view.pinToEnd()
    #expect(view.contentOffset.y == view.endOffset)
}

@MainActor @Test func reducedMotionReleasesTheAnimationBeforePinning() {
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: TallConversationLayout())
    view.prepareToScrollToEnd(animated: true)
    view.scrollToEnd(animated: false)
    #expect(!view.scrollingToEnd)
    #expect(view.contentOffset.y == view.endOffset)
}

@MainActor @Test(arguments: ["tracking", "dragging", "decelerating"])
func bottomPinningPreservesNativeOverscrollDuringInteraction(_ phase: String) {
    let view = InteractingConversationView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                           collectionViewLayout: TallConversationLayout())
    view.layoutIfNeeded()
    view.phase = phase
    let overscroll = view.endOffset + 80
    view.contentOffset.y = overscroll
    view.pinToEnd()
    #expect(view.contentOffset.y == overscroll)
    view.setNeedsLayout()
    view.layoutIfNeeded()
    #expect(view.contentOffset.y == overscroll)
    view.phase = nil
    view.pinToEnd()
    #expect(view.contentOffset.y == view.endOffset)
}

@MainActor @Test func bottomBounceKeepsFollowingUntilTheReaderLeavesTheEnd() {
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: TallConversationLayout())
    view.layoutIfNeeded()
    // Outside a window the layout's size never arrives; without it this chat would be short.
    view.contentSize = TallConversationLayout().collectionViewContentSize
    let end = view.endOffset
    var previous = end
    for offset in [end + 80, end + 40, end + 10, end] {
        view.contentOffset.y = offset
        view.following = view.followingAfterScroll(from: previous)
        #expect(view.following)
        previous = offset
    }
    view.contentOffset.y = end - 100
    view.following = view.followingAfterScroll(from: previous)
    #expect(!view.following)
    view.contentOffset.y = end - 80
    #expect(!view.followingAfterScroll(from: end - 100))
    view.contentOffset.y = end
    #expect(view.followingAfterScroll(from: end - 80))
}

@MainActor @Test func pullingDownPastTheTopKeepsFollowing() {
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: ShortConversationLayout())
    view.layoutIfNeeded()
    var previous = view.endOffset
    for offset in [-40.0, -100, -160] {
        view.contentOffset.y = offset
        view.following = view.followingAfterScroll(from: previous)
        #expect(view.following)
        previous = offset
    }
}

@MainActor private final class InteractingConversationView: ConversationCollectionView {
    var phase: String?
    override var isTracking: Bool { phase == "tracking" }
    override var isDragging: Bool { phase == "dragging" }
    override var isDecelerating: Bool { phase == "decelerating" }
}

@MainActor private final class TallConversationLayout: UICollectionViewLayout {
    override var collectionViewContentSize: CGSize { CGSize(width: 320, height: 5_000) }
    override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { [] }
}

@MainActor private final class ShortConversationLayout: UICollectionViewLayout {
    override var collectionViewContentSize: CGSize { CGSize(width: 320, height: 200) }
    override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { [] }
}
