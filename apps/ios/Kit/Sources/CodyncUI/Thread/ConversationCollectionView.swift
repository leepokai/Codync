import UIKit

/// Only one owner moves the list: the reader, a jump animation, or passive end-following.
class ConversationCollectionView: UICollectionView {
    let keyboardDismissTapDelegate = ConversationBackgroundTapDelegate()
    static let nearEnd: CGFloat = 32
    var following = true {
        didSet { if !following { needsViewportFollow = false } }
    }
    private(set) var updatingViewport = false
    private var needsViewportFollow = false
    private(set) var scrollingToEnd = false
    private var displayLink: CADisplayLink?
    private var lastFrame: TimeInterval?
    private var motion = ConversationScrollMotion()

    @objc func dismissComposerKeyboard() {
        // The composer is a sibling of the collection, not one of its descendants.
        window?.endEditing(true)
    }

    var endOffset: CGFloat {
        max(contentSize.height + adjustedContentInset.bottom - bounds.height, -adjustedContentInset.top)
    }

    /// Where the list rests with a row at `frame` in the middle, as far as its ends allow.
    func centeredOffset(of frame: CGRect) -> CGFloat {
        let insets = adjustedContentInset
        let centered = frame.midY - insets.top - (bounds.height - insets.top - insets.bottom) / 2
        return min(max(centered, -insets.top), endOffset)
    }

    /// Keyboard/composer layout is one change: intermediate offsets aren't reader scrolling.
    func updateViewport(frame: CGRect, insets: UIEdgeInsets, indicators: UIEdgeInsets) {
        guard self.frame != frame || contentInset != insets else { return }
        updatingViewport = true
        defer { updatingViewport = false }
        if following { needsViewportFollow = true }
        self.frame = frame
        contentInset = insets
        verticalScrollIndicatorInsets = indicators
        layoutIfNeeded()
        if following { pinToEnd() }
        if needsViewportFollow, window != nil { startFrames() }
    }

    func followingAfterInteraction() -> Bool {
        (following && needsViewportFollow) || endOffset - contentOffset.y < Self.nearEnd
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if following { pinToEnd() }
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil { cancelScrollingToEnd() }
        else if needsViewportFollow { startFrames() }
    }

    func pinToEnd() {
        // Every caller must yield to UIKit until the drag and its rubber-band return finish.
        guard !scrollingToEnd, !isTracking, !isDragging, !isDecelerating else { return }
        needsViewportFollow = false
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
        startFrames()
    }

    private func startFrames() {
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
        // A stationary touch has no drag-end callback. Resume viewport following once UIKit
        // releases it, without moving the content while a touch or deceleration owns it.
        if !scrollingToEnd {
            guard following, needsViewportFollow else { return cancelScrollingToEnd() }
            pinToEnd()
            if !needsViewportFollow { cancelScrollingToEnd() }
            return
        }
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

/// Keep tap filtering off the scroll view, which also delegates its own pan gesture.
final class ConversationBackgroundTapDelegate: NSObject, UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        touch.view === gestureRecognizer.view
    }
}

/// Holds the list inside the safe area and lets it reach past it by `outsets`, which become
/// its content insets: rows scroll under the bars and the composer but never get a safe area
/// of their own (a row taking one as padding near an edge would resize itself as it moves).
final class ConversationContainer: UIView {
    let list: ConversationCollectionView
    /// The safe area's size as SwiftUI gives it, not this view's bounds: the keyboard changes it and `outsets`
    /// in one update, which carries the animation, and resizes this view afterwards, with none.
    var size = CGSize.zero {
        didSet { if size != oldValue { setNeedsLayout() } }
    }
    var outsets = UIEdgeInsets.zero {
        didSet { if outsets != oldValue { setNeedsLayout() } }
    }
    /// Extra room above the rows while a pull refreshes.
    var hold: CGFloat = 0 {
        didSet { if hold != oldValue { setNeedsLayout() } }
    }

    init(list: ConversationCollectionView) {
        self.list = list
        super.init(frame: .zero)
        addSubview(list)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        let frame = CGRect(origin: .zero, size: size).inset(by: UIEdgeInsets(top: -outsets.top, left: 0, bottom: -outsets.bottom, right: 0))
        let insets = UIEdgeInsets(top: outsets.top + hold, left: 0, bottom: outsets.bottom, right: 0)
        list.updateViewport(frame: frame, insets: insets, indicators: outsets)
    }

    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        list.frame.contains(point)
    }
}
