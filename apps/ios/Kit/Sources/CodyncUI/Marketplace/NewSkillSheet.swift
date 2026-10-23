import CodyncKit
import SwiftUI

struct NewSkillSheet: View {
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var name = ""
    @State private var summary = ""
    @State private var instructions = ""
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("New skill") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") {
                        saving = true
                        Task {
                            do {
                                try await model.addSkill(name: name, description: summary, instructions: instructions)
                                dismiss()
                            } catch {
                                self.error = error.localizedDescription
                            }
                            saving = false
                        }
                    }
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || instructions.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            form
        }
        .background(Palette.background)
    }

    private var form: some View {
        CardForm {
            CardSection(footer: "The bot sees the name and when to use it, and reads the instructions only when a task fits.") {
                TextField(L10n.tr("Name"), text: $name)
                TextField(L10n.tr("When to use it"), text: $summary, axis: .vertical).lineLimit(2...4)
            }
            CardSection("Instructions") {
                TextField(L10n.tr("Step by step, in plain words…"), text: $instructions, axis: .vertical)
                    .lineLimit(6...20)
            }
            if let error {
                CardSection { Text(L10n.tr(error)).foregroundStyle(Palette.danger) }
            }
        }
        .textFieldStyle(.plain)
    }
}
