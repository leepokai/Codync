import CodyncKit
import SwiftUI

/// Every bot that has live or waiting tasks, in one sheet: the bot as a heading, which opens its details, over the
/// rows its details have for them, with the same actions. A row that needs the user opens the bot's chat, where its
/// card waits; any other row of a task a thread has opens the bot's details on its thread. It closes when the last
/// task ends.
public struct AllWorkView: View {
    let bots: [RosterItem]
    /// Several computers: a heading also names the bot's computer.
    let many: Bool
    let store: (RosterItem) -> BotStore?
    let openDetails: (RosterItem) -> Void
    /// The bot's chat, at the card `task` waits on.
    let openChat: (RosterItem, WorkTask) -> Void
    /// The bot's details, on the thread of `task`.
    let openThread: (RosterItem, WorkTask) -> Void
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Where a tap leads, once the sheet is gone: another sheet can't open while this one slides away.
    @State private var leaving: (() -> Void)?

    public init(bots: [RosterItem], many: Bool, store: @escaping (RosterItem) -> BotStore?,
                openDetails: @escaping (RosterItem) -> Void, openChat: @escaping (RosterItem, WorkTask) -> Void,
                openThread: @escaping (RosterItem, WorkTask) -> Void) {
        self.bots = bots
        self.many = many
        self.store = store
        self.openDetails = openDetails
        self.openChat = openChat
        self.openThread = openThread
    }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Work")
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    ForEach(bots) { row in
                        if let store = store(row) {
                            VStack(alignment: .leading, spacing: 8) {
                                heading(row, members: store.members(of: row.bot))
                                WorkTimeline(plain: row.bot, openCard: { task in leave(to: { openChat(row, task) }) },
                                             openThread: { task in leave(to: { openThread(row, task) }) })
                            }
                            .environment(store)
                            .transition(.opacity)
                        }
                    }
                }
                .padding(20)
                .environment(\.cardStyle, .outlined)
                .animation(Motion.reduced(Motion.layout, reduceMotion), value: bots.map(\.id))
            }
        }
        .background(Palette.background)
        .onChange(of: bots.isEmpty, initial: true) { if bots.isEmpty { dismiss() } }
        .onDisappear { leaving?() }
        .tracked(.screenViewed(.allWork))
    }

    private func heading(_ row: RosterItem, members: [Bot]) -> some View {
        Button { leave(to: { openDetails(row) }) } label: {
            HStack(spacing: 8) {
                BotAvatar(bot: row.bot, members: members, size: 22, animated: false)
                Text(verbatim: many ? "\(row.bot.name) · \(row.computer.name)" : row.bot.name)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
            .padding(.leading, 2)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(L10n.tr("{name}, view details").replacingOccurrences(of: "{name}", with: row.bot.name))
    }

    private func leave(to destination: @escaping () -> Void) {
        leaving = destination
        dismiss()
    }
}
