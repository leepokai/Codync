import Foundation
import Observation
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

/// Observe store state instead of racing a two-second polling window on MainActor.
@MainActor
func waitForObservedCondition(_ condition: @escaping @MainActor () -> Bool) async -> Bool {
    let observer = StoreConditionObserver(condition)
    defer { observer.stop() }
    return await waitForTestEvent(observer.events, matching: { $0 })
}

@MainActor
private final class StoreConditionObserver {
    let events: AsyncStream<Bool>
    private let sink: AsyncStream<Bool>.Continuation
    private let condition: @MainActor () -> Bool
    private var stopped = false

    init(_ condition: @escaping @MainActor () -> Bool) {
        self.condition = condition
        let pair = AsyncStream<Bool>.makeStream()
        events = pair.stream
        sink = pair.continuation
        observe()
    }

    func stop() {
        stopped = true
        sink.finish()
    }

    private func observe() {
        guard !stopped else { return }
        let value = withObservationTracking {
            condition()
        } onChange: { [weak self] in
            Task { @MainActor in self?.observe() }
        }
        sink.yield(value)
    }
}
