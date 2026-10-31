import CodyncKit
import SwiftUI

/// The kinds a phone outline can add, in the order the design lists them.
struct WorkflowAddStepSheet: View {
    let route: AddRoute
    var outline: WorkflowOutline?
    var add: (WorkflowNodeKind) -> Void

    private let kinds: [(WorkflowNodeKind, String)] = [
        (.agent, "Assistant"), (.condition, "Condition"), (.approval, "Approval"), (.prompt, "Prompt"), (.output, "Output"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ModalHeader("Add step")
            VStack(alignment: .leading, spacing: 4) {
                ForEach(kinds, id: \.1) { kind, label in
                    row(kind, label)
                }
            }
            Spacer(minLength: 0)
        }
        .background(Palette.background)
    }

    @ViewBuilder private func row(_ kind: WorkflowNodeKind, _ label: String) -> some View {
        let condition = kind == .condition
        let canAddCondition = route.after == outline?.trigger || route.after.map { outline?.main.contains($0) == true } == true
        if condition && outline?.condition != nil && canAddCondition {
            Text(L10n.tr("This workflow already has a condition"))
                .font(.callout)
                .foregroundStyle(Palette.secondary)
                .padding(.vertical, 12)
                .padding(.horizontal, 20)
        } else if condition && !canAddCondition {
            EmptyView()
        } else {
            Button(L10n.tr(label)) { add(kind) }
                .font(.body)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.text)
                .padding(.vertical, 12)
                .padding(.horizontal, 20)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
