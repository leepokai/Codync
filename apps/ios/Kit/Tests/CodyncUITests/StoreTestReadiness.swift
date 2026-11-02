import Foundation
@testable import CodyncUI

/// Wait for a recorded fixture event, allowing the store's real action deadline.
/// Cancelling the losing stream iterator also removes its fixture subscription.
@MainActor
func waitForTestEvent<Event: Sendable>(_ events: AsyncStream<Event>, matching: @escaping @Sendable (Event) -> Bool) async -> Bool {
    let timeout = BotStore.actionPatience
    return await withTaskGroup(of: Bool.self) { group in
        group.addTask {
            for await event in events {
                if matching(event) { return true }
            }
            return false
        }
        group.addTask {
            do { try await Task.sleep(for: timeout) } catch { return false }
            return false
        }
        let result = await group.next()
        group.cancelAll()
        return result ?? false
    }
}

/// Reads `condition` until it holds, for the store's action deadline.
///
/// Connection changes are published inside an animation. On a runner with no
/// display link that notification can be dropped, and the test then sits until
/// its time limit. Reading the value from this task does not depend on it.
@MainActor
func waitForObservedCondition(_ condition: @escaping @MainActor () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + BotStore.actionPatience
    while ContinuousClock.now < deadline {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return condition()
}
