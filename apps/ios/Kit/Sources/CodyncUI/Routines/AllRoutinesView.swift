import CodyncKit
import SwiftUI

/// The account menu's Routines: every bot's routines, a section per bot, each with its switch; + sets one up for a bot.
public struct AllRoutinesView: View {
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var state = AllRoutinesState()
    /// Choosing which bot a new routine belongs to.
    @State private var picking = false
    @State private var editing: EditorTarget?

    public init() {}

    private var owners: [Bot] { Routine.owners(model.roster) }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader(picking ? "Which bot runs it?" : "Routines") {
                if picking {
                    IconButton("Back", systemImage: "chevron.left") { animate { picking = false } }
                } else {
                    IconButton("Set up a routine", systemImage: "plus") { setUp() }
                        .disabled(model.isOffline || owners.isEmpty)
                }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let error = state.error { Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled) }
                    if picking { ownerList } else { listing }
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.background)
        .task {
            repeat {
                await state.load(model: model)
                do { try await Task.sleep(for: .seconds(3)) } catch { break }
            } while !Task.isCancelled
        }
        // A routine's editor opens once that routine is listed (as in a bot's own list).
        .codyncSheet(item: Binding(get: { editing.flatMap { $0.isReady(in: state.routines) ? $0 : nil } }, set: { editing = $0 })) { target in
            RoutineEditorView(botId: target.botId ?? "", routine: state.routines.first { $0.id == target.routineId }) { saved in
                onSaved(target, saved)
            }
            .tracked(.screenViewed(.routines))
        }
    }

    @ViewBuilder private var listing: some View {
        let sections = Routine.sections(state.routines, bots: model.roster + model.hiddenBots)
        if sections.isEmpty {
            Text(L10n.tr(state.loaded ? "No routines yet. Set one up for a bot with +." : "Loading routines…"))
                .font(.system(size: 12))
                .lineSpacing(3)
                .foregroundStyle(Palette.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .outlinedCard(vertical: 14)
        } else {
            ForEach(sections) { section in
                VStack(alignment: .leading, spacing: 8) {
                    BotLine(bot: section.bot, size: 20)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Palette.secondary)
                        .accessibilityAddTraits(.isHeader)
                    ForEach(section.routines) { routine in
                        RoutineRow(routine: routine, summary: summary(routine), disabled: state.busy || model.isOffline,
                                   open: { open(botId: routine.botId, routineId: routine.id) },
                                   toggle: { enabled in Task { await state.setEnabled(routine, enabled: enabled, model: model) } })
                    }
                }
            }
        }
    }

    private var ownerList: some View {
        VStack(spacing: 8) {
            ForEach(owners) { bot in
                Button { open(botId: bot.id, routineId: nil) } label: {
                    BotLine(bot: bot, size: 24)
                        .foregroundStyle(Palette.text)
                        .padding(.vertical, 12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .outlinedCard(vertical: 0)
            }
        }
    }

    /// The row's second line: why its schedule needs a look, else when it runs.
    private func summary(_ routine: Routine) -> String {
        routine.lastError != nil ? "Schedule needs attention" : routine.triggerDescriptions.joined(separator: " · ")
    }

    private func setUp() {
        if owners.count == 1, let only = owners.first { open(botId: only.id, routineId: nil) } else { animate { picking = true } }
    }

    private func open(botId: String, routineId: String?) {
        animate {
            picking = false
            editing = EditorTarget(id: UUID().uuidString, routineId: routineId, botId: botId)
        }
    }

    private func onSaved(_ target: EditorTarget, _ saved: Routine?) {
        if let saved { state.saved(saved) }
        // A new webhook routine stays open: its URL and key exist only now.
        if target.routineId == nil, let saved, saved.triggers.contains(where: { $0.type == "webhook" }) {
            editing = EditorTarget(id: target.id, routineId: saved.id, botId: target.botId)
        } else {
            animate { editing = nil }
        }
        Task { await state.load(model: model) }
    }

    private func animate(_ change: () -> Void) { withAnimation(Motion.reduced(Motion.layout, reduceMotion), change) }
}

/// A bot's face and name on one line: a section's heading, or a choice in "Which bot runs it?".
private struct BotLine: View {
    let bot: Bot
    let size: CGFloat
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            if bot.isGroup {
                GroupAvatar(members: model.members(of: bot), size: size, animated: false)
            } else {
                CharacterAvatar(bot: bot, size: size, animated: false)
            }
            Text(bot.name).lineLimit(1)
        }
    }
}
