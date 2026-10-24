import SwiftUI
import Testing
import UIKit
@testable import CodyncUI

@MainActor @Test func keyboardSafeAreaKeepsTheLatestMessageVisible() async throws {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
    let rows = (0..<30).map { index in
        ConversationRow("\(index)") { Text("Message \(index)").frame(height: 60) }
    }
    let host = UIHostingController(rootView: ConversationList(rows: rows, following: .constant(true))
        .safeAreaInset(edge: .bottom) { Color.clear.frame(height: 60) })
    window.rootViewController = host
    window.makeKeyAndVisible()
    defer { window.isHidden = true }
    try await Task.sleep(for: .milliseconds(300))
    let list = try #require(findConversation(in: host.view))
    let previousHeight = list.bounds.height - list.adjustedContentInset.top - list.adjustedContentInset.bottom
    host.additionalSafeAreaInsets.bottom = 300
    host.view.setNeedsLayout()
    host.view.layoutIfNeeded()
    try await Task.sleep(for: .milliseconds(300))
    #expect(list.following)
    #expect(abs(list.contentOffset.y - list.endOffset) < 1)
    #expect(abs(previousHeight - (list.bounds.height - list.adjustedContentInset.top - list.adjustedContentInset.bottom) - 300) < 1)
}

@MainActor private func findConversation(in view: UIView) -> ConversationCollectionView? {
    if let list = view as? ConversationCollectionView { return list }
    return view.subviews.lazy.compactMap { findConversation(in: $0) }.first
}

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
