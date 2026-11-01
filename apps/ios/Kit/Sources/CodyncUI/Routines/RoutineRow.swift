import CodyncKit
import SwiftUI

/// One routine in a list: its name and a line under it (opens the editor), and its on/off switch.
struct RoutineRow: View {
    let routine: Routine
    let summary: String
    let disabled: Bool
    let open: () -> Void
    let toggle: @MainActor (Bool) -> Void

    var body: some View {
        HStack(spacing: 12) {
            Button(action: open) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(routine.name).foregroundStyle(Palette.text)
                    Text(L10n.tr(summary))
                        .font(.caption)
                        .foregroundStyle(routine.lastError == nil ? Palette.secondary : Palette.danger)
                }
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Toggle(isOn: Binding(get: { routine.enabled }, set: { toggle($0) })) { EmptyView() }
                .toggleStyle(.codync)
                .fixedSize()
                .accessibilityLabel(routine.name)
                .disabled(disabled)
        }
        .outlinedCard(vertical: 0)
    }
}
