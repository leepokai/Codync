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
