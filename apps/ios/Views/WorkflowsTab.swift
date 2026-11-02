import CodyncKit
import CodyncUI
import SwiftUI

/// The Workflows tab: one computer's list. A link, a pin, or a computer's heading picks the computer.
struct WorkflowsTab: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts

    private var store: BotStore? {
        if let id = app.workflowComputer, let found = accounts.store(for: id) { return found }
        return app.currentStore ?? accounts.computers.first.flatMap { accounts.store(for: $0.id) }
    }

    var body: some View {
        NavigationStack {
            Group {
                if let store {
                    let link = app.pendingWorkflow?.computerId == store.computer.id ? app.pendingWorkflow : nil
                    WorkflowsView(pendingWorkflowId: link?.workflowId, pendingRunId: link?.runId,
                                  pendingToken: link?.workflowId == nil ? nil : link?.token)
                        .environment(store)
                        .id("\(store.computer.id)#\(app.workflowEpoch)")
                } else {
                    EmptyState(title: "No computer yet", systemImage: "flowchart")
                }
            }
            .toolbar {
                if accounts.computers.count > 1, let store {
                    ToolbarItem(placement: .topBarLeading) { computerMenu(store) }
                }
            }
        }
    }

    private func computerMenu(_ store: BotStore) -> some View {
        DropdownMenu {
            accounts.computers.map { computer in
                MenuItem(computer.name, selected: computer.id == store.computer.id) {
                    app.showWorkflows(on: computer.id)
                }
            }
        } label: {
            ComputerBadge(store.computer, size: 28)
        }
        .accessibilityLabel("Computer: \(store.hostName)")
    }
}
