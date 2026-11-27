import CodyncKit
import CodyncUI
import SwiftUI

/// The Threads tab, the desktop's Threads list: every task of the bots on the computers the Bots tab shows, in its
/// order, laid out as its filter menu says. A tap pushes the task's thread chat over the list, or opens the bot's chat
/// (on the Bots tab) at its card, or its details. Its header, the Bots tab's: the account, the title over how the
/// computers are connected, then the desktop's buttons: the filter menu, Search (threads by title or bot, flat
/// and newest first) and New thread, a page pushed over the list (`NewThreadView`), with no bot where a computer can
/// start one so, else with a bot.
struct ThreadsTab: View {
    let roster: [RosterItem]
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @AppStorage("hiddenComputers") private var hiddenComputers = ""
    @AppStorage(ListLayout.threadsGroup) private var grouping = ThreadGrouping.folder.rawValue
    @AppStorage(ListLayout.threadsByStatus) private var byStatus = false
    @AppStorage(ListLayout.threadsByActivity) private var byActivity = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var searching = false
    @State private var query = ""
    /// The new-thread page pushed over the list, which becomes the thread's chat once sent.
    @State private var newThread = false
    /// The thread pushed over the list.
    @State private var thread: ThreadTarget?

    var body: some View {
        let key = { (item: RosterItem) in "\(item.ref.computerId)/\(item.ref.botId)" }
        let sections = ThreadSections(roster, grouping: ThreadGrouping(rawValue: grouping) ?? .folder, byStatus: byStatus,
                                      byActivity: byActivity, key: key, computer: { $0.computer.name }, bot: \.bot)
        let query = query.trimmingCharacters(in: .whitespaces)
        let found = query.isEmpty ? nil : ThreadSections.found(roster, matching: query, key: key, bot: \.bot)
        NavigationStack {
            Group {
                if found?.isEmpty == true {
                    EmptyState(title: "No results", systemImage: "magnifyingglass")
                } else if found == nil && sections.needsYou.isEmpty && sections.groups.isEmpty && sections.recent.isEmpty {
                    EmptyState(title: "No threads right now", systemImage: "checklist",
                               message: "Tasks your bots work on show up here.")
                } else {
                    ThreadList(sections: sections, found: found, store: { accounts.store(for: $0.ref.computerId) },
                               openDetails: openDetails,
                               openChat: { item, task in
                                   if task.status == .needsYou { accounts.store(for: item.ref.computerId)?.requestCard(of: item.bot.id, task: task) }
                                   accounts.selection = item.ref
                               },
                               thread: $thread,
                               loadMore: { shownStores.forEach { $0.loadOlderThreads() } })
                }
            }
            .background(Palette.background)
            .threadChat($thread, store: { accounts.store(for: $0.ref.computerId) })
            .navigationDestination(isPresented: $newThread) {
                NewThreadView(bots: threadBots, plain: plainStore, store: { accounts.store(for: $0) },
                              item: { accounts.item($0, botId: $1) }, openDetails: openDetails)
                    .toolbar(.hidden, for: .tabBar)
            }
            .navigationTitle(L10n.tr("Threads"))
            .navigationBarTitleDisplayMode(.inline)
            .navigationSubtitle(accounts.connectionSummary(hidden: hiddenComputers))
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { AccountSwitcherButton() }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    ComputerFilterHeader(accounts: accounts, hidden: $hiddenComputers, manage: { app.showComputers = true }) {
                        group(.folder, "Group by folder", .folder)
                        group(.bot, "Group by bot", .bot)
                        group(.computer, "Group by computer", .computer)
                        Divider()
                        ListLayout.option("Sort by status", ListLayout.tracked($byStatus, .threads, .status, reduceMotion: reduceMotion))
                        ListLayout.option("Sort by last active", ListLayout.tracked($byActivity, .threads, .activity, reduceMotion: reduceMotion))
                    }
                    Button(L10n.tr("Search"), systemImage: "magnifyingglass") { searching = true }
                    if !threadBots.isEmpty || plainStore != nil {
                        Button(L10n.tr("New thread"), systemImage: "plus") { newThread = true }
                    }
                }
            }
            .modifier(ListSearch(searching: $searching, query: $query, prompt: "Search"))
        }
        // Older threads, first page of each computer that lists them, again whenever a computer starts them over.
        .task(id: shownStores.map { "\($0.computer.id)/\($0.olderThreadsGeneration)/\($0.listsOlderThreads)" }) {
            shownStores.forEach { $0.loadFirstOlderThreads() }
        }
    }

    /// The bots a thread can start with, in the roster's order: not a group or the box without a bot, on an online
    /// computer that starts threads (the desktop's `threadBots`).
    private var threadBots: [RosterItem] {
        roster.filter { item in
            guard !item.bot.isGroup, !item.bot.plain, let store = accounts.store(for: item.ref.computerId) else { return false }
            return store.startsThreads && store.connection == .online
        }
    }

    /// The first computer the tab shows where a thread may start without a bot, online and with an agent ready (the
    /// desktop's `plainStores`).
    private var plainStore: BotStore? {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hiddenComputers).shown.lazy
            .compactMap { accounts.store(for: $0) }
            .first { $0.startsThreadsWithoutBot && $0.connection == .online && !$0.readySeats(on: AgentLogin(backend: "")).isEmpty }
    }

    private var shownStores: [BotStore] {
        ComputerSelection(all: accounts.computers.map(\.id), hidden: hiddenComputers).shown.compactMap { accounts.store(for: $0) }
    }

    private func openDetails(_ item: RosterItem) {
        accounts.store(for: item.ref.computerId)?.requestDetails(of: item.bot.id)
        accounts.selection = item.ref
    }

    /// One grouping of the list's three, one at a time: unchecking it groups nothing.
    private func group(_ by: ThreadGrouping, _ title: String, _ option: AnalyticsEvent.ListOption) -> some View {
        ListLayout.option(title, Binding(get: { grouping == by.rawValue }, set: { on in
            Analytics.shared.capture(.listLayoutChanged(.threads, option: option, on: on))
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { grouping = (on ? by : .none).rawValue }
        }))
    }
}
