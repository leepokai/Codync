import CodyncKit
import Foundation
import Observation

/// Whether the computer's linked speech server can transcribe local-engine dictation, as its
/// settings card shows it: the computer's answer, or why it couldn't be asked.
@MainActor @Observable final class LocalDictationStatus {
    /// The computer's answer while it hasn't given one yet.
    private(set) var local: LocalDictation?
    private(set) var error: String?

    /// Asks `store`'s computer. Run while the card shows, again each time it (re)connects.
    func load(on store: BotStore) async {
        guard let client = store.client else {
            error = "Not connected to \(store.hostName)."
            return
        }
        do {
            local = try await client.localDictation()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
