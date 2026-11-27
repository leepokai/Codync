import CodyncKit
import CodyncUI
import SwiftUI

/// The Workflows tab: one computer's workflows. Its header, the Bots tab's: the account, the title over the computer
/// shown, then the computer to show (with more than one) and Search; no New yet.
struct WorkflowsTab: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @State private var searching = false
    @State private var query = ""
    /// The workflow pushed over the list, here so Search opening or closing (which builds the list again) keeps it.
    @State private var opened: OpenedWorkflow?

    private var store: BotStore? {
        if let id = app.workflowComputer, let found = accounts.store(for: id) { return found }
        return app.currentStore ?? accounts.computers.first.flatMap { accounts.store(for: $0.id) }
    }

    var body: some View {
        NavigationStack {
            Group {
                if let store {
                    let link = app.pendingWorkflow?.computerId == store.computer.id ? app.pendingWorkflow : nil
                    WorkflowsView(opened: $opened, pendingWorkflowId: link?.workflowId, pendingRunId: link?.runId,
                                  pendingStepId: link?.stepId,
                                  pendingToken: link?.workflowId == nil ? nil : link?.token, query: query)
                        .environment(store)
                        .id("\(store.computer.id)#\(app.workflowEpoch)")
                        .navigationSubtitle(store.hostName)
                } else {
                    EmptyState(title: "No computer yet", image: Image("workflow").resizable())
                        .navigationTitle(L10n.tr("Workflows"))
                }
            }
            .navigationBarTitleDisplayMode(.inline)
            // Another computer or a new link starts from its list (outside the list, which those build again).
            .onChange(of: store.map { "\($0.computer.id)#\(app.workflowEpoch)" }) { opened = nil }
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { AccountSwitcherButton() }
                if let store {
                    ToolbarItemGroup(placement: .topBarTrailing) {
                        if accounts.computers.count > 1 { computerMenu(store) }
                        Button(L10n.tr("Search"), systemImage: "magnifyingglass") { searching = true }
                    }
                }
            }
            .modifier(ListSearch(searching: $searching, query: $query, prompt: "Search"))
        }
    }

    /// The computer whose workflows show, as a filter (one at a time).
    private func computerMenu(_ store: BotStore) -> some View {
        DropdownMenu {
            accounts.computers.map { computer in
                MenuItem(computer.name, selected: computer.id == store.computer.id) {
                    app.showWorkflows(on: computer.id)
                }
            }
        } label: {
            Image(systemName: "line.3.horizontal.decrease")
        }
        .accessibilityLabel("Computer: \(store.hostName)")
    }
}
