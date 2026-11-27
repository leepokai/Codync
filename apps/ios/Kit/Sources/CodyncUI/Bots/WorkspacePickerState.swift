import CodyncKit
import SwiftUI

/// One page of the folder picker: the recent workspaces, or one folder being browsed. Request lifetime belongs to the
/// page, so loading again invalidates earlier responses.
@MainActor @Observable
final class WorkspacePickerState {
    var listing: DirListing?
    var error: String?
    var busy = false
    private var generation = 0

    func load(_ path: String?, model: BotStore) async {
        generation += 1
        let request = generation
        busy = true
        error = nil
        listing = nil
        do {
            let result = try await model.listDirs(path)
            guard request == generation, !Task.isCancelled else { return }
            listing = result
        } catch {
            guard request == generation, !Task.isCancelled else { return }
            self.error = error.localizedDescription
        }
        if request == generation { busy = false }
    }
}
