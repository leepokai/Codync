import Foundation
import Observation

/// The Codync cloud's emergency floor for this app version: below it, the Worker answers every request,
/// the relay upgrade included, with `426 appUpdateRequired`, and the app shows *Update this app* with the
/// cloud's reason. It holds until the app is updated (a new process).
@MainActor
@Observable
public final class AppFloor {
    public nonisolated static let shared = AppFloor()

    /// Why this app has to update; nil while the cloud takes it.
    public private(set) var reason: String?

    nonisolated init() {}

    func refuse(_ reason: String?) {
        let reason = reason ?? "Update Codync to keep using it."
        guard reason != self.reason else { return }
        Motion.animate { self.reason = reason }
    }
}
