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

/// The picker's answer: checks the folder on the computer (recording it there as recent) and hands back the path it
/// resolved, or keeps the error on screen.
@MainActor @Observable
final class WorkspaceConfirm {
    var saving = false
    var error: String?

    func confirm(
        _ path: String, listing: DirListing?,
        select: (String) async throws -> String, fetch: (String?) async throws -> DirListing
    ) async -> String? {
        guard !path.isEmpty, !saving else { return nil }
        saving = true
        error = nil
        defer { saving = false }
        do {
            // Absence of `recent` means an older host: validate with its existing browsing API.
            let chosen = listing?.recent != nil ? try await select(path) : try await fetch(path).path
            return Task.isCancelled ? nil : chosen
        } catch {
            self.error = error.localizedDescription
            return nil
        }
    }
}
