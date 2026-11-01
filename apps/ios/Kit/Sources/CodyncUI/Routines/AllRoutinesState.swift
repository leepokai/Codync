import CodyncKit
import SwiftUI

/// The account menu's Routines: every bot's routines as last listed, and their switches.
@MainActor @Observable
final class AllRoutinesState {
    var routines: [Routine] = []
    var loaded = false
    /// A switch is being turned; the others wait for it.
    var busy = false
    var error: String?
    private var generation = 0

    /// Lists again; offline, the last listing stays without an error.
    func load(model: BotStore) async {
        guard model.client != nil else { return }
        generation += 1
        let request = generation
        do {
            let result = try await model.allRoutines()
            guard request == generation else { return }
            routines = result
            loaded = true
            error = nil
        } catch is CancellationError {} catch {
            if request == generation { self.error = error.localizedDescription }
        }
    }

    func setEnabled(_ routine: Routine, enabled: Bool, model: BotStore) async {
        guard !busy else { return }
        busy = true
        defer { busy = false }
        do {
            try await model.setRoutineEnabled(routine, enabled: enabled)
            error = nil
            await load(model: model)
        } catch { self.error = error.localizedDescription }
    }

    /// A routine just saved, shown before the next listing brings it.
    func saved(_ routine: Routine) {
        if let i = routines.firstIndex(where: { $0.id == routine.id }) { routines[i] = routine } else { routines.append(routine) }
    }
}
