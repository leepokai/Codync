import Observation
import Synchronization

/// Translation lookups participate in SwiftUI observation without recreating screens or losing drafts.
final class LanguageUpdates: Observable, Sendable {
    static let shared = LanguageUpdates()
    private let registrar = ObservationRegistrar()
    private let counter = Mutex(0)

    var revision: Int {
        registrar.access(self, keyPath: \.revision)
        return counter.withLock { $0 }
    }

    func changed() {
        registrar.withMutation(of: self, keyPath: \.revision) {
            counter.withLock { $0 &+= 1 }
        }
    }
}
