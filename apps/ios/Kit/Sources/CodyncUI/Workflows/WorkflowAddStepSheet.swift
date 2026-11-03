import CodyncKit
import SwiftUI

/// The kinds a phone outline can add, in the order the design lists them.
struct WorkflowAddStepSheet: View {
    var hasCondition: Bool
    var canAddCondition: Bool
    var add: (WorkflowNodeKind) -> Void

    init(route: AddRoute, outline: WorkflowOutline?, add: @escaping (WorkflowNodeKind) -> Void) {
        let onMain = route.after == outline?.trigger || route.after.map { outline?.main.contains($0) == true } == true
        self.hasCondition = outline?.condition != nil
        self.canAddCondition = onMain
        self.add = add
    }

    /// A new workflow's step list: one condition, then more steps run after it.
    init(draftHasCondition: Bool, add: @escaping (WorkflowNodeKind) -> Void) {
        self.hasCondition = draftHasCondition
        self.canAddCondition = true
        self.add = add
    }

    private let kinds: [(WorkflowNodeKind, String, String)] = [
        (.agent, "Agent", "An agent finishes this step with its tools."),
        (.condition, "Condition", "A yes or no that splits into two paths."),
        (.approval, "Approval", "The run waits for you to decide."),
        (.prompt, "Prompt", "Fixed text for the next step."),
        (.output, "Output", "Sends the result to you."),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ModalHeader("Add step")
            VStack(spacing: 10) {
                ForEach(kinds, id: \.1) { kind, title, detail in
                    row(kind, title, detail)
                }
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .background(Palette.background)
        .presentationDetents([.height(520)])
    }

    @ViewBuilder private func row(_ kind: WorkflowNodeKind, _ title: String, _ detail: String) -> some View {
        if kind == .condition && hasCondition && canAddCondition {
            Text(L10n.tr("This workflow already has a condition"))
                .font(.callout)
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(14)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        } else if kind == .condition && !canAddCondition {
            EmptyView()
        } else {
            Button { add(kind) } label: {
                HStack(spacing: 12) {
                    Image(systemName: WorkflowCopy.symbol(kind))
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Palette.text)
                        .frame(width: 36, height: 36)
                        .background(Palette.background, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(L10n.tr(title))
                            .font(.body.weight(.medium))
                            .foregroundStyle(Palette.text)
                        Text(L10n.tr(detail))
                            .font(.caption)
                            .foregroundStyle(Palette.secondary)
                            .lineLimit(2)
                    }
                    Spacer(minLength: 8)
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
            .buttonStyle(.plain)
        }
    }
}
