import CodyncKit
import SwiftUI
import UIKit

/// How a message swiped to reply moves: into the open space beside it, with the finger.
enum ReplySwipe {
    /// How far the message goes before letting go replies.
    static let threshold: CGFloat = 64

    /// The way it moves, as the sign of x: the user's messages left, the bots' right.
    enum Direction: CGFloat {
        case left = -1, right = 1

        var alignment: Alignment { self == .right ? .leading : .trailing }
    }

    /// Only a mostly sideways drag the message's way starts a swipe; anything else stays the list's scroll. Judged on
    /// the finger's velocity: a pan can be asked at its first move, before any translation. Rightward pulls that start
    /// at the screen's left edge stay the navigation back swipe: `startX` is where the finger came down, since a quick
    /// back swipe can already be past the edge when the pan is asked.
    static func begins(_ velocity: CGPoint, _ direction: Direction, startX: CGFloat) -> Bool {
        (direction == .left || startX > 20) && abs(velocity.x) > abs(velocity.y) && velocity.x * direction.rawValue > 0
    }

    /// How far the message moves for the finger's `dx`: with it up to the threshold, a quarter as far past it,
    /// never the other way.
    static func travel(_ dx: CGFloat, _ direction: Direction) -> CGFloat {
        let moved = max(0, dx * direction.rawValue)
        return moved <= threshold ? moved : threshold + (moved - threshold) / 4
    }

    /// The visible movement stays in the open space beside the message, including its outer margin; full travel decides the reply.
    static func shown(_ travel: CGFloat, gap: CGFloat) -> CGFloat {
        min(travel, max(0, gap))
    }
}

extension View {
    /// Quotes the message in the composer when it's dragged sideways, as in Messages; nothing when it can't be quoted.
    @ViewBuilder func swipeToReply(_ direction: ReplySwipe.Direction, margin: CGFloat, reply: (() -> Void)?) -> some View {
        if let reply { modifier(SwipeToReply(direction: direction, margin: margin, reply: reply)) }
        else { frame(maxWidth: .infinity, alignment: direction.alignment) }
    }
}

/// The message follows the finger, the reply arrow grows in the gap it leaves, and past the threshold a light tap
/// says that letting go replies. It always springs back.
private struct SwipeToReply: ViewModifier {
    let direction: ReplySwipe.Direction
    let margin: CGFloat
    let reply: () -> Void
    @State private var travel: CGFloat = 0
    @State private var contentWidth: CGFloat = 0
    @State private var columnWidth: CGFloat = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func body(content: Content) -> some View {
        let progress = min(travel / ReplySwipe.threshold, 1)
        let shown = ReplySwipe.shown(travel, gap: columnWidth - contentWidth + margin)
        content
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { contentWidth = $0 }
            .offset(x: shown * direction.rawValue)
            .frame(maxWidth: .infinity, alignment: direction.alignment)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { columnWidth = $0 }
            .background(alignment: direction.alignment) {
                Image(systemName: "arrowshape.turn.up.left")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.secondary)
                    .scaleEffect(0.5 + progress / 2)
                    .opacity(progress)
                    .frame(width: shown)
                    .clipped()
                    .accessibilityHidden(true)
            }
            .gesture(SidewaysPan(direction: direction, moved: { travel = $0 }, released: release))
            .sensoryFeedback(.impact(weight: .light), trigger: travel >= ReplySwipe.threshold) { !$0 && $1 }
    }

    private func release(distance: CGFloat, replying: Bool) {
        if replying, distance >= ReplySwipe.threshold {
            Analytics.shared.capture(.replySwiped)
            reply()
        }
        // The drag's final reach decides where the spring back starts, which can be past where the bubble showed.
        travel = ReplySwipe.shown(distance, gap: columnWidth - contentWidth + margin)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { travel = 0 }
    }
}

/// A UIKit pan, so the list's scroll and the full-width back swipe keep every drag that isn't clearly sideways the
/// message's way: they wait only for this pan to fail, which happens at the drag's first move.
private struct SidewaysPan: UIGestureRecognizerRepresentable {
    let direction: ReplySwipe.Direction
    let moved: (CGFloat) -> Void
    /// `distance`: the drag's final reach; `replying`: the finger lifted (not a cancelled drag).
    let released: (_ distance: CGFloat, _ replying: Bool) -> Void

    func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
        let pan = UIPanGestureRecognizer()
        pan.delegate = context.coordinator
        return pan
    }

    func updateUIGestureRecognizer(_ pan: UIPanGestureRecognizer, context: Context) {
        context.coordinator.direction = direction
    }

    func makeCoordinator(converter: CoordinateSpaceConverter) -> Coordinator {
        Coordinator(direction: direction)
    }

    func handleUIGestureRecognizerAction(_ pan: UIPanGestureRecognizer, context: Context) {
        let distance = ReplySwipe.travel(pan.translation(in: pan.view).x, direction)
        switch pan.state {
        case .began, .changed: moved(distance)
        // The final reach matters as much as the last move: a quick swipe can cross the threshold in the move that
        // begins the pan and lift without another.
        case .ended: released(distance, true)
        case .cancelled, .failed: released(distance, false)
        default: break
        }
    }

    @MainActor final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var direction: ReplySwipe.Direction

        init(direction: ReplySwipe.Direction) {
            self.direction = direction
        }

        func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
            guard let pan = recognizer as? UIPanGestureRecognizer, let view = pan.view else { return false }
            let location = pan.location(in: nil)
            let startX = location.x - pan.translation(in: nil).x
            return ReplySwipe.begins(pan.velocity(in: view), direction, startX: startX)
        }

        /// The list holding the row and the back swipe wait for this pan; a scroll inside the bubble (a code block's)
        /// doesn't, so it keeps its own sideways drag.
        func gestureRecognizer(_ recognizer: UIGestureRecognizer,
                               shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
            if let scroll = other.view as? UIScrollView, other === scroll.panGestureRecognizer {
                return recognizer.view.map { $0 !== scroll && $0.isDescendant(of: scroll) } ?? false
            }
            return other === (other.view?.next as? UINavigationController)?.interactiveContentPopGestureRecognizer
        }
    }
}
