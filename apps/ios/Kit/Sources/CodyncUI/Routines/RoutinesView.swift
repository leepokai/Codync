import CodyncKit
import SwiftUI

/// A bot's routines; a row opens its editor.
struct RoutinesView: View {
    let botId: String
    var initialId: String?
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.locale) private var locale
    @State private var routines: [Routine] = []
    @State private var runs: [RoutineRun] = []
    @State private var followUps: [FollowUp] = []
    /// The open editor: on a routine, or (`routineId` nil) setting up a new one.
    @State private var editing: EditorTarget?
    @State private var loaded = false
    @State private var busy = false
    @State private var error: String?
    @State private var loadError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(L10n.tr("Routines"))
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .padding(.leading, 2)
                Spacer(minLength: 0)
                IconButton("Set up a routine", systemImage: "plus") { open(nil) }
                    .disabled(model.isOffline)
            }
            if let loadError { Text(loadError).font(.caption).foregroundStyle(Palette.danger) }
            if let error { Text(L10n.tr(error)).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
            listing
        }
        .foregroundStyle(Palette.text)
        .font(.system(size: 13))
        .onChange(of: initialId) { _, id in
            if let id { open(id) }
        }
        .task(id: botId) {
            if let initialId { open(initialId) }
            repeat {
                await load()
                do { try await Task.sleep(for: .seconds(3)) } catch { break }
            } while !Task.isCancelled
        }
        // Item-based, so the editor always gets the routine it was opened on; it opens once that routine is listed.
        .codyncSheet(item: Binding(get: { editing.flatMap { $0.isReady(in: routines) ? $0 : nil } }, set: { editing = $0 })) { target in
            RoutineEditorView(botId: botId, routine: routines.first { $0.id == target.routineId }) { saved in
                if let i = routines.firstIndex(where: { $0.id == saved?.id }), let saved { routines[i] = saved }
                else if let saved { routines.append(saved) }
                // A new webhook routine stays open: its URL and key exist only now.
                if target.routineId == nil, let saved, saved.triggers.contains(where: { $0.type == "webhook" }) {
                    editing = EditorTarget(id: target.id, routineId: saved.id)
                } else {
                    animate { editing = nil }
                }
                Task { await load() }
            }
            // The list is part of bot details; a routine's editor is the routines screen, as on the desktop.
            .tracked(.screenViewed(.routines))
        }
    }

    private var listing: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(followUps) { followUp in
                row(followUp).outlinedCard(vertical: 0, dashed: true)
            }
            if routines.isEmpty {
                Text(L10n.tr(loaded ? "No routines yet. Ask the bot for one, or set it up yourself with +." : "Loading routines…"))
                    .font(.system(size: 12))
                    .lineSpacing(3)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .outlinedCard(vertical: 14)
            } else {
                VStack(spacing: 8) {
                    ForEach(routines) { routine in
                        row(routine).outlinedCard(vertical: 0)
                    }
                }
            }
        }
    }

    private func row(_ routine: Routine) -> some View {
        HStack(spacing: 12) {
            Button { open(routine.id) } label: {
                VStack(alignment: .leading, spacing: 3) {
                    Text(routine.name).foregroundStyle(Palette.text)
                    Text(summary(routine))
                        .font(.caption)
                        .foregroundStyle(routine.lastError == nil ? Palette.secondary : Palette.danger)
                }
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Toggle(isOn: Binding(get: { routine.enabled }, set: { action("setRoutineEnabled", routine, enabled: $0) })) { EmptyView() }
                .toggleStyle(.codync)
                .fixedSize()
                .accessibilityLabel(routine.name)
                .disabled(busy || model.isOffline)
        }
    }

    /// A follow-up is the bot's own one-off, not a standing routine: a clock, when it happens, and a way to drop it.
    private func row(_ followUp: FollowUp) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "clock")
                .font(.system(size: 13))
                .foregroundStyle(Palette.secondary)
                .padding(.top, 12)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(followUp.note).foregroundStyle(Palette.text).lineLimit(2)
                Text(followUp.line(locale: locale)).font(.caption).monospacedDigit().foregroundStyle(Palette.secondary)
            }
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            IconButton("Cancel follow-up", systemImage: "xmark") { cancel(followUp) }
                .disabled(busy || model.isOffline)
                .padding(.top, 2)
                .padding(.trailing, -6)
        }
    }

    /// The row's second line, translated: what's happening now, else when it runs.
    private func summary(_ routine: Routine) -> String {
        if let active = runs.first(where: { $0.routineId == routine.id && $0.isActive }) { return L10n.tr(runLabel(active)) }
        if routine.lastError != nil { return L10n.tr("Schedule needs attention") }
        if let last = runs.first(where: { $0.routineId == routine.id }), ["failed", "interrupted"].contains(last.status) {
            return L10n.tr("Last run {status}").replacingOccurrences(of: "{status}", with: L10n.tr(runLabel(last)).lowercased())
        }
        return routine.scheduleLine
    }

    private func runLabel(_ run: RoutineRun) -> String {
        switch run.status {
        case "pending": run.notBefore.map(retrying) ?? "Queued"
        case "starting": "Starting"
        case "running": "Running"
        case "recovering": "Resuming after restart"
        case "failed": "Failed"
        case "interrupted": "Interrupted"
        default: run.status.capitalized
        }
    }

    /// A retry of a run that failed before acting, waiting until `at` (Unix ms).
    private func retrying(_ at: Int64) -> String {
        let date = Date(milliseconds: at)
        return date > .now ? "Retrying at \(date.formatted(date: .omitted, time: .shortened))" : "Retrying"
    }

    private func animate(_ change: () -> Void) { withAnimation(Motion.reduced(Motion.layout, reduceMotion), change) }
    private func open(_ id: String?) { animate { editing = EditorTarget(id: UUID().uuidString, routineId: id) } }

    private func action(_ method: String, _ routine: Routine, enabled: Bool? = nil) {
        perform { try await $0.routineAction(method, botId: botId, id: routine.id, enabled: enabled) }
    }

    private func cancel(_ followUp: FollowUp) {
        perform { try await $0.cancelFollowUp(botId: botId, id: followUp.id) }
    }

    /// One change at a time, then the list as the host now has it.
    private func perform(_ change: @escaping @MainActor (HostClient) async throws -> Void) {
        guard !busy, let client = model.client else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                try await change(client)
                error = nil
                await load()
            } catch { self.error = error.localizedDescription }
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            let result = try await client.routines(botId: botId)
            routines = result.routines
            runs = result.runs
            followUps = result.followUps ?? []
            loaded = true
            loadError = nil
        } catch is CancellationError {} catch { self.loadError = error.localizedDescription }
    }
}

struct EditorTarget: Identifiable {
    /// One per opening; kept when a new routine is saved and the editor stays on it.
    let id: String
    let routineId: String?

    /// A routine's editor waits until the routine is listed: a chat link can name one before the
    /// first load (or the next poll), and an empty form would save defaults over it.
    func isReady(in routines: [Routine]) -> Bool {
        routineId == nil || routines.contains { $0.id == routineId }
    }
}

extension Routine {
    /// When it runs: each schedule line translated, joined.
    var scheduleLine: String { triggerDescriptions.map { L10n.tr($0) }.joined(separator: " · ") }
}
