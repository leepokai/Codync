import Foundation
import Network
import Observation

/// Whether the phone's current path costs the user bytes (cellular, a personal hotspot): `NWPath.isExpensive`.
/// One monitor for the app, so a view can hold back a fetch that isn't worth its size on such a link.
@MainActor
@Observable
public final class NetworkCost {
    public static let shared = NetworkCost()

    /// True on an expensive path; nil until the path first reports.
    public private(set) var expensive: Bool?

    @ObservationIgnored private let monitor = NWPathMonitor()

    private init() {
        monitor.pathUpdateHandler = { path in
            let expensive = path.isExpensive
            Task { @MainActor in NetworkCost.shared.expensive = expensive }
        }
        monitor.start(queue: DispatchQueue(label: "codync.network-cost", qos: .utility))
    }
}
