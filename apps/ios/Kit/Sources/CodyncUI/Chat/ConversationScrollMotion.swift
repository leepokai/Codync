import Foundation

/// Critically damped motion: one continuous velocity even when self-sizing rows move the end.
struct ConversationScrollMotion {
    private(set) var velocity: CGFloat = 0

    mutating func step(from position: CGFloat, to target: CGFloat, elapsed: TimeInterval) -> CGFloat {
        let dt = CGFloat(min(max(elapsed, 0), 1.0 / 15))
        let response: CGFloat = 18
        let distance = position - target
        let decay = exp(-response * dt)
        let coefficient = velocity + response * distance
        let next = target + (distance + coefficient * dt) * decay
        velocity = (velocity - response * coefficient * dt) * decay
        // A shrinking row can bring the end into our path. Never bounce past it.
        if (target - position) * (target - next) <= 0 {
            velocity = 0
            return target
        }
        return next
    }

    func isSettled(at position: CGFloat, target: CGFloat) -> Bool {
        abs(target - position) < 0.5 && abs(velocity) < 5
    }
}
