import Testing
import UIKit
@testable import CodyncUI

@MainActor @Test func keyboardResizeDuringTouchKeepsFollowingWhenTheTouchEnds() {
    let list = ViewportConversationView(frame: CGRect(x: 0, y: 0, width: 390, height: 700),
                                        collectionViewLayout: ViewportConversationLayout())
    list.layoutIfNeeded()
    list.contentSize = CGSize(width: 390, height: 2_000)
    list.pinToEnd()
    list.simulatingTouch = true
    list.updateViewport(frame: list.frame, insets: UIEdgeInsets(top: 0, left: 0, bottom: 300, right: 0), indicators: .zero)
    #expect(list.followingAfterInteraction())
    list.simulatingTouch = false
    list.following = list.followingAfterInteraction()
    list.pinToEnd()
    #expect(abs(list.contentOffset.y - list.endOffset) < 1)
}

@MainActor @Test func keyboardResizePreservesAnOlderMessageBeingRead() {
    let list = ViewportConversationView(frame: CGRect(x: 0, y: 0, width: 390, height: 700),
                                        collectionViewLayout: ViewportConversationLayout())
    list.following = false
    list.layoutIfNeeded()
    list.contentSize = CGSize(width: 390, height: 2_000)
    list.contentOffset.y = 400
    list.updateViewport(frame: list.frame, insets: UIEdgeInsets(top: 0, left: 0, bottom: 300, right: 0), indicators: .zero)
    #expect(list.contentOffset.y == 400)
    #expect(!list.followingAfterInteraction())
}

@MainActor @Test func scrollingAwayCancelsADeferredKeyboardFollow() {
    let list = ViewportConversationView(frame: CGRect(x: 0, y: 0, width: 390, height: 700),
                                        collectionViewLayout: ViewportConversationLayout())
    list.layoutIfNeeded()
    list.contentSize = CGSize(width: 390, height: 2_000)
    list.pinToEnd()
    list.simulatingTouch = true
    list.updateViewport(frame: list.frame, insets: UIEdgeInsets(top: 0, left: 0, bottom: 300, right: 0), indicators: .zero)
    let previous = list.contentOffset.y
    list.contentOffset.y -= 100
    list.following = list.followingAfterScroll(from: previous)
    list.simulatingTouch = false
    #expect(!list.followingAfterInteraction())
}

/// The keyboard hands over the new size and insets in one animated update and resizes the container afterwards,
/// unanimated: that second step must leave the list where the first one put it.
@MainActor @Test func keyboardResizeOfTheContainerLeavesTheListInPlace() {
    let list = ViewportConversationView(frame: .zero, collectionViewLayout: ViewportConversationLayout())
    let container = ConversationContainer(list: list)
    container.frame = CGRect(x: 0, y: 100, width: 390, height: 600)
    container.size = CGSize(width: 390, height: 300)
    container.outsets = UIEdgeInsets(top: 100, left: 0, bottom: 444, right: 0)
    container.layoutIfNeeded()
    let placed = list.frame
    #expect(placed == CGRect(x: 0, y: -100, width: 390, height: 844))
    #expect(list.contentInset.bottom == 444)

    container.frame.size.height = 300
    container.layoutIfNeeded()

    #expect(list.frame == placed)
}

@MainActor @Test func aRowByTheEndCannotBeBroughtToTheMiddle() {
    let list = ViewportConversationView(frame: CGRect(x: 0, y: 0, width: 390, height: 700),
                                        collectionViewLayout: ViewportConversationLayout())
    list.layoutIfNeeded()
    list.contentSize = CGSize(width: 390, height: 2_000)
    list.pinToEnd()
    // The last row: the list is already where bringing it into view would leave it.
    #expect(list.centeredOffset(of: CGRect(x: 0, y: 1_800, width: 390, height: 200)) == list.contentOffset.y)
    #expect(list.centeredOffset(of: CGRect(x: 0, y: 900, width: 390, height: 200)) == 650)
    #expect(list.centeredOffset(of: CGRect(x: 0, y: 0, width: 390, height: 200)) == 0)
}

@MainActor @Test func stationaryTouchResumesViewportFollowingWithoutADragCallback() async throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 700))
    let controller = UIViewController()
    window.rootViewController = controller
    let list = ViewportConversationView(frame: window.bounds, collectionViewLayout: ViewportConversationLayout())
    controller.view.addSubview(list)
    window.makeKeyAndVisible()
    defer { window.isHidden = true; list.removeFromSuperview() }
    list.layoutIfNeeded()
    list.contentSize = CGSize(width: 390, height: 2_000)
    list.pinToEnd()
    list.simulatingTouch = true
    list.updateViewport(frame: list.frame, insets: UIEdgeInsets(top: 0, left: 0, bottom: 300, right: 0), indicators: .zero)
    list.simulatingTouch = false
    try await Task.sleep(for: .milliseconds(200))
    #expect(list.following)
    #expect(abs(list.contentOffset.y - list.endOffset) < 1)
}

@MainActor private final class ViewportConversationView: ConversationCollectionView {
    var simulatingTouch = false
    override var isTracking: Bool { simulatingTouch }
}

@MainActor private final class ViewportConversationLayout: UICollectionViewLayout {
    override var collectionViewContentSize: CGSize { CGSize(width: 390, height: 2_000) }
    override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { [] }
}
