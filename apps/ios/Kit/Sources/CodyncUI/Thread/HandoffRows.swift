import CodyncKit
import SwiftUI

/// The bot's chat starts over: what the new conversation carries, which the user can edit, or turn down to carry on.
struct HandoffCardRow: View {
    let entry: Entry
    let card: HandoffCard
    @Environment(BotStore.self) private var model
    @State private var expanded = false
    @State private var editing = false

    var body: some View {
        if card.isDraft {
            VStack(alignment: .leading, spacing: 10) {
                Label(entry.data.text ?? "", systemImage: "arrow.triangle.branch")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                Disclosure(isExpanded: $expanded) {
                    carried
                } label: {
                    Text(L10n.tr("What it carries")).font(.footnote.weight(.medium)).foregroundStyle(Palette.text)
                }
                HStack(spacing: 8) {
                    if model.answering[entry.id] != nil {
                        Spinner()
                    } else {
                        Button(L10n.tr("Keep this conversation")) { model.keepConversation(entry) }
                            .buttonStyle(SecondaryButtonStyle())
                        IconButton("Edit what it carries", systemImage: "pencil") {
                            withAnimation(Motion.layout) { editing = true }
                        }
                    }
                }
            }
            .padding(12)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14))
            .padding(.trailing, 40)
            .codyncSheet(isPresented: $editing) { HandoffEditorView(entry: entry, text: card.text ?? "") }
        } else {
            Disclosure(isExpanded: $expanded) {
                carried
            } label: {
                Label(entry.data.text ?? "", systemImage: "arrow.triangle.branch")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private var carried: some View {
        Text(card.text ?? "")
            .font(.footnote.monospaced())
            .foregroundStyle(Palette.secondary)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The bot moved to another agent of its chain, or back to its own: one line, like a fresh conversation's, that
/// unfolds to what the new agent was told.
struct AgentSwitchRow: View {
    let entry: Entry
    let change: AgentSwitch
    @State private var expanded = false

    var body: some View {
        if let carried = change.carried, !carried.isEmpty {
            Disclosure(isExpanded: $expanded) {
                Text(carried)
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .textSelection(.enabled)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 12))
                    .transition(.opacity)
            } label: {
                line
            }
        } else {
            line.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var line: some View {
        Label(entry.data.text ?? "", systemImage: "arrow.turn.down.right")
            .font(.footnote)
            .foregroundStyle(Palette.secondary)
    }
}

/// Edits what the next conversation starts with.
struct HandoffEditorView: View {
    let entry: Entry
    @State private var text: String
    @State private var saving = false
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss

    init(entry: Entry, text: String) {
        self.entry = entry
        _text = State(initialValue: text)
    }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Edit the handoff") {
                if saving {
                    Spinner()
                } else {
                    IconButton("Save", systemImage: "checkmark") {
                        saving = true
                        Task {
                            if await model.editHandoff(entry, text: trimmed) { dismiss() }
                            saving = false
                        }
                    }
                    .disabled(trimmed.isEmpty)
                }
            }
            CardForm {
                CardSection(footer: "Your next message starts a fresh conversation that begins with this.") {
                    TextField("What the new conversation should know", text: $text, axis: .vertical)
                        .lineLimit(10...24)
                        .font(.footnote.monospaced())
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }
}
