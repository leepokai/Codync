import CodyncKit
import Foundation

/// An update this phone asked of a computer that's behind (`installHostUpdate`).
public enum HostUpdate: Equatable, Sendable {
    case requested(at: Date)
    case failed(String)
}

/// What the update notice offers for a computer that's behind (`.updateHost`).
public enum HostUpdateOffer: Equatable, Sendable {
    /// Asked within `BotStore.hostUpdateWait`: the computer is updating.
    case updating
    /// The computer updates when asked; `last` is a request that didn't (it failed, or the
    /// computer never came back updated).
    case available(last: HostUpdate?)
    /// Not from here: the notice says how to update it on the computer.
    case unavailable
}

extension BotStore {
    /// How long an update may take before the notice offers it again.
    static let hostUpdateWait: TimeInterval = 10 * 60
    /// While it updates, `hello` is asked this often, so the notice goes away soon after.
    static let updatingRecheck: Duration = .seconds(5)

    public func hostUpdateOffer(now: Date = .now) -> HostUpdateOffer {
        if case let .requested(at) = hostUpdate, now.timeIntervalSince(at) < Self.hostUpdateWait { return .updating }
        return hello?.canUpdate == true ? .available(last: hostUpdate) : .unavailable
    }

    /// Asks the computer to update itself; the notice follows along until its version changes.
    public func requestHostUpdate() {
        Motion.animate { hostUpdate = .requested(at: .now) }
        Task {
            do {
                try await withLink { try await $0.installHostUpdate() }
            } catch {
                Motion.animate { hostUpdate = .failed(error.localizedDescription) }
            }
        }
    }
}
