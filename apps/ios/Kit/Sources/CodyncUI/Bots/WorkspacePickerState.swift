import CodyncKit
import SwiftUI

/// Request lifetime belongs to the picker: changing folders invalidates earlier responses.
@MainActor @Observable
final class WorkspacePickerState {
    enum Mode { case recent, browse }
    var mode = Mode.recent
    var listing: DirListing?
    var selected = ""
    var filter = ""
    var error: String?
    var busy = false
    var saving = false
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
            if mode == .browse { selected = result.path }
        } catch {
            guard request == generation, !Task.isCancelled else { return }
            self.error = error.localizedDescription
        }
        if request == generation { busy = false }
    }

    func confirm(model: BotStore) async -> String? {
        guard !selected.isEmpty, !busy, !saving else { return nil }
        saving = true
        let request = generation
        error = nil
        defer { saving = false }
        do {
            let path: String
            if listing?.recent != nil { path = try await model.selectWorkspace(selected) }
            else { path = try await model.listDirs(selected).path }
            guard request == generation, !Task.isCancelled else { return nil }
            return path
        } catch {
            self.error = error.localizedDescription
            return nil
        }
    }
}
