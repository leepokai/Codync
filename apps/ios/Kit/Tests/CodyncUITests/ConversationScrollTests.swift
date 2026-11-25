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

@MainActor @Test func lightUpwardScrollWithinNearEndSuppressesAutoFollowForWindow() {
    var currentTime: TimeInterval = 100.0
    let view = ConversationCollectionView(frame: CGRect(x: 0, y: 0, width: 320, height: 600),
                                          collectionViewLayout: TallConversationLayout())
    view.clock = { currentTime }
    view.layoutIfNeeded()
    view.contentSize = TallConversationLayout().collectionViewContentSize
    let end = view.endOffset
    view.contentOffset.y = end

    // Scroll up lightly by 16 pt (< 32 pt nearEnd): must pause following and arm 0.25s suppression.
    view.contentOffset.y = end - 16
    view.following = view.followingAfterScroll(from: end)
    #expect(!view.following)
    #expect(view.isAutoScrollSuppressed)

    // Inside the 0.25s window, moving slightly toward the end does not snap back down.
    currentTime = 100.12
    view.contentOffset.y = end - 4
    #expect(!view.followingAfterScroll(from: end - 16))
    #expect(!view.followingAfterInteraction())

    // Once the suppression window expires, scrolling near the end restores following.
    currentTime = 100.26
    #expect(!view.isAutoScrollSuppressed)
    view.contentOffset.y = end - 4
    #expect(view.followingAfterScroll(from: end - 8))
    #expect(view.followingAfterInteraction())
}

@MainActor @Test func contentGrowthFollowsSmoothlyAndStopsDisplayLinkWhenSettled() async throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 600))
    let controller = UIViewController()
    window.rootViewController = controller
    let layout = DynamicConversationLayout(height: 1_200)
    let view = ConversationCollectionView(frame: window.bounds, collectionViewLayout: layout)
    controller.view.addSubview(view)
    window.makeKeyAndVisible()
    defer { window.isHidden = true; view.removeFromSuperview() }

    view.layoutIfNeeded()
    view.contentSize = layout.collectionViewContentSize
    view.pinToEnd()
    view.animatesContentGrowth = true

    // Simulate a streaming reply growing height monotonically across consecutive updates.
    layout.height = 1_600
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(view.followingContentGrowth)
    #expect(view.hasActiveDisplayLink)

    // Retarget while still moving without dropping followingContentGrowth.
    try await Task.sleep(for: .milliseconds(32))
    layout.height = 1_900
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(view.followingContentGrowth)
    #expect(view.contentOffset.y <= view.endOffset + 0.5)

    // Wait for critically damped motion to settle; CADisplayLink must tear down automatically.
    for _ in 0..<80 where view.hasActiveDisplayLink {
        try await Task.sleep(for: .milliseconds(25))
    }
    #expect(!view.hasActiveDisplayLink)
    #expect(!view.followingContentGrowth)
    #expect(abs(view.contentOffset.y - view.endOffset) < 1)
}

@MainActor @Test func displayLinkStopsWhenRemovedFromWindowOrDragBegins() {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 600))
    let controller = UIViewController()
    window.rootViewController = controller
    let layout = DynamicConversationLayout(height: 1_200)
    let view = ConversationCollectionView(frame: window.bounds, collectionViewLayout: layout)
    controller.view.addSubview(view)
    window.makeKeyAndVisible()
    defer { window.isHidden = true; view.removeFromSuperview() }

    view.layoutIfNeeded()
    view.contentSize = layout.collectionViewContentSize
    view.pinToEnd()
    view.animatesContentGrowth = true
    layout.height = 1_800
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(view.hasActiveDisplayLink)

    view.removeFromSuperview()
    #expect(!view.hasActiveDisplayLink)
    #expect(!view.followingContentGrowth)

    controller.view.addSubview(view)
    view.layoutIfNeeded()
    layout.height = 2_200
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(view.hasActiveDisplayLink)

    // Drag start / explicit cancel stops the active display link immediately.
    view.cancelScrollingToEnd()
    #expect(!view.hasActiveDisplayLink)
    #expect(!view.followingContentGrowth)
}

@MainActor @Test func contentShrinkageDuringGrowthFollowClampsWithoutOvershoot() {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 600))
    let controller = UIViewController()
    window.rootViewController = controller
    let layout = DynamicConversationLayout(height: 1_800)
    let view = ConversationCollectionView(frame: window.bounds, collectionViewLayout: layout)
    controller.view.addSubview(view)
    window.makeKeyAndVisible()
    defer { window.isHidden = true; view.removeFromSuperview() }

    view.layoutIfNeeded()
    view.contentSize = layout.collectionViewContentSize
    view.pinToEnd()
    view.animatesContentGrowth = true

    layout.height = 1_400
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(!view.hasActiveDisplayLink)
    #expect(abs(view.contentOffset.y - view.endOffset) < 1)
}

@Test func contentGrowthMotionSlowsPaceWithoutOvershoot() {
    #expect(ConversationScrollMotion.contentGrowthResponse == 12.6)
    var fast = ConversationScrollMotion(response: ConversationScrollMotion.defaultResponse)
    var gentle = ConversationScrollMotion(response: ConversationScrollMotion.contentGrowthResponse)
    var fastPos: CGFloat = 0
    var gentlePos: CGFloat = 0
    for _ in 0 ..< 10 {
        fastPos = fast.step(from: fastPos, to: 1_000, elapsed: 1.0 / 60)
        gentlePos = gentle.step(from: gentlePos, to: 1_000, elapsed: 1.0 / 60)
    }
    #expect(gentlePos < fastPos)
    #expect(gentlePos > 0)

    for _ in 0 ..< 120 {
        let next = gentle.step(from: gentlePos, to: 1_000, elapsed: 1.0 / 60)
        #expect(next >= gentlePos)
        #expect(next <= 1_000)
        gentlePos = next
    }
    #expect(gentle.isSettled(at: gentlePos, target: 1_000))
}

@MainActor @Test func disablingContentGrowthAnimationStopsDisplayLinkAndSettlesToEnd() {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 600))
    let controller = UIViewController()
    window.rootViewController = controller
    let layout = DynamicConversationLayout(height: 1_200)
    let view = ConversationCollectionView(frame: window.bounds, collectionViewLayout: layout)
    controller.view.addSubview(view)
    window.makeKeyAndVisible()
    defer { window.isHidden = true; view.removeFromSuperview() }

    view.layoutIfNeeded()
    view.contentSize = layout.collectionViewContentSize
    view.pinToEnd()
    view.animatesContentGrowth = true

    layout.height = 2_000
    view.contentSize = layout.collectionViewContentSize
    view.followContentGrowth()
    #expect(view.followingContentGrowth)
    #expect(view.hasActiveDisplayLink)

    // Simulates ConversationList disabling animatesContentGrowth when scenePhase leaves .active.
    view.animatesContentGrowth = false
    #expect(!view.followingContentGrowth)
    #expect(!view.hasActiveDisplayLink)
    #expect(abs(view.contentOffset.y - view.endOffset) < 1)
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

@MainActor private final class DynamicConversationLayout: UICollectionViewLayout {
    var height: CGFloat
    init(height: CGFloat) {
        self.height = height
        super.init()
    }
    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }
    override var collectionViewContentSize: CGSize { CGSize(width: 320, height: height) }
    override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { [] }
}
