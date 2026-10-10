import UIKit

/// Only one owner moves the list: the reader, a jump animation, or passive end-following.
class ConversationCollectionView: UICollectionView {
    static let nearEnd: CGFloat = 32
    var following = true
    private(set) var scrollingToEnd = false
    private var displayLink: CADisplayLink?
    private var lastFrame: TimeInterval?
    private var motion = ConversationScrollMotion()

    var endOffset: CGFloat {
        max(contentSize.height + adjustedContentInset.bottom - bounds.height, -adjustedContentInset.top)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if following { pinToEnd() }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil { cancelScrollingToEnd() }
    }

    func pinToEnd() {
        // Every caller must yield to UIKit until the drag and its rubber-band return finish.
        guard !scrollingToEnd, !isTracking, !isDragging, !isDecelerating else { return }
        let end = endOffset
        if abs(contentOffset.y - end) > 0.5 { contentOffset.y = end }
    }

    func followingAfterScroll(from previousOffset: CGFloat) -> Bool {
        // Returning from overscroll moves upward too, but is not reading older messages.
        if endOffset - contentOffset.y < Self.nearEnd { return true }
        // Neither is pulling down past the top to refresh. Flipping here re-renders the list
        // mid-pull, and every snapshot applied during a drag snaps the pull back.
        if contentOffset.y < -adjustedContentInset.top { return following }
        if contentOffset.y < previousOffset - 0.5 { return false }
        return following
    }

    /// Called before enabling following, applying a snapshot, or updating keyboard insets.
    func prepareToScrollToEnd(animated: Bool) {
        if animated {
            scrollingToEnd = true
            // A jump can be tapped while a previous swipe is still coasting.
            if isDecelerating { setContentOffset(contentOffset, animated: false) }
        } else {
            cancelScrollingToEnd()
        }
    }

    func scrollToEnd(animated: Bool) {
        guard animated, window != nil else {
            cancelScrollingToEnd()
            return pinToEnd()
        }
        scrollingToEnd = true
        // Repeated requests retarget the existing motion; they never rewind or restart it.
        guard displayLink == nil else { return }
        motion = ConversationScrollMotion()
        lastFrame = nil
        let link = CADisplayLink(target: ConversationScrollFrames(self), selector: #selector(ConversationScrollFrames.tick(_:)))
        displayLink = link
        link.add(to: .main, forMode: .common)
    }

    func cancelScrollingToEnd() {
        displayLink?.invalidate()
        displayLink = nil
        lastFrame = nil
        scrollingToEnd = false
    }

    fileprivate func advance(_ link: CADisplayLink) {
        guard following, !isTracking, !isDecelerating else { return cancelScrollingToEnd() }
        // Measure only the current viewport. Do not jump offscreen to pre-size estimated rows.
        layoutIfNeeded()
        let elapsed = lastFrame.map { link.timestamp - $0 } ?? (link.targetTimestamp - link.timestamp)
        lastFrame = link.timestamp
        let target = endOffset
        let next = motion.step(from: contentOffset.y, to: target, elapsed: elapsed)
        UIView.performWithoutAnimation { contentOffset.y = next }
        if motion.isSettled(at: contentOffset.y, target: endOffset) {
            cancelScrollingToEnd()
            pinToEnd()
        }
    }
}

/// The run loop retains the link, so its target must not retain the collection view.
@MainActor private final class ConversationScrollFrames: NSObject {
    private weak var view: ConversationCollectionView?

    init(_ view: ConversationCollectionView) { self.view = view }

    @objc func tick(_ link: CADisplayLink) {
        guard let view else { return link.invalidate() }
        view.advance(link)
    }
}
