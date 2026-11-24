import CodyncKit
import CodyncUI
import SwiftUI

/// The Threads tab: every bot with live or waiting tasks, on the computers the Bots tab shows, in its order. A tap
/// opens the bot's chat (on the Bots tab) at its card, its details, or its details on the task's thread.
struct ThreadsTab: View {
    let work: AllWork<RosterItem>
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        NavigationStack {
            Group {
                if work.bots.isEmpty {
                    EmptyState(title: "No threads right now", systemImage: "checklist",
                               message: "Tasks your bots work on show up here.")
                } else {
                    AllWorkView(bots: work.bots, many: Set(work.bots.map(\.ref.computerId)).count > 1,
                                store: { accounts.store(for: $0.ref.computerId) },
                                openDetails: { item in
                                    accounts.store(for: item.ref.computerId)?.requestDetails(of: item.bot.id)
                                    accounts.selection = item.ref
                                },
                                openChat: { item, task in
                                    accounts.store(for: item.ref.computerId)?.requestCard(of: item.bot.id, task: task)
                                    accounts.selection = item.ref
                                },
                                openThread: { item, task in
                                    accounts.store(for: item.ref.computerId)?.requestThread(of: item.bot.id, task: task)
                                    accounts.selection = item.ref
                                })
                }
            }
            .background(Palette.background)
            .navigationTitle(L10n.tr("Threads"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

extension AccountStore {
    /// Every bot on the computers the list shows (`hidden`: the ones left out), in the order the Bots tab has them,
    /// folded computers and sections included, with how much each one works on.
    func work(hidden: String, sectionOrder: RosterSectionOrder) -> AllWork<RosterItem> {
        let shown = ComputerSelection(all: computers.map(\.id), hidden: hidden).shown
        let rows = computers.filter { shown.contains($0.id) }.flatMap { computer -> [RosterItem] in
            RosterSection.onScreen(roster.filter { $0.ref.computerId == computer.id }, order: sectionOrder.saved(on: computer.id)) { $0.bot }
        }
        return AllWork(rows) { $0.bot.work }
    }
}
